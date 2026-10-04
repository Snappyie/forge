"use client";

/**
 * Toast helpers (UI.md section 58).
 *
 * Success toasts auto-dismiss; errors stay until dismissed; a toast can carry a
 * single action, which is what makes "Job failed" useful rather than merely
 * alarming.
 */

import { useToastManager } from "@/components/ui/toast";

interface ToastAction {
  label: string;
  onClick: () => void;
}

export function useToast() {
  const manager = useToastManager();

  /** The action renders as a button inside the toast. */
  const actionProps = (action?: ToastAction) =>
    action ? { label: action.label, onClick: action.onClick } : undefined;

  return {
    /** A completed action. Dismisses itself. */
    success: (title: string, action?: ToastAction) =>
      manager.add({
        type: "success",
        title,
        ...(actionProps(action) ? { actionProps: actionProps(action) } : {}),
        timeout: 4000,
      }),

    /** A failure. Stays until dismissed, because the user must act on it. */
    error: (
      title: string,
      description?: string,
      action?: ToastAction,
    ) =>
      manager.add({
        type: "error",
        title,
        ...(description ? { description } : {}),
        ...(actionProps(action) ? { actionProps: actionProps(action) } : {}),
        // Errors linger (UI.md section 58: "Stay for errors").
        timeout: 0,
      }),

    info: (title: string, description?: string) =>
      manager.add({
        type: "info",
        title,
        ...(description ? { description } : {}),
        timeout: 4000,
      }),
  };
}
