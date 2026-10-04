"use client";

/**
 * Ask Scheduler (UI.md section 80).
 *
 * The assistant answers from stored data and proposes configuration changes
 * without applying them. The spec is explicit that it must never silently
 * modify production configuration, so the propose path returns a diff and the
 * human applies it through the ordinary UI.
 */

import { useState } from "react";
import { Sparkles } from "lucide-react";

import { api } from "@/lib/api";
import { useToast } from "@/lib/useToast";
import { EmptyState } from "@/components/states";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";

interface Answer {
  intent: string;
  answer: string;
  results: Record<string, unknown>[];
}

interface Proposal {
  proposal: { kind: string; cron: string; timezone: string; derived_from: string };
  applied: boolean;
  detail: string;
}

const EXAMPLES = [
  "What jobs are running right now?",
  "Why did a job fail?",
  "Which jobs failed more than once this week?",
];

export default function AssistantPage() {
  const [question, setQuestion] = useState("");
  const [answer, setAnswer] = useState<Answer | null>(null);
  const [proposal, setProposal] = useState<Proposal | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const toast = useToast();

  async function ask(term: string) {
    if (term.trim() === "") return;
    setBusy(true);
    setError(null);
    setProposal(null);
    try {
      const result = await api.post<Answer>("/assistant/ask", { question: term });
      setAnswer(result);
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : "Could not reach the assistant");
    } finally {
      setBusy(false);
    }
  }

  async function propose(term: string) {
    if (term.trim() === "") return;
    setBusy(true);
    setError(null);
    try {
      const result = await api.post<Proposal>("/assistant/propose", {
        question: term,
      });
      setProposal(result);
      setAnswer(null);
      toast.info("Proposal ready", "Nothing has been changed.");
    } catch (caught) {
      setError(
        caught instanceof Error ? caught.message : "Could not build a proposal",
      );
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="flex flex-col gap-4 p-6">
      <header>
        <h1 className="flex items-center gap-2 text-lg font-semibold">
          <Sparkles className="size-4" aria-hidden />
          Ask Scheduler
        </h1>
        <p className="text-xs text-muted-foreground">
          Answers come from your data. Proposals are never applied automatically.
        </p>
      </header>

      <div className="flex flex-col gap-2">
        <div className="flex gap-2">
          <Input
            value={question}
            onChange={(e) => setQuestion(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") ask(question);
            }}
            placeholder="What jobs are running right now?"
            aria-label="Ask the scheduler"
          />
          <Button onClick={() => ask(question)} disabled={busy}>
            Ask
          </Button>
          <Button variant="outline" onClick={() => propose(question)} disabled={busy}>
            Propose a change
          </Button>
        </div>

        <div className="flex flex-wrap gap-1.5">
          {EXAMPLES.map((example) => (
            <button
              key={example}
              type="button"
              onClick={() => {
                setQuestion(example);
                ask(example);
              }}
              className="rounded border border-border px-2 py-0.5 text-[11px] text-muted-foreground hover:bg-accent/60"
            >
              {example}
            </button>
          ))}
        </div>

        {error ? (
          <p role="alert" className="text-xs text-red-600">
            {error}
          </p>
        ) : null}
      </div>

      {proposal ? (
        <Card className="border-amber-500/40">
          <CardHeader>
            <CardTitle className="text-sm">Proposed change</CardTitle>
          </CardHeader>
          <CardContent className="flex flex-col gap-2 text-xs">
            <p>
              Derived from: <em>{proposal.proposal.derived_from}</em>
            </p>
            <p>
              Cron:{" "}
              <code className="rounded bg-muted px-1.5 py-0.5 font-mono">
                {proposal.proposal.cron}
              </code>
            </p>
            <p>
              Timezone: <code className="font-mono">{proposal.proposal.timezone}</code>
            </p>
            <Badge variant="outline" className="w-fit text-[10px]">
              applied: {String(proposal.applied)}
            </Badge>
            <p className="text-muted-foreground">{proposal.detail}</p>
          </CardContent>
        </Card>
      ) : null}

      {proposal ? (
        <Card className="border-amber-500/40">
          <CardHeader>
            <CardTitle className="text-sm">Proposed change</CardTitle>
          </CardHeader>
          <CardContent className="flex flex-col gap-2 text-xs">
            <p>
              Derived from: <em>{proposal.proposal.derived_from}</em>
            </p>
            <p>
              Cron:{" "}
              <code className="rounded bg-muted px-1.5 py-0.5 font-mono">
                {proposal.proposal.cron}
              </code>
            </p>
            <p>
              Timezone:{" "}
              <code className="font-mono">{proposal.proposal.timezone}</code>
            </p>
            <Badge variant="outline" className="w-fit text-[10px]">
              applied: {String(proposal.applied)}
            </Badge>
            <p className="text-muted-foreground">{proposal.detail}</p>
          </CardContent>
        </Card>
      ) : null}

      {answer ? (
        <Card>
          <CardHeader className="flex-row items-center justify-between">
            <CardTitle className="text-sm">Answer</CardTitle>
            <Badge variant="secondary" className="text-[10px]">
              {answer.intent.replace(/_/g, " ")}
            </Badge>
          </CardHeader>
          <CardContent className="flex flex-col gap-2 text-xs">
            <p>{answer.answer}</p>
            {answer.results.length > 0 ? (
              <ul className="flex flex-col gap-1 border-t border-border pt-2">
                {answer.results.map((row, index) => (
                  <li key={index} className="font-mono text-[11px]">
                    {Object.entries(row)
                      .map(([key, value]) => `${key}=${String(value)}`)
                      .join("  ")}
                  </li>
                ))}
              </ul>
            ) : null}
          </CardContent>
        </Card>
      ) : !proposal ? (
        <EmptyState
          title="Ask a question"
          description="Pick an example above, or type your own."
        />
      ) : null}
    </div>
  );
}
