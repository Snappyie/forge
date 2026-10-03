import { Button } from "@/components/ui/button";
import { RotateCcw, AlertTriangle, HelpCircle } from "lucide-react";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle, DialogTrigger } from "@/components/ui/dialog";

export function RetryReplayAction({ executionId = "ex_19302", status = "FAILED", hasRetriesLeft = false }: any) {
  if (status === "SUCCESS") return null;

  return (
    <Dialog>
      <DialogTrigger asChild>
        <Button variant="outline" className="border-indigo-200 text-indigo-700 hover:bg-indigo-50 dark:border-indigo-900/50 dark:text-indigo-400 dark:hover:bg-indigo-900/20">
          <RotateCcw className="w-4 h-4 mr-2" /> {hasRetriesLeft ? 'Force Retry Now' : 'Replay Execution'}
        </Button>
      </DialogTrigger>
      <DialogContent className="border-border/50">
        <DialogHeader>
          <div className="flex items-center gap-2 mb-2">
            <RotateCcw className="w-5 h-5 text-indigo-500" />
            <DialogTitle>Replay Execution #{executionId}</DialogTitle>
          </div>
          <DialogDescription className="text-base text-foreground">
            This will create a brand new execution attempt using the exact same parameters and configuration as #{executionId}.
          </DialogDescription>
        </DialogHeader>

        <div className="bg-amber-50/50 dark:bg-amber-900/10 p-4 rounded-md border border-amber-100 dark:border-amber-900/30 flex gap-3 my-4">
          <AlertTriangle className="w-5 h-5 text-amber-600 mt-0.5 shrink-0" />
          <div className="space-y-1">
            <p className="text-sm font-semibold text-amber-800 dark:text-amber-300">Idempotency Warning</p>
            <p className="text-xs text-amber-700 dark:text-amber-400">
              Ensure this job is safe to replay. If the previous execution failed midway, replaying it might result in duplicate database writes or side effects if the job isn't perfectly idempotent.
            </p>
          </div>
        </div>

        <DialogFooter>
          <Button variant="outline">Cancel</Button>
          <Button className="bg-indigo-600 hover:bg-indigo-700"><RotateCcw className="w-4 h-4 mr-2" /> Confirm Replay</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
