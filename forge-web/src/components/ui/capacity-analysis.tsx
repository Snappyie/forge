import { Card, CardContent, CardHeader, CardTitle, CardDescription } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Server, Zap, ArrowRight } from "lucide-react";
import { Button } from "@/components/ui/button";

export function CapacityAnalysisWidget() {
  return (
    <Card className="border-border/50 shadow-sm">
      <CardHeader className="pb-3">
        <div className="flex justify-between items-start">
          <div>
            <CardTitle className="text-base flex items-center gap-2">
              <Zap className="w-5 h-5 text-yellow-500" /> Intelligent Capacity
            </CardTitle>
            <CardDescription className="mt-1">Predictive worker scaling recommendations</CardDescription>
          </div>
          <Badge variant="outline" className="font-mono bg-background">
            Active
          </Badge>
        </div>
      </CardHeader>
      <CardContent className="pt-4 space-y-4">
        
        <div className="flex gap-4">
          <div className="flex-1 bg-muted/30 rounded p-3 border border-border/50 text-center">
            <p className="text-xs text-muted-foreground mb-1 uppercase tracking-wider">Current Load</p>
            <p className="text-2xl font-bold font-mono">82%</p>
          </div>
          <div className="flex-1 bg-muted/30 rounded p-3 border border-border/50 text-center">
            <p className="text-xs text-muted-foreground mb-1 uppercase tracking-wider">Predicted (+1h)</p>
            <p className="text-2xl font-bold font-mono text-amber-600">95%</p>
          </div>
        </div>

        <div className="bg-indigo-50/50 dark:bg-indigo-900/10 rounded-md p-3 border border-indigo-100 dark:border-indigo-900/30">
          <p className="text-sm font-semibold text-indigo-700 dark:text-indigo-400 mb-1">Recommendation</p>
          <p className="text-sm text-muted-foreground mb-3">
            At 03:00 UTC, 45 heavy jobs are scheduled to run concurrently. We predict a worker capacity shortfall of ~3 nodes.
          </p>
          <Button size="sm" className="w-full bg-indigo-600 hover:bg-indigo-700 text-white">
            <Server className="w-4 h-4 mr-2" /> Provision 3 Extra Workers
          </Button>
        </div>
      </CardContent>
    </Card>
  );
}
