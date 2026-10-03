import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Copy } from "lucide-react";
import { useState } from "react";

interface JobCloneModalProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  originalJobName: string;
  onClone: (newName: string, newEnv: string) => void;
}

export function JobCloneModal({ open, onOpenChange, originalJobName, onClone }: JobCloneModalProps) {
  const [name, setName] = useState(`${originalJobName} (Copy)`);
  const [env, setEnv] = useState("development");

  const handleClone = () => {
    onClone(name, env);
    onOpenChange(false);
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <div className="flex items-center gap-2 mb-2">
            <Copy className="w-5 h-5 text-indigo-500" />
            <DialogTitle>Clone Job Configuration</DialogTitle>
          </div>
          <DialogDescription>
            This will copy the schedule, retries, timeouts, parameters, and worker requirements from <strong>{originalJobName}</strong>.
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-4 py-4">
          <div className="space-y-2">
            <Label htmlFor="clone-name">New Job Name</Label>
            <Input 
              id="clone-name" 
              value={name} 
              onChange={e => setName(e.target.value)} 
            />
          </div>
          <div className="space-y-2">
            <Label htmlFor="clone-env">Target Environment</Label>
            <select 
              id="clone-env"
              value={env}
              onChange={e => setEnv(e.target.value)}
              className="flex h-9 w-full rounded-md border border-input bg-transparent px-3 py-1 text-sm shadow-sm transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
            >
              <option value="development">Development</option>
              <option value="staging">Staging</option>
              <option value="production">Production</option>
            </select>
          </div>
        </div>

        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)}>Cancel</Button>
          <Button onClick={handleClone} className="bg-indigo-600 hover:bg-indigo-700">Clone Job</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
