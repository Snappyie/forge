// Drives one real execution through the Java SDK against a live server.
import com.fasterxml.jackson.core.type.TypeReference;
import com.fasterxml.jackson.databind.ObjectMapper;
import io.forge.sdk.ForgeWorker;
import java.net.URI;
import java.net.http.*;
import java.time.Duration;
import java.util.*;

public class E2E {
    static final String API = System.getenv("FORGE_TEST_API");
    static final ObjectMapper M = new ObjectMapper();

    static Map<String, Object> post(String path, String token, Map<String, Object> body) throws Exception {
        HttpRequest req = HttpRequest.newBuilder()
                .uri(URI.create(API + path))
                .header("content-type", "application/json")
                .header("Authorization", "Bearer " + token)
                .timeout(Duration.ofSeconds(15))
                .POST(HttpRequest.BodyPublishers.ofString(M.writeValueAsString(body)))
                .build();
        HttpResponse<String> res = HttpClient.newHttpClient().send(req, HttpResponse.BodyHandlers.ofString());
        if (res.statusCode() >= 400) throw new RuntimeException(path + " -> " + res.statusCode() + " " + res.body());
        Map<String, Object> env = M.readValue(res.body(), new TypeReference<>() {});
        Object d = env.get("data");
        return d instanceof Map ? cast(d) : Map.of();
    }

    static Map<String, Object> get(String path, String token) throws Exception {
        HttpRequest req = HttpRequest.newBuilder().uri(URI.create(API + path))
                .header("Authorization", "Bearer " + token)
                .timeout(Duration.ofSeconds(15)).GET().build();
        HttpResponse<String> res = HttpClient.newHttpClient().send(req, HttpResponse.BodyHandlers.ofString());
        Map<String, Object> env = M.readValue(res.body(), new TypeReference<>() {});
        Object d = env.get("data");
        return d instanceof Map ? cast(d) : Map.of();
    }

    @SuppressWarnings("unchecked")
    static Map<String, Object> cast(Object o) { return (Map<String, Object>) o; }

    public static void main(String[] args) throws Exception {
        // Registration is closed for this run, so the script's own account
        // claimed the one-time bootstrap slot and became OWNER. Reuse its token.
        String token = System.getenv("FORGE_TEST_TOKEN");
        String tenant = System.getenv("FORGE_TEST_TENANT");

        // The queue is created once by the verification script and passed in:
        // only the first account to claim the one-time bootstrap slot becomes
        // OWNER, so each probe cannot create its own.
        String qid = System.getenv("FORGE_TEST_QUEUE_ID");
        String qname = System.getenv("FORGE_TEST_QUEUE_NAME");

        ForgeWorker w = new ForgeWorker(API, tenant, token).withWorkerId("java-e2e-1");
        w.registerWorker("java-e2e", "localhost", List.of("*"));
        System.out.printf("  registered: worker_id=%.8s... token replaced=%b%n",
                w.getWorkerId().substring(0, 8), !w.getToken().equals(token));

        w.register("Settle", ctx -> {
            ctx.log("settling payments from Java");
            return Map.of("settled", 5);
        });

        Map<String, Object> job = post("/jobs", token,
                Map.of("name", "Settle", "key", "java-settle-" + System.currentTimeMillis(), "default_queue_id", qid));
        String jid = (String) job.get("id");
        Map<String, Object> ver = post("/jobs/" + jid + "/versions", token,
                Map.of("execution_type", "WORKER_TASK"));
        post("/jobs/" + jid + "/versions/" + ver.get("id") + "/publish", token, Map.of());
        post("/jobs/" + jid + "/trigger", token, Map.of());

        Map<String, Object> claimed = w.pollOnce(qname);
        if (claimed == null) throw new RuntimeException("dequeue returned nothing");
        System.out.printf("  claimed execution=%.8s... lease_id=%.8s...%n",
                ((String) claimed.get("id")).substring(0, 8),
                String.valueOf(claimed.get("lease_id")).substring(0, 8));

        w.execute(claimed);

        Map<String, Object> final_ = get("/executions/" + claimed.get("id"), token);
        System.out.println("  final execution status: " + final_.get("status"));
        if (!"SUCCEEDED".equals(final_.get("status"))) {
            System.out.println("  UNEXPECTED: " + final_);
            System.exit(1);
        }
        System.out.println("  RESULT: the Java SDK executed a real job and the server recorded success");
    }
}
