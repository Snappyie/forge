package io.forge.sdk;

/**
 * Error classes the server accepts.
 *
 * <p>Mirrors {@code forge_domain::ErrorClass}. Anything unrecognised is recorded
 * as {@link #PERMANENT} by the server and therefore never retried, so getting
 * this right is what makes the platform's retry policy usable from Java.
 */
public final class ErrorClass {
    public static final String TRANSIENT = "TRANSIENT";
    public static final String DEPENDENCY_UNAVAILABLE = "DEPENDENCY_UNAVAILABLE";
    public static final String TIMEOUT = "TIMEOUT";
    public static final String RESOURCE_EXHAUSTED = "RESOURCE_EXHAUSTED";
    public static final String RATE_LIMITED = "RATE_LIMITED";
    public static final String VALIDATION = "VALIDATION";
    public static final String AUTHENTICATION = "AUTHENTICATION";
    public static final String AUTHORIZATION = "AUTHORIZATION";
    public static final String NOT_FOUND = "NOT_FOUND";
    public static final String CONFLICT = "CONFLICT";
    public static final String CANCELLATION = "CANCELLATION";
    public static final String PERMANENT = "PERMANENT";
    public static final String INTERNAL = "INTERNAL";

    private ErrorClass() {
    }

    /**
     * Picks a class for an exception a handler did not classify.
     *
     * <p>An unreachable dependency or a timeout is worth another attempt, while
     * bad input is not. Defaulting everything to {@link #PERMANENT} would leave
     * the retry machinery unreachable from Java, so an unrecognised failure is
     * treated as transient: a wrongly-retried failure is recoverable, while a
     * wrongly-abandoned one silently loses work.
     */
    public static String classify(Throwable cause) {
        if (cause == null) {
            return TRANSIENT;
        }
        if (cause instanceof ForgeException forge) {
            // The server already classified it; keep its answer rather than
            // second-guessing it, which is how a validation failure ends up
            // retried forever.
            if (!forge.getCode().isEmpty() && !"UNKNOWN".equals(forge.getCode())
                    && !"HTTP_ERROR".equals(forge.getCode())) {
                return forge.getCode();
            }
            switch (forge.getStatus()) {
                case 400:
                    return VALIDATION;
                case 401:
                    return AUTHENTICATION;
                case 403:
                    return AUTHORIZATION;
                case 404:
                    return NOT_FOUND;
                case 409:
                    return CONFLICT;
                case 429:
                    return RATE_LIMITED;
                default:
                    return forge.isTemporary() ? TRANSIENT : PERMANENT;
            }
        }
        if (cause instanceof java.net.http.HttpTimeoutException
                || cause instanceof java.net.SocketTimeoutException
                || cause instanceof java.util.concurrent.TimeoutException) {
            return TIMEOUT;
        }
        if (cause instanceof InterruptedException) {
            return CANCELLATION;
        }
        if (cause instanceof IllegalArgumentException
                || cause instanceof NullPointerException
                || cause instanceof ClassCastException) {
            return VALIDATION;
        }
        if (cause instanceof OutOfMemoryError) {
            return RESOURCE_EXHAUSTED;
        }
        return TRANSIENT;
    }
}
