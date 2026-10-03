"use client";

import { motion } from "framer-motion";
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from "@/components/ui/card";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Users, PlusCircle, UserPlus, Settings2, ShieldAlert } from "lucide-react";
import { PowerfulFilterBar } from "@/components/ui/filter-bar";

export default function TeamsPage() {
  const teams = [
    { name: "Payments", members: 12, jobs: 45, alerts: 2, onCall: "sarah.smith@acme.com" },
    { name: "Data Engineering", members: 8, jobs: 132, alerts: 0, onCall: "alex.j@acme.com" },
    { name: "Platform", members: 5, jobs: 14, alerts: 1, onCall: "ops@acme.com" },
  ];

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
        <Button className="bg-blue-600 hover:bg-blue-700 text-white">
          <PlusCircle className="mr-2 h-4 w-4" /> Create Team
        </Button>
      </header>

      <motion.div initial={{ opacity: 0, y: 20 }} animate={{ opacity: 1, y: 0 }}>
        <Card className="bg-card/50 backdrop-blur-sm border-border/50">
          <div className="border-b p-2">
            <PowerfulFilterBar />
          </div>
          <CardContent className="p-0">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead className="pl-6">Team Name</TableHead>
                  <TableHead>Members</TableHead>
                  <TableHead>Owned Jobs</TableHead>
                  <TableHead>Active Alerts</TableHead>
                  <TableHead>Current On-Call</TableHead>
                  <TableHead className="text-right pr-6">Manage</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {teams.map((team) => (
                  <TableRow key={team.name} className="cursor-pointer hover:bg-muted/50 transition-colors">
                    <TableCell className="font-medium pl-6">{team.name}</TableCell>
                    <TableCell>
                      <div className="flex items-center gap-2">
                        <Users className="w-4 h-4 text-muted-foreground" /> {team.members}
                      </div>
                    </TableCell>
                    <TableCell className="text-muted-foreground">{team.jobs} jobs</TableCell>
                    <TableCell>
                      {team.alerts > 0 ? (
                        <Badge variant="outline" className="text-rose-500 border-rose-500/30 bg-rose-500/10">
                          {team.alerts} Active
                        </Badge>
                      ) : (
                        <Badge variant="outline" className="text-muted-foreground">0 Active</Badge>
                      )}
                    </TableCell>
                    <TableCell className="text-muted-foreground text-sm">{team.onCall}</TableCell>
                    <TableCell className="text-right pr-6">
                      <div className="flex justify-end gap-2">
                        <Button variant="ghost" size="icon"><UserPlus className="h-4 w-4 text-muted-foreground" /></Button>
                        <Button variant="ghost" size="icon"><Settings2 className="h-4 w-4 text-muted-foreground" /></Button>
                      </div>
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
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
                  <Badge variant="secondary">3 Users</Badge>
                </div>
                <div className="flex justify-between items-center p-3 border rounded-md">
                  <div>
                    <div className="font-medium">Viewer</div>
                    <div className="text-xs text-muted-foreground">Read-only access to executions and jobs.</div>
                  </div>
                  <Badge variant="secondary">142 Users</Badge>
                </div>
              </div>
            </CardContent>
          </Card>
        </motion.div>
      </div>
    </main>
  );
}
