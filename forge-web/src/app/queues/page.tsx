"use client";

import { motion } from "framer-motion";
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from "@/components/ui/card";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { Badge } from "@/components/ui/badge";

export default function QueuesPage() {
  return (
    <main className="p-8 relative min-h-screen bg-[radial-gradient(ellipse_at_top_right,_var(--tw-gradient-stops))] from-indigo-50/50 via-background to-background dark:from-indigo-900/10 dark:via-background dark:to-background">
      <header className="flex justify-between items-center mb-8">
        <div>
          <motion.h1 
            initial={{ y: -20, opacity: 0 }}
            animate={{ y: 0, opacity: 1 }}
            className="text-4xl font-bold tracking-tight mb-2 text-foreground"
          >
            Queues
          </motion.h1>
          <p className="text-muted-foreground">Monitor and manage priority execution queues.</p>
        </div>
      </header>

      <motion.div
        initial={{ opacity: 0, y: 20 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ delay: 0.1 }}
      >
        <Card className="bg-card/50 backdrop-blur-sm shadow-sm">
          <CardHeader>
            <CardTitle className="text-foreground">System Queues</CardTitle>
            <CardDescription className="text-muted-foreground">
              Current load and depth across different priority bands.
            </CardDescription>
          </CardHeader>
          <CardContent>
            <Table>
              <TableHeader>
                <TableRow className="hover:bg-transparent">
                  <TableHead className="text-muted-foreground font-medium">Queue Name</TableHead>
                  <TableHead className="text-muted-foreground font-medium">Pending Items</TableHead>
                  <TableHead className="text-muted-foreground font-medium">Processing Rate</TableHead>
                  <TableHead className="text-muted-foreground font-medium text-right">Status</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                <TableRow className="transition-colors group hover:bg-muted/50 cursor-pointer">
                  <TableCell className="font-medium text-foreground transition-colors">default</TableCell>
                  <TableCell className="font-mono text-muted-foreground group-hover:text-foreground transition-colors">0</TableCell>
                  <TableCell className="text-muted-foreground">12/sec</TableCell>
                  <TableCell className="text-right"><Badge className="bg-green-100 text-green-700 hover:bg-green-200 dark:bg-green-500/10 dark:text-green-400 dark:hover:bg-green-500/20 shadow-none border-none">Healthy</Badge></TableCell>
                </TableRow>
                
                <TableRow className="transition-colors group hover:bg-muted/50 cursor-pointer">
                  <TableCell className="font-medium text-foreground transition-colors">high-priority</TableCell>
                  <TableCell className="font-mono text-muted-foreground group-hover:text-foreground transition-colors">4</TableCell>
                  <TableCell className="text-muted-foreground">45/sec</TableCell>
                  <TableCell className="text-right"><Badge className="bg-green-100 text-green-700 hover:bg-green-200 dark:bg-green-500/10 dark:text-green-400 dark:hover:bg-green-500/20 shadow-none border-none">Healthy</Badge></TableCell>
                </TableRow>
              </TableBody>
            </Table>
          </CardContent>
        </Card>
      </motion.div>
    </main>
  );
}
