import { AlertCircle } from "lucide-react";

export function ScheduleConflictWarning({ jobCount = 30, capacity = 20 }: any) {
  if (jobCount <= capacity) return null;

  return (
    <div className="bg-amber-50/50 dark:bg-amber-900/10 border border-amber-200 dark:border-amber-900/50 rounded-md p-4 flex gap-3 shadow-sm mb-4">
      <AlertCircle className="w-5 h-5 text-amber-500 shrink-0 mt-0.5" />
      <div>
        <h4 className="text-sm font-semibold text-amber-800 dark:text-amber-500">Schedule Collision Detected</h4>
        <p className="text-sm text-amber-700 dark:text-amber-400 mt-1">
          <strong>{jobCount} jobs</strong> are scheduled to start at this exact time (02:00 UTC). 
          Your current worker pool capacity is limited to <strong>{capacity} concurrent executions</strong>.
        </p>
        <p className="text-sm text-amber-700 dark:text-amber-400 mt-2 font-medium">
          Impact: 10 executions will be queued and face start delays. Consider jittering schedules or scaling worker capacity.
        </p>
      </div>
    </div>
  );
}
