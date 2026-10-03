"use client";

import { motion } from "framer-motion";
import { useRouter } from "next/navigation";
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from "@/components/ui/card";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { Badge } from "@/components/ui/badge";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuLabel, DropdownMenuSeparator, DropdownMenuTrigger, DropdownMenuGroup } from "@/components/ui/dropdown-menu";
import { Button } from "@/components/ui/button";
import { MoreHorizontal, Play, XCircle, RotateCcw, GitCompare } from "lucide-react";
import { PowerfulFilterBar } from "@/components/ui/filter-bar";
import { useToast } from "@/hooks/use-toast";
import Link from "next/link";

export default function ExecutionsPage() {
  const router = useRouter();
  const { toast } = useToast();

  const executions = [
    { id: "1a2b3c", job: "Data Pipeline Etl", status: "Running", time: "2m 14s", user: "system" },
    { id: "9f8e7d", job: "Weekly Report Gen", status: "Failed", time: "45s", user: "neel@example.com" },
    { id: "5x4y3z", job: "Image Resize Batch", status: "Completed", time: "12ms", user: "system" },
    { id: "11a22b", job: "Data Pipeline Etl", status: "Completed", time: "4m 02s", user: "system" },
  ];

  return (
    <main className="p-8 relative min-h-screen bg-[radial-gradient(ellipse_at_top_right,_var(--tw-gradient-stops))] from-indigo-50/50 via-background to-background dark:from-indigo-900/10 dark:via-background dark:to-background">
      <header className="flex justify-between items-center mb-8">
        <div>
          <motion.h1 
            initial={{ y: -20, opacity: 0 }}
            animate={{ y: 0, opacity: 1 }}
            className="text-4xl font-bold tracking-tight mb-2 text-foreground"
          >
            Executions
          </motion.h1>
          <p className="text-muted-foreground">Monitor and control active workflow traces.</p>
        </div>
        <div className="flex gap-2">
          <Link href="/executions/compare" passHref>
            <Button variant="outline" className="border-indigo-200 dark:border-indigo-900/50 text-indigo-700 dark:text-indigo-400 bg-indigo-50 dark:bg-indigo-900/20">
              <GitCompare className="w-4 h-4 mr-2" /> Compare Executions
            </Button>
          </Link>
          <Button variant="outline">Export Logs</Button>
        </div>
      </header>

      <PowerfulFilterBar />

      <motion.div
        initial={{ opacity: 0, y: 20 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ delay: 0.1 }}
      >
        <Card className="bg-card/50 backdrop-blur-sm shadow-sm border-border/50">
          <CardHeader>
            <div className="flex items-center justify-between">
              <div>
                <CardTitle className="text-foreground">Execution History</CardTitle>
                <CardDescription className="text-muted-foreground">
                  A comprehensive audit trail of all triggered jobs in this tenant.
                </CardDescription>
              </div>
            </div>
          </CardHeader>
          <CardContent>
            <Tabs defaultValue="all" className="w-full">
              <TabsList className="mb-4">
                <TabsTrigger value="all">All</TabsTrigger>
                <TabsTrigger value="running">Running</TabsTrigger>
                <TabsTrigger value="failed">Failed</TabsTrigger>
                <TabsTrigger value="completed">Completed</TabsTrigger>
              </TabsList>
              
              <TabsContent value="all">
                <div className="rounded-md border border-border/50">
                  <Table>
                    <TableHeader>
                      <TableRow className="hover:bg-transparent bg-muted/20">
                        <TableHead className="font-medium text-muted-foreground">Execution ID</TableHead>
                        <TableHead className="font-medium text-muted-foreground">Job Name</TableHead>
                        <TableHead className="font-medium text-muted-foreground">Triggered By</TableHead>
                        <TableHead className="font-medium text-muted-foreground">Duration</TableHead>
                        <TableHead className="font-medium text-muted-foreground">Status</TableHead>
                        <TableHead className="text-right"></TableHead>
                      </TableRow>
                    </TableHeader>
                    <TableBody>
                      {executions.map((exec) => (
                        <TableRow 
                          key={exec.id} 
                          className="transition-colors group hover:bg-muted/50 cursor-pointer"
                          onClick={() => router.push(`/executions/${exec.id}`)}
                        >
                          <TableCell className="font-mono text-xs text-muted-foreground group-hover:text-foreground transition-colors">
                            exec_{exec.id}
                          </TableCell>
                          <TableCell className="font-medium">{exec.job}</TableCell>
                          <TableCell className="text-muted-foreground text-sm">{exec.user}</TableCell>
                          <TableCell className="text-muted-foreground text-sm">{exec.time}</TableCell>
                          <TableCell>
                            <Badge 
                              className={`shadow-none border-none ${
                                exec.status === 'Running' ? 'bg-blue-100 text-blue-700 dark:bg-blue-500/10 dark:text-blue-400' :
                                exec.status === 'Completed' ? 'bg-green-100 text-green-700 dark:bg-green-500/10 dark:text-green-400' :
                                'bg-rose-100 text-rose-700 dark:bg-rose-500/10 dark:text-rose-400'
                              }`}
                            >
                              {exec.status}
                            </Badge>
                          </TableCell>
                          <TableCell className="text-right">
                            <DropdownMenu>
                              <DropdownMenuTrigger asChild>
                                <Button variant="ghost" className="h-8 w-8 p-0 opacity-0 group-hover:opacity-100 transition-opacity">
                                  <span className="sr-only">Open menu</span>
                                  <MoreHorizontal className="h-4 w-4" />
                                </Button>
                              </DropdownMenuTrigger>
                              <DropdownMenuContent align="end">
                                <DropdownMenuGroup>
                                  <DropdownMenuLabel>Actions</DropdownMenuLabel>
                                  <DropdownMenuItem onClick={() => router.push(`/executions/${exec.id}`)}><Play className="mr-2 h-4 w-4" /> View Trace</DropdownMenuItem>
                                  <DropdownMenuItem onClick={(e) => { e.stopPropagation(); toast({ title: "Retrying", description: `Queued execution ${exec.id} for retry.` }); }}><RotateCcw className="mr-2 h-4 w-4" /> Retry Execution</DropdownMenuItem>
                                  <DropdownMenuSeparator />
                                  <DropdownMenuItem 
                                    className="text-rose-600 dark:text-rose-400"
                                    onClick={(e) => { e.stopPropagation(); toast({ title: "Cancelled", description: `Execution ${exec.id} was cancelled.`, variant: "destructive" }); }}
                                  >
                                    <XCircle className="mr-2 h-4 w-4" /> Cancel
                                  </DropdownMenuItem>
                                </DropdownMenuGroup>
                              </DropdownMenuContent>
                            </DropdownMenu>
                          </TableCell>
                        </TableRow>
                      ))}
                    </TableBody>
                  </Table>
                </div>
              </TabsContent>
            </Tabs>
          </CardContent>
        </Card>
      </motion.div>
    </main>
  );
}
