package io.forge.sdk;

import com.fasterxml.jackson.core.type.TypeReference;
import com.fasterxml.jackson.databind.ObjectMapper;

import java.net.URI;
import java.net.http.HttpClient;
import java.net.http.HttpRequest;
import java.net.http.HttpResponse;
import java.time.Duration;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.Executors;
import java.util.concurrent.ScheduledExecutorService;
import java.util.concurrent.ScheduledFuture;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicBoolean;

/**
 * A Forge worker: polls a queue and runs the registered handlers.
 *
 * <p>Three behaviours are load-bearing rather than cosmetic, and all three were
 * absent before:
 *
 * <ul>
 *   <li>The {@code lease_id} returned by dequeue is captured and sent back on
 *       every heartbeat and completion. Without it the server's CompletionGate is
 *       unreachable: a completion after the lease expired returns 409, and the
 *       old code discarded that error, so the work was re-dispatched with the
 *       side effect applied twice.
 *   <li>Failures carry an {@code error_class}. The server records an absent one
 *       as PERMANENT, which is not retryable, so every failure terminated the
 *       execution and the retry machinery was unreachable from Java.
 *   <li>Every request has a read timeout, not only a connect timeout. A connect
 *       timeout does nothing about a server that accepts the connection and then
 *       never answers, which hung the worker thread indefinitely - and the old
 *       {@code completeJob} and {@code failJob} discarded the response
 *       entirely, so a lost lease looked like success.
 * </ul>
 *
 * <p>Typical use:
 *
 * <pre>{@code
 * ForgeWorker w = new ForgeWorker(baseUrl, tenantId, adminToken).withWorkerId("w-1");
 * w.registerWorker("worker-1", "localhost", List.of("*"));
 * w.register("settle", ctx -> settle(ctx.getPayload()));
 * w.start("critical", Duration.ofSeconds(2));
 * }</pre>
 */
public class ForgeWorker {
    private final String baseUrl;
    private final String tenantId;
    private String apiKey;
    private String workerId = "java-worker-1";
    private final HttpClient client;
    private final ObjectMapper mapper = new ObjectMapper();
    private final Map<String, JobHandler> handlers = new ConcurrentHashMap<>();
    private final AtomicBoolean running = new AtomicBoolean(false);
    private final AtomicBoolean stopping = new AtomicBoolean(false);
    private final long requestTimeoutMillis;

    private ScheduledExecutorService workerHeartbeat;

    public ForgeWorker(String baseUrl, String tenantId, String apiKey) {
        this(baseUrl, tenantId, apiKey, Duration.ofSeconds(15));
    }

    public ForgeWorker(String baseUrl, String tenantId, String apiKey, Duration requestTimeout) {
        this.baseUrl = baseUrl.replaceAll("/+$", "");
        this.tenantId = tenantId;
        this.apiKey = apiKey;
        this.requestTimeoutMillis = requestTimeout.toMillis();
        this.client = HttpClient.newBuilder()
                .connectTimeout(Duration.ofSeconds(10))
                .build();
    }

    public ForgeWorker withWorkerId(String workerId) {
        this.workerId = workerId;
        return this;
    }

    /** The credential in use: a worker token after {@link #registerWorker}. */
    public String getToken() {
        return apiKey;
    }

    public String getWorkerId() {
        return workerId;
    }

    /** Associates a handler with a job name. */
    public ForgeWorker register(String jobName, JobHandler handler) {
        handlers.put(jobName, handler);
        return this;
    }

    /** Backwards-compatible alias for {@link #register}. */
    public void registerJob(String name, JobHandler handler) {
        register(name, handler);
    }

    /**
     * Registers with the server, acquiring a worker id and worker token.
     *
     * <p>Needs an operator credential: a worker token carries only
     * {@code workers:claim}, {@code workers:heartbeat} and
     * {@code executions:write}, so a worker cannot register itself.
     */
    public Map<String, Object> registerWorker(String name, String hostname, List<String> capabilities)
            throws Exception {
        List<String> caps = capabilities == null || capabilities.isEmpty()
                ? List.of("*")
                : capabilities;

        Map<String, Object> data = post("/workers/register",
                Map.of("name", name,
                        "hostname", hostname != null ? hostname : "localhost",
                        "capabilities", caps),
                Duration.ofSeconds(15));

        Object id = data.get("id");
        if (id != null && !id.toString().isEmpty()) {
            this.workerId = id.toString();
        }
        Object token = data.get("token");
        if (token != null && !token.toString().isEmpty()) {
            this.apiKey = token.toString();
        }
        return data;
    }

    /**
     * Polls the queue until {@link #stop()} is called.
     *
     * @throws IllegalStateException when no handler is registered. Better to
     *     fail immediately than to poll a queue nothing can execute.
     */
    public void start(String queue, Duration pollInterval) throws Exception {
        if (handlers.isEmpty()) {
            throw new IllegalStateException(
                    "ForgeWorker: no handlers registered; call register(...) first");
        }
        running.set(true);
        stopping.set(false);
        System.out.printf("Forge worker %s listening on queue \"%s\" for %d handler(s)%n",
                workerId, queue, handlers.size());

        // A worker that stops reporting is marked OFFLINE and filtered out of
        // dispatch, so the worker itself must heartbeat, not only its executions.
        workerHeartbeat = Executors.newSingleThreadScheduledExecutor(r -> {
            Thread thread = new Thread(r, "forge-worker-heartbeat");
            thread.setDaemon(true);
            return thread;
        });
        workerHeartbeat.scheduleAtFixedRate(() -> {
            try {
                post("/workers/" + workerId + "/heartbeat", Map.of(), Duration.ofSeconds(5));
            } catch (Exception ignored) {
                // A missed beat is not fatal; the next one follows.
            }
        }, 30, 30, TimeUnit.SECONDS);

        try {
            while (running.get() && !stopping.get()) {
                Map<String, Object> execution = null;
                try {
                    execution = pollOnce(queue);
                } catch (ForgeException failure) {
                    System.err.printf("forge: error polling queue %s: %s%n",
                            queue, failure.getMessage());
                    if (failure.getStatus() == 401 || failure.getStatus() == 403) {
                        // A credential problem will not fix itself by polling
                        // harder, and a tight loop against it buries the reason.
                        throw failure;
                    }
                } catch (Exception other) {
                    System.err.printf("forge: error polling queue %s: %s%n", queue, other);
                }

                if (execution != null) {
                    execute(execution);
                    continue;
                }
                Thread.sleep(pollInterval.toMillis());
            }
        } finally {
            running.set(false);
            stop();
        }
    }

    /** Stops polling and the heartbeat. */
    public void stop() {
        stopping.set(true);
        running.set(false);
        if (workerHeartbeat != null) {
            workerHeartbeat.shutdownNow();
            workerHeartbeat = null;
        }
    }

    /**
     * Claims a single execution, or {@code null} when the queue is empty.
     *
     * <p>Separate from {@link #start} because a serverless or event-driven
     * worker wants one claim per invocation, and a test wants to drive the
     * protocol deterministically rather than racing a loop.
     */
    public Map<String, Object> pollOnce(String queue) throws Exception {
        Map<String, Object> data = post("/queues/" + queue + "/dequeue",
                Map.of("worker_id", workerId), Duration.ofSeconds(20));
        if (data == null || data.get("id") == null) {
            return null;
        }
        // The lease travels with the work. Dropping it here is what made a
        // completion after lease expiry an unrecoverable 409.
        return data;
    }

    /**
     * Runs a claimed execution to completion and reports its outcome.
     *
     * <p>Separate from {@link #start} for the same reason as
     * {@link #pollOnce}: the caller decides the lifecycle.
     */
    public void execute(Map<String, Object> execution) throws Exception {
        String executionId = String.valueOf(execution.get("id"));
        String jobName = execution.get("job_name") != null
                ? String.valueOf(execution.get("job_name"))
                : String.valueOf(execution.getOrDefault("type", ""));
        Object rawLease = execution.get("lease_id");
        String leaseId = rawLease == null ? null : String.valueOf(rawLease);

        Map<String, Object> payload = execution.get("input") instanceof Map
                ? castMap(execution.get("input"))
                : Map.of();

        JobHandler handler = handlers.get(jobName);
        if (handler == null) {
            fail(executionId, leaseId,
                    "no handler registered for job \"" + jobName + "\"",
                    ErrorClass.NOT_FOUND, null);
            return;
        }

        JobContext ctx = new JobContext(executionId, jobName, payload, leaseId, workerId,
                this::sendLog);

        // The lease is renewed while the handler runs; without it a long handler
        // outlives its lease and the work is re-dispatched underneath it.
        ScheduledExecutorService heartbeat =
                Executors.newSingleThreadScheduledExecutor(r -> {
                    Thread thread = new Thread(r, "forge-execution-heartbeat");
                    thread.setDaemon(true);
                    return thread;
                });
        ScheduledFuture<?> beats = heartbeat.scheduleAtFixedRate(() -> {
            try {
                Map<String, Object> body = new HashMap<>();
                body.put("worker_id", workerId);
                if (leaseId != null) {
                    body.put("lease_id", leaseId);
                }
                post("/executions/" + executionId + "/heartbeat", body, Duration.ofSeconds(5));
            } catch (Exception ignored) {
                // A missed beat is not fatal on its own; the lease may already
                // have been recovered, which the completion will report.
            }
        }, 10, 10, TimeUnit.SECONDS);

        try {
            ctx.log("starting " + jobName);
            Object result = handler.handle(ctx);
            complete(executionId, leaseId, result);
            ctx.log("completed " + jobName);
        } catch (ForgeException failure) {
            if (failure.getStatus() == 409) {
                // The lease is gone, so this worker no longer owns the
                // execution. Reporting a success would apply the side effect a
                // second time; the right action is to stop and let the reaper
                // retry it.
                ctx.log("lease lost during " + jobName
                        + "; abandoning without reporting an outcome");
                return;
            }
            ctx.log(jobName + " failed: " + failure.getMessage());
            fail(executionId, leaseId, failure.getMessage(),
                    ErrorClass.classify(failure), stackTraceOf(failure));
        } catch (Exception other) {
            ctx.log(jobName + " failed: " + other);
            fail(executionId, leaseId, String.valueOf(other),
                    ErrorClass.classify(other), stackTraceOf(other));
        } finally {
            // Cancelled, not shut down: an in-flight renewal must not race the
            // completion that follows it.
            beats.cancel(true);
            heartbeat.shutdownNow();
        }
    }

    /**
     * Reports a successful execution.
     *
     * <p>Throws when the lease has gone, so the caller can distinguish "someone
     * else owns this now" from "the network broke" and stop rather than retry.
     */
    public void complete(String executionId, String leaseId, Object output) throws Exception {
        Map<String, Object> body = new HashMap<>();
        body.put("worker_id", workerId);
        if (leaseId != null) {
            body.put("lease_id", leaseId);
        }
        body.put("succeeded", true);
        body.put("output", output);
        post("/executions/" + executionId + "/complete", body,
                Duration.ofMillis(requestTimeoutMillis));
    }

    /**
     * Reports a failure with a classification.
     *
     * <p>{@code error_class} is the field that decides whether the execution is
     * retried; omitting it makes every failure PERMANENT, which is what made
     * retries unreachable from Java.
     */
    public void fail(String executionId, String leaseId, String message,
                     String errorClass, String trace) throws Exception {
        Map<String, Object> body = new HashMap<>();
        body.put("worker_id", workerId);
        if (leaseId != null) {
            body.put("lease_id", leaseId);
        }
        body.put("succeeded", false);
        body.put("error", message);
        body.put("error_class", errorClass);
        if (trace != null && !trace.isEmpty()) {
            body.put("trace", trace);
        }
        try {
            post("/executions/" + executionId + "/fail", body,
                    Duration.ofMillis(requestTimeoutMillis));
        } catch (ForgeException conflict) {
            // A lost lease is not a failure to report; the work will be retried.
            if (conflict.getStatus() != 409) {
                throw conflict;
            }
        }
    }

    /** Logs a line for an execution. Best-effort by contract. */
    void sendLog(String executionId, String stream, String message) {
        try {
            post("/executions/" + executionId + "/logs",
                    Map.of("stream", stream, "message", message), Duration.ofSeconds(5));
        } catch (Exception ignored) {
            // Logging is never worth failing an execution over.
        }
    }

    /**
     * Issues one request and unwraps the standard envelope.
     *
     * <p>Every call goes through here, which makes "every request is bounded" and
     * "errors are structured" properties of the SDK rather than things each call
     * site has to remember.
     */
    private Map<String, Object> post(String path, Map<String, Object> body, Duration timeout)
            throws Exception {
        String json = mapper.writeValueAsString(body);
        HttpRequest request = HttpRequest.newBuilder()
                .uri(URI.create(baseUrl + path))
                .header("Authorization", "Bearer " + apiKey)
                .header("Content-Type", "application/json")
                .timeout(timeout)
                .POST(HttpRequest.BodyPublishers.ofString(json))
                .build();

        HttpResponse<String> response;
        try {
            response = client.send(request, HttpResponse.BodyHandlers.ofString());
        } catch (java.net.http.HttpTimeoutException timedOut) {
            throw new ForgeException(
                    "request to " + path + " timed out after " + timeout.toMillis() + "ms",
                    ErrorClass.TIMEOUT, 504, null);
        } catch (java.io.IOException unreachable) {
            throw new ForgeException(
                    "could not reach Forge at " + path + ": " + unreachable.getMessage(),
                    "UNREACHABLE", 503, null);
        }

        String raw = response.body() == null ? "" : response.body();

        if (response.statusCode() >= 400) {
            throw toForgeException(path, response.statusCode(), raw);
        }
        if (raw.isBlank()) {
            return Map.of();
        }

        Map<String, Object> envelope = mapper.readValue(raw, new TypeReference<>() { });
        Object data = envelope.get("data");
        return data instanceof Map ? castMap(data) : Map.of();
    }

    /** Turns the server's error envelope into a typed exception. */
    private ForgeException toForgeException(String path, int status, String raw) {
        String message = "Forge returned HTTP " + status + " for " + path;
        String code = "HTTP_ERROR";
        String requestId = null;

        try {
            Map<String, Object> parsed = mapper.readValue(raw, new TypeReference<>() { });
            Object error = parsed.get("error");
            if (error instanceof Map) {
                Map<String, Object> typed = castMap(error);
                if (typed.get("message") != null) {
                    message = String.valueOf(typed.get("message"));
                }
                if (typed.get("code") != null) {
                    code = String.valueOf(typed.get("code"));
                }
                if (typed.get("request_id") != null) {
                    requestId = String.valueOf(typed.get("request_id"));
                }
            }
        } catch (Exception ignored) {
            // Not an envelope; the status-based message above stands.
        }
        return new ForgeException(message, code, status, requestId);
    }

    @SuppressWarnings("unchecked")
    private static Map<String, Object> castMap(Object value) {
        return (Map<String, Object>) value;
    }

    private static String stackTraceOf(Throwable cause) {
        if (cause == null) {
            return null;
        }
        // The server stores `trace` but does not surface it, so what is useful
        // here is the exception's own chain, which at least names the failing
        // step.
        StringBuilder builder = new StringBuilder();
        Throwable current = cause;
        while (current != null) {
            builder.append(current).append('\n');
            current = current.getCause();
        }
        return builder.toString();
    }
}
