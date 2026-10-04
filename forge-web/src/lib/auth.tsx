"use client";

/**
 * Session state.
 *
 * The access token lives in memory only; the refresh token is kept in
 * `localStorage` so a page reload does not sign the user out. The server issues
 * rotating refresh tokens, so the stored value is replaced on every refresh and
 * a replay is refused by the server.
 */

import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";

import { api, configureApi, request, type AccessToken } from "@/lib/api";
import type { Role } from "@/lib/types";

const STORAGE_KEY = "forge.session";

interface StoredSession {
  accessToken: string;
  refreshToken: string;
  tenantId?: string;
  role?: Role;
}

export interface Session {
  accessToken: string;
  refreshToken: string;
  tenantId?: string;
  role?: Role;
}

interface AuthContextValue {
  session: Session | null;
  loading: boolean;
  signIn: (email: string, password: string) => Promise<void>;
  signOut: () => void;
  refresh: () => Promise<void>;
}

const AuthContext = createContext<AuthContextValue | null>(null);

function readStoredSession(): Session | null {
  if (typeof window === "undefined") return null;
  try {
    const raw = window.localStorage.getItem(STORAGE_KEY);
    if (!raw) return null;
    const parsed = JSON.parse(raw) as StoredSession;
    if (!parsed?.accessToken) return null;
    return parsed;
  } catch {
    // Corrupt storage must not wedge the app at a sign-in screen forever.
    return null;
  }
}

function writeStoredSession(session: Session | null): void {
  if (typeof window === "undefined") return;
  if (!session) {
    window.localStorage.removeItem(STORAGE_KEY);
    return;
  }
  window.localStorage.setItem(STORAGE_KEY, JSON.stringify(session));
}

interface TokensResponse {
  access_token: string;
  refresh_token: string;
  tenant_id?: string;
  role?: Role;
  expires_in_secs?: number;
}

/**
 * Exchanges a refresh token for a new pair, collapsing concurrent callers.
 *
 * Forge refresh tokens are single-use: spending one revokes it and issues a new
 * one, so presenting the same token twice fails with 401. A mount effect can run
 * more than once, and the console issues several authenticated requests the
 * moment it boots, so the same token can reach this function twice.
 *
 * The in-flight exchange is keyed by the token it is spending. That matters:
 * clearing the entry as soon as the exchange settles would let a caller that
 * arrives moments later start a *second* exchange with a token that has already
 * been rotated away. Keying it means a repeat caller with the same token reuses
 * the result, while a genuine sign-in with a new token still gets its own
 * exchange.
 *
 * The exchange bypasses `request()`, which would call `onUnauthorized` on a 401
 * and sign the operator out while they are still establishing a session.
 */
let exchangeInFlight: { token: string; promise: Promise<TokensResponse> } | null = null;

function exchangeRefreshToken(token: string): Promise<TokensResponse> {
  if (exchangeInFlight?.token === token) return exchangeInFlight.promise;

  const promise = request<TokensResponse>("/auth/refresh", {
    method: "POST",
    body: { refresh_token: token },
  }).finally(() => {
    // Only released once it settles, so a repeat caller still joins it. A
    // later sign-in carries a different token and starts a fresh exchange.
    if (exchangeInFlight?.promise === promise) exchangeInFlight = null;
  });

  exchangeInFlight = { token, promise };
  return promise;
}

export function AuthProvider({ children }: { children: React.ReactNode }) {
  const [session, setSession] = useState<Session | null>(null);
  const [loading, setLoading] = useState(true);

  // Mirrors `session` for the API client's token reader, which is called from
  // outside React's render cycle and so cannot close over the state value.
  const sessionRef = useRef<Session | null>(null);

  const applySession = useCallback((next: Session | null) => {
    sessionRef.current = next;
    setSession(next);
    writeStoredSession(next);
  }, []);

  const signOut = useCallback(() => {
    applySession(null);
  }, [applySession]);

  const refresh = useCallback(async () => {
    const current = readStoredSession();
    if (!current?.refreshToken) {
      applySession(null);
      return;
    }
    try {
      // Same single-use hazard as the mount path, so it goes through the same
      // collapsing exchange rather than issuing its own request.
      const data = await exchangeRefreshToken(current.refreshToken);
      applySession({
        accessToken: data.access_token,
        refreshToken: data.refresh_token,
        tenantId: data.tenant_id,
        role: data.role ?? current.role,
      });
    } catch {
      // The refresh token was refused or reused, so the session is over.
      applySession(null);
    }
  }, [applySession]);

  // On mount, trade the stored refresh token for a fresh access token.
  //
  // The controller matters more than it looks. A Forge refresh token is
  // single-use: exchanging it revokes it and issues a new one, so a second
  // exchange of the same token is refused with a 401. React runs mount effects
  // more than once (StrictMode in development, and remounts on navigation), and
  // without an abort the superseded request stays in flight, spends the token,
  // and its 401 then trips `onUnauthorized` and signs the user out — which
  // looks exactly like a session that never worked.
  //
  // `exchangeRefreshToken` is module-level and shared, so a re-run joins the
  // in-flight exchange instead of starting a second one with the same token.
  useEffect(() => {
    let cancelled = false;

    const stored = readStoredSession();
    if (!stored) {
      setLoading(false);
      return;
    }

    exchangeRefreshToken(stored.refreshToken)
      .then((data) => {
        if (cancelled) return;
        applySession({
          accessToken: data.access_token,
          refreshToken: data.refresh_token,
          tenantId: data.tenant_id,
          role: data.role,
        });
      })
      .catch(() => {
        if (!cancelled) applySession(null);
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });

    return () => {
      cancelled = true;
    };
  }, [applySession]);

  // Keep the API client pointed at the current session.
  //
  // The token is published imperatively rather than through an effect that
  // depends on `session`. The console fires its first authenticated requests
  // from a child effect that runs *before* this one, so wiring the client in an
  // effect left those early requests going out with no Authorization header.
  // They came back 401, `onUnauthorized` fired, and the operator was bounced to
  // sign-in despite holding a perfectly good session.
  //
  // Reading through a ref keeps `getToken` correct for the current session
  // without re-running anything that might issue a request.
  useEffect(() => {
    sessionRef.current = session;
    configureApi({
      getToken: () => {
        const current = sessionRef.current;
        return current
          ? ({
              accessToken: current.accessToken,
              refreshToken: current.refreshToken,
            } as AccessToken)
          : null;
      },
      // A 401 anywhere clears the session so the shell can route to sign-in.
      onUnauthorized: () => signOut(),
    });
  }, [session, signOut]);

  const signIn = useCallback(
    async (email: string, password: string) => {
      const data = await api.post<TokensResponse>("/auth/login", { email, password });
      applySession({
        accessToken: data.access_token,
        refreshToken: data.refresh_token,
        tenantId: data.tenant_id,
        role: data.role,
      });
    },
    [applySession],
  );

  const value = useMemo(
    () => ({ session, loading, signIn, signOut, refresh }),
    [session, loading, signIn, signOut, refresh],
  );

  return <AuthContext.Provider value={value}>{children}</AuthContext.Provider>;
}

/** Reads the session. Must be used inside an `AuthProvider`. */
export function useAuth(): AuthContextValue {
  const context = useContext(AuthContext);
  if (!context) {
    throw new Error("useAuth must be used inside an AuthProvider");
  }
  return context;
}
