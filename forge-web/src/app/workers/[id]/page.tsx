"use client";

import { ResourceShell } from "@/components/ui/resource-shell";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import React from "react";
import { ServerCrash, Cpu, MemoryStick, Activity, Network } from "lucide-react";

export default function WorkerDetailsPage({ params }: { params: Promise<{ id: string }> }) {
  const resolvedParams = React.use(params as any) as { id: string };
  const { id } = resolvedParams;
  const headerActions = (
    <div className="flex items-center space-x-2">
      <Button variant="outline" size="sm" className="text-rose-500 hover:text-rose-600">
        <ServerCrash className="mr-2 h-4 w-4" /> Restart Worker
      </Button>
    </div>
  );

  const titleComponent = (
    <div className="flex items-center gap-3">
      <span>Worker {id}</span>
      <Badge variant="outline" className="text-green-500 bg-green-500/10 border-green-500/20">
        <span className="w-1.5 h-1.5 rounded-full bg-green-500 mr-2 animate-pulse" /> Online
      </Badge>
    </div>
  );

  return (
    <ResourceShell
      title={titleComponent}
      subtitle="Region: us-east-1 • IP: 10.0.12.145 • Version: v2.1.4"
      actions={headerActions}
      breadcrumbs={[
        { label: "Workers", href: "/workers" },
        { label: id },
      ]}
      tabs={[
        { id: "overview", label: "Overview" },
        { id: "jobs", label: "Running Jobs" },
        { id: "metrics", label: "Metrics" },
        { id: "logs", label: "Logs" },
        { id: "capabilities", label: "Capabilities" },
      ]}
      defaultTab="overview"
    >
      <div className="grid grid-cols-1 md:grid-cols-4 gap-6">
        <div className="border rounded-xl p-6 bg-card/50 shadow-sm flex flex-col gap-2">
          <div className="flex items-center gap-2 text-muted-foreground mb-2"><Cpu className="w-5 h-5"/> CPU Usage</div>
          <div className="text-3xl font-bold">31%</div>
          <div className="w-full bg-muted rounded-full h-2 mt-2 overflow-hidden">
            <div className="bg-indigo-500 h-full" style={{ width: '31%' }} />
          </div>
        </div>
        <div className="border rounded-xl p-6 bg-card/50 shadow-sm flex flex-col gap-2">
          <div className="flex items-center gap-2 text-muted-foreground mb-2"><MemoryStick className="w-5 h-5"/> Memory</div>
          <div className="text-3xl font-bold">4.2 GB <span className="text-sm font-normal text-muted-foreground">/ 16 GB</span></div>
          <div className="w-full bg-muted rounded-full h-2 mt-2 overflow-hidden">
            <div className="bg-emerald-500 h-full" style={{ width: '26%' }} />
          </div>
        </div>
        <div className="border rounded-xl p-6 bg-card/50 shadow-sm flex flex-col gap-2">
          <div className="flex items-center gap-2 text-muted-foreground mb-2"><Activity className="w-5 h-5"/> Active Jobs</div>
          <div className="text-3xl font-bold">4</div>
          <div className="text-sm text-muted-foreground mt-2">Capacity: 20 concurrent slots</div>
        </div>
        <div className="border rounded-xl p-6 bg-card/50 shadow-sm flex flex-col gap-2">
          <div className="flex items-center gap-2 text-muted-foreground mb-2"><Network className="w-5 h-5"/> Uptime</div>
          <div className="text-3xl font-bold">14d 2h</div>
          <div className="text-sm text-muted-foreground mt-2">Since last restart</div>
        </div>
      </div>
      
      <div className="mt-8 border rounded-xl bg-card/50 p-6">
        <h3 className="text-lg font-semibold mb-4">Worker Capabilities (Tags)</h3>
        <div className="flex gap-2">
          <Badge variant="secondary">gpu-enabled</Badge>
          <Badge variant="secondary">high-memory</Badge>
          <Badge variant="secondary">region:us-east-1</Badge>
          <Badge variant="secondary">payments-cluster</Badge>
        </div>
      </div>
    </ResourceShell>
  );
}
