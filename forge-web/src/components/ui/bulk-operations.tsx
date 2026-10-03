import { Button } from "@/components/ui/button";
import { CheckSquare, PauseCircle, PlayCircle, Trash2, Tag, AlertCircle } from "lucide-react";
import { Badge } from "@/components/ui/badge";

export function BulkOperationsBar({ selectedCount = 0, onClear }: any) {
  if (selectedCount === 0) return null;

  return (
    <div className="fixed bottom-6 left-1/2 -translate-x-1/2 z-50 animate-in slide-in-from-bottom-5">
      <div className="bg-foreground text-background shadow-xl rounded-full px-6 py-3 flex items-center gap-6">
        <div className="flex items-center gap-2">
          <Badge className="bg-background text-foreground border-none font-mono px-2 py-0.5">{selectedCount}</Badge>
          <span className="text-sm font-medium">Jobs Selected</span>
        </div>

        <div className="h-6 w-[1px] bg-background/20" />

        <div className="flex items-center gap-2">
          <Button variant="ghost" size="sm" className="h-8 text-background hover:bg-background/20 hover:text-background">
            <PauseCircle className="w-4 h-4 mr-2" /> Pause All
          </Button>
          <Button variant="ghost" size="sm" className="h-8 text-background hover:bg-background/20 hover:text-background">
            <PlayCircle className="w-4 h-4 mr-2" /> Resume All
          </Button>
          <Button variant="ghost" size="sm" className="h-8 text-background hover:bg-background/20 hover:text-background">
            <Tag className="w-4 h-4 mr-2" /> Tag
          </Button>
          <Button variant="ghost" size="sm" className="h-8 text-red-400 hover:bg-red-400/20 hover:text-red-400">
            <Trash2 className="w-4 h-4 mr-2" /> Delete
          </Button>
        </div>

        <div className="h-6 w-[1px] bg-background/20" />

        <Button variant="ghost" size="sm" onClick={onClear} className="h-8 text-background/60 hover:text-background hover:bg-transparent">
          Cancel
        </Button>
      </div>

      <div className="mt-2 text-center text-xs text-muted-foreground flex justify-center items-center gap-1">
        <AlertCircle className="w-3 h-3" /> Note: 14 of these jobs are active in Production.
      </div>
    </div>
  );
}
