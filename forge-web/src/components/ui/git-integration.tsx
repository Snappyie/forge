import { Card, CardContent, CardHeader, CardTitle, CardDescription } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { GitBranch, GitCommit, RefreshCw } from "lucide-react";
import { Button } from "@/components/ui/button";

export function GitIntegrationPanel() {
  return (
    <Card className="border-border/50 shadow-sm bg-muted/5">
      <CardHeader className="pb-3 border-b border-border/50">
        <div className="flex justify-between items-start">
          <div>
            <CardTitle className="text-base flex items-center gap-2">
              <GitBranch className="w-4 h-4" /> Git & IaC Integration
            </CardTitle>
            <CardDescription className="mt-1">
              This job is managed via Configuration-as-Code.
            </CardDescription>
          </div>
          <Badge className="bg-green-100 text-green-700 shadow-none border-none">Synced</Badge>
        </div>
      </CardHeader>
      <CardContent className="pt-4 space-y-4">
        <div className="grid grid-cols-2 gap-4">
          <div>
            <p className="text-xs text-muted-foreground mb-1">Repository</p>
            <p className="text-sm font-medium flex items-center gap-2 text-indigo-600 dark:text-indigo-400">
              <GitBranch className="w-3 h-3" /> acme-corp/forge-configs
            </p>
          </div>
          <div>
            <p className="text-xs text-muted-foreground mb-1">File Path</p>
            <p className="text-sm font-mono text-muted-foreground">jobs/payments/settlement.yaml</p>
          </div>
          <div>
            <p className="text-xs text-muted-foreground mb-1">Target Branch</p>
            <p className="text-sm font-mono flex items-center gap-1">
              <GitBranch className="w-3 h-3 text-muted-foreground" /> main
            </p>
          </div>
          <div>
            <p className="text-xs text-muted-foreground mb-1">Active Commit</p>
            <p className="text-sm font-mono flex items-center gap-1">
              <GitCommit className="w-3 h-3 text-muted-foreground" /> a7f8c92
            </p>
          </div>
        </div>

        <div className="pt-4 border-t border-border/50 flex justify-between items-center">
          <p className="text-xs text-muted-foreground">Last synced: 14 mins ago</p>
          <Button variant="outline" size="sm" className="h-7 text-xs border-border/50">
            <RefreshCw className="w-3 h-3 mr-1" /> Force Sync
          </Button>
        </div>
      </CardContent>
    </Card>
  );
}
