import { Card, CardContent, CardHeader, CardTitle, CardDescription } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { ActivitySquare, TrendingUp, AlertOctagon } from "lucide-react";
import { Button } from "@/components/ui/button";

export function AnomalyDetectionWidget() {
  return (
    <Card className="border-amber-200 dark:border-amber-900/50 bg-gradient-to-br from-amber-50/50 to-background dark:from-amber-900/10 shadow-sm">
      <CardHeader className="pb-3 border-b border-amber-100 dark:border-amber-900/30">
        <div className="flex justify-between items-start">
          <div>
            <CardTitle className="text-base flex items-center gap-2 text-amber-700 dark:text-amber-500">
              <ActivitySquare className="w-5 h-5" /> Anomaly Detection
            </CardTitle>
            <CardDescription className="mt-1">Real-time execution pattern analysis</CardDescription>
          </div>
          <Badge variant="outline" className="bg-amber-100 text-amber-700 border-none shadow-none font-mono flex items-center gap-1">
            <span className="w-1.5 h-1.5 rounded-full bg-amber-500 animate-pulse"></span> 1 Anomaly Detected
          </Badge>
        </div>
      </CardHeader>
      <CardContent className="pt-4 space-y-4">
        
        <div className="bg-background rounded-md p-3 border border-border/50">
          <div className="flex justify-between items-start mb-2">
            <div className="flex items-center gap-2">
              <AlertOctagon className="w-4 h-4 text-amber-500" />
              <span className="font-semibold text-sm">Duration Spike Detected</span>
            </div>
            <span className="text-xs text-muted-foreground">Just now</span>
          </div>
          <p className="text-sm text-muted-foreground mb-3">
            Execution <span className="font-mono text-xs">ex_19302</span> (nightly-settlement) took <strong>21m 42s</strong>. 
            This is <span className="text-amber-600 font-medium">+250% higher</span> than the 7-day moving average of <strong>8m 10s</strong>.
          </p>
          <div className="flex gap-2">
            <Button size="sm" variant="outline" className="h-8 text-xs bg-background">Investigate</Button>
            <Button size="sm" variant="ghost" className="h-8 text-xs">Dismiss</Button>
          </div>
        </div>

        <div className="space-y-2">
          <h5 className="text-xs font-semibold text-muted-foreground uppercase tracking-wider">Monitored Signals</h5>
          <div className="flex items-center justify-between text-sm py-1 border-b border-border/50">
            <span className="flex items-center gap-2"><TrendingUp className="w-3 h-3 text-muted-foreground" /> Failure Rate</span>
            <Badge variant="secondary" className="bg-green-100 text-green-700 font-normal">Normal</Badge>
          </div>
          <div className="flex items-center justify-between text-sm py-1 border-b border-border/50">
            <span className="flex items-center gap-2"><TrendingUp className="w-3 h-3 text-muted-foreground" /> Execution Duration</span>
            <Badge variant="secondary" className="bg-amber-100 text-amber-700 font-normal">Spiking</Badge>
          </div>
          <div className="flex items-center justify-between text-sm py-1">
            <span className="flex items-center gap-2"><TrendingUp className="w-3 h-3 text-muted-foreground" /> Queue Latency</span>
            <Badge variant="secondary" className="bg-green-100 text-green-700 font-normal">Normal</Badge>
          </div>
        </div>

      </CardContent>
    </Card>
  );
}
