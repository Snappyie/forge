package io.forge.sdk;

/**
 * An error returned by the Forge API.
 *
 * <p>Carries the server's structured envelope rather than a bare status code, so
 * a caller can branch on {@link #getCode()} and an operator can quote
 * {@link #getRequestId()} in a bug report.
 */
public class ForgeException extends RuntimeException {
    private final String code;
    private final int status;
    private final String requestId;

    public ForgeException(String message, String code, int status, String requestId) {
        super(requestId == null || requestId.isEmpty()
                ? message + " (code=" + code + ")"
                : message + " (code=" + code + ", request_id=" + requestId + ")");
        this.code = code;
        this.status = status;
        this.requestId = requestId;
    }

    public String getCode() {
        return code;
    }

    public int getStatus() {
        return status;
    }

    public String getRequestId() {
        return requestId;
    }

    /**
     * Whether retrying the same request could succeed.
     *
     * <p>Used to back off on a 5xx or a dropped connection without giving up on a
     * 4xx, which would fail identically however many times it was retried.
     */
    public boolean isTemporary() {
        return status >= 500 || status == 429
                || ErrorClass.DEPENDENCY_UNAVAILABLE.equals(code)
                || "UNREACHABLE".equals(code);
    }
}
