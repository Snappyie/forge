"use client";

import { useState } from "react";
import { motion } from "framer-motion";
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from "@/components/ui/card";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Bell, AlertTriangle, XCircle, CheckCircle, Search, Filter } from "lucide-react";

export default function AlertsPage() {
  const alerts = [
    { id: "al_101", severity: "Critical", source: "Job: settlement", message: "Job failed 3 consecutive times", status: "Open", time: "10m ago" },
    { id: "al_102", severity: "Warning", source: "Worker: prod-worker-1", message: "CPU utilization > 90%", status: "Open", time: "1h ago" },
    { id: "al_103", severity: "Warning", source: "Queue: default", message: "Queue depth exceeds 1000", status: "Acknowledged", time: "3h ago" },
    { id: "al_104", severity: "Info", source: "System", message: "Database backup completed", status: "Resolved", time: "1d ago" },
  ];

  return (
    <main className="p-8 relative min-h-screen bg-background">
      <header className="flex justify-between items-center mb-8">
        <div>
          <motion.h1 
            initial={{ y: -20, opacity: 0 }}
            animate={{ y: 0, opacity: 1 }}
            className="text-4xl font-bold tracking-tight mb-2 flex items-center gap-3"
          >
            <Bell className="w-8 h-8 text-rose-500" /> Alerts Center
          </motion.h1>
          <p className="text-muted-foreground">Central inbox for system anomalies and job failures.</p>
        </div>
        <div className="flex gap-2">
          <Button variant="outline"><Filter className="w-4 h-4 mr-2"/> Filter</Button>
          <Button variant="outline">Acknowledge All</Button>
        </div>
      </header>

      <div className="grid grid-cols-1 md:grid-cols-3 gap-6 mb-8">
        <Card className="bg-rose-50/50 dark:bg-rose-900/10 border-rose-200 dark:border-rose-900">
          <CardContent className="p-6">
            <h3 className="text-3xl font-bold text-rose-600 dark:text-rose-400">1</h3>
            <p className="text-sm font-medium text-rose-600/80 dark:text-rose-400/80">Critical Alerts</p>
          </CardContent>
        </Card>
        <Card className="bg-amber-50/50 dark:bg-amber-900/10 border-amber-200 dark:border-amber-900">
          <CardContent className="p-6">
            <h3 className="text-3xl font-bold text-amber-600 dark:text-amber-400">2</h3>
            <p className="text-sm font-medium text-amber-600/80 dark:text-amber-400/80">Warnings</p>
          </CardContent>
        </Card>
        <Card className="bg-muted/50 border-border/50">
          <CardContent className="p-6">
            <h3 className="text-3xl font-bold text-muted-foreground">1</h3>
            <p className="text-sm font-medium text-muted-foreground/80">Resolved (24h)</p>
          </CardContent>
        </Card>
      </div>

      <Card className="border-border/50 shadow-sm">
        <CardHeader>
          <div className="flex justify-between items-center">
            <CardTitle>Recent Alerts</CardTitle>
            <div className="relative">
              <Search className="absolute left-2.5 top-2.5 h-4 w-4 text-muted-foreground" />
              <input 
                className="h-9 w-64 rounded-md border border-input bg-background pl-9 pr-3 text-sm focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
                placeholder="Search alerts..."
              />
            </div>
          </div>
        </CardHeader>
        <CardContent>
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Severity</TableHead>
                <TableHead>Message</TableHead>
                <TableHead>Source</TableHead>
                <TableHead>Time</TableHead>
                <TableHead>Status</TableHead>
                <TableHead className="text-right">Actions</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {alerts.map((alert) => (
                <TableRow key={alert.id} className="group cursor-pointer hover:bg-muted/50">
                  <TableCell>
                    {alert.severity === 'Critical' && <Badge variant="destructive" className="flex w-fit items-center gap-1"><XCircle className="w-3 h-3"/> Critical</Badge>}
                    {alert.severity === 'Warning' && <Badge variant="outline" className="border-amber-500 text-amber-600 flex w-fit items-center gap-1"><AlertTriangle className="w-3 h-3"/> Warning</Badge>}
                    {alert.severity === 'Info' && <Badge variant="secondary" className="flex w-fit items-center gap-1"><CheckCircle className="w-3 h-3"/> Info</Badge>}
                  </TableCell>
                  <TableCell className="font-medium">{alert.message}</TableCell>
                  <TableCell className="text-muted-foreground">{alert.source}</TableCell>
                  <TableCell className="text-muted-foreground whitespace-nowrap">{alert.time}</TableCell>
                  <TableCell>
                    <Badge variant="outline" className={
                      alert.status === 'Open' ? 'bg-background' :
                      alert.status === 'Acknowledged' ? 'bg-blue-50 text-blue-600 border-blue-200' :
                      'bg-muted text-muted-foreground'
                    }>
                      {alert.status}
                    </Badge>
                  </TableCell>
                  <TableCell className="text-right">
                    <Button variant="ghost" size="sm" className="opacity-0 group-hover:opacity-100">Acknowledge</Button>
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </CardContent>
      </Card>
    </main>
  );
}
