/**
 * Typed API client for the Forge server.
 *
 * Spec 05 §5.1 defines one response envelope, so every call unwraps `data`
 * here rather than in each page. Errors arrive as a typed `ApiError` carrying
 * the server's error code, which is what spec 02.14 guarantees a client can
 * branch on.
 */

export const API_URL =
  process.env.NEXT_PUBLIC_API_URL ?? "http://localhost:3000/api/v1";

/** The spec 02.14 error taxonomy, as codes the server returns. */
export type ApiErrorCode =
  | "VALIDATION_ERROR"
  | "AUTHENTICATION_REQUIRED"
  | "AUTHORIZATION_DENIED"
  | "NOT_FOUND"
  | "CONFLICT"
  | "IDEMPOTENCY_KEY_CONFLICT"
  | "RATE_LIMITED"
  | "DEPENDENCY_UNAVAILABLE"
  | "INTERNAL_ERROR";

export class ApiError extends Error {
  readonly status: number;
  readonly code: string;
  readonly details: { field: string; message: string }[];
  readonly requestId?: string;

  constructor(
    status: number,
    code: string,
    message: string,
    details: { field: string; message: string }[] = [],
    requestId?: string,
  ) {
    super(message);
    this.name = "ApiError";
    this.status = status;
    this.code = code;
    this.details = details;
    this.requestId = requestId;
  }

  /** Whether the caller needs to sign in again. */
  get isAuthFailure(): boolean {
    return this.status === 401;
  }

  /** Whether the caller is authenticated but lacks permission. */
  get isForbidden(): boolean {
    return this.status === 403;
  }

  get isNotFound(): boolean {
    return this.status === 404;
  }

  get isConflict(): boolean {
    return this.status === 409;
  }

  /** A message suitable for showing beside the offending field. */
  fieldError(field: string): string | undefined {
    return this.details.find((d) => d.field === field)?.message;
  }
}

/** A page of results, from the list envelope's `page` object. */
export interface PageInfo {
  next_cursor: string | null;
  has_more: boolean;
}

/** How a token pair is stored between requests. */
type TokenReader = () => AccessToken | null;
type UnauthorizedHandler = () => void | Promise<void>;

let readToken: TokenReader = () => null;
let onUnauthorized: UnauthorizedHandler = () => {};

/**
 * Wires the client to the auth store. Done once at start-up so no page has to
 * pass a token explicitly.
 */
export function configureApi(options: {
  getToken?: TokenReader;
  onUnauthorized?: UnauthorizedHandler;
}): void {
  if (options.getToken) readToken = options.getToken;
  if (options.onUnauthorized) onUnauthorized = options.onUnauthorized;
}

export interface AccessToken {
  accessToken: string;
  refreshToken: string;
  tenantId?: string;
  expiresIn?: number;
}

interface RequestOptions {
  method?: "GET" | "POST" | "PUT" | "PATCH" | "DELETE";
  body?: unknown;
  /** Makes the request idempotent when retried (spec 02.15). */
  idempotencyKey?: string;
  signal?: AbortSignal;
}

/** Performs one request and unwraps the response envelope. */
export async function request<T = unknown>(
  path: string,
  options: RequestOptions = {},
): Promise<T> {
  const { method = "GET", body, idempotencyKey, signal } = options;

  const headers: Record<string, string> = {
    "content-type": "application/json",
  };

  const token = readToken();
  if (token?.accessToken) {
    headers.authorization = `Bearer ${token.accessToken}`;
  }
  if (idempotencyKey) {
    headers["idempotency-key"] = idempotencyKey;
  }

  let response: Response;
  try {
    response = await fetch(`${API_URL}${path}`, {
      method,
      headers,
      body: body === undefined ? undefined : JSON.stringify(body),
      signal,
      cache: "no-store",
    });
  } catch (cause) {
    // A network failure is not a server error; the UI shows it as degraded
    // rather than as a rejected request.
    throw new ApiError(
      0,
      "NETWORK_UNREACHABLE",
      cause instanceof Error ? cause.message : "the server is unreachable",
    );
  }

  const text = await response.text();
  let parsed: unknown = null;
  if (text) {
    try {
      parsed = JSON.parse(text);
    } catch {
      // A proxy may return HTML; treat it as an opaque failure.
      parsed = null;
    }
  }

  if (!response.ok) {
    const envelope = (parsed ?? {}) as {
      error?: {
        code?: string;
        message?: string;
        details?: { field: string; message: string }[];
        request_id?: string;
      };
    };
    const error = new ApiError(
      response.status,
      envelope.error?.code ?? "INTERNAL_ERROR",
      envelope.error?.message ?? `request failed with ${response.status}`,
      envelope.error?.details ?? [],
      envelope.error?.request_id,
    );

    // A 401 means the stored token is no longer usable; the auth store clears
    // it so the UI falls back to the sign-in screen rather than looping on
    // failed requests.
    if (error.isAuthFailure) {
      await onUnauthorized();
    }
    throw error;
  }

  // Success responses wrap their payload in `data` (spec 05 §5.1). Unwrapping
  // it here means callers receive the resource itself; a list endpoint yields
  // the array of rows directly, not an object with a `data` property.
  const envelope = (parsed ?? {}) as { data?: T };
  return (envelope.data ?? (parsed as T)) as T;
}

export const api = {
  get: <T,>(path: string, signal?: AbortSignal) =>
    request<T>(path, { method: "GET", signal }),
  post: <T,>(path: string, body?: unknown, idempotencyKey?: string) =>
    request<T>(path, { method: "POST", body, idempotencyKey }),
  patch: <T,>(path: string, body?: unknown) =>
    request<T>(path, { method: "PATCH", body }),
  put: <T,>(path: string, body?: unknown) =>
    request<T>(path, { method: "PUT", body }),
  delete: <T,>(path: string) => request<T>(path, { method: "DELETE" }),
};

/**
 * A list payload with its cursor metadata.
 *
 * The server sends `{ data: [...], page: {...}, request_id }` inside the
 * envelope, so after unwrapping the caller receives the rows and the page
 * together rather than having to re-read the envelope.
 */
export interface Paged<T> {
  rows: T[];
  page: PageInfo;
}

/** Fetches a list, preserving the page metadata alongside the rows. */
export async function getList<T>(
  path: string,
  signal?: AbortSignal,
): Promise<Paged<T>> {
  const response = await fetch(`${API_URL}${path}`, {
    headers: buildHeaders(),
    signal,
    cache: "no-store",
  });

  const text = await response.text();
  const parsed = text ? (JSON.parse(text) as unknown) : null;

  if (!response.ok) {
    throw toApiError(response.status, parsed);
  }

  const envelope = (parsed ?? {}) as {
    data?: T[] | Paged<T>;
    page?: PageInfo;
  };
  const payload = envelope.data;

  if (Array.isArray(payload)) {
    return { rows: payload, page: envelope.page ?? emptyPage() };
  }
  // The API may return the paged shape directly.
  if (payload && Array.isArray(payload.rows)) {
    return payload;
  }
  return { rows: [], page: envelope.page ?? emptyPage() };
}

function emptyPage(): PageInfo {
  return { next_cursor: null, has_more: false };
}

function buildHeaders(): Record<string, string> {
  const headers: Record<string, string> = { "content-type": "application/json" };
  const token = readToken();
  if (token?.accessToken) {
    headers.authorization = `Bearer ${token.accessToken}`;
  }
  return headers;
}

function toApiError(status: number, parsed: unknown): ApiError {
  const envelope = (parsed ?? {}) as {
    error?: {
      code?: string;
      message?: string;
      details?: { field: string; message: string }[];
      request_id?: string;
    };
  };
  const error = new ApiError(
    status,
    envelope.error?.code ?? "INTERNAL_ERROR",
    envelope.error?.message ?? `request failed with ${status}`,
    envelope.error?.details ?? [],
    envelope.error?.request_id,
  );
  if (error.isAuthFailure) {
    void onUnauthorized();
  }
  return error;
}
