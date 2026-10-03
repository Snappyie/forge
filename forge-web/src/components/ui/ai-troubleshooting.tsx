import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Sparkles, MessageSquare, AlertTriangle, ArrowRight, CheckCircle2 } from "lucide-react";
import { Input } from "@/components/ui/input";

export function AiTroubleshootingPanel({ executionId = "ex_19302", logs = "" }: any) {
  const [analyzing, setAnalyzing] = useState(false);
  const [analysis, setAnalysis] = useState<any>(null);

  const analyze = () => {
    setAnalyzing(true);
    // Simulate AI API call
    setTimeout(() => {
      setAnalysis({
        summary: "Execution failed due to an Out of Memory (OOM) error resulting from an unusually large payload in the settlement batch.",
        rootCause: "The batch size fetched from upstream was 4,200 records, which exceeded the max memory allocation of 1.2 GB on worker-17.",
        recommendations: [
          "Increase the max_memory parameter for this job to 2.0 GB.",
          "Implement pagination in the data fetching script to process 1000 records at a time.",
          "Retry the execution on a high-memory worker tier."
        ]
      });
      setAnalyzing(false);
    }, 1500);
  };

  return (
    <Card className="border-indigo-200 dark:border-indigo-900/50 shadow-sm bg-gradient-to-b from-indigo-50/50 to-background dark:from-indigo-900/10">
      <CardHeader className="pb-3 border-b border-indigo-100 dark:border-indigo-900/30">
        <CardTitle className="text-base flex items-center gap-2 text-indigo-700 dark:text-indigo-400">
          <Sparkles className="w-5 h-5" /> AI Troubleshooting Assistant
        </CardTitle>
      </CardHeader>
      <CardContent className="pt-4">
        {!analysis && !analyzing && (
          <div className="text-center py-6">
            <MessageSquare className="w-8 h-8 mx-auto text-indigo-300 mb-3" />
            <p className="text-sm text-muted-foreground mb-4">
              Need help understanding why execution #{executionId} failed? Let AI analyze the logs, metrics, and correlating alerts to find the root cause.
            </p>
            <Button onClick={analyze} className="bg-indigo-600 hover:bg-indigo-700">
              <Sparkles className="w-4 h-4 mr-2" /> Analyze Failure
            </Button>
          </div>
        )}

        {analyzing && (
          <div className="text-center py-8 space-y-4">
            <div className="w-8 h-8 border-4 border-indigo-200 border-t-indigo-600 rounded-full animate-spin mx-auto"></div>
            <p className="text-sm text-indigo-600 animate-pulse">Correlating logs and system metrics...</p>
          </div>
        )}

        {analysis && (
          <div className="space-y-4">
            <div className="bg-background rounded-md p-3 border border-border/50 text-sm">
              <p className="font-semibold text-foreground flex items-center gap-2 mb-1">
                <AlertTriangle className="w-4 h-4 text-amber-500" /> Root Cause
              </p>
              <p className="text-muted-foreground">{analysis.rootCause}</p>
            </div>
            
            <div>
              <p className="font-semibold text-sm mb-2">Recommended Actions:</p>
              <ul className="space-y-2">
                {analysis.recommendations.map((rec: string, i: number) => (
                  <li key={i} className="flex items-start gap-2 text-sm bg-muted/30 p-2 rounded border border-border/50">
                    <CheckCircle2 className="w-4 h-4 text-green-500 mt-0.5 shrink-0" />
                    <span>{rec}</span>
                  </li>
                ))}
              </ul>
            </div>
            
            <div className="pt-4 border-t border-border/50 flex gap-2">
              <Input placeholder="Ask a follow-up question..." className="flex-1 bg-background" />
              <Button size="icon" className="bg-indigo-600 hover:bg-indigo-700 shrink-0"><ArrowRight className="w-4 h-4" /></Button>
            </div>
          </div>
        )}
      </CardContent>
    </Card>
  );
}
