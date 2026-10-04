"use client";

/**
 * Import and export (UI.md section 41).
 *
 * Export downloads the tenant's jobs and schedules as JSON for backup or for
 * Git/IaC review. Import creates jobs from such a document and reports each one,
 * so a partially bad file tells you which entries were rejected.
 */

import { useRef, useState } from "react";
import { Download, MoreHorizontal, Upload } from "lucide-react";

import { api } from "@/lib/api";
import { downloadText } from "@/lib/clipboard";
import { useToast } from "@/lib/useToast";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";

interface ImportResult {
  name: string;
  ok: boolean;
  error?: string;
  id?: string;
}

export function ImportExport({ onImported }: { onImported: () => void }) {
  const toast = useToast();
  const fileRef = useRef<HTMLInputElement>(null);
  const [busy, setBusy] = useState(false);
  const [results, setResults] = useState<ImportResult[]>([]);

  async function exportJobs() {
    setBusy(true);
    try {
      const document = await api.get<{ jobs: unknown[]; schedules: unknown[] }>(
        "/jobs/export",
      );
      downloadText(
        "forge-jobs.json",
        JSON.stringify(document, null, 2),
        "application/json",
      );
      toast.success("Exported");
    } catch (error) {
      toast.error(
        "Could not export",
        error instanceof Error ? error.message : undefined,
      );
    } finally {
      setBusy(false);
    }
  }

  async function importFile(file: File) {
    setBusy(true);
    setResults([]);
    try {
      const parsed = JSON.parse(await file.text());
      const response = await api.post<{
        imported: number;
        failed: number;
        results: ImportResult[];
      }>("/jobs/import", { jobs: parsed.jobs ?? [] });

      setResults(response.results);
      if (response.failed === 0) {
        toast.success(`Imported ${response.imported} job(s)`);
      } else {
        toast.error(
          `${response.imported} of ${response.imported + response.failed} imported`,
          `${response.failed} could not be imported.`,
        );
      }
      onImported();
    } catch (error) {
      toast.error(
        "Could not import",
        error instanceof Error ? error.message : "The file is not valid JSON",
      );
    } finally {
      setBusy(false);
    }
  }

  const failures = results.filter((r) => !r.ok);

  /*
   * A dropdown rather than a panel.
   *
   * Import/export is a rare, self-contained task. As a full-width card it
   * occupied a band of its own on every list page to offer two buttons; in the
   * toolbar it becomes one control that stays out of the way until wanted.
   */
  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        render={
          <Button
            variant="outline"
            size="sm"
            disabled={busy}
            aria-label="Import or export jobs"
            className="h-7 text-[12.5px]"
          />
        }
      >
        <MoreHorizontal aria-hidden />
        Import / export
      </DropdownMenuTrigger>

      <DropdownMenuContent align="end" className="w-56">
        <DropdownMenuItem onClick={exportJobs}>
          <Download aria-hidden />
          Export jobs as JSON
        </DropdownMenuItem>
        <DropdownMenuItem onClick={() => fileRef.current?.click()}>
          <Upload aria-hidden />
          Import jobs from JSON
        </DropdownMenuItem>
        <input
          ref={fileRef}
          type="file"
          accept="application/json"
          className="hidden"
          aria-label="Import jobs file"
          onChange={(e) => {
            const file = e.target.files?.[0];
            if (file) importFile(file);
            e.target.value = "";
          }}
        />

        {/*
          Import failures are reported here rather than as a toast: a partially
          rejected file needs every rejected entry visible at once, and a toast
          that auto-dismisses cannot hold them.
        */}
        {failures.length > 0 ? (
          <div className="border-t border-border px-2 py-2">
            <p className="mb-1 text-[11px] font-medium text-danger-foreground">
              {failures.length} job(s) rejected
            </p>
            <ul className="flex max-h-40 flex-col gap-1 overflow-y-auto">
              {failures.map((result) => (
                <li key={result.name} className="text-[11px] text-muted-foreground">
                  <span className="text-foreground">{result.name}</span>:{" "}
                  {result.error}
                </li>
              ))}
            </ul>
          </div>
        ) : null}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
