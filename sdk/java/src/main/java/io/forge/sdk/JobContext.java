package io.forge.sdk;

import com.fasterxml.jackson.databind.ObjectMapper;
import java.net.URI;
import java.net.http.HttpClient;
import java.net.http.HttpRequest;
import java.net.http.HttpResponse;
import java.util.Map;

public class JobContext {
    private final String executionId;
    private final Map<String, Object> payload;
    private final String baseUrl;
    private final String apiKey;
    private final HttpClient client;
    private final ObjectMapper mapper = new ObjectMapper();

    public JobContext(String executionId, Map<String, Object> payload, String baseUrl, String apiKey, HttpClient client) {
        this.executionId = executionId;
        this.payload = payload;
        this.baseUrl = baseUrl;
        this.apiKey = apiKey;
        this.client = client;
    }

    public String getExecutionId() {
        return executionId;
    }

    public Map<String, Object> getPayload() {
        return payload;
    }

    public void log(String message) {
        System.out.println("[" + executionId + "] " + message);
        try {
            String body = mapper.writeValueAsString(Map.of("message", message));
            HttpRequest request = HttpRequest.newBuilder()
                    .uri(URI.create(baseUrl + "/executions/" + executionId + "/logs"))
                    .header("Authorization", "Bearer " + apiKey)
                    .header("Content-Type", "application/json")
                    .POST(HttpRequest.BodyPublishers.ofString(body))
                    .build();
            client.sendAsync(request, HttpResponse.BodyHandlers.discarding());
        } catch (Exception e) {
            // Best effort
        }
    }
}
