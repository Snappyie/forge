"use client";

import { useState } from "react";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { Badge } from "@/components/ui/badge";
import { Search, ShieldAlert, Filter, Download } from "lucide-react";
import { Button } from "@/components/ui/button";
import { ResourceShell } from "@/components/ui/resource-shell";

export default function AuditPage() {
  const auditLogs = [
    { id: "au_901", user: "Admin", action: "Changed schedule", object: "Nightly Settlement", before: "0 2 * * *", after: "0 3 * * *", ip: "192.168.1.42", time: "10m ago", env: "Production" },
    { id: "au_902", user: "System", action: "Paused job", object: "Data Sync", before: "Active", after: "Paused", ip: "10.0.0.1", time: "1h ago", env: "Production" },
    { id: "au_903", user: "DevUser1", action: "Created job", object: "Report Gen", before: "-", after: "Created", ip: "192.168.1.105", time: "3h ago", env: "Dev" },
  ];

  return (
    <ResourceShell
      title="Audit Trail"
      subtitle="Track who changed what, when, and where."
      actions={
        <div className="flex gap-2">
          <Button variant="outline"><Filter className="w-4 h-4 mr-2" /> Filter</Button>
          <Button variant="outline"><Download className="w-4 h-4 mr-2" /> Export CSV</Button>
        </div>
      }
    >
      <Card className="border-border/50 shadow-sm">
        <CardHeader>
          <div className="flex justify-between items-center">
            <CardTitle className="flex items-center gap-2"><ShieldAlert className="w-5 h-5" /> Security & Changes</CardTitle>
            <div className="relative">
              <Search className="absolute left-2.5 top-2.5 h-4 w-4 text-muted-foreground" />
              <input 
                className="h-9 w-64 rounded-md border border-input bg-background pl-9 pr-3 text-sm focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
                placeholder="Search audit logs..."
              />
            </div>
          </div>
        </CardHeader>
        <CardContent>
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Time</TableHead>
                <TableHead>User</TableHead>
                <TableHead>Action</TableHead>
                <TableHead>Object</TableHead>
                <TableHead>Environment</TableHead>
                <TableHead>Change (Before → After)</TableHead>
                <TableHead className="text-right">IP Address</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {auditLogs.map((log) => (
                <TableRow key={log.id} className="group">
                  <TableCell className="text-muted-foreground whitespace-nowrap">{log.time}</TableCell>
                  <TableCell className="font-medium">{log.user}</TableCell>
                  <TableCell>{log.action}</TableCell>
                  <TableCell className="font-mono text-xs">{log.object}</TableCell>
                  <TableCell>
                    <Badge variant="outline" className={log.env === 'Production' ? 'bg-red-50 text-red-700 border-red-200' : 'bg-muted'}>
                      {log.env}
                    </Badge>
                  </TableCell>
                  <TableCell>
                    <div className="flex items-center gap-2 text-xs font-mono">
                      <span className="text-red-500 bg-red-50 dark:bg-red-900/20 px-1 rounded">{log.before}</span>
                      <span className="text-muted-foreground">→</span>
                      <span className="text-green-500 bg-green-50 dark:bg-green-900/20 px-1 rounded">{log.after}</span>
                    </div>
                  </TableCell>
                  <TableCell className="text-right font-mono text-xs text-muted-foreground">{log.ip}</TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </CardContent>
      </Card>
    </ResourceShell>
  );
}
