"use client";

import { useState } from "react";
import { motion } from "framer-motion";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Calendar as CalendarIcon, ChevronLeft, ChevronRight, Filter } from "lucide-react";
import { ResourceShell } from "@/components/ui/resource-shell";

export default function CalendarPage() {
  const [view, setView] = useState<"month" | "week" | "day">("month");
  
  // Mock calendar data
  const days = Array.from({ length: 35 }, (_, i) => {
    const isCurrentMonth = i >= 4 && i <= 34; // just mock layout
    const day = (i - 3) % 31 + 1;
    const hasJobs = isCurrentMonth && [2, 5, 12, 14, 18, 25, 29].includes(day);
    const hasSpike = isCurrentMonth && [12, 29].includes(day);
    
    return {
      day,
      isCurrentMonth,
      hasJobs,
      hasSpike,
      jobCount: hasJobs ? Math.floor(Math.random() * 8) + 1 : 0
    };
  });

  return (
    <ResourceShell
      title="Execution Calendar"
      subtitle="Monthly/weekly visualization of scheduled jobs and load spikes."
      actions={
        <div className="flex gap-2">
          <div className="flex bg-muted rounded-md p-1">
            <button onClick={() => setView("month")} className={`px-3 py-1.5 text-xs font-medium rounded-sm ${view === 'month' ? 'bg-background shadow-sm text-foreground' : 'text-muted-foreground'}`}>Month</button>
            <button onClick={() => setView("week")} className={`px-3 py-1.5 text-xs font-medium rounded-sm ${view === 'week' ? 'bg-background shadow-sm text-foreground' : 'text-muted-foreground'}`}>Week</button>
            <button onClick={() => setView("day")} className={`px-3 py-1.5 text-xs font-medium rounded-sm ${view === 'day' ? 'bg-background shadow-sm text-foreground' : 'text-muted-foreground'}`}>Day</button>
          </div>
          <Button variant="outline"><Filter className="w-4 h-4 mr-2" /> Filter</Button>
        </div>
      }
    >
      <div className="flex flex-col h-[calc(100vh-140px)]">
        <div className="flex items-center justify-between mb-4">
          <h2 className="text-xl font-semibold flex items-center gap-2">
            <CalendarIcon className="w-5 h-5 text-indigo-500" />
            October 2026
          </h2>
          <div className="flex items-center gap-2">
            <Button variant="outline" size="icon" className="h-8 w-8"><ChevronLeft className="w-4 h-4" /></Button>
            <Button variant="outline" size="sm" className="h-8">Today</Button>
            <Button variant="outline" size="icon" className="h-8 w-8"><ChevronRight className="w-4 h-4" /></Button>
          </div>
        </div>

        <Card className="flex-1 flex flex-col border-border/50 shadow-sm overflow-hidden">
          <div className="grid grid-cols-7 border-b border-border/50 bg-muted/20">
            {['Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun'].map(day => (
              <div key={day} className="py-2 text-center text-sm font-medium text-muted-foreground">
                {day}
              </div>
            ))}
          </div>
          <div className="grid grid-cols-7 flex-1">
            {days.map((d, i) => (
              <div key={i} className={`border-r border-b border-border/50 p-2 flex flex-col ${d.isCurrentMonth ? 'bg-background' : 'bg-muted/10'} hover:bg-muted/30 transition-colors`}>
                <span className={`text-sm font-medium w-6 h-6 flex items-center justify-center rounded-full mb-1 ${!d.isCurrentMonth ? 'text-muted-foreground/50' : d.day === 14 ? 'bg-indigo-600 text-white' : 'text-foreground'}`}>
                  {d.day}
                </span>
                <div className="flex-1 flex flex-col gap-1 overflow-y-auto">
                  {d.hasSpike && (
                    <Badge variant="destructive" className="w-fit text-[10px] px-1 py-0 h-4">
                      ⚠ Load Spike
                    </Badge>
                  )}
                  {d.hasJobs && (
                    <div className="bg-indigo-100 dark:bg-indigo-900/30 text-indigo-700 dark:text-indigo-300 text-[10px] px-1.5 py-0.5 rounded truncate">
                      {d.jobCount} scheduled runs
                    </div>
                  )}
                </div>
              </div>
            ))}
          </div>
        </Card>
      </div>
    </ResourceShell>
  );
}
