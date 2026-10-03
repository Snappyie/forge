import { useState } from "react";
import { Label } from "@/components/ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Input } from "@/components/ui/input";
import { Clock } from "lucide-react";

export function ScheduleBuilder() {
  const [mode, setMode] = useState<"simple" | "advanced">("simple");

  return (
    <div className="space-y-4 bg-muted/30 p-4 rounded-md border border-border/50">
      <div className="flex justify-between items-center mb-4">
        <h4 className="text-sm font-semibold flex items-center gap-2"><Clock className="w-4 h-4"/> Schedule Configuration</h4>
        <div className="flex bg-muted rounded-md p-1">
          <button 
            type="button"
            onClick={() => setMode("simple")} 
            className={`px-3 py-1 text-xs font-medium rounded-sm ${mode === 'simple' ? 'bg-background shadow-sm' : 'text-muted-foreground'}`}
          >
            Simple
          </button>
          <button 
            type="button"
            onClick={() => setMode("advanced")} 
            className={`px-3 py-1 text-xs font-medium rounded-sm ${mode === 'advanced' ? 'bg-background shadow-sm' : 'text-muted-foreground'}`}
          >
            Cron
          </button>
        </div>
      </div>

      {mode === "simple" ? (
        <div className="grid grid-cols-2 gap-4">
          <div className="space-y-2">
            <Label>Run</Label>
            <Select defaultValue="every_day">
              <SelectTrigger>
                <SelectValue placeholder="Select frequency" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="every_hour">Every Hour</SelectItem>
                <SelectItem value="every_day">Every Day</SelectItem>
                <SelectItem value="weekdays">Weekdays (Mon-Fri)</SelectItem>
                <SelectItem value="weekly">Weekly</SelectItem>
                <SelectItem value="monthly">Monthly</SelectItem>
              </SelectContent>
            </Select>
          </div>
          <div className="space-y-2">
            <Label>At</Label>
            <Input type="time" defaultValue="02:00" className="w-full" />
          </div>
        </div>
      ) : (
        <div className="space-y-2">
          <Label>Cron Expression</Label>
          <Input defaultValue="0 2 * * *" className="font-mono bg-muted text-foreground" />
          <p className="text-xs text-muted-foreground mt-1">Runs every day at 2:00 AM.</p>
        </div>
      )}

      <div className="space-y-2 pt-2 border-t border-border/50">
        <Label>Timezone</Label>
        <Select defaultValue="UTC">
          <SelectTrigger>
            <SelectValue placeholder="Select timezone" />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="UTC">UTC (Universal Coordinated Time)</SelectItem>
            <SelectItem value="America/New_York">America/New_York (EST)</SelectItem>
            <SelectItem value="America/Los_Angeles">America/Los_Angeles (PST)</SelectItem>
            <SelectItem value="Asia/Kolkata">Asia/Kolkata (IST)</SelectItem>
            <SelectItem value="Europe/London">Europe/London (GMT)</SelectItem>
          </SelectContent>
        </Select>
      </div>
      
      <div className="bg-indigo-50/50 dark:bg-indigo-900/20 p-3 rounded-md border border-indigo-100 dark:border-indigo-900/50 mt-4">
        <h5 className="text-xs font-semibold text-indigo-700 dark:text-indigo-400 mb-1">Next Expected Executions</h5>
        <ul className="text-xs text-muted-foreground space-y-1 font-mono">
          <li>Tomorrow at 02:00:00</li>
          <li>In 2 days at 02:00:00</li>
          <li>In 3 days at 02:00:00</li>
        </ul>
      </div>
    </div>
  );
}
