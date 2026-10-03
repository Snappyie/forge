"use client";

import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { GitCompare, Clock, CheckCircle, XCircle } from "lucide-react";
import { ResourceShell } from "@/components/ui/resource-shell";

export default function CompareExecutionsPage() {
  return (
    <ResourceShell
      title="Compare Executions"
      subtitle="Comparing ex_19281 against ex_19302"
      backUrl="/jobs"
    >
      <div className="grid grid-cols-2 gap-6">
        {/* Left Side: Execution A */}
        <Card className="border-border/50">
          <CardHeader className="bg-muted/30 border-b border-border/50">
            <div className="flex justify-between items-center">
              <CardTitle className="text-lg">Execution #19281</CardTitle>
              <Badge className="bg-green-100 text-green-700 hover:bg-green-100 shadow-none border-none">
                <CheckCircle className="w-3 h-3 mr-1" /> SUCCESS
              </Badge>
            </div>
            <p className="text-sm text-muted-foreground">Oct 2, 2026 - 02:00:00</p>
          </CardHeader>
          <CardContent className="p-0">
            <div className="flex justify-between p-4 border-b border-border/50">
              <span className="text-sm text-muted-foreground">Duration</span>
              <span className="text-sm font-mono font-medium">8m 34s</span>
            </div>
            <div className="flex justify-between p-4 border-b border-border/50">
              <span className="text-sm text-muted-foreground">Worker Node</span>
              <span className="text-sm font-mono text-indigo-600 dark:text-indigo-400">worker-17 (ip-10-0-1-44)</span>
            </div>
            <div className="flex justify-between p-4 border-b border-border/50">
              <span className="text-sm text-muted-foreground">Max Memory</span>
              <span className="text-sm font-mono">1.2 GB</span>
            </div>
            <div className="flex justify-between p-4 border-b border-border/50">
              <span className="text-sm text-muted-foreground">Parameters</span>
              <pre className="text-xs bg-muted p-2 rounded text-muted-foreground">
                {"{\n  \"batch_size\": 1000,\n  \"dry_run\": false\n}"}
              </pre>
            </div>
          </CardContent>
        </Card>

        {/* Right Side: Execution B */}
        <Card className="border-red-200 dark:border-red-900/50 relative">
          {/* Highlight Diff Indicator */}
          <div className="absolute -left-3 top-24 bottom-24 flex flex-col items-center justify-center gap-16 z-10 w-6">
            <div className="w-6 h-6 rounded-full bg-red-100 dark:bg-red-900/50 flex items-center justify-center text-red-600 dark:text-red-400 shadow-sm border border-red-200">
              <GitCompare className="w-3 h-3" />
            </div>
            <div className="w-6 h-6 rounded-full bg-red-100 dark:bg-red-900/50 flex items-center justify-center text-red-600 dark:text-red-400 shadow-sm border border-red-200">
              <GitCompare className="w-3 h-3" />
            </div>
          </div>

          <CardHeader className="bg-red-50/50 dark:bg-red-900/10 border-b border-red-100 dark:border-red-900/50">
            <div className="flex justify-between items-center">
              <CardTitle className="text-lg">Execution #19302</CardTitle>
              <Badge variant="destructive" className="shadow-none border-none">
                <XCircle className="w-3 h-3 mr-1" /> FAILED
              </Badge>
            </div>
            <p className="text-sm text-muted-foreground">Oct 3, 2026 - 02:00:00</p>
          </CardHeader>
          <CardContent className="p-0">
            <div className="flex justify-between p-4 border-b border-border/50 bg-red-50/30 dark:bg-red-900/10">
              <span className="text-sm text-muted-foreground">Duration</span>
              <span className="text-sm font-mono font-medium text-red-600">21m 42s <span className="text-xs ml-1">(+13m 8s)</span></span>
            </div>
            <div className="flex justify-between p-4 border-b border-border/50">
              <span className="text-sm text-muted-foreground">Worker Node</span>
              <span className="text-sm font-mono text-indigo-600 dark:text-indigo-400">worker-17 (ip-10-0-1-44)</span>
            </div>
            <div className="flex justify-between p-4 border-b border-border/50 bg-red-50/30 dark:bg-red-900/10">
              <span className="text-sm text-muted-foreground">Max Memory</span>
              <span className="text-sm font-mono text-red-600">4.8 GB <span className="text-xs ml-1">(+3.6 GB)</span></span>
            </div>
            <div className="flex justify-between p-4 border-b border-border/50">
              <span className="text-sm text-muted-foreground">Parameters</span>
              <pre className="text-xs bg-muted p-2 rounded text-muted-foreground">
                {"{\n  \"batch_size\": 1000,\n  \"dry_run\": false\n}"}
              </pre>
            </div>
          </CardContent>
        </Card>
      </div>
    </ResourceShell>
  );
}
