"use client";

import { motion } from "framer-motion";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from "@/components/ui/card";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { Badge } from "@/components/ui/badge";
import { PlusCircle } from "lucide-react";

export default function JobsPage() {
  return (
    <main className="p-8 relative min-h-screen bg-[radial-gradient(ellipse_at_top_right,_var(--tw-gradient-stops))] from-indigo-50/50 via-background to-background dark:from-indigo-900/10 dark:via-background dark:to-background">
      <header className="flex justify-between items-center mb-8">
        <div>
          <motion.h1 
            initial={{ y: -20, opacity: 0 }}
            animate={{ y: 0, opacity: 1 }}
            className="text-4xl font-bold tracking-tight mb-2 text-foreground"
          >
            Jobs
          </motion.h1>
          <p className="text-muted-foreground">Manage and orchestrate job definitions.</p>
        </div>
        
        <motion.div whileHover={{ scale: 1.05 }} whileTap={{ scale: 0.95 }}>
          <Button className="bg-indigo-600 text-white hover:bg-indigo-700 shadow-lg shadow-indigo-500/25 transition-all">
            <PlusCircle className="mr-2 h-4 w-4" />
            Create Job
          </Button>
        </motion.div>
      </header>

      <motion.div
        initial={{ opacity: 0, y: 20 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ delay: 0.1 }}
      >
        <Card className="bg-card/50 backdrop-blur-sm shadow-sm">
          <CardHeader>
            <CardTitle className="text-foreground">All Jobs</CardTitle>
            <CardDescription className="text-muted-foreground">
              A complete list of registered job definitions.
            </CardDescription>
          </CardHeader>
          <CardContent>
            <Table>
              <TableHeader>
                <TableRow className="hover:bg-transparent">
                  <TableHead className="text-muted-foreground font-medium">Job Name</TableHead>
                  <TableHead className="text-muted-foreground font-medium">Version</TableHead>
                  <TableHead className="text-muted-foreground font-medium">Status</TableHead>
                  <TableHead className="text-muted-foreground font-medium text-right">Last Updated</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                <TableRow className="transition-colors group hover:bg-muted/50 cursor-pointer">
                  <TableCell className="font-medium text-foreground transition-colors">Daily Database Backup</TableCell>
                  <TableCell className="font-mono text-muted-foreground group-hover:text-foreground transition-colors">v2</TableCell>
                  <TableCell><Badge className="bg-green-100 text-green-700 hover:bg-green-200 dark:bg-green-500/10 dark:text-green-400 dark:hover:bg-green-500/20 shadow-none border-none">Active</Badge></TableCell>
                  <TableCell className="text-right text-muted-foreground">2 hours ago</TableCell>
                </TableRow>
                
                <TableRow className="transition-colors group hover:bg-muted/50 cursor-pointer">
                  <TableCell className="font-medium text-foreground transition-colors">Send Newsletter Campaign</TableCell>
                  <TableCell className="font-mono text-muted-foreground group-hover:text-foreground transition-colors">v1</TableCell>
                  <TableCell><Badge className="bg-green-100 text-green-700 hover:bg-green-200 dark:bg-green-500/10 dark:text-green-400 dark:hover:bg-green-500/20 shadow-none border-none">Active</Badge></TableCell>
                  <TableCell className="text-right text-muted-foreground">3 days ago</TableCell>
                </TableRow>
                
                <TableRow className="transition-colors group hover:bg-muted/50 cursor-pointer">
                  <TableCell className="font-medium text-foreground transition-colors">Legacy Data Migration</TableCell>
                  <TableCell className="font-mono text-muted-foreground group-hover:text-foreground transition-colors">v1</TableCell>
                  <TableCell><Badge variant="outline" className="text-muted-foreground">Archived</Badge></TableCell>
                  <TableCell className="text-right text-muted-foreground">1 year ago</TableCell>
                </TableRow>
              </TableBody>
            </Table>
          </CardContent>
        </Card>
      </motion.div>
    </main>
  );
}
