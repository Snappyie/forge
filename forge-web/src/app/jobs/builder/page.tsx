"use client";

/**
 * Job creation (UI.md section 7).
 *
 * The spec is explicit that this should not be one giant form, so it is a
 * wizard. Every step writes through to the API: a draft is a real draft row and
 * resuming means loading it, not restoring local state.
 */

import { JobWizard } from "@/components/ui/job-wizard";

export default function JobBuilderPage() {
  return (
    <div className="flex flex-col gap-4 p-6">
      <header>
        <h1 className="text-lg font-semibold">Create a job</h1>
        <p className="text-xs text-muted-foreground">
          Saved as a draft until you publish a version. You can leave and resume.
        </p>
      </header>
      <JobWizard />
    </div>
  );
}
