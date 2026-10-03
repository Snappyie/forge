import { Card, CardContent, CardHeader, CardTitle, CardDescription } from "@/components/ui/card";
import { PlayCircle, Clock, Globe, ArrowDown, Webhook } from "lucide-react";
import { Badge } from "@/components/ui/badge";

export function WhyIsThisRunning() {
  return (
    <Card className="border-border/50 shadow-sm bg-muted/10">
      <CardHeader className="pb-3 border-b border-border/50">
        <div className="flex justify-between items-start">
          <div>
            <CardTitle className="text-base flex items-center gap-2">
              <PlayCircle className="w-5 h-5 text-green-500" /> Why is this running?
            </CardTitle>
            <CardDescription className="mt-1">Traceability view of the execution trigger.</CardDescription>
          </div>
          <Badge className="bg-green-100 text-green-700 shadow-none border-none">Cron Trigger</Badge>
        </div>
      </CardHeader>
      <CardContent className="pt-6">
        <div className="flex flex-col items-center max-w-sm mx-auto">
          
          {/* Node 1 */}
          <div className="bg-background border border-border p-3 rounded-lg w-full flex items-center justify-between shadow-sm">
            <span className="text-sm font-semibold text-foreground">Trigger Mechanism</span>
            <Badge variant="secondary" className="font-mono text-xs"><Clock className="w-3 h-3 mr-1" /> Cron</Badge>
          </div>
          
          <ArrowDown className="w-4 h-4 text-muted-foreground my-2" />
          
          {/* Node 2 */}
          <div className="bg-background border border-border p-3 rounded-lg w-full flex items-center justify-between shadow-sm">
            <span className="text-sm font-semibold text-foreground">Schedule</span>
            <code className="font-mono text-xs bg-muted px-1.5 py-0.5 rounded text-foreground">0 3 * * *</code>
          </div>

          <ArrowDown className="w-4 h-4 text-muted-foreground my-2" />
          
          {/* Node 3 */}
          <div className="bg-background border border-border p-3 rounded-lg w-full flex items-center justify-between shadow-sm">
            <span className="text-sm font-semibold text-foreground">Timezone</span>
            <Badge variant="outline" className="font-mono text-xs"><Globe className="w-3 h-3 mr-1" /> UTC</Badge>
          </div>

          <ArrowDown className="w-4 h-4 text-muted-foreground my-2" />
          
          {/* Node 4 */}
          <div className="bg-indigo-50/50 dark:bg-indigo-900/20 border border-indigo-200 dark:border-indigo-800 p-3 rounded-lg w-full flex items-center justify-between shadow-sm">
            <span className="text-sm font-semibold text-indigo-700 dark:text-indigo-400">Next Execution</span>
            <span className="text-sm font-mono font-medium text-indigo-700 dark:text-indigo-400">Tomorrow 03:00</span>
          </div>

        </div>
      </CardContent>
    </Card>
  );
}
