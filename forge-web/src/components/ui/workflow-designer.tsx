"use client";

/**
 * Workflow designer (UI.md section 23).
 *
 * Drag, connect, edit, and validate a stored DAG. `reactflow` supplies the
 * canvas; the definition is read from and written to the API, so what is drawn
 * is what the engine will run.
 */

import { useCallback, useEffect, useMemo, useState } from "react";
import ReactFlow, {
  Background,
  Controls,
  MiniMap,
  addEdge,
  applyEdgeChanges,
  type Connection,
  type Edge,
  type EdgeChange,
  type Node,
  type NodeChange,
} from "reactflow";
import "reactflow/dist/style.css";
import { Save, Trash2 } from "lucide-react";

import { api } from "@/lib/api";
import { useToast } from "@/lib/useToast";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { useUnsavedChanges } from "@/lib/useUnsavedChanges";

interface Definition {
  nodes?: { key: string; name?: string; type: string; config?: Record<string, unknown> }[];
  edges?: { from: string; to: string }[];
}

const NODE_TYPES = ["JOB", "APPROVAL", "DELAY", "CONDITION", "MAP", "WEBHOOK", "SUB_WORKFLOW"] as const;

export function WorkflowDesigner({
  workflowId,
  definition: initial,
}: {
  workflowId: string;
  definition: Definition;
}) {
  const toast = useToast();
  const [nodes, setNodes] = useState<Node[]>([]);
  const [edges, setEdges] = useState<Edge[]>([]);
  const [dirty, setDirty] = useState(false);
  const [busy, setBusy] = useState(false);
  const [selected, setSelected] = useState<string | null>(null);
  const [jobs, setJobs] = useState<{ id: string, name: string }[]>([]);
  const [workflows, setWorkflows] = useState<{ id: string, name: string }[]>([]);

  useUnsavedChanges(dirty);

  // Fetch jobs and workflows for autocomplete
  useEffect(() => {
    api.get<{ id: string, name: string }[]>("/jobs")
      .then(res => setJobs(Array.isArray(res) ? res : []))
      .catch(() => {});
    api.get<{ id: string, name: string }[]>("/workflows")
      .then(res => setWorkflows(Array.isArray(res) ? res.filter(w => w.id !== workflowId) : []))
      .catch(() => {});
  }, [workflowId]);

  // Seed the canvas from the stored definition once it arrives.
  useEffect(() => {
    setNodes(
      (initial.nodes ?? []).map((node) => ({
        id: node.key,
        position: { x: 80 + (initial.nodes ?? []).indexOf(node) * 200, y: 80 },
        data: { label: `${node.name ?? node.key}\n[${node.type}]`, type: node.type, config: node.config || {} },
        type: "default",
        style: { borderRadius: '8px', padding: '12px', fontWeight: '500', boxShadow: '0 2px 4px rgba(0,0,0,0.1)' }
      })),
    );
    setEdges(
      (initial.edges ?? []).map((edge) => ({
        id: `${edge.from}-${edge.to}`,
        source: edge.from,
        target: edge.to,
        animated: true,
        style: { strokeWidth: 2, stroke: '#94a3b8' }
      })),
    );
    setDirty(false);
  }, [initial]);

  const onNodesChange = useCallback((changes: NodeChange[]) => {
    setNodes((current) => applyNodeChangesCompat(changes, current));
    setDirty(true);
  }, []);

  const onEdgesChange = useCallback((changes: EdgeChange[]) => {
    setEdges((current) => applyEdgeChanges(changes, current));
    setDirty(true);
  }, []);

  const onConnect = useCallback((connection: Connection) => {
    // A self-loop would make the definition unrunnable; the spec asks for
    // cycle detection, and this is its cheapest form.
    if (connection.source === connection.target) return;
    setEdges((current) =>
      addEdge({ ...connection, id: `${connection.source}-${connection.target}`, animated: true, style: { strokeWidth: 2, stroke: '#94a3b8' } }, current),
    );
    setDirty(true);
  }, []);

  const cycle = useMemo(() => findCycle(nodes, edges), [nodes, edges]);
  const current = nodes.find((n) => n.id === selected);

  async function save() {
    setBusy(true);
    try {
      const definition = {
        nodes: nodes.map((node) => ({
          key: node.id,
          name: String(node.data.label ?? node.id).split("\n")[0],
          type: String(node.data.type ?? "JOB"),
          config: node.data.config || {},
        })),
        edges: edges.map((edge) => ({ from: edge.source, to: edge.target })),
      };
      await api.put(`/workflows/${workflowId}/definition`, { definition });
      toast.success("Workflow saved");
      setDirty(false);
    } catch (error) {
      toast.error(
        "Could not save",
        error instanceof Error ? error.message : undefined,
      );
    } finally {
      setBusy(false);
    }
  }

  function addNode() {
    const key = `node_${nodes.length + 1}`;
    setNodes((current) => [
      ...current,
      {
        id: key,
        position: { x: 80 + current.length * 60, y: 260 },
        data: { label: key, type: "JOB", config: {} },
        style: { borderRadius: '8px', padding: '12px', fontWeight: '500', boxShadow: '0 2px 4px rgba(0,0,0,0.1)' }
      },
    ]);
    setSelected(key);
    setDirty(true);
  }

  return (
    <div className="flex flex-col gap-3">
      <div className="flex flex-wrap items-center gap-2">
        <Button variant="outline" size="sm" onClick={addNode}>
          Add node
        </Button>
        <Button
          size="sm"
          disabled={!dirty || busy || cycle !== null}
          onClick={save}
        >
          <Save className="mr-1 size-3.5" aria-hidden />
          {busy ? "Saving…" : "Save"}
        </Button>
        {cycle ? (
          <span role="alert" className="text-xs text-red-600">
            Cycle detected: {cycle.join(" → ")}
          </span>
        ) : null}
      </div>

      <div className="h-[600px] rounded-lg border border-border bg-slate-50/50 dark:bg-slate-900/50">
        <ReactFlow
          nodes={nodes}
          edges={edges}
          onNodesChange={onNodesChange}
          onEdgesChange={onEdgesChange}
          onConnect={onConnect}
          onNodeClick={(_, node) => setSelected(node.id)}
          fitView
        >
          <Background />
          <Controls />
          <MiniMap />
        </ReactFlow>
      </div>

      {current ? (
        <div className="flex flex-wrap items-end gap-2 rounded-lg border border-border p-3">
          <div className="flex flex-col gap-1">
            <Label htmlFor="node-key">Node key</Label>
            <Input id="node-key" value={current.id} readOnly className="w-40" />
          </div>
          <div className="flex flex-col gap-1">
            <Label htmlFor="node-name">Name</Label>
            <Input
              id="node-name"
              value={String(current.data.label ?? "").split("\n")[0]}
              onChange={(e) => {
                setNodes((all) =>
                  all.map((n) =>
                    n.id === current.id
                      ? { ...n, data: { ...n.data, label: `${e.target.value}\n${n.data.type ?? "JOB"}` } }
                      : n,
                  ),
                );
                setDirty(true);
              }}
              className="w-48"
            />
          </div>
          <div className="flex flex-col gap-1">
            <Label htmlFor="node-type">Type</Label>
            <Select
              value={String(current.data.type ?? "JOB")}
              onValueChange={(value) => {
                setNodes((all) =>
                  all.map((n) =>
                    n.id === current.id
                      ? {
                          ...n,
                          data: {
                            ...n.data,
                            type: value,
                            label: `${String(n.data.label ?? n.id).split("\n")[0]}\n${value}`,
                          },
                        }
                      : n,
                  ),
                );
                setDirty(true);
              }}
            >
              <SelectTrigger className="w-36" aria-label="Node type">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {NODE_TYPES.map((type) => (
                  <SelectItem key={type} value={type}>
                    {type}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
          {current.data.type === "JOB" && (
            <div className="flex flex-col gap-1 relative">
              <Label htmlFor="node-job">Linked Job</Label>
              <Input
                id="node-job"
                list="wf-jobs-list"
                value={jobs.find(j => j.id === current.data.config?.job_id)?.name || current.data.config?.job_id || ""}
                onChange={(e) => {
                  const match = jobs.find(j => j.name === e.target.value);
                  const jobId = match ? match.id : e.target.value;
                  setNodes(all => all.map(n => n.id === current.id ? { ...n, data: { ...n.data, config: { ...n.data.config, job_id: jobId } } } : n));
                  setDirty(true);
                }}
                placeholder="Search thousands of jobs..."
                autoComplete="off"
                className="w-56"
              />
              <datalist id="wf-jobs-list">
                {jobs.map(j => (
                  <option key={j.id} value={j.name} />
                ))}
              </datalist>
            </div>
          )}
          {current.data.type === "DELAY" && (
            <div className="flex flex-col gap-1">
              <Label htmlFor="node-delay">Delay (seconds)</Label>
              <Input
                id="node-delay"
                type="number"
                value={current.data.config?.seconds || ""}
                onChange={(e) => {
                  setNodes(all => all.map(n => n.id === current.id ? { ...n, data: { ...n.data, config: { ...n.data.config, seconds: parseInt(e.target.value) || 0 } } } : n));
                  setDirty(true);
                }}
                className="w-32"
              />
            </div>
          )}
          {current.data.type === "WEBHOOK" && (
            // The URL alone is not a usable node: the API now requires a URL, a
            // method and a non-zero timeout, so the designer collects all three
            // rather than publishing a configuration the server refuses.
            <div className="flex flex-wrap items-end gap-2">
              <div className="flex flex-col gap-1">
                <Label htmlFor="node-webhook">Webhook URL</Label>
                <Input
                  id="node-webhook"
                  value={current.data.config?.url || ""}
                  onChange={(e) => {
                    setNodes(all => all.map(n => n.id === current.id ? { ...n, data: { ...n.data, config: { ...n.data.config, url: e.target.value } } } : n));
                    setDirty(true);
                  }}
                  className="w-64"
                  placeholder="https://..."
                />
              </div>
              <div className="flex flex-col gap-1">
                <Label htmlFor="node-webhook-method">Method</Label>
                <Select
                  value={current.data.config?.method || "GET"}
                  onValueChange={(method) => {
                    setNodes(all => all.map(n => n.id === current.id ? { ...n, data: { ...n.data, config: { ...n.data.config, method } } } : n));
                    setDirty(true);
                  }}
                >
                  <SelectTrigger id="node-webhook-method" size="sm" className="w-28">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {(["GET", "POST", "PUT", "PATCH", "DELETE"] as const).map(method => (
                      <SelectItem key={method} value={method}>{method}</SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>
              <div className="flex flex-col gap-1">
                <Label htmlFor="node-webhook-timeout">Timeout (s)</Label>
                <Input
                  id="node-webhook-timeout"
                  type="number"
                  min={1}
                  max={300}
                  className="w-24"
                  value={current.data.config?.timeout_seconds ?? 30}
                  onChange={(e) => {
                    const timeout = Number(e.target.value);
                    setNodes(all => all.map(n => n.id === current.id ? { ...n, data: { ...n.data, config: { ...n.data.config, timeout_seconds: Number.isFinite(timeout) ? timeout : 30 } } } : n));
                    setDirty(true);
                  }}
                />
              </div>
            </div>
          )}
          {current.data.type === "SUB_WORKFLOW" && (
            <div className="flex flex-col gap-1 relative">
              <Label htmlFor="node-sub-wf">Child Workflow</Label>
              <Input
                id="node-sub-wf"
                list="wf-subworkflows-list"
                value={workflows.find(w => w.id === current.data.config?.workflow_id)?.name || current.data.config?.workflow_id || ""}
                onChange={(e) => {
                  const match = workflows.find(w => w.name === e.target.value);
                  const targetWfId = match ? match.id : e.target.value;
                  setNodes(all => all.map(n => n.id === current.id ? { ...n, data: { ...n.data, config: { ...n.data.config, workflow_id: targetWfId } } } : n));
                  setDirty(true);
                }}
                placeholder="Select or enter workflow..."
                autoComplete="off"
                className="w-56"
              />
              <datalist id="wf-subworkflows-list">
                {workflows.map(w => (
                  <option key={w.id} value={w.name} />
                ))}
              </datalist>
            </div>
          )}
          {current.data.type === "CONDITION" && (
            <div className="flex flex-col gap-1">
              <Label htmlFor="node-condition">Expression</Label>
              <Input
                id="node-condition"
                value={current.data.config?.expression || ""}
                onChange={(e) => {
                  setNodes(all => all.map(n => n.id === current.id ? { ...n, data: { ...n.data, config: { ...n.data.config, expression: e.target.value } } } : n));
                  setDirty(true);
                }}
                className="w-56"
                placeholder="e.g. 1 == 1"
              />
            </div>
          )}
          {current.data.type === "APPROVAL" && (
            <div className="flex flex-col gap-1">
              <Label htmlFor="node-approval">Required Role</Label>
              <Input
                id="node-approval"
                value={current.data.config?.required_role || "ADMIN"}
                onChange={(e) => {
                  setNodes(all => all.map(n => n.id === current.id ? { ...n, data: { ...n.data, config: { ...n.data.config, required_role: e.target.value } } } : n));
                  setDirty(true);
                }}
                className="w-36"
                placeholder="ADMIN"
              />
            </div>
          )}
          {current.data.type === "MAP" && (
            <div className="flex flex-col gap-1">
              <Label htmlFor="node-map">Target Node Key</Label>
              <Input
                id="node-map"
                value={current.data.config?.target_node_id || ""}
                onChange={(e) => {
                  setNodes(all => all.map(n => n.id === current.id ? { ...n, data: { ...n.data, config: { ...n.data.config, target_node_id: e.target.value } } } : n));
                  setDirty(true);
                }}
                className="w-40"
                placeholder="e.g. process_item"
              />
            </div>
          )}
          <Button
            variant="outline"
            size="sm"
            onClick={() => {
              setNodes((all) => all.filter((n) => n.id !== current.id));
              setEdges((all) =>
                all.filter((e) => e.source !== current.id && e.target !== current.id),
              );
              setSelected(null);
              setDirty(true);
            }}
          >
            <Trash2 className="mr-1 size-3.5" aria-hidden />
            Delete node
          </Button>
        </div>
      ) : null}
    </div>
  );
}

/** Applies node changes; wrapped so the import stays local to this file. */
function applyNodeChangesCompat(changes: NodeChange[], nodes: Node[]): Node[] {
  return changes.reduce(
    (current, change) => {
      if (change.type === "position" && change.position) {
        return current.map((n) =>
          n.id === change.id ? { ...n, position: change.position! } : n,
        );
      }
      if (change.type === "remove") {
        return current.filter((n) => n.id !== change.id);
      }
      return current;
    },
    nodes,
  );
}

/**
 * Returns a cycle path, or null when the graph is acyclic.
 *
 * The spec asks for cycle detection, and a cyclic workflow would never finish.
 */
function findCycle(nodes: Node[], edges: Edge[]): string[] | null {
  const adjacency = new Map<string, string[]>();
  for (const node of nodes) adjacency.set(node.id, []);
  for (const edge of edges) {
    adjacency.get(edge.source)?.push(edge.target);
  }

  const visiting = new Set<string>();
  const done = new Set<string>();
  const path: string[] = [];

  const walk = (id: string): string[] | null => {
    if (visiting.has(id)) {
      return [...path.slice(path.indexOf(id)), id];
    }
    if (done.has(id)) return null;

    visiting.add(id);
    path.push(id);
    for (const next of adjacency.get(id) ?? []) {
      const found = walk(next);
      if (found) return found;
    }
    path.pop();
    visiting.delete(id);
    done.add(id);
    return null;
  };

  for (const node of nodes) {
    const found = walk(node.id);
    if (found) return found;
  }
  return null;
}
