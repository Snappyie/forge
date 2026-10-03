import { useState } from "react";
import { 
  Dialog, 
  DialogContent, 
  DialogDescription, 
  DialogFooter, 
  DialogHeader, 
  DialogTitle 
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { AlertTriangle } from "lucide-react";

interface ProductionGuardrailProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  resourceName: string;
  actionName: string;
  onConfirm: () => void;
  impact?: string[];
}

export function ProductionGuardrail({
  open,
  onOpenChange,
  resourceName,
  actionName,
  onConfirm,
  impact = ["1 downstream workflow", "3 dependent jobs"]
}: ProductionGuardrailProps) {
  const [confirmationInput, setConfirmationInput] = useState("");
  const expectedConfirmation = resourceName.toUpperCase();

  const handleConfirm = () => {
    if (confirmationInput === expectedConfirmation) {
      onConfirm();
      onOpenChange(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="border-red-200 dark:border-red-900/50">
        <DialogHeader>
          <div className="flex items-center gap-2 text-red-600 dark:text-red-400 mb-2">
            <AlertTriangle className="w-5 h-5" />
            <DialogTitle>You are modifying PRODUCTION</DialogTitle>
          </div>
          <DialogDescription className="text-base text-foreground">
            Are you sure you want to <strong>{actionName}</strong> <code className="bg-muted px-1 py-0.5 rounded text-sm">{resourceName}</code>?
          </DialogDescription>
        </DialogHeader>
        
        <div className="bg-red-50/50 dark:bg-red-900/10 p-4 rounded-md border border-red-100 dark:border-red-900/30 space-y-2">
          <p className="text-sm font-semibold text-red-800 dark:text-red-300">Expected Impact:</p>
          <ul className="text-sm text-red-700 dark:text-red-400 list-disc list-inside">
            {impact.map((imp, idx) => (
              <li key={idx}>{imp}</li>
            ))}
          </ul>
        </div>

        <div className="space-y-2 mt-4">
          <label className="text-sm font-medium">
            Type <strong>{expectedConfirmation}</strong> to confirm.
          </label>
          <Input 
            value={confirmationInput}
            onChange={(e) => setConfirmationInput(e.target.value)}
            placeholder={expectedConfirmation}
            className="border-red-200 focus-visible:ring-red-500"
          />
        </div>

        <DialogFooter className="mt-4">
          <Button variant="outline" onClick={() => onOpenChange(false)}>Cancel</Button>
          <Button 
            variant="destructive" 
            disabled={confirmationInput !== expectedConfirmation}
            onClick={handleConfirm}
          >
            {actionName} Resource
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
