import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Sparkles, CalendarClock, ArrowRight } from "lucide-react";

export function NaturalLanguageScheduler() {
  const [prompt, setPrompt] = useState("");
  const [result, setResult] = useState<any>(null);
  const [generating, setGenerating] = useState(false);

  const handleGenerate = () => {
    if (!prompt) return;
    
    setGenerating(true);
    // Simulate AI parsing
    setTimeout(() => {
      setResult({
        cron: "0 2 * * 1-5",
        timezone: "Asia/Kolkata",
        description: "Runs every weekday at 2:00 AM India Standard Time."
      });
      setGenerating(false);
    }, 1000);
  };

  return (
    <div className="bg-indigo-50/50 dark:bg-indigo-900/10 p-4 rounded-md border border-indigo-100 dark:border-indigo-900/30">
      <h4 className="text-sm font-semibold flex items-center gap-2 text-indigo-700 dark:text-indigo-400 mb-2">
        <Sparkles className="w-4 h-4" /> AI Schedule Generator
      </h4>
      <p className="text-xs text-muted-foreground mb-3">
        Describe when you want this job to run in plain English.
      </p>
      
      <div className="flex gap-2 mb-4">
        <Input 
          value={prompt}
          onChange={(e) => setPrompt(e.target.value)}
          placeholder="e.g. Every weekday at 2 AM India time except holidays"
          className="bg-background"
          onKeyDown={(e) => e.key === 'Enter' && handleGenerate()}
        />
        <Button onClick={handleGenerate} disabled={generating} className="bg-indigo-600 hover:bg-indigo-700 shrink-0">
          {generating ? <Sparkles className="w-4 h-4 mr-2 animate-pulse" /> : <ArrowRight className="w-4 h-4" />}
        </Button>
      </div>

      {result && (
        <div className="bg-background rounded p-3 border border-border/50 flex flex-col gap-2">
          <div className="flex justify-between items-center">
            <span className="text-xs text-muted-foreground">Generated Cron</span>
            <code className="font-mono text-sm bg-muted px-1.5 py-0.5 rounded text-foreground">{result.cron}</code>
          </div>
          <div className="flex justify-between items-center">
            <span className="text-xs text-muted-foreground">Timezone</span>
            <span className="text-sm font-medium">{result.timezone}</span>
          </div>
          <div className="pt-2 border-t border-border/50">
            <span className="text-xs text-indigo-600 dark:text-indigo-400 flex items-center gap-1">
              <CalendarClock className="w-3 h-3" /> {result.description}
            </span>
          </div>
          <Button size="sm" variant="outline" className="w-full mt-2">Apply Schedule</Button>
        </div>
      )}
    </div>
  );
}
