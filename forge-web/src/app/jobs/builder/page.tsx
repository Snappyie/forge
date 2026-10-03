"use client";

import { useState } from "react";
import { motion } from "framer-motion";
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Save, ArrowLeft, Code, LayoutTemplate, Play, Settings } from "lucide-react";
import Link from "next/link";
import ReactFlow, { Background, Controls, MiniMap } from "reactflow";
import "reactflow/dist/style.css";
import Editor from "@monaco-editor/react";

const initialNodes = [
  { id: '1', position: { x: 250, y: 50 }, data: { label: 'Webhook Trigger' }, style: { background: '#6366f1', color: 'white', borderRadius: '8px' } },
  { id: '2', position: { x: 250, y: 150 }, data: { label: 'Parse Payload' }, style: { background: '#3b82f6', color: 'white', borderRadius: '8px' } },
];

const initialEdges = [
  { id: 'e1-2', source: '1', target: '2', animated: true },
];

const defaultYaml = `name: Webhook Processor
description: Parses incoming webhook payloads
trigger:
  type: webhook
  path: /api/webhook
steps:
  - id: parse
    type: function
    runtime: nodejs20
    code: |
      export default async function(input) {
        return { parsed: true, data: input };
      }
`;

export default function WorkflowBuilder() {
  const [nodes, setNodes] = useState(initialNodes);
  const [edges, setEdges] = useState(initialEdges);
  const [yamlContent, setYamlContent] = useState(defaultYaml);
  const [activeTab, setActiveTab] = useState("visual");

  return (
    <main className="flex flex-col h-screen bg-background">
      <header className="px-6 py-4 border-b border-border/50 bg-card/50 backdrop-blur-sm flex justify-between items-center z-10 shrink-0">
        <div className="flex items-center gap-4">
          <Link href="/jobs" className="text-muted-foreground hover:text-foreground">
            <ArrowLeft className="w-5 h-5" />
          </Link>
          <div>
            <h1 className="text-xl font-bold tracking-tight">Create Workflow</h1>
            <p className="text-xs text-muted-foreground">Draft mode &bull; Unsaved changes</p>
          </div>
        </div>
        
        <div className="flex items-center gap-2">
          <div className="bg-muted/50 p-1 rounded-md flex mr-4">
            <button 
              onClick={() => setActiveTab("visual")}
              className={`px-3 py-1.5 text-sm font-medium rounded-sm flex items-center gap-2 transition-colors ${activeTab === 'visual' ? 'bg-background shadow-sm text-foreground' : 'text-muted-foreground hover:text-foreground'}`}
            >
              <LayoutTemplate className="w-4 h-4" /> Visual
            </button>
            <button 
              onClick={() => setActiveTab("code")}
              className={`px-3 py-1.5 text-sm font-medium rounded-sm flex items-center gap-2 transition-colors ${activeTab === 'code' ? 'bg-background shadow-sm text-foreground' : 'text-muted-foreground hover:text-foreground'}`}
            >
              <Code className="w-4 h-4" /> Code
            </button>
          </div>
          
          <Button variant="outline" className="border-border/50">
            <Settings className="w-4 h-4 mr-2" /> Settings
          </Button>
          <Button className="bg-indigo-600 hover:bg-indigo-700 text-white">
            <Save className="w-4 h-4 mr-2" /> Save Job
          </Button>
        </div>
      </header>

      <div className="flex-1 flex overflow-hidden relative">
        {/* Visual Builder */}
        {activeTab === "visual" && (
          <div className="flex-1 flex flex-col relative bg-muted/10">
            <ReactFlow 
              nodes={nodes} 
              edges={edges}
              fitView
              className="bg-transparent"
            >
              <Background color="#ccc" gap={16} />
              <Controls />
              <MiniMap />
            </ReactFlow>
            
            {/* Toolbox Overlay */}
            <Card className="absolute top-4 left-4 w-64 bg-card/90 backdrop-blur-md shadow-xl border-border/50">
              <CardHeader className="py-3 px-4 border-b border-border/50">
                <CardTitle className="text-sm font-medium">Node Toolbox</CardTitle>
              </CardHeader>
              <CardContent className="p-2 space-y-1">
                <div className="p-2 rounded hover:bg-muted cursor-grab text-sm flex items-center gap-2 border border-transparent hover:border-border transition-colors">
                  <div className="w-3 h-3 rounded-full bg-indigo-500" /> Webhook Trigger
                </div>
                <div className="p-2 rounded hover:bg-muted cursor-grab text-sm flex items-center gap-2 border border-transparent hover:border-border transition-colors">
                  <div className="w-3 h-3 rounded-full bg-emerald-500" /> Schedule (Cron)
                </div>
                <div className="p-2 rounded hover:bg-muted cursor-grab text-sm flex items-center gap-2 border border-transparent hover:border-border transition-colors">
                  <div className="w-3 h-3 rounded-full bg-blue-500" /> API Request
                </div>
                <div className="p-2 rounded hover:bg-muted cursor-grab text-sm flex items-center gap-2 border border-transparent hover:border-border transition-colors">
                  <div className="w-3 h-3 rounded-full bg-amber-500" /> Human Approval
                </div>
              </CardContent>
            </Card>
          </div>
        )}

        {/* Code Editor */}
        {activeTab === "code" && (
          <div className="flex-1 flex relative">
            <Editor
              height="100%"
              defaultLanguage="yaml"
              theme="vs-dark"
              value={yamlContent}
              onChange={(val) => setYamlContent(val || "")}
              options={{
                minimap: { enabled: false },
                fontSize: 14,
                fontFamily: "var(--font-mono)",
                padding: { top: 20 },
                renderLineHighlight: "all"
              }}
            />
          </div>
        )}
      </div>
    </main>
  );
}
