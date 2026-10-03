"use client";

import { motion } from "framer-motion";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { Badge } from "@/components/ui/badge";
import { GitMerge, PlusCircle, MoreHorizontal, Play } from "lucide-react";
import Link from "next/link";
import { PowerfulFilterBar } from "@/components/ui/filter-bar";
import { useToast } from "@/hooks/use-toast";

export default function WorkflowsPage() {
  const { toast } = useToast();
  const workflows = [
    { id: "wf-7721", name: "End-of-Month Settlement", nodes: 14, status: "Active", lastRun: "2 hours ago", owner: "Finance" },
    { id: "wf-3392", name: "Data Warehouse Sync", nodes: 5, status: "Active", lastRun: "10 mins ago", owner: "Data Eng" },
    { id: "wf-1120", name: "Onboarding Sequence", nodes: 8, status: "Paused", lastRun: "3 days ago", owner: "Product" },
  ];

  return (
    <main className="p-8 relative min-h-screen bg-[radial-gradient(ellipse_at_top_right,_var(--tw-gradient-stops))] from-pink-50/50 via-background to-background dark:from-pink-900/10 dark:via-background dark:to-background">
      <header className="flex justify-between items-center mb-8">
        <div>
          <motion.h1 
            initial={{ y: -20, opacity: 0 }}
            animate={{ y: 0, opacity: 1 }}
            className="text-4xl font-bold tracking-tight mb-2 text-foreground flex items-center gap-3"
          >
            <GitMerge className="h-8 w-8 text-pink-500" /> Workflows
          </motion.h1>
          <p className="text-muted-foreground">Design and manage complex job dependencies (DAGs).</p>
        </div>
        <Button className="bg-pink-600 hover:bg-pink-700 text-white">
          <PlusCircle className="mr-2 h-4 w-4" /> Create Workflow
        </Button>
      </header>

      <motion.div initial={{ opacity: 0, y: 20 }} animate={{ opacity: 1, y: 0 }} transition={{ delay: 0.1 }}>
        <Card className="bg-card/50 backdrop-blur-sm border-border/50">
          <div className="border-b p-2">
            <PowerfulFilterBar />
          </div>
          <CardContent className="p-0">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead className="pl-6">Workflow Name</TableHead>
                  <TableHead>Status</TableHead>
                  <TableHead>Complexity</TableHead>
                  <TableHead>Last Execution</TableHead>
                  <TableHead>Owner</TableHead>
                  <TableHead className="text-right pr-6">Actions</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {workflows.map((wf) => (
                  <TableRow key={wf.id} className="cursor-pointer hover:bg-muted/50 group transition-colors">
                    <TableCell className="font-medium pl-6">
                      <Link href={`/workflows/${wf.id}`} className="hover:underline flex items-center gap-2">
                        {wf.name}
                        <span className="text-xs text-muted-foreground font-mono">({wf.id})</span>
                      </Link>
                    </TableCell>
                    <TableCell>
                      <Badge variant="outline" className={wf.status === 'Active' ? 'text-green-600 border-green-600/30 bg-green-500/10' : 'text-amber-600 border-amber-600/30 bg-amber-500/10'}>
                        {wf.status === 'Active' && <span className="w-1.5 h-1.5 rounded-full bg-green-500 mr-2 animate-pulse" />}
                        {wf.status}
                      </Badge>
                    </TableCell>
                    <TableCell className="text-muted-foreground">{wf.nodes} nodes</TableCell>
                    <TableCell className="text-muted-foreground">{wf.lastRun}</TableCell>
                    <TableCell>{wf.owner}</TableCell>
                    <TableCell className="text-right pr-6">
                      <div className="flex justify-end gap-2">
                        <Button variant="ghost" size="icon" className="opacity-0 group-hover:opacity-100 transition-opacity">
                          <Play className="h-4 w-4" />
                        </Button>
                        <Button variant="ghost" size="icon" className="opacity-0 group-hover:opacity-100 transition-opacity">
                          <MoreHorizontal className="h-4 w-4" />
                        </Button>
                      </div>
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </CardContent>
        </Card>
      </motion.div>
    </main>
  );
}
