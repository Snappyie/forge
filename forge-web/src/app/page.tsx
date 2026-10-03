"use client";

import { motion } from "framer-motion";
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from "@/components/ui/card";
import { Activity, Box, ServerCrash, Users, Terminal, CheckCircle2, XCircle, Clock } from "lucide-react";
import { Skeleton } from "@/components/ui/skeleton";

export default function Dashboard() {
  const stats = [
    { name: "Active Executions", value: "1,204", icon: Activity, change: "+12.5%", trend: "up" },
    { name: "Success Rate", value: "99.8%", icon: CheckCircle2, change: "+0.2%", trend: "up" },
    { name: "Failed Jobs", value: "3", icon: XCircle, change: "-4", trend: "down" },
    { name: "Avg. Queue Time", value: "45ms", icon: Clock, change: "-12ms", trend: "down" },
  ];

  return (
    <main className="p-8 relative min-h-screen bg-[radial-gradient(ellipse_at_top_right,_var(--tw-gradient-stops))] from-indigo-50/50 via-background to-background dark:from-indigo-900/10 dark:via-background dark:to-background">
      <header className="mb-8">
        <motion.h1 
          initial={{ y: -20, opacity: 0 }}
          animate={{ y: 0, opacity: 1 }}
          className="text-4xl font-bold tracking-tight mb-2 text-foreground"
        >
          Overview
        </motion.h1>
        <p className="text-muted-foreground">Real-time pulse of your orchestration engine.</p>
      </header>

      <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-6 mb-8">
        {stats.map((stat, idx) => (
          <motion.div
            key={stat.name}
            initial={{ opacity: 0, y: 20 }}
            animate={{ opacity: 1, y: 0 }}
            transition={{ delay: idx * 0.1 }}
          >
            <Card className="bg-card/50 backdrop-blur-sm border-border/50 hover:border-indigo-500/50 transition-colors">
              <CardContent className="p-6">
                <div className="flex items-center justify-between space-y-0 pb-2">
                  <p className="text-sm font-medium text-muted-foreground">{stat.name}</p>
                  <stat.icon className="h-4 w-4 text-indigo-500" />
                </div>
                <div className="flex items-baseline space-x-2">
                  <h2 className="text-3xl font-bold tracking-tight">{stat.value}</h2>
                  <span className={`text-xs font-medium ${stat.trend === 'up' ? 'text-green-500' : 'text-rose-500'}`}>
                    {stat.change}
                  </span>
                </div>
              </CardContent>
            </Card>
          </motion.div>
        ))}
      </div>

      <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
        <motion.div
          initial={{ opacity: 0, scale: 0.95 }}
          animate={{ opacity: 1, scale: 1 }}
          transition={{ delay: 0.4 }}
          className="lg:col-span-2"
        >
          <Card className="bg-card/50 backdrop-blur-sm shadow-sm h-full">
            <CardHeader>
              <CardTitle>Execution Throughput</CardTitle>
              <CardDescription>Jobs processed per minute over the last 24 hours.</CardDescription>
            </CardHeader>
            <CardContent className="h-[300px] flex items-center justify-center flex-col space-y-4">
              {/* Mock Graph skeleton */}
              <div className="w-full flex items-end justify-between h-48 space-x-2">
                {[40, 70, 45, 90, 65, 85, 100, 60, 40, 50, 75, 80].map((h, i) => (
                  <motion.div 
                    key={i}
                    initial={{ height: 0 }}
                    animate={{ height: `${h}%` }}
                    transition={{ delay: 0.5 + (i * 0.05), duration: 0.5 }}
                    className="w-full bg-indigo-500/20 rounded-t-md hover:bg-indigo-500/40 transition-colors relative group cursor-pointer"
                  >
                    <div className="absolute -top-8 left-1/2 -translate-x-1/2 opacity-0 group-hover:opacity-100 transition-opacity bg-black text-white text-xs py-1 px-2 rounded">
                      {h}k
                    </div>
                  </motion.div>
                ))}
              </div>
            </CardContent>
          </Card>
        </motion.div>

        <motion.div
          initial={{ opacity: 0, scale: 0.95 }}
          animate={{ opacity: 1, scale: 1 }}
          transition={{ delay: 0.5 }}
        >
          <Card className="bg-card/50 backdrop-blur-sm shadow-sm h-full">
            <CardHeader>
              <CardTitle>Live Activity Feed</CardTitle>
            </CardHeader>
            <CardContent className="space-y-6">
              {[1, 2, 3, 4].map((i) => (
                <div key={i} className="flex items-start space-x-4">
                  <div className="w-2 h-2 mt-2 rounded-full bg-indigo-500 shrink-0" />
                  <div className="space-y-2 flex-1">
                    <Skeleton className="h-4 w-[90%]" />
                    <Skeleton className="h-3 w-[60%]" />
                  </div>
                </div>
              ))}
            </CardContent>
          </Card>
        </motion.div>
      </div>
    </main>
  );
}
