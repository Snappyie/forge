import { AlertCircle, Calendar, Cpu, Clock, Activity, ArrowRight } from "lucide-react";
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";

export function DiagnosticPanel({ jobId, scheduleStatus = "Active", schedulerHealth = "Healthy", calendarStatus = "Holiday", workerStatus = "Available", reason = "October 2 is excluded by 'India Banking Calendar'." }: any) {
  return (
    <Card className="border-red-200 dark:border-red-900 bg-red-50/50 dark:bg-red-900/10">
      <CardHeader className="pb-3">
        <div className="flex items-center gap-2 text-red-600 dark:text-red-400">
          <AlertCircle className="w-5 h-5" />
          <CardTitle className="text-lg">Why didn't this run?</CardTitle>
        </div>
        <CardDescription>Diagnostic trace for missing execution at 02:00.</CardDescription>
      </CardHeader>
      <CardContent>
        <div className="grid grid-cols-2 gap-4 mb-6">
          <div className="flex justify-between items-center py-1 border-b border-border/50">
            <span className="text-sm text-muted-foreground flex items-center gap-2"><Clock className="w-4 h-4" /> Schedule</span>
            <Badge variant="outline" className={scheduleStatus === 'Active' ? 'bg-green-100 text-green-700' : 'bg-red-100 text-red-700'}>{scheduleStatus === 'Active' ? '✓ Active' : '✕ Disabled'}</Badge>
          </div>
          <div className="flex justify-between items-center py-1 border-b border-border/50">
            <span className="text-sm text-muted-foreground flex items-center gap-2"><Activity className="w-4 h-4" /> Scheduler</span>
            <Badge variant="outline" className={schedulerHealth === 'Healthy' ? 'bg-green-100 text-green-700' : 'bg-red-100 text-red-700'}>{schedulerHealth === 'Healthy' ? '✓ Healthy' : '✕ Down'}</Badge>
          </div>
          <div className="flex justify-between items-center py-1 border-b border-border/50">
            <span className="text-sm text-muted-foreground flex items-center gap-2"><Calendar className="w-4 h-4" /> Calendar</span>
            <Badge variant="outline" className={calendarStatus === 'Holiday' ? 'bg-red-100 text-red-700' : 'bg-green-100 text-green-700'}>{calendarStatus === 'Holiday' ? '✕ Holiday' : '✓ Cleared'}</Badge>
          </div>
          <div className="flex justify-between items-center py-1 border-b border-border/50">
            <span className="text-sm text-muted-foreground flex items-center gap-2"><Cpu className="w-4 h-4" /> Worker Pool</span>
            <Badge variant="outline" className={workerStatus === 'Available' ? 'bg-green-100 text-green-700' : 'bg-red-100 text-red-700'}>{workerStatus === 'Available' ? '✓ Available' : '✕ Empty'}</Badge>
          </div>
        </div>
        
        <div className="bg-background rounded-md p-4 border border-border">
          <h4 className="text-xs font-semibold text-muted-foreground uppercase tracking-wider mb-2">Primary Reason</h4>
          <p className="text-sm font-medium">{reason}</p>
        </div>
      </CardContent>
    </Card>
  );
}
