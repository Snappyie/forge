"use client";

import { useEffect, useState } from "react";
import { motion } from "framer-motion";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { Badge } from "@/components/ui/badge";
import { Terminal, Activity, Box, Users, PlusCircle, ServerCrash } from "lucide-react";

export default function Home() {
  const [isKeyboardHintVisible, setKeyboardHintVisible] = useState(true);

  // Keyboard shortcut listener
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      // CMD/CTRL + J to create a job
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'j') {
        e.preventDefault();
        alert("Keyboard shortcut triggered: Create New Job");
      }
    };
    
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, []);

  return (
    <div className="flex h-screen bg-background text-foreground overflow-hidden font-sans">
      {/* Sidebar Mockup with Framer Motion slide-in */}
      <motion.aside 
        initial={{ x: -300 }}
        animate={{ x: 0 }}
        transition={{ type: "spring", stiffness: 300, damping: 30 }}
        className="w-64 border-r bg-card flex flex-col p-4 shadow-sm z-10"
      >
        <motion.div 
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          transition={{ delay: 0.2 }}
          className="font-bold text-3xl tracking-tight mb-8 bg-gradient-to-br from-indigo-500 via-purple-500 to-pink-500 bg-clip-text text-transparent px-4 py-2"
        >
          Forge
        </motion.div>
        
        <nav className="space-y-1 flex-1">
          <Button variant="secondary" className="w-full justify-start transition-transform hover:scale-[1.02]">
            <Activity className="mr-2 h-4 w-4" />
            Dashboard
          </Button>
          <Button variant="ghost" className="w-full justify-start text-muted-foreground hover:text-foreground transition-transform hover:scale-[1.02]">
            <Box className="mr-2 h-4 w-4" />
            Jobs
          </Button>
          <Button variant="ghost" className="w-full justify-start text-muted-foreground hover:text-foreground transition-transform hover:scale-[1.02]">
            <Terminal className="mr-2 h-4 w-4" />
            Executions
          </Button>
          <Button variant="ghost" className="w-full justify-start text-muted-foreground hover:text-foreground transition-transform hover:scale-[1.02]">
            <ServerCrash className="mr-2 h-4 w-4" />
            Queues
          </Button>
          <Button variant="ghost" className="w-full justify-start text-muted-foreground hover:text-foreground transition-transform hover:scale-[1.02]">
            <Users className="mr-2 h-4 w-4" />
            Workers
          </Button>
        </nav>
      </motion.aside>

      {/* Main Content */}
      <main className="flex-1 p-8 overflow-y-auto relative bg-[radial-gradient(ellipse_at_top_right,_var(--tw-gradient-stops))] from-indigo-50/50 via-background to-background dark:from-indigo-900/10 dark:via-background dark:to-background">
        <header className="flex justify-between items-center mb-8">
          <div>
            <motion.h1 
              initial={{ y: -20, opacity: 0 }}
              animate={{ y: 0, opacity: 1 }}
              className="text-4xl font-bold tracking-tight mb-2 text-foreground"
            >
              Dashboard Overview
            </motion.h1>
            <p className="text-muted-foreground">Welcome to the orchestration engine.</p>
          </div>
          
          <motion.div whileHover={{ scale: 1.05 }} whileTap={{ scale: 0.95 }}>
            <Button className="bg-indigo-600 text-white hover:bg-indigo-700 shadow-lg shadow-indigo-500/25 transition-all">
              <PlusCircle className="mr-2 h-4 w-4" />
              Create New Job
            </Button>
          </motion.div>
        </header>

        <motion.div 
          initial={{ opacity: 0, y: 20 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ delay: 0.1 }}
          className="grid grid-cols-3 gap-6 mb-8"
        >
          {/* Animated Stats Cards */}
          <Card className="bg-card/50 backdrop-blur-sm hover:border-indigo-500/50 transition-colors duration-300 shadow-sm">
            <CardHeader className="pb-2">
              <CardTitle className="text-muted-foreground text-sm font-medium">Active Jobs</CardTitle>
            </CardHeader>
            <CardContent>
              <div className="text-4xl font-black text-foreground">12</div>
            </CardContent>
          </Card>
          
          <Card className="bg-card/50 backdrop-blur-sm hover:border-indigo-500/50 transition-colors duration-300 shadow-sm">
            <CardHeader className="pb-2">
              <CardTitle className="text-muted-foreground text-sm font-medium">Running Executions</CardTitle>
            </CardHeader>
            <CardContent>
              <div className="text-4xl font-black text-foreground">4</div>
            </CardContent>
          </Card>
          
          <Card className="bg-card/50 backdrop-blur-sm hover:border-indigo-500/50 transition-colors duration-300 shadow-sm">
            <CardHeader className="pb-2">
              <CardTitle className="text-muted-foreground text-sm font-medium">Online Workers</CardTitle>
            </CardHeader>
            <CardContent>
              <div className="text-4xl font-black text-foreground">3</div>
            </CardContent>
          </Card>
        </motion.div>

        {/* Data Table */}
        <motion.div
          initial={{ opacity: 0, y: 20 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ delay: 0.2 }}
        >
          <Card className="bg-card/50 backdrop-blur-sm shadow-sm">
            <CardHeader>
              <CardTitle className="text-foreground flex items-center justify-between">
                Recent Executions
                {isKeyboardHintVisible && (
                  <Badge variant="outline" className="text-muted-foreground font-normal">
                    Tip: Press <kbd className="mx-1 bg-muted px-1.5 rounded text-xs font-mono text-foreground">⌘ + J</kbd> to create a job
                  </Badge>
                )}
              </CardTitle>
              <CardDescription className="text-muted-foreground">
                A live stream of the latest job runs across the platform.
              </CardDescription>
            </CardHeader>
            <CardContent>
              <Table>
                <TableHeader>
                  <TableRow className="hover:bg-transparent">
                    <TableHead className="text-muted-foreground font-medium">Execution ID</TableHead>
                    <TableHead className="text-muted-foreground font-medium">Job Name</TableHead>
                    <TableHead className="text-muted-foreground font-medium">Status</TableHead>
                    <TableHead className="text-muted-foreground font-medium text-right">Started</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  <TableRow className="transition-colors group hover:bg-muted/50 cursor-pointer">
                    <TableCell className="font-mono text-muted-foreground group-hover:text-foreground transition-colors">exec_1</TableCell>
                    <TableCell className="font-medium text-foreground transition-colors">Daily Database Backup</TableCell>
                    <TableCell><Badge className="bg-green-100 text-green-700 hover:bg-green-200 dark:bg-green-500/10 dark:text-green-400 dark:hover:bg-green-500/20 shadow-none border-none">Succeeded</Badge></TableCell>
                    <TableCell className="text-right text-muted-foreground">2 mins ago</TableCell>
                  </TableRow>
                  
                  <TableRow className="transition-colors group hover:bg-muted/50 cursor-pointer">
                    <TableCell className="font-mono text-muted-foreground group-hover:text-foreground transition-colors">exec_2</TableCell>
                    <TableCell className="font-medium text-foreground transition-colors">Send Newsletter Campaign</TableCell>
                    <TableCell>
                      <Badge className="bg-blue-100 text-blue-700 hover:bg-blue-200 dark:bg-blue-500/10 dark:text-blue-400 dark:hover:bg-blue-500/20 shadow-none animate-pulse border-none">
                        <span className="h-1.5 w-1.5 rounded-full bg-blue-500 dark:bg-blue-400 mr-2"></span>
                        Running
                      </Badge>
                    </TableCell>
                    <TableCell className="text-right text-muted-foreground">10 mins ago</TableCell>
                  </TableRow>
                  
                  <TableRow className="transition-colors group hover:bg-muted/50 cursor-pointer">
                    <TableCell className="font-mono text-muted-foreground group-hover:text-foreground transition-colors">exec_3</TableCell>
                    <TableCell className="font-medium text-foreground transition-colors">Clear Temporary Files</TableCell>
                    <TableCell><Badge className="bg-red-100 text-red-700 hover:bg-red-200 dark:bg-red-500/10 dark:text-red-400 dark:hover:bg-red-500/20 shadow-none border-none">Failed</Badge></TableCell>
                    <TableCell className="text-right text-muted-foreground">1 hour ago</TableCell>
                  </TableRow>
                </TableBody>
              </Table>
            </CardContent>
          </Card>
        </motion.div>
      </main>
    </div>
  );
}
