"use client";

/**
 * Table preferences (UI.md section 5, section 81).
 *
 * Sorting, density, and hidden columns persist per browser. `useState` starts
 * from defaults so the server and client render the same table on first paint;
 * the stored values are applied in an effect.
 */

import { useCallback, useEffect, useState } from "react";

export type Density = "compact" | "comfortable";

export interface TablePrefs {
  density: Density;
  sortKey: string;
  sortDirection: "asc" | "desc";
  hiddenColumns: string[];
}

const DEFAULTS: TablePrefs = {
  density: "comfortable",
  sortKey: "updated_at",
  sortDirection: "desc",
  hiddenColumns: [],
};

/** Reads one table's preferences, namespaced so two tables do not collide. */
export function useTablePrefs(namespace: string) {
  const key = `forge.table.${namespace}`;

  const [prefs, setPrefs] = useState<TablePrefs>(DEFAULTS);

  useEffect(() => {
    try {
      const raw = window.localStorage.getItem(key);
      if (raw) setPrefs({ ...DEFAULTS, ...JSON.parse(raw) });
    } catch {
      // Corrupt preferences must not break the table.
    }
  }, [key]);

  const persist = useCallback(
    (next: TablePrefs) => {
      setPrefs(next);
      try {
        window.localStorage.setItem(key, JSON.stringify(next));
      } catch {
        // A disabled store is not worth failing a table over.
      }
    },
    [key],
  );

  /** Clicking the active column flips direction; a new column starts ascending. */
  const toggleSort = useCallback(
    (column: string) => {
      persist(
        prefs.sortKey === column
          ? {
              ...prefs,
              sortDirection: prefs.sortDirection === "asc" ? "desc" : "asc",
            }
          : { ...prefs, sortKey: column, sortDirection: "asc" },
      );
    },
    [prefs, persist],
  );

  const setDensity = useCallback(
    (density: Density) => persist({ ...prefs, density }),
    [prefs, persist],
  );

  const toggleColumn = useCallback(
    (column: string) =>
      persist({
        ...prefs,
        hiddenColumns: prefs.hiddenColumns.includes(column)
          ? prefs.hiddenColumns.filter((c) => c !== column)
          : [...prefs.hiddenColumns, column],
      }),
    [prefs, persist],
  );

  return { prefs, toggleSort, setDensity, toggleColumn };
}

/** Compares two values for a sort, treating null as last regardless of direction. */
export function compare(a: unknown, b: unknown): number {
  const left = a ?? null;
  const right = b ?? null;
  if (left === null && right === null) return 0;
  if (left === null) return 1;
  if (right === null) return -1;
  if (typeof left === "number" && typeof right === "number") {
    return left - right;
  }
  return String(left).localeCompare(String(right));
}
