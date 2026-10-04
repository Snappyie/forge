"use client";

/**
 * Job dependency map (UI.md section 70).
 *
 * Renders the edges this job actually has, in both directions, from
 * `job_dependencies`. A job with no edges says so rather than implying it is
 * isolated from anything.
 */

import Link from "next/link";
import { ArrowDown, ArrowUp } from "lucide-react";

import { useQuery } from "@/lib/useQuery";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";

interface Edge {
  id: string;
  name: string;
  condition: string;
}

interface Dependencies {
  job_id: string;
  upstream: Edge[];
  downstream: Edge[];
}

export function DependencyMap({ jobId }: { jobId: string }) {
  const deps = useQuery<Dependencies>(`/jobs/${jobId}/dependencies`);

  if (deps.state === "loading") {
    return (
      <Card>
        <CardContent className="py-6 text-center text-xs text-muted-foreground">
          Loading dependencies…
        </CardContent>
      </Card>
    );
  }

  const upstream = deps.data?.upstream ?? [];
  const downstream = deps.data?.downstream ?? [];

  return (
    <Card>
      <CardHeader>
        <CardTitle className="text-sm">Dependencies</CardTitle>
      </CardHeader>
      <CardContent className="flex flex-col gap-3 text-xs">
        {upstream.length === 0 && downstream.length === 0 ? (
          <p className="text-muted-foreground">
            This job has no declared dependencies. It runs on its own schedule.
          </p>
        ) : null}

        {upstream.length > 0 ? (
          <section>
            <h3 className="mb-1 flex items-center gap-1 text-[11px] uppercase tracking-wide text-muted-foreground">
              <ArrowDown className="size-3" aria-hidden />
              Waits for
            </h3>
            <ul className="flex flex-col gap-1">
              {upstream.map((edge) => (
                <li key={edge.id}>
                  <Link
                    href={`/jobs/${edge.id}`}
                    className="flex items-center gap-2 rounded px-1 py-0.5 hover:bg-accent/60"
                  >
                    <span className="font-medium">{edge.name}</span>
                    <span className="text-muted-foreground">
                      must be {edge.condition.toLowerCase()}
                    </span>
                  </Link>
                </li>
              ))}
            </ul>
          </section>
        ) : null}

        {downstream.length > 0 ? (
          <section>
            <h3 className="mb-1 flex items-center gap-1 text-[11px] uppercase tracking-wide text-muted-foreground">
              <ArrowUp className="size-3" aria-hidden />
              Blocks
            </h3>
            <ul className="flex flex-col gap-1">
              {downstream.map((edge) => (
                <li key={edge.id}>
                  <Link
                    href={`/jobs/${edge.id}`}
                    className="flex items-center gap-2 rounded px-1 py-0.5 hover:bg-accent/60"
                  >
                    <span className="font-medium">{edge.name}</span>
                    <span className="text-muted-foreground">
                      waits for this job to be {edge.condition.toLowerCase()}
                    </span>
                  </Link>
                </li>
              ))}
            </ul>
          </section>
        ) : null}
      </CardContent>
    </Card>
  );
}
