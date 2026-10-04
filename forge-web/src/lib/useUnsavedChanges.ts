"use client";

/**
 * Unsaved-changes guard (UI.md section 61).
 *
 * Warns on tab close and on an in-app navigation away from a dirty form. The
 * in-app case uses `beforeunload` too, because Next's client router does not
 * always fire it — so both paths are covered.
 */

import { useCallback, useEffect } from "react";

/**
 * @param dirty   whether the form holds unsaved edits
 * @param message the warning text; browsers show their own copy, not this
 */
export function useUnsavedChanges(dirty: boolean, message = "You have unsaved changes.") {
  const handler = useCallback(
    (event: BeforeUnloadEvent) => {
      if (!dirty) return;
      event.preventDefault();
      // Browsers require returnValue to be set for the prompt to appear.
      event.returnValue = message;
      return message;
    },
    [dirty, message],
  );

  useEffect(() => {
    if (!dirty) return;
    window.addEventListener("beforeunload", handler);
    return () => window.removeEventListener("beforeunload", handler);
  }, [dirty, handler]);
}
