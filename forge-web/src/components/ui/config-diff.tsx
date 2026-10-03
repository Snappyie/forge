import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { History, GitCommit } from "lucide-react";

export function ConfigurationDiffViewer() {
  return (
    <Card className="border-border/50 shadow-sm">
      <CardHeader className="bg-muted/30 border-b border-border/50">
        <div className="flex items-center justify-between">
          <CardTitle className="text-base flex items-center gap-2">
            <History className="w-5 h-5 text-indigo-500" /> Version History & Diffs
          </CardTitle>
          <div className="flex items-center gap-2 text-xs font-mono text-muted-foreground">
            <span className="bg-muted px-2 py-1 rounded">v2</span>
            <span>→</span>
            <span className="bg-muted px-2 py-1 rounded text-foreground font-semibold">v3 (Current)</span>
          </div>
        </div>
      </CardHeader>
      <CardContent className="p-0">
        <div className="font-mono text-xs overflow-x-auto">
          <div className="flex items-center gap-4 p-2 bg-muted/20 border-b border-border/50 text-muted-foreground">
            <span className="w-16">@@ -1,5 +1,5 @@</span>
            <span>Job: nightly-settlement</span>
          </div>
          <div className="flex gap-4 p-2 hover:bg-muted/20">
            <span className="w-8 text-right text-muted-foreground select-none">12</span>
            <span>{"{"}</span>
          </div>
          <div className="flex gap-4 p-2 hover:bg-muted/20">
            <span className="w-8 text-right text-muted-foreground select-none">13</span>
            <span>{"  \"environment\": \"production\","}</span>
          </div>
          
          <div className="flex gap-4 p-2 bg-red-50/50 dark:bg-red-900/10 text-red-700 dark:text-red-400">
            <span className="w-8 text-right select-none opacity-50">-</span>
            <span>{"  \"schedule\": \"0 2 * * *\","}</span>
          </div>
          <div className="flex gap-4 p-2 bg-green-50/50 dark:bg-green-900/10 text-green-700 dark:text-green-400">
            <span className="w-8 text-right select-none opacity-50">+</span>
            <span>{"  \"schedule\": \"0 3 * * *\","}</span>
          </div>

          <div className="flex gap-4 p-2 hover:bg-muted/20">
            <span className="w-8 text-right text-muted-foreground select-none">15</span>
            <span>{"  \"retries\": 5"}</span>
          </div>
          <div className="flex gap-4 p-2 hover:bg-muted/20">
            <span className="w-8 text-right text-muted-foreground select-none">16</span>
            <span>{"}"}</span>
          </div>
        </div>
        
        <div className="p-3 bg-muted/30 border-t border-border/50 flex items-center justify-between text-xs text-muted-foreground">
          <span className="flex items-center gap-1"><GitCommit className="w-3 h-3" /> Changed by Admin 2 hours ago</span>
          <span>1 line added, 1 line removed</span>
        </div>
      </CardContent>
    </Card>
  );
}
