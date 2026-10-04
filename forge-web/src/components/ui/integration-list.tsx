"use client";

/**
 * Integrations (UI.md section 74).
 *
 * The spec names ten integrations and, per integration, a status, last success,
 * last failure, credentials, and a test-connection action. This lists what is
 * configured and what is not, and offers the test action where one exists.
 */

import { Plug, RefreshCw } from "lucide-react";

import { useQuery } from "@/lib/useQuery";
import { api } from "@/lib/api";
import { formatRelative } from "@/lib/types";
import { useToast } from "@/lib/useToast";
import { AsyncBoundary } from "@/components/states";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";

interface Integration {
  id: string;
  kind: string;
  name: string;
  enabled: boolean;
  last_success_at: string | null;
  last_failure_at: string | null;
  last_error: string | null;
}

interface ListResponse {
  data: Integration[];
  supported: string[];
  not_configured: string[];
}

export function IntegrationList() {
  const integrations = useQuery<ListResponse>("/integrations");
  const toast = useToast();

  async function test(integration: Integration) {
    try {
      const result = await api.post<{ detail: string }>(
        `/integrations/${integration.id}/test`,
        {},
      );
      toast.info("Connection checked", result.detail);
      integrations.reload();
    } catch (error) {
      toast.error(
        "Check failed",
        error instanceof Error ? error.message : undefined,
      );
    }
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center gap-2 text-sm">
          <Plug className="size-3.5" aria-hidden />
          Integrations
        </CardTitle>
      </CardHeader>
      <CardContent>
        <AsyncBoundary
          state={integrations.state}
          error={integrations.error}
          forbidden={integrations.forbidden}
          empty={false}
          onRetry={integrations.reload}
          loadingLabel="Loading integrations"
        >
          {integrations.data ? (
            <div className="flex flex-col gap-3">
              <ul className="flex flex-col divide-y divide-border/50">
                {integrations.data.data.map((integration) => (
                  <li key={integration.id} className="flex items-center gap-2 py-2 text-xs">
                    <span className="font-medium">{integration.name}</span>
                    <Badge variant="secondary" className="text-[10px]">
                      {integration.kind.toLowerCase()}
                    </Badge>
                    <span className="text-muted-foreground">
                      {integration.last_success_at
                        ? `last ok ${formatRelative(integration.last_success_at)}`
                        : integration.last_failure_at
                          ? `last failed ${formatRelative(integration.last_failure_at)}`
                          : "never checked"}
                    </span>
                    <Button
                      variant="ghost"
                      size="sm"
                      onClick={() => test(integration)}
                      aria-label={`Test ${integration.name}`}
                      className="ml-auto"
                    >
                      <RefreshCw className="size-3" aria-hidden />
                      Test connection
                    </Button>
                  </li>
                ))}
              </ul>

              {integrations.data.not_configured.length > 0 ? (
                <div>
                  <p className="mb-1 text-[11px] uppercase tracking-wide text-muted-foreground">
                    Not configured
                  </p>
                  <div className="flex flex-wrap gap-1">
                    {integrations.data.not_configured.map((kind) => (
                      <Badge key={kind} variant="outline" className="text-[10px]">
                        {kind.toLowerCase().replace(/_/g, " ")}
                      </Badge>
                    ))}
                  </div>
                </div>
              ) : null}
            </div>
          ) : null}
        </AsyncBoundary>
      </CardContent>
    </Card>
  );
}
