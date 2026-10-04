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

const NODE_TYPES = ["JOB", "APPROVAL", "DELAY", "CONDITION", "MAP", "WEBHOOK"] as const;

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

  useUnsavedChanges(dirty);

  // Seed the canvas from the stored definition once it arrives.
  useEffect(() => {
    setNodes(
      (initial.nodes ?? []).map((node) => ({
        id: node.key,
        position: { x: 80 + (initial.nodes ?? []).indexOf(node) * 200, y: 80 },
        data: { label: `${node.name ?? node.key}\n${node.type}` },
        type: "default",
      })),
    );
    setEdges(
      (initial.edges ?? []).map((edge) => ({
        id: `${edge.from}-${edge.to}`,
        source: edge.from,
        target: edge.to,
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
      addEdge({ ...connection, id: `${connection.source}-${connection.target}` }, current),
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
        data: { label: key, type: "JOB" },
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

      <div className="h-96 rounded-lg border border-border">
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
