package io.forge.sdk;

import java.util.Map;

/**
 * What a handler is given for one execution.
 *
 * <p>The logger is injected rather than the transport so that a log line cannot
 * hang on its own: the worker owns the HTTP client, its timeouts and its
 * credentials, and this class only decides what to say.
 */
public class JobContext {
    /** Sends one log line for an execution. Implementations must not block. */
    @FunctionalInterface
    public interface LogSink {
        void send(String executionId, String stream, String message);
    }

    private final String executionId;
    private final String jobName;
    private final Map<String, Object> payload;
    private final String leaseId;
    private final String workerId;
    private final LogSink sink;

    public JobContext(String executionId, String jobName, Map<String, Object> payload,
                       String leaseId, String workerId, LogSink sink) {
        this.executionId = executionId;
        this.jobName = jobName;
        this.payload = payload;
        this.leaseId = leaseId;
        this.workerId = workerId;
        this.sink = sink;
    }

    public String getExecutionId() {
        return executionId;
    }

    public String getJobName() {
        return jobName;
    }

    public Map<String, Object> getPayload() {
        return payload;
    }

    /**
     * The lease that came with this execution.
     *
     * <p>Must accompany every heartbeat and completion; the server rejects a
     * completion whose lease has been reassigned.
     */
    public String getLeaseId() {
        return leaseId;
    }

    public String getWorkerId() {
        return workerId;
    }

    /**
     * Streams a log line back to Forge.
     *
     * <p>Best-effort by design: a log line is never worth failing an execution
     * over, so a transport failure is swallowed rather than raised.
     */
    public void log(String message) {
        log(message, "stdout");
    }

    public void log(String message, String stream) {
        System.out.println("[" + executionId + "] " + message);
        try {
            sink.send(executionId, stream, message);
        } catch (Exception ignored) {
            // Logging must never fail a healthy run.
        }
    }

    public void logf(String format, Object... args) {
        log(String.format(format, args));
    }
}
