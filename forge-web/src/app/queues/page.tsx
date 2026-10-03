"use client";

import { motion } from "framer-motion";
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from "@/components/ui/card";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { Activity, LayoutList, Layers } from "lucide-react";

export default function QueuesPage() {
  const queues = [
    { name: "Critical", depth: 0, oldest: "—", throughput: "200/min" },
    { name: "Normal", depth: 31, oldest: "12s", throughput: "80/min" },
    { name: "Low", depth: 82, oldest: "3m", throughput: "20/min" },
  ];

  return (
    <main className="p-8 relative min-h-screen bg-[radial-gradient(ellipse_at_top_right,_var(--tw-gradient-stops))] from-orange-50/50 via-background to-background dark:from-orange-900/10 dark:via-background dark:to-background">
      <header className="mb-8">
        <motion.h1 
          initial={{ y: -20, opacity: 0 }}
          animate={{ y: 0, opacity: 1 }}
          className="text-4xl font-bold tracking-tight mb-2 text-foreground flex items-center gap-3"
        >
          <Layers className="h-8 w-8 text-orange-500" /> Queue Management
        </motion.h1>
        <p className="text-muted-foreground">Monitor processing throughput and queue depth.</p>
      </header>

      <motion.div initial={{ opacity: 0, y: 20 }} animate={{ opacity: 1, y: 0 }} className="mb-8">
        <Card className="bg-card/50 backdrop-blur-sm border-border/50">
          <CardHeader>
            <CardTitle>Active Queues</CardTitle>
            <CardDescription>Current backlog across priority tiers.</CardDescription>
          </CardHeader>
          <CardContent>
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Queue</TableHead>
                  <TableHead>Depth</TableHead>
                  <TableHead>Oldest Message</TableHead>
                  <TableHead>Throughput</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {queues.map((q) => (
                  <TableRow key={q.name}>
                    <TableCell className="font-medium">{q.name}</TableCell>
                    <TableCell>
                      <span className={`px-2 py-1 rounded-full text-xs font-medium ${q.depth > 50 ? 'bg-rose-100 text-rose-700 dark:bg-rose-900/30 dark:text-rose-400' : 'bg-muted text-foreground'}`}>
                        {q.depth}
                      </span>
                    </TableCell>
                    <TableCell className="text-muted-foreground">{q.oldest}</TableCell>
                    <TableCell className="font-mono text-xs">{q.throughput}</TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </CardContent>
        </Card>
      </motion.div>

      <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
        <motion.div initial={{ opacity: 0, y: 20 }} animate={{ opacity: 1, y: 0 }} transition={{ delay: 0.1 }}>
          <Card className="bg-card/50 backdrop-blur-sm h-full border-border/50">
            <CardHeader>
              <CardTitle>Queue Depth Over Time</CardTitle>
            </CardHeader>
            <CardContent className="h-48 flex items-center justify-center border-t border-border/50 bg-muted/10">
              <span className="text-muted-foreground text-sm flex items-center gap-2">
                <Activity className="h-4 w-4" /> Real-time depth chart initialized
              </span>
            </CardContent>
          </Card>
        </motion.div>

        <motion.div initial={{ opacity: 0, y: 20 }} animate={{ opacity: 1, y: 0 }} transition={{ delay: 0.2 }}>
          <Card className="bg-card/50 backdrop-blur-sm h-full border-border/50">
            <CardHeader>
              <CardTitle>Arrival vs Processing Rate</CardTitle>
            </CardHeader>
            <CardContent className="h-48 flex items-center justify-center border-t border-border/50 bg-muted/10">
              <span className="text-muted-foreground text-sm flex items-center gap-2">
                <LayoutList className="h-4 w-4" /> Throughput analysis active
              </span>
            </CardContent>
          </Card>
        </motion.div>
      </div>
    </main>
  );
}
