import { Input } from "@/components/ui/input"
import { Search } from "lucide-react"

export function GlobalSearch() {
  return (
    <div className="relative w-full max-w-md">
      <Search className="absolute left-2.5 top-2.5 h-4 w-4 text-muted-foreground" />
      <Input
        type="search"
        placeholder="Search jobs, executions, workers... (Press '/')"
        className="w-full bg-background/60 shadow-none pl-9 border-border/50 focus-visible:ring-indigo-500 rounded-lg"
      />
      <div className="absolute right-2.5 top-2 flex items-center gap-1">
        <kbd className="inline-flex h-5 items-center gap-1 rounded border border-border bg-muted px-1.5 font-mono text-[10px] font-medium text-muted-foreground opacity-100">
          <span className="text-xs">⌘</span>K
        </kbd>
      </div>
    </div>
  )
}
