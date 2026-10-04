"use client";

/**
 * Import and export (UI.md section 41).
 *
 * Export downloads the tenant's jobs and schedules as JSON for backup or for
 * Git/IaC review. Import creates jobs from such a document and reports each one,
 * so a partially bad file tells you which entries were rejected.
 */

import { useRef, useState } from "react";
import { Download, Upload } from "lucide-react";

import { api } from "@/lib/api";
import { downloadText } from "@/lib/clipboard";
import { useToast } from "@/lib/useToast";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";

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

  return (
    <Card>
      <CardHeader>
        <CardTitle className="text-sm">Import / export</CardTitle>
      </CardHeader>
      <CardContent className="flex flex-col gap-3">
        <div className="flex flex-wrap gap-2">
          <Button variant="outline" size="sm" disabled={busy} onClick={exportJobs}>
            <Download className="mr-1 size-3.5" aria-hidden />
            Export jobs
          </Button>
          <Button
            variant="outline"
            size="sm"
            disabled={busy}
            onClick={() => fileRef.current?.click()}
          >
            <Upload className="mr-1 size-3.5" aria-hidden />
            Import jobs
          </Button>
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
        </div>

        {failures.length > 0 ? (
          <ul className="flex flex-col gap-0.5 border-t border-border pt-2">
            {failures.map((result) => (
              <li key={result.name} className="text-[11px] text-red-600 dark:text-red-400">
                {result.name}: {result.error}
              </li>
            ))}
          </ul>
        ) : null}
      </CardContent>
    </Card>
  );
}
