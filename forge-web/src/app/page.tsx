"use client";

import { motion } from "framer-motion";
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from "@/components/ui/card";
import { Activity, Box, ServerCrash, Users, Terminal, CheckCircle2, XCircle, Clock, AlertTriangle, PlayCircle } from "lucide-react";
import { Skeleton } from "@/components/ui/skeleton";
import { Badge } from "@/components/ui/badge";

export default function Dashboard() {
  const stats = [
    { name: "Jobs", value: "1,248", active: "18 Running", pending: "3 Failed, 2 Delayed" },
    { name: "Workers", value: "42", active: "31 Queued", pending: "1 SLA Issue" },
  ];

  return (
    <main className="p-8 relative min-h-screen bg-[radial-gradient(ellipse_at_top_right,_var(--tw-gradient-stops))] from-indigo-50/50 via-background to-background dark:from-indigo-900/10 dark:via-background dark:to-background">
      <header className="mb-8">
        <motion.h1 
          initial={{ y: -20, opacity: 0 }}
          animate={{ y: 0, opacity: 1 }}
          className="text-4xl font-bold tracking-tight mb-2 text-foreground"
        >
          Overview
        </motion.h1>
        <p className="text-muted-foreground">Real-time pulse of your orchestration engine.</p>
      </header>

      <div className="grid grid-cols-1 md:grid-cols-2 gap-6 mb-8">
        {stats.map((stat, idx) => (
          <motion.div
            key={stat.name}
            initial={{ opacity: 0, y: 20 }}
            animate={{ opacity: 1, y: 0 }}
            transition={{ delay: idx * 0.1 }}
          >
            <Card className="bg-card/50 backdrop-blur-sm border-border/50 hover:border-indigo-500/50 transition-colors">
              <CardContent className="p-6">
                <div className="flex items-center justify-between pb-2">
                  <p className="text-sm font-medium text-muted-foreground">{stat.name}</p>
                </div>
                <div className="flex items-baseline space-x-2">
                  <h2 className="text-3xl font-bold tracking-tight">{stat.value}</h2>
                </div>
                <div className="mt-4 flex flex-col gap-1 text-sm">
                  <div className="flex justify-between">
                    <span className="text-muted-foreground">{stat.name === 'Jobs' ? 'Running' : 'Queued'}</span>
                    <span className="font-medium">{stat.active.split(' ')[0]}</span>
                  </div>
                  <div className="flex justify-between">
                    <span className="text-muted-foreground">{stat.name === 'Jobs' ? 'Failed/Delayed' : 'SLA Issues'}</span>
                    <span className="font-medium text-rose-500">{stat.pending.split(' ')[0]}</span>
                  </div>
                </div>
              </CardContent>
            </Card>
          </motion.div>
        ))}
      </div>

      <div className="grid grid-cols-1 md:grid-cols-3 gap-6 mb-8">
        <Card className="bg-card/50 backdrop-blur-sm shadow-sm md:col-span-1">
          <CardHeader>
            <CardTitle>Real-time Status</CardTitle>
          </CardHeader>
          <CardContent className="space-y-4">
            <div className="flex justify-between items-center"><span className="text-sm">Scheduler</span> <Badge variant="outline" className="text-green-500 bg-green-500/10 border-green-500/20">● Healthy</Badge></div>
            <div className="flex justify-between items-center"><span className="text-sm">Database</span> <Badge variant="outline" className="text-green-500 bg-green-500/10 border-green-500/20">● Healthy</Badge></div>
            <div className="flex justify-between items-center"><span className="text-sm">Queue</span> <Badge variant="outline" className="text-green-500 bg-green-500/10 border-green-500/20">● Healthy</Badge></div>
            <div className="flex justify-between items-center"><span className="text-sm">Workers</span> <Badge variant="outline" className="text-amber-500 bg-amber-500/10 border-amber-500/20">⚠ 2 degraded</Badge></div>
          </CardContent>
        </Card>

        <Card className="bg-card/50 backdrop-blur-sm shadow-sm md:col-span-2 border-rose-500/20">
          <CardHeader>
            <CardTitle className="text-rose-500 flex items-center"><AlertTriangle className="mr-2 h-5 w-5" /> Needs Attention</CardTitle>
          </CardHeader>
          <CardContent className="space-y-4">
            <div className="flex justify-between items-center p-3 rounded-md bg-rose-500/10 border border-rose-500/20">
              <div className="flex items-center gap-3"><div className="w-2 h-2 rounded-full bg-rose-500" /> <span className="font-medium text-sm">Settlement failed 3 times</span></div>
              <span className="text-xs text-muted-foreground hover:underline cursor-pointer">Investigate →</span>
            </div>
            <div className="flex justify-between items-center p-3 rounded-md bg-orange-500/10 border border-orange-500/20">
              <div className="flex items-center gap-3"><div className="w-2 h-2 rounded-full bg-orange-500" /> <span className="font-medium text-sm">Worker-17 offline</span></div>
              <span className="text-xs text-muted-foreground hover:underline cursor-pointer">View Worker →</span>
            </div>
            <div className="flex justify-between items-center p-3 rounded-md bg-amber-500/10 border border-amber-500/20">
              <div className="flex items-center gap-3"><div className="w-2 h-2 rounded-full bg-amber-500" /> <span className="font-medium text-sm">Reconciliation SLA approaching</span></div>
              <span className="text-xs text-muted-foreground hover:underline cursor-pointer">View SLAs →</span>
            </div>
            <div className="flex justify-between items-center p-3 rounded-md bg-amber-500/10 border border-amber-500/20">
              <div className="flex items-center gap-3"><div className="w-2 h-2 rounded-full bg-amber-500" /> <span className="font-medium text-sm">Queue backlog increasing</span></div>
              <span className="text-xs text-muted-foreground hover:underline cursor-pointer">View Queues →</span>
            </div>
          </CardContent>
        </Card>
      </div>

      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
        <motion.div
          initial={{ opacity: 0, scale: 0.95 }}
          animate={{ opacity: 1, scale: 1 }}
          transition={{ delay: 0.4 }}
        >
          <Card className="bg-card/50 backdrop-blur-sm shadow-sm h-full">
            <CardHeader>
              <CardTitle>Upcoming Executions</CardTitle>
            </CardHeader>
            <CardContent>
              <div className="space-y-4">
                {[
                  { time: "10:00", job: "Settlement", env: "Production" },
                  { time: "10:05", job: "Reconciliation", env: "Production" },
                  { time: "10:15", job: "Billing", env: "Production" },
                  { time: "10:30", job: "Reports", env: "QA" }
                ].map((run, i) => (
                  <div key={i} className="flex justify-between items-center text-sm border-b border-border/50 pb-2 last:border-0">
                    <div className="flex gap-4">
                      <span className="font-mono text-muted-foreground">{run.time}</span>
                      <span className="font-medium">{run.job}</span>
                    </div>
                    <Badge variant="outline" className="text-xs bg-muted/20">{run.env}</Badge>
                  </div>
                ))}
              </div>
            </CardContent>
          </Card>
        </motion.div>

        <motion.div
          initial={{ opacity: 0, scale: 0.95 }}
          animate={{ opacity: 1, scale: 1 }}
          transition={{ delay: 0.5 }}
        >
          <Card className="bg-card/50 backdrop-blur-sm shadow-sm h-full">
            <CardHeader>
              <CardTitle>Recent Executions</CardTitle>
            </CardHeader>
            <CardContent>
               <div className="space-y-4">
                {[
                  { status: "Success", job: "Data Pipeline", duration: "2m 14s" },
                  { status: "Failure", job: "Nightly Sync", duration: "45s" },
                  { status: "Retry", job: "Webhook Delivery", duration: "12s" },
                  { status: "Timeout", job: "Heavy Analytics", duration: "60m" }
                ].map((run, i) => (
                  <div key={i} className="flex justify-between items-center text-sm border-b border-border/50 pb-2 last:border-0">
                    <div className="flex gap-4 items-center">
                      {run.status === 'Success' && <CheckCircle2 className="w-4 h-4 text-green-500" />}
                      {run.status === 'Failure' && <XCircle className="w-4 h-4 text-rose-500" />}
                      {run.status === 'Retry' && <Activity className="w-4 h-4 text-amber-500" />}
                      {run.status === 'Timeout' && <Clock className="w-4 h-4 text-orange-500" />}
                      <span className="font-medium">{run.job}</span>
                    </div>
                    <span className="text-muted-foreground text-xs">{run.duration}</span>
                  </div>
                ))}
              </div>
            </CardContent>
          </Card>
        </motion.div>
      </div>
    </main>
  );
}
