"use client";

/**
 * Copy, share and export helpers (UI.md sections 66, 67, 68).
 *
 * "Copy ID everywhere" means one helper so every surface behaves the same,
 * including when the clipboard API is unavailable over plain HTTP.
 */

import { useCallback, useState } from "react";

/** Copies text, falling back to a selection when the clipboard is blocked. */
export async function copyText(value: string): Promise<boolean> {
  try {
    if (navigator.clipboard?.writeText) {
      await navigator.clipboard.writeText(value);
      return true;
    }
  } catch {
    // Fall through to the legacy path rather than reporting a false failure.
  }

  try {
    const area = document.createElement("textarea");
    area.value = value;
    area.setAttribute("readonly", "");
    area.style.position = "fixed";
    area.style.opacity = "0";
    document.body.appendChild(area);
    area.select();
    const ok = document.execCommand("copy");
    document.body.removeChild(area);
    return ok;
  } catch {
    return false;
  }
}

/** Triggers a client-side file download from a string. */
export function downloadText(filename: string, contents: string, type = "text/plain") {
  const blob = new Blob([contents], { type: `${type};charset=utf-8` });
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = filename;
  document.body.appendChild(anchor);
  anchor.click();
  document.body.removeChild(anchor);
  // Revoking immediately can cancel the download in some browsers.
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}

/** Formats rows as CSV, quoting anything that would break a column. */
export function toCsv(rows: Record<string, unknown>[]): string {
  if (rows.length === 0) return "";
  const columns = Object.keys(rows[0]);
  const escape = (value: unknown) => {
    const text = value === null || value === undefined ? "" : String(value);
    return /[",\n]/.test(text) ? `"${text.replace(/"/g, '""')}"` : text;
  };
  return [
    columns.join(","),
    ...rows.map((row) => columns.map((column) => escape(row[column])).join(",")),
  ].join("\n");
}

/** A copy button that reports its own result. */
export function useCopy() {
  const [copied, setCopied] = useState(false);
  const [failed, setFailed] = useState(false);

  const copy = useCallback(async (value: string) => {
    const ok = await copyText(value);
    setCopied(ok);
    setFailed(!ok);
    if (ok) {
      setTimeout(() => setCopied(false), 1500);
    } else {
      setTimeout(() => setFailed(false), 2500);
    }
    return ok;
  }, []);

  return { copy, copied, failed };
}
