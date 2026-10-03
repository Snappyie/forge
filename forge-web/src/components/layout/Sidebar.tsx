"use client";

import { motion } from "framer-motion";
import { Button } from "@/components/ui/button";
import { Terminal, Activity, Box, Users, ServerCrash, Hexagon, ChevronDown, PlusCircle } from "lucide-react";
import Link from "next/link";
import { usePathname } from "next/navigation";
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuLabel, DropdownMenuSeparator, DropdownMenuTrigger, DropdownMenuGroup } from "@/components/ui/dropdown-menu";

export function Sidebar() {
  const pathname = usePathname();

  // Hide sidebar on auth routes
  if (pathname === '/login' || pathname === '/register') {
    return null;
  }

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
        className="font-bold text-3xl tracking-tight mb-6 bg-gradient-to-br from-indigo-500 via-purple-500 to-pink-500 bg-clip-text text-transparent px-4 py-2"
      >
        Forge
      </motion.div>

      {/* Tenant Switcher */}
      <div className="mb-6 px-1">
        <DropdownMenu>
          <DropdownMenuTrigger className="flex items-center w-full hover:bg-muted/50 rounded-md p-2 transition-colors outline-none border border-transparent hover:border-border">
            <div className="bg-indigo-600/20 p-2 rounded-lg border border-indigo-500/30">
              <Hexagon className="h-5 w-5 text-indigo-400" />
            </div>
            <div className="flex flex-col items-start ml-3">
              <span className="text-sm font-medium leading-none mb-1">Acme Corp</span>
              <span className="text-xs text-muted-foreground leading-none">Production</span>
            </div>
            <ChevronDown className="h-4 w-4 ml-auto text-muted-foreground" />
          </DropdownMenuTrigger>
          <DropdownMenuContent className="w-56" align="start">
            <DropdownMenuGroup>
              <DropdownMenuLabel>Workspaces</DropdownMenuLabel>
              <DropdownMenuSeparator />
              <DropdownMenuItem>
                <Hexagon className="mr-2 h-4 w-4 text-indigo-400" />
                <span>Acme Corp</span>
              </DropdownMenuItem>
              <DropdownMenuItem>
                <Hexagon className="mr-2 h-4 w-4 text-emerald-400" />
                <span>Personal Project</span>
              </DropdownMenuItem>
              <DropdownMenuSeparator />
              <DropdownMenuItem>
                <PlusCircle className="mr-2 h-4 w-4" />
                <span>Create Workspace</span>
              </DropdownMenuItem>
            </DropdownMenuGroup>
          </DropdownMenuContent>
        </DropdownMenu>
      </div>
      
      <nav className="space-y-1 flex-1">
        <Link href="/" passHref>
          <Button variant={getVariant("/")} className={getStyle("/")}>
            <Activity className="mr-2 h-4 w-4 text-indigo-500" />
            Dashboard
          </Button>
        </Link>
        <Link href="/jobs" passHref>
          <Button variant={getVariant("/jobs")} className={getStyle("/jobs")}>
            <Box className="mr-2 h-4 w-4 text-blue-500" />
            Jobs
          </Button>
        </Link>
        <Link href="/executions" passHref>
          <Button variant={getVariant("/executions")} className={getStyle("/executions")}>
            <Terminal className="mr-2 h-4 w-4 text-emerald-500" />
            Executions
          </Button>
        </Link>
        <Link href="/workers" passHref>
          <Button variant={getVariant("/workers")} className={getStyle("/workers")}>
            <ServerCrash className="mr-2 h-4 w-4 text-rose-500" />
            Workers
          </Button>
        </Link>
        <div className="pt-4 pb-1">
          <p className="px-4 text-xs font-semibold text-muted-foreground uppercase tracking-wider">Features</p>
        </div>
        <Link href="/alerts" passHref>
          <Button variant={getVariant("/alerts")} className={getStyle("/alerts")}>
            <Activity className="mr-2 h-4 w-4 text-orange-500" />
            Alerts
          </Button>
        </Link>
        <Link href="/calendar" passHref>
          <Button variant={getVariant("/calendar")} className={getStyle("/calendar")}>
            <Terminal className="mr-2 h-4 w-4 text-purple-500" />
            Calendar
          </Button>
        </Link>
        <Link href="/audit" passHref>
          <Button variant={getVariant("/audit")} className={getStyle("/audit")}>
            <Box className="mr-2 h-4 w-4 text-teal-500" />
            Audit Trail
          </Button>
        </Link>
      </nav>

      <div className="mt-auto px-4 py-4 border-t border-border/50">
        <Link href="/login" className="flex items-center space-x-3 group">
          <div className="h-8 w-8 rounded-full bg-gradient-to-tr from-indigo-500 to-purple-500 flex items-center justify-center text-xs font-bold text-white shadow-inner">
            ND
          </div>
          <div className="flex flex-col">
            <span className="text-sm font-medium text-foreground group-hover:text-indigo-400 transition-colors">Neel D.</span>
            <span className="text-xs text-muted-foreground">Sign out</span>
          </div>
        </Link>
      </div>
    </motion.aside>
  );
}
