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
import java.util.concurrent.*;
import java.util.concurrent.atomic.AtomicBoolean;

public class ForgeWorker {
    private final String baseUrl;
    private final String tenantId;
    private String apiKey;
    private String workerId = "java-worker-1";
    private final HttpClient client;
    private final Map<String, JobHandler> handlers = new ConcurrentHashMap<>();
    private final ObjectMapper mapper = new ObjectMapper();
    private final AtomicBoolean running = new AtomicBoolean(false);
    private final ExecutorService executor = Executors.newCachedThreadPool();
    private ScheduledExecutorService workerHbScheduler;

    public ForgeWorker(String baseUrl, String tenantId, String apiKey) {
        this.baseUrl = baseUrl.replaceAll("/+$", "");
        this.tenantId = tenantId;
        this.apiKey = apiKey;
        this.client = HttpClient.newBuilder()
                .connectTimeout(Duration.ofSeconds(10))
                .build();
    }

    public ForgeWorker withWorkerId(String workerId) {
        this.workerId = workerId;
        return this;
    }

    public Map<String, Object> registerWorker(String name, String hostname, List<String> capabilities) throws Exception {
        Map<String, Object> req = Map.of(
                "name", name,
                "hostname", hostname != null ? hostname : "localhost",
                "capabilities", capabilities != null ? capabilities : List.of("*")
        );
        String reqBody = mapper.writeValueAsString(req);
        HttpRequest request = HttpRequest.newBuilder()
                .uri(URI.create(baseUrl + "/workers/register"))
                .header("Authorization", "Bearer " + apiKey)
                .header("Content-Type", "application/json")
                .POST(HttpRequest.BodyPublishers.ofString(reqBody))
                .build();

        HttpResponse<String> response = client.send(request, HttpResponse.BodyHandlers.ofString());
        if (response.statusCode() == 201) {
            Map<String, Object> envelope = mapper.readValue(response.body(), new TypeReference<>() {});
            @SuppressWarnings("unchecked")
            Map<String, Object> data = (Map<String, Object>) envelope.get("data");
            if (data != null) {
                if (data.containsKey("id")) {
                    this.workerId = String.valueOf(data.get("id"));
                }
                if (data.containsKey("token")) {
                    this.apiKey = String.valueOf(data.get("token"));
                }
                return data;
            }
        }
        throw new RuntimeException("Worker registration failed with HTTP " + response.statusCode() + ": " + response.body());
    }

    public void registerJob(String name, JobHandler handler) {
        handlers.put(name, handler);
    }

    public void start(String queue, long pollIntervalMs) {
        running.set(true);
        System.out.println("ForgeWorker started. Listening on queue '" + queue + "'");

        // Worker heartbeat loop
        workerHbScheduler = Executors.newSingleThreadScheduledExecutor();
        workerHbScheduler.scheduleAtFixedRate(this::workerHeartbeat, 0, 30, TimeUnit.SECONDS);

        while (running.get()) {
            try {
                String reqBody = mapper.writeValueAsString(Map.of("worker_id", workerId));
                HttpRequest request = HttpRequest.newBuilder()
                        .uri(URI.create(baseUrl + "/queues/" + queue + "/dequeue"))
                        .header("Authorization", "Bearer " + apiKey)
                        .header("Content-Type", "application/json")
                        .POST(HttpRequest.BodyPublishers.ofString(reqBody))
                        .build();

                HttpResponse<String> response = client.send(request, HttpResponse.BodyHandlers.ofString());

                if (response.statusCode() == 200 && response.body() != null && !response.body().isBlank()) {
                    Map<String, Object> envelope = mapper.readValue(response.body(), new TypeReference<>() {});
                    Object payload = envelope.get("data");
                    if (payload instanceof Map<?, ?> data) {
                        @SuppressWarnings("unchecked")
                        Map<String, Object> execution = (Map<String, Object>) data;
                        executor.submit(() -> executeJob(execution));
                    } else {
                        Thread.sleep(pollIntervalMs);
                    }
                } else {
                    Thread.sleep(pollIntervalMs);
                }
            } catch (Exception e) {
                try {
                    Thread.sleep(pollIntervalMs);
                } catch (InterruptedException ignored) {}
            }
        }
    }

    public void stop() {
        running.set(false);
        if (workerHbScheduler != null) {
            workerHbScheduler.shutdownNow();
        }
        executor.shutdown();
    }

    private void workerHeartbeat() {
        try {
            HttpRequest request = HttpRequest.newBuilder()
                    .uri(URI.create(baseUrl + "/workers/" + workerId + "/heartbeat"))
                    .header("Authorization", "Bearer " + apiKey)
                    .POST(HttpRequest.BodyPublishers.noBody())
                    .build();
            client.sendAsync(request, HttpResponse.BodyHandlers.discarding());
        } catch (Exception ignored) {}
    }

    private void executeJob(Map<String, Object> execution) {
        String executionId = (String) execution.get("id");
        String jobName = (String) execution.getOrDefault("job_name", execution.get("type"));
        
        @SuppressWarnings("unchecked")
        Map<String, Object> payload = (Map<String, Object>) execution.getOrDefault("input", new HashMap<>());

        JobHandler handler = handlers.get(jobName);
        if (handler == null) {
            failJob(executionId, "No handler registered for job: " + jobName, null);
            return;
        }

        JobContext ctx = new JobContext(executionId, payload, baseUrl, apiKey, client);
        
        ScheduledExecutorService scheduler = Executors.newSingleThreadScheduledExecutor();
        scheduler.scheduleAtFixedRate(() -> heartbeat(executionId), 0, 15, TimeUnit.SECONDS);

        try {
            ctx.log("Starting execution of " + jobName);
            Object result = handler.handle(ctx);
            completeJob(executionId, result);
            ctx.log("Successfully completed " + jobName);
        } catch (Exception e) {
            ctx.log("Execution failed: " + e.getMessage());
            failJob(executionId, e.getMessage(), null);
        } finally {
            scheduler.shutdownNow();
        }
    }

    private void heartbeat(String executionId) {
        try {
            String body = mapper.writeValueAsString(Map.of("worker_id", workerId));
            HttpRequest request = HttpRequest.newBuilder()
                    .uri(URI.create(baseUrl + "/executions/" + executionId + "/heartbeat"))
                    .header("Authorization", "Bearer " + apiKey)
                    .header("Content-Type", "application/json")
                    .POST(HttpRequest.BodyPublishers.ofString(body))
                    .build();
            client.sendAsync(request, HttpResponse.BodyHandlers.discarding());
        } catch (Exception ignored) {}
    }

    private void completeJob(String executionId, Object output) {
        try {
            Map<String, Object> req = new HashMap<>();
            req.put("worker_id", workerId);
            req.put("output", output);
            String body = mapper.writeValueAsString(req);
            HttpRequest request = HttpRequest.newBuilder()
                    .uri(URI.create(baseUrl + "/executions/" + executionId + "/complete"))
                    .header("Authorization", "Bearer " + apiKey)
                    .header("Content-Type", "application/json")
                    .POST(HttpRequest.BodyPublishers.ofString(body))
                    .build();
            client.send(request, HttpResponse.BodyHandlers.discarding());
        } catch (Exception ignored) {}
    }

    private void failJob(String executionId, String error, String trace) {
        try {
            Map<String, Object> bodyMap = new HashMap<>();
            bodyMap.put("worker_id", workerId);
            bodyMap.put("error", error);
            if (trace != null) bodyMap.put("trace", trace);
            
            String body = mapper.writeValueAsString(bodyMap);
            HttpRequest request = HttpRequest.newBuilder()
                    .uri(URI.create(baseUrl + "/executions/" + executionId + "/fail"))
                    .header("Authorization", "Bearer " + apiKey)
                    .header("Content-Type", "application/json")
                    .POST(HttpRequest.BodyPublishers.ofString(body))
                    .build();
            client.send(request, HttpResponse.BodyHandlers.discarding());
        } catch (Exception ignored) {}
    }
}
