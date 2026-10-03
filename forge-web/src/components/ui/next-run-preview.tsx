import { Card, CardContent, CardHeader, CardTitle, CardDescription } from "@/components/ui/card";
import { Clock, CheckCircle2, ChevronRight } from "lucide-react";
import { Badge } from "@/components/ui/badge";

export function NextRunPreview({ scheduleStr = "0 2 * * *", timezone = "Asia/Kolkata" }: any) {
  return (
    <Card className="border-border/50 shadow-sm bg-muted/10">
      <CardHeader className="pb-3 border-b border-border/50">
        <div className="flex justify-between items-center">
          <CardTitle className="text-base flex items-center gap-2">
            <Clock className="w-4 h-4 text-indigo-500" /> Expected Executions
          </CardTitle>
          <Badge variant="outline" className="font-mono text-xs shadow-none bg-background">
            {timezone}
          </Badge>
        </div>
        <CardDescription>
          Based on cron: <code className="font-mono bg-muted px-1 py-0.5 rounded text-foreground">{scheduleStr}</code>
        </CardDescription>
      </CardHeader>
      <CardContent className="pt-4 space-y-3">
        <div className="flex items-center justify-between text-sm bg-indigo-50/50 dark:bg-indigo-900/10 p-2 rounded border border-indigo-100 dark:border-indigo-900/50">
          <span className="font-semibold text-indigo-700 dark:text-indigo-400">Next Run</span>
          <span className="font-mono font-medium">Tomorrow at 02:00:00</span>
        </div>
        
        <div className="flex items-center justify-between text-sm p-2 text-muted-foreground border-b border-border/50">
          <span>Run 2</span>
          <span className="font-mono">In 2 days at 02:00:00</span>
        </div>
        <div className="flex items-center justify-between text-sm p-2 text-muted-foreground border-b border-border/50">
          <span>Run 3</span>
          <span className="font-mono">In 3 days at 02:00:00</span>
        </div>
        <div className="flex items-center justify-between text-sm p-2 text-muted-foreground border-b border-border/50">
          <span>Run 4</span>
          <span className="font-mono">In 4 days at 02:00:00</span>
        </div>
        <div className="flex items-center justify-between text-sm p-2 text-muted-foreground">
          <span>Run 5</span>
          <span className="font-mono">In 5 days at 02:00:00</span>
        </div>
      </CardContent>
    </Card>
  );
}
