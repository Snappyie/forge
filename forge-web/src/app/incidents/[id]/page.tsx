"use client";

import { ResourceShell } from "@/components/ui/resource-shell";
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { AlertCircle, ServerCrash, Clock, FileText, Search } from "lucide-react";
import { Button } from "@/components/ui/button";

export default function IncidentContextPage() {
  return (
    <ResourceShell
      title="Incident INC-8021"
      subtitle="Settlement Failure (P1)"
      statusBadge={<Badge variant="destructive">Active Incident</Badge>}
      actions={
        <div className="flex gap-2">
          <Button variant="outline">Acknowledge</Button>
          <Button variant="outline">Resolve</Button>
        </div>
      }
    >
      <div className="grid grid-cols-1 md:grid-cols-3 gap-6">
        <div className="md:col-span-2 space-y-6">
          <Card className="border-red-200 dark:border-red-900/50">
            <CardHeader className="bg-red-50/50 dark:bg-red-900/10 border-b border-red-100 dark:border-red-900/30">
              <CardTitle className="flex items-center gap-2"><AlertCircle className="w-5 h-5 text-red-500" /> Correlated Alerts</CardTitle>
            </CardHeader>
            <CardContent className="p-0">
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead>Time</TableHead>
                    <TableHead>Alert</TableHead>
                    <TableHead>Component</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  <TableRow className="bg-red-50/20 dark:bg-red-900/5 text-red-900 dark:text-red-200">
                    <TableCell className="font-mono text-xs">02:08:41</TableCell>
                    <TableCell className="font-semibold">Job Settlement Failed (3rd attempt)</TableCell>
                    <TableCell>Job Scheduler</TableCell>
                  </TableRow>
                  <TableRow>
                    <TableCell className="font-mono text-xs">02:08:00</TableCell>
                    <TableCell>Worker Node CPU Spiked to 99%</TableCell>
                    <TableCell>worker-17</TableCell>
                  </TableRow>
                  <TableRow>
                    <TableCell className="font-mono text-xs">02:05:12</TableCell>
                    <TableCell>Database Latency &gt; 2000ms</TableCell>
                    <TableCell>PostgreSQL (Primary)</TableCell>
                  </TableRow>
                </TableBody>
              </Table>
            </CardContent>
          </Card>

          <Card className="border-border/50">
            <CardHeader className="border-b border-border/50">
              <CardTitle className="flex items-center gap-2"><FileText className="w-5 h-5" /> Extracted Logs</CardTitle>
            </CardHeader>
            <CardContent className="p-0">
              <div className="bg-[#1e1e1e] text-[#d4d4d4] font-mono text-xs p-4 overflow-x-auto">
                <div className="flex gap-4"><span className="text-gray-500">02:07:55</span><span className="text-blue-400">INFO</span><span>Beginning transaction settlement batch #9182</span></div>
                <div className="flex gap-4"><span className="text-gray-500">02:08:00</span><span className="text-yellow-400">WARN</span><span>Query taking longer than expected...</span></div>
                <div className="flex gap-4"><span className="text-gray-500">02:08:31</span><span className="text-red-400">ERROR</span><span>Connection timeout waiting for lock</span></div>
                <div className="flex gap-4"><span className="text-gray-500">02:08:41</span><span className="text-red-400">FATAL</span><span>Job execution failed. Maximum retries (3) reached.</span></div>
              </div>
            </CardContent>
          </Card>
        </div>

        <div className="space-y-6">
          <Card className="border-border/50">
            <CardHeader>
              <CardTitle className="text-base">Impact Analysis</CardTitle>
            </CardHeader>
            <CardContent className="space-y-4">
              <div>
                <p className="text-sm text-muted-foreground mb-1">Downstream Workflows Blocked</p>
                <div className="flex flex-col gap-2">
                  <Badge variant="outline" className="w-fit">Billing Generation</Badge>
                  <Badge variant="outline" className="w-fit">Daily Report Sync</Badge>
                </div>
              </div>
              <div className="pt-4 border-t border-border/50">
                <p className="text-sm text-muted-foreground mb-1">SLA Status</p>
                <Badge variant="destructive">Missed by 45m</Badge>
              </div>
            </CardContent>
          </Card>

          <Card className="border-border/50">
            <CardHeader>
              <CardTitle className="text-base flex items-center gap-2"><ServerCrash className="w-4 h-4" /> Affected Workers</CardTitle>
            </CardHeader>
            <CardContent>
              <div className="space-y-2">
                <div className="flex justify-between items-center bg-muted/30 p-2 rounded border border-border/50">
                  <span className="text-sm font-mono">worker-17</span>
                  <Badge className="bg-red-100 text-red-700 shadow-none border-none">Degraded</Badge>
                </div>
              </div>
            </CardContent>
          </Card>
          
          <Button className="w-full bg-indigo-600 hover:bg-indigo-700 text-white"><Search className="w-4 h-4 mr-2" /> Start AI Troubleshooting</Button>
        </div>
      </div>
    </ResourceShell>
  );
}
