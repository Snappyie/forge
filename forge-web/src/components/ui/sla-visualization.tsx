import { Card, CardContent, CardHeader, CardTitle, CardDescription } from "@/components/ui/card";
import { CheckCircle2, AlertTriangle, TrendingUp, TrendingDown, Minus } from "lucide-react";
import { Badge } from "@/components/ui/badge";

export function SlaVisualization() {
  return (
    <Card className="border-border/50 shadow-sm">
      <CardHeader>
        <div className="flex justify-between items-start">
          <div>
            <CardTitle className="text-lg">SLA Compliance</CardTitle>
            <CardDescription>Service Level Agreement metrics for the last 30 days.</CardDescription>
          </div>
          <Badge className="bg-green-100 text-green-700 shadow-none border-none">
            99.8% Healthy
          </Badge>
        </div>
      </CardHeader>
      <CardContent>
        <div className="grid grid-cols-3 gap-4 mb-6">
          <div className="bg-muted/30 p-4 rounded-lg border border-border/50">
            <p className="text-sm text-muted-foreground mb-1">Expected Duration</p>
            <p className="text-2xl font-bold font-mono">02:00 - 02:30</p>
            <p className="text-xs text-muted-foreground mt-1 flex items-center"><Minus className="w-3 h-3 mr-1" /> Baseline</p>
          </div>
          <div className="bg-muted/30 p-4 rounded-lg border border-border/50">
            <p className="text-sm text-muted-foreground mb-1">Actual P95</p>
            <p className="text-2xl font-bold font-mono">02:08</p>
            <p className="text-xs text-green-600 mt-1 flex items-center"><TrendingDown className="w-3 h-3 mr-1" /> 22m under SLA</p>
          </div>
          <div className="bg-muted/30 p-4 rounded-lg border border-border/50">
            <p className="text-sm text-muted-foreground mb-1">Violations</p>
            <p className="text-2xl font-bold font-mono">1</p>
            <p className="text-xs text-amber-600 mt-1 flex items-center"><TrendingUp className="w-3 h-3 mr-1" /> +1 vs last month</p>
          </div>
        </div>

        <div>
          <h4 className="text-sm font-semibold mb-3">Recent Runs vs SLA</h4>
          <div className="space-y-3">
            {[
              { date: "Oct 2", actual: "8m", expected: "30m", status: "met" },
              { date: "Oct 1", actual: "7m", expected: "30m", status: "met" },
              { date: "Sep 30", actual: "34m", expected: "30m", status: "violated" },
              { date: "Sep 29", actual: "9m", expected: "30m", status: "met" },
            ].map((run, i) => (
              <div key={i} className="flex items-center justify-between text-sm py-2 border-b border-border/50 last:border-0 last:pb-0">
                <span className="w-16 font-medium text-muted-foreground">{run.date}</span>
                <div className="flex-1 px-4">
                  <div className="w-full bg-muted rounded-full h-2 relative">
                    {/* Expected mark */}
                    <div className="absolute left-[80%] top-0 bottom-0 w-0.5 bg-foreground/20 z-10" title="SLA Limit" />
                    {/* Actual fill */}
                    <div 
                      className={`h-full rounded-full ${run.status === 'violated' ? 'bg-amber-500' : 'bg-indigo-500'}`} 
                      style={{ width: run.status === 'violated' ? '90%' : '25%' }} 
                    />
                  </div>
                </div>
                <span className="w-12 text-right font-mono text-xs">{run.actual}</span>
                <span className="w-20 text-right">
                  {run.status === 'met' ? (
                    <span className="text-green-600 flex items-center justify-end text-xs"><CheckCircle2 className="w-3 h-3 mr-1"/> Met</span>
                  ) : (
                    <span className="text-amber-600 flex items-center justify-end text-xs"><AlertTriangle className="w-3 h-3 mr-1"/> Violated</span>
                  )}
                </span>
              </div>
            ))}
          </div>
        </div>
      </CardContent>
    </Card>
  );
}
