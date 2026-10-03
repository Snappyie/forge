"use client";

import { motion } from "framer-motion";
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from "@/components/ui/card";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { ServerCrash, Activity, Cpu, MemoryStick, Signal, Server, PowerOff } from "lucide-react";

export default function WorkersPage() {
  const workers = [
    { id: "wrk_prod_alpha", status: "Online", hostname: "ip-10-0-1-44.ec2", version: "v1.2.4", uptime: "14d 2h", cpu: "14%", mem: "42%" },
    { id: "wrk_prod_beta", status: "Online", hostname: "ip-10-0-1-105.ec2", version: "v1.2.4", uptime: "14d 2h", cpu: "28%", mem: "38%" },
    { id: "wrk_prod_gamma", status: "Online", hostname: "ip-10-0-2-18.ec2", version: "v1.2.4", uptime: "5d 11h", cpu: "64%", mem: "71%" },
    { id: "wrk_analytics_01", status: "Offline", hostname: "ip-10-0-3-99.ec2", version: "v1.2.3", uptime: "-", cpu: "-", mem: "-" },
  ];

  return (
    <main className="p-8 relative min-h-screen bg-[radial-gradient(ellipse_at_top_right,_var(--tw-gradient-stops))] from-rose-50/50 via-background to-background dark:from-rose-900/10 dark:via-background dark:to-background">
      <header className="flex justify-between items-center mb-8">
        <div>
          <motion.h1 
            initial={{ y: -20, opacity: 0 }}
            animate={{ y: 0, opacity: 1 }}
            className="text-4xl font-bold tracking-tight mb-2 text-foreground"
          >
            Worker Fleet
          </motion.h1>
          <p className="text-muted-foreground">Monitor cluster health and worker execution capacity.</p>
        </div>
        <Button className="bg-rose-600 hover:bg-rose-700 text-white">
          <ServerCrash className="mr-2 h-4 w-4" /> Provision Worker
        </Button>
      </header>

      <div className="grid grid-cols-1 md:grid-cols-3 gap-6 mb-8">
        <motion.div initial={{ opacity: 0, y: 20 }} animate={{ opacity: 1, y: 0 }} transition={{ delay: 0.1 }}>
          <Card className="bg-card/50 backdrop-blur-sm shadow-sm border-border/50">
            <CardContent className="p-6">
              <div className="flex justify-between items-start mb-4">
                <div className="bg-rose-100 text-rose-700 dark:bg-rose-500/20 dark:text-rose-400 p-2 rounded-lg">
                  <Activity className="w-5 h-5" />
                </div>
                <Badge className="bg-green-100 text-green-700 dark:bg-green-500/20 dark:text-green-400 shadow-none border-none animate-pulse">Healthy</Badge>
              </div>
              <h3 className="text-3xl font-bold tracking-tight">3 / 4</h3>
              <p className="text-sm text-muted-foreground mt-1">Active Workers Online</p>
            </CardContent>
          </Card>
        </motion.div>
        
        <motion.div initial={{ opacity: 0, y: 20 }} animate={{ opacity: 1, y: 0 }} transition={{ delay: 0.2 }}>
          <Card className="bg-card/50 backdrop-blur-sm shadow-sm border-border/50">
            <CardContent className="p-6">
              <div className="flex justify-between items-start mb-4">
                <div className="bg-blue-100 text-blue-700 dark:bg-blue-500/20 dark:text-blue-400 p-2 rounded-lg">
                  <Server className="w-5 h-5" />
                </div>
              </div>
              <h3 className="text-3xl font-bold tracking-tight">1,204</h3>
              <p className="text-sm text-muted-foreground mt-1">Jobs Processed (24h)</p>
            </CardContent>
          </Card>
        </motion.div>

        <motion.div initial={{ opacity: 0, y: 20 }} animate={{ opacity: 1, y: 0 }} transition={{ delay: 0.3 }}>
          <Card className="bg-card/50 backdrop-blur-sm shadow-sm border-border/50">
            <CardContent className="p-6">
              <div className="flex justify-between items-start mb-4">
                <div className="bg-amber-100 text-amber-700 dark:bg-amber-500/20 dark:text-amber-400 p-2 rounded-lg">
                  <Signal className="w-5 h-5" />
                </div>
                <span className="text-xs font-medium text-amber-600 dark:text-amber-400 flex items-center"><Activity className="w-3 h-3 mr-1" /> Load Spiking</span>
              </div>
              <h3 className="text-3xl font-bold tracking-tight">42</h3>
              <p className="text-sm text-muted-foreground mt-1">Queue Depth (Pending)</p>
            </CardContent>
          </Card>
        </motion.div>
      </div>

      <motion.div
        initial={{ opacity: 0, y: 20 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ delay: 0.4 }}
      >
        <Card className="bg-card/50 backdrop-blur-sm shadow-sm border-border/50">
          <CardHeader>
            <CardTitle>Worker Nodes</CardTitle>
            <CardDescription>
              Detailed health metrics for connected pollers.
            </CardDescription>
          </CardHeader>
          <CardContent>
            <div className="rounded-md border border-border/50">
              <Table>
                <TableHeader>
                  <TableRow className="hover:bg-transparent bg-muted/20">
                    <TableHead>Worker ID</TableHead>
                    <TableHead>Status</TableHead>
                    <TableHead>Hostname</TableHead>
                    <TableHead>Version</TableHead>
                    <TableHead>Uptime</TableHead>
                    <TableHead>Load</TableHead>
                    <TableHead className="text-right">Actions</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {workers.map((w) => (
                    <TableRow key={w.id} className="group">
                      <TableCell className="font-mono text-sm font-medium">{w.id}</TableCell>
                      <TableCell>
                        <Badge 
                          className={`shadow-none border-none ${
                            w.status === 'Online' ? 'bg-green-100 text-green-700 dark:bg-green-500/10 dark:text-green-400' :
                            'bg-gray-100 text-gray-700 dark:bg-gray-800 dark:text-gray-400'
                          }`}
                        >
                          {w.status === 'Online' && <span className="w-1.5 h-1.5 rounded-full bg-green-500 mr-2 animate-pulse" />}
                          {w.status}
                        </Badge>
                      </TableCell>
                      <TableCell className="text-muted-foreground">{w.hostname}</TableCell>
                      <TableCell className="font-mono text-xs">{w.version}</TableCell>
                      <TableCell className="text-muted-foreground">{w.uptime}</TableCell>
                      <TableCell>
                        <div className="flex flex-col gap-1 w-32">
                          <div className="flex items-center justify-between text-xs text-muted-foreground">
                            <span className="flex items-center"><Cpu className="w-3 h-3 mr-1"/> {w.cpu}</span>
                            <span className="flex items-center"><MemoryStick className="w-3 h-3 mr-1"/> {w.mem}</span>
                          </div>
                          {w.status === 'Online' && (
                            <div className="w-full bg-muted rounded-full h-1.5 overflow-hidden flex">
                              <div className="bg-indigo-500 h-full" style={{ width: w.cpu }} />
                            </div>
                          )}
                        </div>
                      </TableCell>
                      <TableCell className="text-right">
                        <Button variant="ghost" size="icon" className="opacity-0 group-hover:opacity-100 transition-opacity">
                          <PowerOff className="h-4 w-4 text-muted-foreground" />
                        </Button>
                      </TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
            </div>
          </CardContent>
        </Card>
      </motion.div>
    </main>
  );
}
