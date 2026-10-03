"use client";

import { ResourceShell } from "@/components/ui/resource-shell";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { Badge } from "@/components/ui/badge";

export default function EnvironmentComparePage() {
  const comparison = [
    { property: "Schedule", dev: "0 2 * * *", qa: "0 2 * * *", prod: "0 3 * * *", diff: true },
    { property: "Retry Count", dev: "3", qa: "3", prod: "5", diff: true },
    { property: "Timeout", dev: "30m", qa: "30m", prod: "60m", diff: true },
    { property: "Workers", dev: "dev-pool", qa: "qa-pool", prod: "prod-pool", diff: true },
    { property: "Timezone", dev: "UTC", qa: "UTC", prod: "UTC", diff: false },
    { property: "Concurrency", dev: "1", qa: "1", prod: "1", diff: false },
  ];

  return (
    <ResourceShell
      title="Environment Comparison"
      subtitle="Job: nightly-settlement"
      backUrl="/jobs"
    >
      <Card className="border-border/50 shadow-sm overflow-hidden">
        <CardHeader className="bg-muted/30 border-b border-border/50">
          <CardTitle className="text-lg">Configuration Differences</CardTitle>
        </CardHeader>
        <CardContent className="p-0">
          <Table>
            <TableHeader>
              <TableRow className="bg-muted/50 hover:bg-muted/50">
                <TableHead className="w-[200px] font-semibold text-foreground">Configuration Property</TableHead>
                <TableHead className="font-semibold text-indigo-600 dark:text-indigo-400">DEV</TableHead>
                <TableHead className="font-semibold text-amber-600 dark:text-amber-400">QA</TableHead>
                <TableHead className="font-semibold text-rose-600 dark:text-rose-400">PROD</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {comparison.map((row, i) => (
                <TableRow key={i} className={row.diff ? "bg-muted/20" : ""}>
                  <TableCell className="font-medium text-muted-foreground">{row.property}</TableCell>
                  <TableCell className="font-mono text-sm">{row.dev}</TableCell>
                  <TableCell className="font-mono text-sm">{row.qa}</TableCell>
                  <TableCell>
                    {row.diff ? (
                      <Badge className="bg-rose-100 text-rose-700 hover:bg-rose-100 shadow-none border-none font-mono text-sm">
                        {row.prod}
                      </Badge>
                    ) : (
                      <span className="font-mono text-sm">{row.prod}</span>
                    )}
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </CardContent>
      </Card>
    </ResourceShell>
  );
}
