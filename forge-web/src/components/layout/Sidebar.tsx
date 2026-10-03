"use client";

import { motion } from "framer-motion";
import { Button } from "@/components/ui/button";
import { Terminal, Activity, Box, Users, ServerCrash } from "lucide-react";
import Link from "next/link";
import { usePathname } from "next/navigation";

export function Sidebar() {
  const pathname = usePathname();

  const getVariant = (path: string) => pathname === path ? "secondary" : "ghost";
  const getStyle = (path: string) => pathname === path 
    ? "w-full justify-start transition-transform hover:scale-[1.02]" 
    : "w-full justify-start text-muted-foreground hover:text-foreground transition-transform hover:scale-[1.02]";

  return (
    <motion.aside 
      initial={{ x: -300 }}
      animate={{ x: 0 }}
      transition={{ type: "spring", stiffness: 300, damping: 30 }}
      className="w-64 border-r bg-card flex flex-col p-4 shadow-sm z-10 h-screen sticky top-0"
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
        <Link href="/" passHref>
          <Button variant={getVariant("/")} className={getStyle("/")}>
            <Activity className="mr-2 h-4 w-4" />
            Dashboard
          </Button>
        </Link>
        <Link href="/jobs" passHref>
          <Button variant={getVariant("/jobs")} className={getStyle("/jobs")}>
            <Box className="mr-2 h-4 w-4" />
            Jobs
          </Button>
        </Link>
        <Link href="/executions" passHref>
          <Button variant={getVariant("/executions")} className={getStyle("/executions")}>
            <Terminal className="mr-2 h-4 w-4" />
            Executions
          </Button>
        </Link>
        <Link href="/queues" passHref>
          <Button variant={getVariant("/queues")} className={getStyle("/queues")}>
            <ServerCrash className="mr-2 h-4 w-4" />
            Queues
          </Button>
        </Link>
        <Link href="/workers" passHref>
          <Button variant={getVariant("/workers")} className={getStyle("/workers")}>
            <Users className="mr-2 h-4 w-4" />
            Workers
          </Button>
        </Link>
      </nav>
    </motion.aside>
  );
}
