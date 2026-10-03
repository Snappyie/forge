"use client";

import React, { useState } from "react";
import { ResourceShell } from "@/components/ui/resource-shell";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { PlayCircle, Settings, Save, Network, ZoomIn, ZoomOut, Maximize } from "lucide-react";
import { useToast } from "@/hooks/use-toast";
import ReactFlow, { Background, Controls, applyNodeChanges, applyEdgeChanges, NodeChange, EdgeChange, Node, Edge } from 'reactflow';
import 'reactflow/dist/style.css';

const initialNodes: Node[] = [
  { id: '1', position: { x: 250, y: 50 }, data: { label: 'Start (Trigger)' }, type: 'input' },
  { id: '2', position: { x: 100, y: 150 }, data: { label: 'Extract Data' } },
  { id: '3', position: { x: 400, y: 150 }, data: { label: 'Validate Format' } },
  { id: '4', position: { x: 250, y: 250 }, data: { label: 'Transform' } },
  { id: '5', position: { x: 250, y: 350 }, data: { label: 'Load to DWH' }, type: 'output' },
];

const initialEdges: Edge[] = [
  { id: 'e1-2', source: '1', target: '2', animated: true },
  { id: 'e1-3', source: '1', target: '3' },
  { id: 'e2-4', source: '2', target: '4' },
  { id: 'e3-4', source: '3', target: '4' },
  { id: 'e4-5', source: '4', target: '5' },
];

export default function WorkflowDesignerPage({ params }: { params: Promise<{ id: string }> }) {
  const resolvedParams = React.use(params as any) as { id: string };
  const { id } = resolvedParams;
  const { toast } = useToast();
  const [nodes, setNodes] = useState<Node[]>(initialNodes);
  const [edges, setEdges] = useState<Edge[]>(initialEdges);
  const [isSaving, setIsSaving] = useState(false);

  const onNodesChange = (changes: NodeChange[]) => setNodes((nds) => applyNodeChanges(changes, nds));
  const onEdgesChange = (changes: EdgeChange[]) => setEdges((eds) => applyEdgeChanges(changes, eds));

  const handleSave = () => {
    setIsSaving(true);
    setTimeout(() => {
      setIsSaving(false);
      toast({
        title: "Workflow Saved",
        description: "Your DAG configuration has been persisted.",
        type: "success"
      });
    }, 800);
  };

  const headerActions = (
    <div className="flex items-center space-x-2">
      <Button variant="outline" size="sm" onClick={handleSave} disabled={isSaving}>
        <Save className="mr-2 h-4 w-4" /> {isSaving ? "Saving..." : "Save Draft"}
      </Button>
      <Button variant="default" size="sm" className="bg-indigo-600 hover:bg-indigo-700 text-white">
        <PlayCircle className="mr-2 h-4 w-4" /> Trigger Workflow
      </Button>
      <Button variant="ghost" size="icon">
        <Settings className="h-4 w-4" />
      </Button>
    </div>
  );

  const titleComponent = (
    <div className="flex items-center gap-3">
      <span>End-of-Month Settlement</span>
      <Badge variant="outline" className="text-green-500 bg-green-500/10 border-green-500/20">
        Active
      </Badge>
    </div>
  );

  return (
    <ResourceShell
      title={titleComponent}
      subtitle={`Workflow ID: ${id} • Maintained by Finance Team`}
      actions={headerActions}
      breadcrumbs={[
        { label: "Workflows", href: "/workflows" },
        { label: id },
      ]}
      tabs={[
        { id: "designer", label: "Designer" },
        { id: "executions", label: "Executions" },
        { id: "triggers", label: "Triggers" },
        { id: "settings", label: "Settings" },
      ]}
      defaultTab="designer"
    >
      <div className="h-[70vh] w-full border rounded-xl overflow-hidden bg-background relative shadow-sm">
        <div className="absolute top-4 left-4 z-10 bg-card/80 backdrop-blur-sm p-2 rounded-lg border shadow-sm flex flex-col gap-2">
          <div className="text-xs font-semibold text-muted-foreground uppercase tracking-wider mb-1">Toolbox</div>
          <Button variant="outline" size="sm" className="justify-start"><Network className="w-4 h-4 mr-2" /> Add Job Node</Button>
          <Button variant="outline" size="sm" className="justify-start"><Network className="w-4 h-4 mr-2" /> Add Sub-Workflow</Button>
        </div>
        
        <ReactFlow 
          nodes={nodes} 
          edges={edges} 
          onNodesChange={onNodesChange}
          onEdgesChange={onEdgesChange}
          fitView
          className="bg-muted/10"
        >
          <Background color="#ccc" gap={16} />
          <Controls />
        </ReactFlow>
      </div>
    </ResourceShell>
  );
}
