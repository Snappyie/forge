"use client";

import { motion } from "framer-motion";
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from "@/components/ui/card";
import { Terminal } from "lucide-react";

export default function ExecutionsPage() {
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
          <p className="text-muted-foreground">View and monitor all execution runs.</p>
        </div>
      </header>

      <motion.div
        initial={{ opacity: 0, y: 20 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ delay: 0.1 }}
      >
        <Card className="bg-card/50 backdrop-blur-sm shadow-sm flex flex-col items-center justify-center p-24 border-dashed">
          <Terminal className="h-16 w-16 text-muted-foreground mb-4 opacity-50" />
          <h2 className="text-xl font-semibold text-foreground mb-2">Executions Filter API In Development</h2>
          <p className="text-muted-foreground max-w-md text-center">
            Advanced search and filtering for executions is currently being built in Phase 9 (Observability).
          </p>
        </Card>
      </motion.div>
    </main>
  );
}
