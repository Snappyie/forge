"use client";

import React, { useEffect, useState, useRef } from "react";
import { motion } from "framer-motion";
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Play, RotateCcw, XCircle, Terminal, Activity, ArrowLeft, Check, CheckCircle2 } from "lucide-react";
import Link from "next/link";
import ReactFlow, { Background, Controls, MarkerType } from "reactflow";
import "reactflow/dist/style.css";
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle, DialogTrigger, DialogFooter } from "@/components/ui/dialog";

import { AiTroubleshootingPanel } from "@/components/ui/ai-troubleshooting";
import { AnomalyDetectionWidget } from "@/components/ui/anomaly-detection";
import { WhyIsThisRunning } from "@/components/ui/why-running";
import { DiagnosticPanel } from "@/components/ui/diagnostic-panel";
import { RetryReplayAction } from "@/components/ui/retry-replay";
import { useToast } from "@/hooks/use-toast";

const initialNodes = [
  { id: '1', position: { x: 250, y: 0 }, data: { label: 'Start Execution' }, style: { background: '#10b981', color: 'white', border: 'none', borderRadius: '8px' } },
  { id: '2', position: { x: 250, y: 100 }, data: { label: 'Fetch Data from API' }, style: { background: '#6366f1', color: 'white', border: 'none', borderRadius: '8px' } },
  { id: '3', position: { x: 250, y: 200 }, data: { label: 'Wait for Human Approval' }, style: { background: '#f59e0b', color: 'white', border: 'none', borderRadius: '8px' } },
  { id: '4', position: { x: 250, y: 300 }, data: { label: 'Process Dataset' }, style: { background: '#3b82f6', color: 'white', border: 'none', borderRadius: '8px' } },
];

const initialEdges = [
  { id: 'e1-2', source: '1', target: '2', animated: true, markerEnd: { type: MarkerType.ArrowClosed } },
  { id: 'e2-3', source: '2', target: '3', animated: true, markerEnd: { type: MarkerType.ArrowClosed } },
  { id: 'e3-4', source: '3', target: '4', animated: false, style: { strokeDasharray: '5 5' }, markerEnd: { type: MarkerType.ArrowClosed } },
];

export default function ExecutionDetail({ params }: { params: Promise<{ id: string }> }) {
  const resolvedParams = React.use(params as any) as { id: string };
  const { id } = resolvedParams;
  const { toast } = useToast();
  const [logs, setLogs] = useState<string[]>([
    "[10:45:01.200] INF Execution started by system.",
    "[10:45:01.215] INF Running step: 'Start Execution'",
    "[10:45:01.220] INF Step complete.",
    "[10:45:01.225] INF Running step: 'Fetch Data from API'",
    "[10:45:02.050] INF Step complete. Fetched 1523 records.",
    "[10:45:02.055] WRN Human-in-the-loop intervention required.",
    "[10:45:02.060] INF Pausing execution until manual approval is received...",
  ]);
  const [status, setStatus] = useState("Paused");
  const bottomRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [logs]);

  const handleApprove = () => {
    setStatus("Running");
    toast({ title: "Approved", description: "Execution resumed." });
    setLogs(prev => [
      ...prev,
      `[${new Date().toISOString().substring(11, 23)}] INF Manual approval granted.`,
      `[${new Date().toISOString().substring(11, 23)}] INF Resuming execution...`,
      `[${new Date().toISOString().substring(11, 23)}] INF Running step: 'Process Dataset'`
    ]);
    
    setTimeout(() => {
      setStatus("Completed");
      toast({ title: "Completed", description: "Execution finished successfully." });
      setLogs(prev => [
        ...prev,
        `[${new Date().toISOString().substring(11, 23)}] INF Step complete. Dataset processed successfully.`,
        `[${new Date().toISOString().substring(11, 23)}] INF Execution finished successfully.`
      ]);
    }, 2000);
  };

  const handleCancel = () => {
     setStatus("Failed");
     toast({ title: "Cancelled", description: "Execution was cancelled.", variant: "destructive" });
     setLogs(prev => [
        ...prev,
        `[${new Date().toISOString().substring(11, 23)}] ERR Execution cancelled by user.`
     ]);
  };

  return (
    <main className="p-8 relative min-h-screen bg-[radial-gradient(ellipse_at_top_right,_var(--tw-gradient-stops))] from-indigo-50/50 via-background to-background dark:from-indigo-900/10 dark:via-background dark:to-background">
      <header className="mb-6">
        <Link href="/executions" className="text-muted-foreground hover:text-foreground inline-flex items-center text-sm font-medium transition-colors mb-4">
          <ArrowLeft className="mr-2 h-4 w-4" /> Back to Executions
        </Link>
        <div className="flex justify-between items-start">
          <div>
            <motion.h1 
              initial={{ y: -20, opacity: 0 }}
              animate={{ y: 0, opacity: 1 }}
              className="text-3xl font-bold tracking-tight mb-2 flex items-center gap-4"
            >
              Execution <span className="font-mono text-xl text-muted-foreground bg-muted/50 px-2 py-1 rounded-md">exec_{id}</span>
            </motion.h1>
            <div className="flex items-center gap-3 text-sm mt-3">
              <Badge 
                className={`shadow-none border-none ${
                  status === 'Running' ? 'bg-blue-100 text-blue-700 dark:bg-blue-500/10 dark:text-blue-400' :
                  status === 'Completed' ? 'bg-green-100 text-green-700 dark:bg-green-500/10 dark:text-green-400' :
                  status === 'Paused' ? 'bg-amber-100 text-amber-700 dark:bg-amber-500/10 dark:text-amber-400' :
                  'bg-rose-100 text-rose-700 dark:bg-rose-500/10 dark:text-rose-400'
                }`}
              >
                {status === 'Paused' && <Activity className="w-3 h-3 mr-1 animate-pulse" />}
                {status === 'Completed' && <CheckCircle2 className="w-3 h-3 mr-1" />}
                {status}
              </Badge>
              <span className="text-muted-foreground">Triggered by <span className="font-medium text-foreground">system</span></span>
              <span className="text-muted-foreground">&bull;</span>
              <span className="text-muted-foreground">Job: <Link href="/jobs" className="font-medium text-indigo-500 hover:underline">Data Pipeline Etl</Link></span>
            </div>
          </div>
          
          <div className="flex gap-2">
            <RetryReplayAction executionId={id} status={status} hasRetriesLeft={status === "Failed"} />

            {status === "Paused" && (
              <Dialog>
                <DialogTrigger className="bg-amber-500 hover:bg-amber-600 text-white shadow-md animate-pulse inline-flex shrink-0 items-center justify-center rounded-lg text-sm font-medium h-9 px-4 py-2">
                     <Check className="mr-2 h-4 w-4" /> Approve Step
                </DialogTrigger>
                <DialogContent>
                  <DialogHeader>
                    <DialogTitle>Approve Execution Step</DialogTitle>
                    <DialogDescription>
                      This workflow has hit a Human-in-the-Loop breakpoint. Please confirm you want to proceed to the next step.
                    </DialogDescription>
                  </DialogHeader>
                  <DialogFooter>
                    <DialogTrigger className="inline-flex items-center justify-center rounded-lg text-sm font-medium h-9 px-4 py-2 border border-input bg-background hover:bg-accent hover:text-accent-foreground">Cancel</DialogTrigger>
                    <DialogTrigger className="inline-flex items-center justify-center rounded-lg text-sm font-medium h-9 px-4 py-2 bg-indigo-600 hover:bg-indigo-700 text-white" onClick={handleApprove}>Approve & Resume</DialogTrigger>
                  </DialogFooter>
                </DialogContent>
              </Dialog>
            )}
            {status !== "Completed" && status !== "Failed" && (
              <Button variant="outline" onClick={handleCancel} className="border-border/50 bg-card/50 backdrop-blur-sm">
                <XCircle className="mr-2 h-4 w-4 text-rose-500" /> Cancel
              </Button>
            )}
          </div>
        </div>
      </header>

      <div className="grid grid-cols-1 xl:grid-cols-3 gap-6 mb-6">
        <WhyIsThisRunning />
        <AnomalyDetectionWidget />
        {status === "Failed" && <DiagnosticPanel />}
      </div>

      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6 h-[60vh] mb-6">
        {/* ReactFlow Visual Graph */}
        <Card className="bg-card/50 backdrop-blur-sm shadow-sm border-border/50 flex flex-col overflow-hidden h-full">
          <CardHeader className="py-4 border-b border-border/50">
            <CardTitle className="text-lg flex items-center">
              <Activity className="w-4 h-4 mr-2 text-indigo-500" /> State Traversal Graph
            </CardTitle>
          </CardHeader>
          <div className="flex-1 bg-muted/10 relative">
            <ReactFlow 
              nodes={initialNodes.map(n => ({
                ...n,
                style: {
                  ...n.style,
                  opacity: (status === 'Paused' && n.id === '4') ? 0.5 : 1,
                  boxShadow: (status === 'Paused' && n.id === '3') ? '0 0 0 4px rgba(245, 158, 11, 0.3)' : 'none'
                }
              }))} 
              edges={initialEdges.map(e => ({
                ...e,
                animated: status === 'Running' || (e.source !== '3' && status !== 'Completed'),
              }))}
              fitView
              attributionPosition="bottom-right"
            >
              <Background color="#ccc" gap={16} />
              <Controls />
            </ReactFlow>
          </div>
        </Card>

        {/* Terminal Logs */}
        <Card className="bg-[#0D1117] text-gray-300 border-border/50 flex flex-col overflow-hidden h-full shadow-xl">
          <CardHeader className="py-3 px-4 border-b border-[#30363D] bg-[#161B22] flex flex-row items-center justify-between space-y-0">
            <div className="flex items-center">
              <Terminal className="w-4 h-4 mr-2 text-gray-400" />
              <span className="font-mono text-sm font-semibold text-gray-200">Execution Logs</span>
            </div>
            <div className="flex space-x-1.5">
              <div className="w-3 h-3 rounded-full bg-[#FF5F56]"></div>
              <div className="w-3 h-3 rounded-full bg-[#FFBD2E]"></div>
              <div className="w-3 h-3 rounded-full bg-[#27C93F]"></div>
            </div>
          </CardHeader>
          <div className="flex-1 overflow-y-auto p-4 font-mono text-xs md:text-sm">
            {logs.map((log, i) => (
              <div key={i} className="mb-1">
                <span className="text-gray-500">{log.substring(0, 15)}</span>
                <span className={
                  log.includes("INF") ? "text-blue-400 ml-2" : 
                  log.includes("WRN") ? "text-yellow-400 ml-2" : 
                  log.includes("ERR") ? "text-red-400 ml-2" : "ml-2"
                }>
                  {log.substring(15, 19)}
                </span>
                <span className={log.includes("success") || log.includes("granted") ? "text-green-400 ml-2" : "text-gray-200 ml-2"}>
                  {log.substring(19)}
                </span>
              </div>
            ))}
            <div ref={bottomRef} />
          </div>
        </Card>
      </div>

      {status === "Failed" && (
         <div className="mb-6">
           <AiTroubleshootingPanel executionId={id} />
         </div>
      )}
    </main>
  );
}
