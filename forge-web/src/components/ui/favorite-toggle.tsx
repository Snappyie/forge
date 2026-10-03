import { Star } from "lucide-react";
import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from "@/components/ui/tooltip";

export function FavoriteToggle({ initialFavorite = false, onToggle }: any) {
  const [isFavorite, setIsFavorite] = useState(initialFavorite);

  const toggle = () => {
    const nextState = !isFavorite;
    setIsFavorite(nextState);
    if (onToggle) onToggle(nextState);
  };

  return (
    <TooltipProvider>
      <Tooltip>
        <TooltipTrigger
          render={
            <Button
              variant="ghost"
              size="icon"
              onClick={toggle}
              className={`h-8 w-8 rounded-full ${isFavorite ? 'text-amber-500 hover:text-amber-600 hover:bg-amber-50 dark:hover:bg-amber-900/20' : 'text-muted-foreground hover:text-foreground'}`}
            />
          }
        >
          <Star className={`w-4 h-4 ${isFavorite ? 'fill-current' : ''}`} />
        </TooltipTrigger>
        <TooltipContent>
          <p>{isFavorite ? 'Remove from Favorites' : 'Add to Favorites'}</p>
        </TooltipContent>
      </Tooltip>
    </TooltipProvider>
  );
}
