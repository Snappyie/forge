import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Badge } from "@/components/ui/badge";
import { Search, Filter, Save, X, Bookmark } from "lucide-react";
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger } from "@/components/ui/dropdown-menu";

export function PowerfulFilterBar({ onFilterChange }: any) {
  const [activeFilters, setActiveFilters] = useState<string[]>(["environment:prod", "status:failed"]);
  
  const savedViews = [
    { name: "Production Failures", filters: ["environment:prod", "status:failed"] },
    { name: "My Team's Jobs", filters: ["team:payments"] },
    { name: "Long Running", filters: ["duration:>10m"] }
  ];

  const removeFilter = (filter: string) => {
    setActiveFilters(activeFilters.filter(f => f !== filter));
  };

  const loadView = (view: any) => {
    setActiveFilters(view.filters);
  };

  return (
    <div className="flex flex-col gap-3 mb-4">
      <div className="flex items-center gap-2">
        <div className="relative flex-1">
          <Search className="absolute left-2.5 top-2.5 h-4 w-4 text-muted-foreground" />
          <Input 
            placeholder="Search with syntax... e.g. status:failed team:payments duration:>10m"
            className="pl-9 h-10 border-border/50 bg-background/50 focus-visible:ring-indigo-500"
          />
        </div>
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button variant="outline" className="border-border/50">
              <Bookmark className="w-4 h-4 mr-2 text-indigo-500" /> Saved Views
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end" className="w-56">
            {savedViews.map((v, i) => (
              <DropdownMenuItem key={i} onClick={() => loadView(v)} className="flex items-center justify-between cursor-pointer">
                <span>{v.name}</span>
              </DropdownMenuItem>
            ))}
          </DropdownMenuContent>
        </DropdownMenu>
        <Button className="bg-indigo-600 hover:bg-indigo-700">
          <Save className="w-4 h-4 mr-2" /> Save View
        </Button>
      </div>

      {activeFilters.length > 0 && (
        <div className="flex items-center gap-2">
          <span className="text-xs font-medium text-muted-foreground flex items-center gap-1">
            <Filter className="w-3 h-3" /> Active Filters:
          </span>
          <div className="flex flex-wrap gap-1.5">
            {activeFilters.map((filter, i) => {
              const [key, value] = filter.split(":");
              return (
                <Badge key={i} variant="secondary" className="bg-muted text-foreground border-border/50 font-mono text-xs font-normal">
                  <span className="text-muted-foreground mr-1">{key}:</span>{value}
                  <button onClick={() => removeFilter(filter)} className="ml-1.5 hover:bg-muted-foreground/20 rounded-full p-0.5">
                    <X className="w-3 h-3" />
                  </button>
                </Badge>
              );
            })}
            <Button variant="ghost" size="sm" className="h-5 px-2 text-xs text-muted-foreground hover:text-foreground" onClick={() => setActiveFilters([])}>
              Clear all
            </Button>
          </div>
        </div>
      )}
    </div>
  );
}
