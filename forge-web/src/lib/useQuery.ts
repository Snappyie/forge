"use client";

/**
 * Data fetching for the console.
 *
 * Spec 7.2 requires every page to define loading, empty, error, and
 * permission-denied states. This hook makes those four cases explicit so a page
 * cannot accidentally omit one.
 */

import { useCallback, useEffect, useRef, useState } from "react";

import { ApiError, getList, type Paged } from "@/lib/api";
import type { PageInfo } from "@/lib/api";

export type LoadState = "loading" | "ready" | "error";

export interface QueryResult<T> {
  data: T | null;
  state: LoadState;
  /** The failure, when `state` is `error`. */
  error: ApiError | null;
  /** Whether the caller lacks permission (spec 7.2 permission-denied). */
  forbidden: boolean;
  /** Whether the collection came back empty. */
  empty: boolean;
  reload: () => void;
}

/**
 * Reads a list endpoint, returning its rows and page metadata.
 *
 * Prefer this over `useQuery` for collections: `getList` unwraps the response
 * envelope once, so a page receives the array of rows rather than an object it
 * might unwrap a second time.
 */
export function useList<T>(
  path: string | null,
  deps: unknown[] = [],
): QueryResult<T[]> & { rows: T[]; page: PageInfo | null } {
  const [payload, setPayload] = useState<Paged<T> | null>(null);
  const [state, setState] = useState<LoadState>("loading");
  const [error, setError] = useState<ApiError | null>(null);
  const [nonce, setNonce] = useState(0);

  useEffect(() => {
    if (!path) {
      setState("loading");
      return;
    }

    const controller = new AbortController();
    setState("loading");
    setError(null);

    (async () => {
      try {
        const result = await getList<T>(path, controller.signal);
        if (controller.signal.aborted) return;
        setPayload(result);
        setState("ready");
      } catch (cause) {
        if (controller.signal.aborted) return;
        if (cause instanceof DOMException && cause.name === "AbortError") return;
        setError(
          cause instanceof ApiError
            ? cause
            : new ApiError(0, "INTERNAL_ERROR", String(cause)),
        );
        setState("error");
      }
    })();

    return () => controller.abort();
     
  }, [path, nonce, ...deps]);

  const reload = useCallback(() => setNonce((n) => n + 1), []);
  const rows = payload?.rows ?? [];

  return {
    data: rows,
    rows,
    page: payload?.page ?? null,
    state,
    error,
    forbidden: error?.isForbidden ?? false,
    empty: state === "ready" && rows.length === 0,
    reload,
  };
}

/** Reads one resource or a page of them. */
export function useQuery<T>(
  path: string | null,
  deps: unknown[] = [],
): QueryResult<T> {
  const [data, setData] = useState<T | null>(null);
  const [state, setState] = useState<LoadState>("loading");
  const [error, setError] = useState<ApiError | null>(null);
  const [nonce, setNonce] = useState(0);
  const abortRef = useRef<AbortController | null>(null);

  useEffect(() => {
    // A null path means the caller is not ready to fetch yet.
    if (!path) {
      setState("loading");
      return;
    }

    // Abandon an in-flight request when the path changes, so a slow earlier
    // response cannot overwrite a newer one.
    abortRef.current?.abort();
    const controller = new AbortController();
    abortRef.current = controller;

    setState("loading");
    setError(null);

    (async () => {
      try {
        const { api } = await import("@/lib/api");
        const result = await api.get<T>(path, controller.signal);
        if (controller.signal.aborted) return;
        setData(result);
        setState("ready");
      } catch (cause) {
        if (controller.signal.aborted) return;
        if (cause instanceof DOMException && cause.name === "AbortError") return;
        const apiError =
          cause instanceof ApiError
            ? cause
            : new ApiError(0, "INTERNAL_ERROR", String(cause));
        setError(apiError);
        setState("error");
      }
    })();

    return () => controller.abort();
     
  }, [path, nonce, ...deps]);

  const reload = useCallback(() => setNonce((n) => n + 1), []);

  return {
    data,
    state,
    error,
    forbidden: error?.isForbidden ?? false,
    empty: state === "ready" && isEmpty(data),
    reload,
  };
}

/** Whether a payload represents "nothing to show". */
function isEmpty(data: unknown): boolean {
  if (data === null || data === undefined) return true;
  if (Array.isArray(data)) return data.length === 0;
  if (typeof data === "object") {
    const record = data as Record<string, unknown>;
    // The list envelope nests its rows under `data`.
    if (Array.isArray(record.data)) return record.data.length === 0;
    if (Array.isArray(record.items)) return record.items.length === 0;
    if (Array.isArray(record.lines)) return record.lines.length === 0;
    if (Array.isArray(record.occurrences)) return record.occurrences.length === 0;
  }
  return false;
}

/**
 * Fetches successive pages with cursor pagination (spec 05 §5.14).
 *
 * The cursor is opaque, so it is passed straight back to the server without
 * interpretation.
 */
export function usePaginatedQuery<T>(
  path: string | null,
  pageSize = 50,
): QueryResult<T[]> & { page: PageInfo | null; loadMore: () => void } {
  const [cursor, setCursor] = useState<string | null>(null);
  const [items, setItems] = useState<T[]>([]);
  const [page, setPage] = useState<PageInfo | null>(null);
  const [state, setState] = useState<LoadState>("loading");
  const [error, setError] = useState<ApiError | null>(null);
  // A reload has to change something the effect depends on. Resetting `cursor`
  // to null is not enough: it is already null after a first page, so React
  // bails out of the re-render and the list keeps showing the rows it had.
  const [nonce, setNonce] = useState(0);

  useEffect(() => {
    if (!path) return;
    let cancelled = false;

    setState("loading");
    setError(null);

    (async () => {
      try {
        const { getList } = await import("@/lib/api");
        const separator = path.includes("?") ? "&" : "?";
        const query = `${path}${separator}limit=${pageSize}${cursor ? `&cursor=${encodeURIComponent(cursor)}` : ""}`;
        // `getList` reads the rows *and* the page envelope, so the cursor the
        // server hands back is preserved. Reading the array alone would lose
        // `next_cursor` and pagination could never advance.
        const result = await getList<T>(query);
        if (cancelled) return;

        const rows = Array.isArray(result.rows) ? result.rows : [];
        setItems((previous) => (cursor ? [...previous, ...rows] : rows));
        setPage(result.page ?? { next_cursor: null, has_more: false });
        setState("ready");
      } catch (cause) {
        if (cancelled) return;
        setError(
          cause instanceof ApiError ? cause : new ApiError(0, "INTERNAL_ERROR", String(cause)),
        );
        setState("error");
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [path, cursor, pageSize, nonce]);

  const loadMore = useCallback(() => {
    // Only meaningful once a cursor exists; otherwise there is nothing more to
    // walk to and re-running the same query would just refetch page one.
    if (!page?.has_more) return;
    setCursor(page.next_cursor ?? "");
  }, [page]);

  const reload = useCallback(() => {
    setItems([]);
    setPage(null);
    setCursor(null);
    // Drop any accumulated rows and force the effect to run again.
    setNonce((n) => n + 1);
  }, []);

  return {
    data: items,
    state,
    error,
    forbidden: error?.isForbidden ?? false,
    empty: state === "ready" && items.length === 0,
    reload,
    page,
    loadMore,
  };
}
