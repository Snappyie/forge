import { Card, CardContent, CardHeader, CardTitle, CardDescription } from "@/components/ui/card";
import { CheckCircle2, XCircle } from "lucide-react";

export function ProductionReadinessChecklist() {
  const checks = [
    { label: "Owner assigned", status: true },
    { label: "Team assigned", status: true },
    { label: "Schedule valid", status: true },
    { label: "Timezone configured", status: true },
    { label: "Retry configured", status: true },
    { label: "Timeout configured", status: true },
    { label: "Alert configured", status: true },
    { label: "Runbook missing", status: false },
    { label: "SLA missing", status: false },
  ];

  const failedCount = checks.filter(c => !c.status).length;

  return (
    <Card className={`border-border/50 shadow-sm ${failedCount > 0 ? 'border-amber-200 dark:border-amber-900/50' : 'border-green-200 dark:border-green-900/50'}`}>
      <CardHeader className={failedCount > 0 ? 'bg-amber-50/30 dark:bg-amber-900/10' : 'bg-green-50/30 dark:bg-green-900/10'}>
        <CardTitle className="text-lg">Production Readiness</CardTitle>
        <CardDescription>
          {failedCount > 0 
            ? <span className="text-amber-600 dark:text-amber-400 font-medium">{failedCount} items require attention before deploying to Production.</span>
            : <span className="text-green-600 dark:text-green-400 font-medium">All checks passed. Ready for Production.</span>
          }
        </CardDescription>
      </CardHeader>
      <CardContent className="pt-6">
        <ul className="grid grid-cols-2 gap-3">
          {checks.map((check, i) => (
            <li key={i} className="flex items-center gap-2 text-sm">
              {check.status ? (
                <CheckCircle2 className="w-4 h-4 text-green-500 shrink-0" />
              ) : (
                <XCircle className="w-4 h-4 text-amber-500 shrink-0" />
              )}
              <span className={check.status ? "text-foreground" : "text-amber-700 dark:text-amber-400 font-medium"}>
                {check.label}
              </span>
            </li>
          ))}
        </ul>
      </CardContent>
    </Card>
  );
}
