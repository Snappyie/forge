"use client";

import { useState } from "react";
import { motion } from "framer-motion";
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from "@/components/ui/card";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Users, PlusCircle, Trash2, RefreshCw, Loader2, ShieldAlert } from "lucide-react";
import { api, ApiError } from "@/lib/api";
import { useAuth } from "@/lib/auth";
import { useList } from "@/lib/useQuery";
import { roleCan, type Team } from "@/lib/types";

export default function TeamsPage() {
  const { session } = useAuth();
  const query = useList<Team>("/teams");
  const [creating, setCreating] = useState(false);
  const canWrite = roleCan(session?.role, "users:write");

  const rows = query.rows;

  async function handleDelete(id: string) {
    if (!confirm("Are you sure you want to delete this team?")) return;
    try {
      await api.delete(`/teams/${id}`);
      query.reload();
    } catch (err) {
      alert(err instanceof Error ? err.message : "Failed to delete team");
    }
  }

  return (
    <main className="p-8 relative min-h-screen bg-[radial-gradient(ellipse_at_top_right,_var(--tw-gradient-stops))] from-blue-50/50 via-background to-background dark:from-blue-900/10 dark:via-background dark:to-background">
      <header className="flex justify-between items-center mb-8">
        <div>
          <motion.h1 
            initial={{ y: -20, opacity: 0 }}
            animate={{ y: 0, opacity: 1 }}
            className="text-4xl font-bold tracking-tight mb-2 text-foreground flex items-center gap-3"
          >
            <Users className="h-8 w-8 text-blue-500" /> Teams & Access
          </motion.h1>
          <p className="text-muted-foreground">Manage RBAC, team ownership, and on-call routing.</p>
        </div>
        <div className="flex items-center gap-2">
          <Button variant="outline" size="sm" onClick={query.reload} aria-label="Refresh">
            <RefreshCw className="mr-2 h-4 w-4" /> Refresh
          </Button>
          {canWrite ? (
            <Button
              className="bg-blue-600 hover:bg-blue-700 text-white"
              onClick={() => setCreating((open) => !open)}
            >
              <PlusCircle className="mr-2 h-4 w-4" /> {creating ? "Cancel" : "Create Team"}
            </Button>
          ) : null}
        </div>
      </header>

      {creating && canWrite ? (
        <CreateTeamForm
          onCreated={() => {
            setCreating(false);
            query.reload();
          }}
        />
      ) : null}

      <motion.div initial={{ opacity: 0, y: 20 }} animate={{ opacity: 1, y: 0 }}>
        <Card className="bg-card/50 backdrop-blur-sm border-border/50">
          <CardContent className="p-0">
            {query.state === "loading" ? (
              <div className="flex items-center justify-center p-12 text-muted-foreground">
                <Loader2 className="h-6 w-6 animate-spin mr-2" /> Loading teams...
              </div>
            ) : rows.length === 0 ? (
              <div className="p-12 text-center text-muted-foreground">
                No teams created yet. {canWrite ? "Click 'Create Team' to add one." : ""}
              </div>
            ) : (
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead className="pl-6">Team Name</TableHead>
                    <TableHead>Description</TableHead>
                    <TableHead>Members</TableHead>
                    <TableHead>Current On-Call</TableHead>
                    <TableHead className="text-right pr-6">Manage</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {rows.map((team) => (
                    <TableRow key={team.id} className="hover:bg-muted/50 transition-colors">
                      <TableCell className="font-medium pl-6">{team.name}</TableCell>
                      <TableCell className="text-muted-foreground">{team.description || "—"}</TableCell>
                      <TableCell>
                        <div className="flex items-center gap-2">
                          <Users className="w-4 h-4 text-muted-foreground" /> {team.members}
                        </div>
                      </TableCell>
                      <TableCell className="text-muted-foreground text-sm">{team.on_call || "—"}</TableCell>
                      <TableCell className="text-right pr-6">
                        {canWrite ? (
                          <div className="flex justify-end gap-2">
                            <Button
                              variant="ghost"
                              size="icon"
                              onClick={() => handleDelete(team.id)}
                              aria-label="Delete team"
                            >
                              <Trash2 className="h-4 w-4 text-rose-500" />
                            </Button>
                          </div>
                        ) : null}
                      </TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
            )}
          </CardContent>
        </Card>
      </motion.div>
      
      <div className="mt-8 grid grid-cols-1 md:grid-cols-2 gap-6">
        <motion.div initial={{ opacity: 0, y: 20 }} animate={{ opacity: 1, y: 0 }} transition={{ delay: 0.1 }}>
          <Card className="bg-card/50 backdrop-blur-sm border-border/50">
            <CardHeader>
              <CardTitle className="flex items-center gap-2"><ShieldAlert className="w-5 h-5 text-indigo-500"/> Global Roles</CardTitle>
              <CardDescription>System-wide permissions across all teams.</CardDescription>
            </CardHeader>
            <CardContent>
              <div className="space-y-4">
                <div className="flex justify-between items-center p-3 border rounded-md">
                  <div>
                    <div className="font-medium">Platform Admin</div>
                    <div className="text-xs text-muted-foreground">Full access to modify workers, clusters, and global settings.</div>
                  </div>
                  <Badge variant="secondary">ADMIN</Badge>
                </div>
                <div className="flex justify-between items-center p-3 border rounded-md">
                  <div>
                    <div className="font-medium">Operator / Developer</div>
                    <div className="text-xs text-muted-foreground">Manage jobs, executions, and workflow triggers.</div>
                  </div>
                  <Badge variant="secondary">OPERATOR</Badge>
                </div>
                <div className="flex justify-between items-center p-3 border rounded-md">
                  <div>
                    <div className="font-medium">Viewer / Auditor</div>
                    <div className="text-xs text-muted-foreground">Read-only access to executions, audit trails, and jobs.</div>
                  </div>
                  <Badge variant="secondary">VIEWER</Badge>
                </div>
              </div>
            </CardContent>
          </Card>
        </motion.div>
      </div>
    </main>
  );
}

function CreateTeamForm({ onCreated }: { onCreated: () => void }) {
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [onCallEmail, setOnCallEmail] = useState("");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    if (!name.trim()) return;

    setSaving(true);
    setError(null);
    try {
      await api.post("/teams", {
        name: name.trim(),
        description: description.trim() || undefined,
        on_call_email: onCallEmail.trim() || undefined,
      });
      onCreated();
    } catch (err) {
      if (err instanceof ApiError) {
        setError(err.message);
      } else {
        setError("Failed to create team");
      }
    } finally {
      setSaving(false);
    }
  }

  return (
    <Card className="mb-6 border-blue-500/30 bg-card/60 backdrop-blur-sm">
      <CardHeader>
        <CardTitle className="text-base">Create new team</CardTitle>
        <CardDescription>Teams group ownership of jobs, workflows, and on-call escalation.</CardDescription>
      </CardHeader>
      <CardContent>
        <form onSubmit={handleSubmit} className="space-y-4 max-w-xl">
          {error ? (
            <div className="p-3 text-sm rounded bg-rose-500/10 text-rose-500 border border-rose-500/20">
              {error}
            </div>
          ) : null}
          <div className="space-y-1">
            <Label htmlFor="team-name">Team Name *</Label>
            <Input
              id="team-name"
              placeholder="e.g. Core Infrastructure"
              value={name}
              onChange={(e) => setName(e.target.value)}
              required
            />
          </div>
          <div className="space-y-1">
            <Label htmlFor="team-desc">Description</Label>
            <Input
              id="team-desc"
              placeholder="Responsibilities or purpose"
              value={description}
              onChange={(e) => setDescription(e.target.value)}
            />
          </div>
          <div className="space-y-1">
            <Label htmlFor="team-oncall">On-Call Email</Label>
            <Input
              id="team-oncall"
              type="email"
              placeholder="oncall@acme.com"
              value={onCallEmail}
              onChange={(e) => setOnCallEmail(e.target.value)}
            />
          </div>
          <Button type="submit" disabled={saving || !name.trim()}>
            {saving ? <Loader2 className="mr-2 h-4 w-4 animate-spin" /> : null}
            Save Team
          </Button>
        </form>
      </CardContent>
    </Card>
  );
}
