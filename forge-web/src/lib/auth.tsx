"use client";

/**
 * Session state.
 *
 * The access token lives in memory only; the refresh token is kept in
 * `localStorage` so a page reload does not sign the user out. The server issues
 * rotating refresh tokens, so the stored value is replaced on every refresh and
 * a replay is refused by the server.
 */

import { createContext, useCallback, useContext, useEffect, useMemo, useState } from "react";

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

export function AuthProvider({ children }: { children: React.ReactNode }) {
  const [session, setSession] = useState<Session | null>(null);
  const [loading, setLoading] = useState(true);

  const applySession = useCallback((next: Session | null) => {
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
      const data = await request<TokensResponse>("/auth/refresh", {
        method: "POST",
        body: { refresh_token: current.refreshToken },
      });
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

  // On mount, try to trade the stored refresh token for a fresh access token.
  useEffect(() => {
    let cancelled = false;

    const stored = readStoredSession();
    if (!stored) {
      setLoading(false);
      return;
    }

    (async () => {
      try {
        const data = await request<TokensResponse>("/auth/refresh", {
          method: "POST",
          body: { refresh_token: stored.refreshToken },
        });
        if (cancelled) return;
        applySession({
          accessToken: data.access_token,
          refreshToken: data.refresh_token,
          tenantId: data.tenant_id,
          role: data.role,
        });
      } catch {
        if (!cancelled) applySession(null);
      } finally {
        if (!cancelled) setLoading(false);
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [applySession]);

  // Keep the API client pointed at the current session.
  useEffect(() => {
    configureApi({
      getToken: () =>
        session
          ? ({ accessToken: session.accessToken, refreshToken: session.refreshToken } as AccessToken)
          : null,
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
