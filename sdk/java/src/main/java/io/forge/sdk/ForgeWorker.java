package io.forge.sdk;

import com.fasterxml.jackson.core.type.TypeReference;
import com.fasterxml.jackson.databind.ObjectMapper;

import java.net.URI;
import java.net.http.HttpClient;
import java.net.http.HttpRequest;
import java.net.http.HttpResponse;
import java.time.Duration;
import java.util.HashMap;
import java.util.Map;
import java.util.concurrent.*;
import java.util.concurrent.atomic.AtomicBoolean;

public class ForgeWorker {
    private final String baseUrl;
    private final String tenantId;
    private final String apiKey;
    private final HttpClient client;
    private final Map<String, JobHandler> handlers = new ConcurrentHashMap<>();
    private final ObjectMapper mapper = new ObjectMapper();
    private final AtomicBoolean running = new AtomicBoolean(false);
    private final ExecutorService executor = Executors.newCachedThreadPool();

    public ForgeWorker(String baseUrl, String tenantId, String apiKey) {
        this.baseUrl = baseUrl;
        this.tenantId = tenantId;
        this.apiKey = apiKey;
        this.client = HttpClient.newBuilder()
                .connectTimeout(Duration.ofSeconds(10))
                .build();
    }

    public void registerJob(String name, JobHandler handler) {
        handlers.put(name, handler);
    }

    public void start(String queue, long pollIntervalMs) {
        running.set(true);
        System.out.println("ForgeWorker started. Listening on queue '" + queue + "'");

        while (running.get()) {
            try {
                String reqBody = mapper.writeValueAsString(Map.of("worker_id", "java-worker-1"));
                HttpRequest request = HttpRequest.newBuilder()
                        .uri(URI.create(baseUrl + "/queues/" + queue + "/dequeue"))
                        .header("Authorization", "Bearer " + apiKey)
                        .header("Content-Type", "application/json")
                        .POST(HttpRequest.BodyPublishers.ofString(reqBody))
                        .build();

                HttpResponse<String> response = client.send(request, HttpResponse.BodyHandlers.ofString());

                if (response.statusCode() == 200 && response.body() != null && !response.body().isBlank()) {
                    Map<String, Object> execution = mapper.readValue(response.body(), new TypeReference<>() {});
                    executor.submit(() -> executeJob(execution));
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
        executor.shutdown();
    }

    private void executeJob(Map<String, Object> execution) {
        String executionId = (String) execution.get("id");
        String jobName = (String) execution.getOrDefault("job_name", execution.get("type"));
        
        @SuppressWarnings("unchecked")
        Map<String, Object> payload = (Map<String, Object>) execution.getOrDefault("payload", new HashMap<>());

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
            HttpRequest request = HttpRequest.newBuilder()
                    .uri(URI.create(baseUrl + "/executions/" + executionId + "/heartbeat"))
                    .header("Authorization", "Bearer " + apiKey)
                    .method("PATCH", HttpRequest.BodyPublishers.noBody())
                    .build();
            client.sendAsync(request, HttpResponse.BodyHandlers.discarding());
        } catch (Exception ignored) {}
    }

    private void completeJob(String executionId, Object output) {
        try {
            String body = mapper.writeValueAsString(Map.of("output", output));
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
            Map<String, String> bodyMap = new HashMap<>();
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
