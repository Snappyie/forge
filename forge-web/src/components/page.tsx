"use client";

/**
 * Shared page furniture for the console.
 *
 * Every list screen in this app is the same three things — a title, a toolbar,
 * and a dense table of rows — so those three things live here rather than being
 * re-typed per page. That is what makes `Jobs`, `Executions`, `Workers` and the
 * rest read as one product instead of five screens that happen to share a
 * stylesheet.
 */

import Link from "next/link";

import { cn } from "cn";
import {
  Table,
  TableCell,
  TableHead,
  TableRow,
} from "@/components/ui/table";
import { Skeleton } from "@/components/ui/skeleton";

/**
 * The title block for a page.
 *
 * The title is 15px semibold rather than a display size: this is a screen an
 * operator reads all day, and a large heading costs vertical space that a table
 * row needs more.
 */
export function PageHeader({
  title,
  description,
  actions,
  className,
}: {
  title: React.ReactNode;
  description?: React.ReactNode;
  actions?: React.ReactNode;
  className?: string;
}) {
  return (
    <div
      className={cn(
        "flex flex-wrap items-start justify-between gap-3 border-b border-border px-4 py-3",
        className,
      )}
    >
      <div className="min-w-0">
        <h1 className="text-[15px] leading-tight font-semibold tracking-tight">
          {title}
        </h1>
        {description ? (
          <p className="mt-0.5 text-[12.5px] text-muted-foreground">
            {description}
          </p>
        ) : null}
      </div>
      {actions ? (
        <div className="flex shrink-0 items-center gap-2">{actions}</div>
      ) : null}
    </div>
  );
}

/**
 * The bar between a page's title and its rows: filters on the left, view
 * controls on the right.
 *
 * Separated from the table by a border rather than by padding so the controls
 * read as one toolbar instead of drifting into the first row.
 */
export function Toolbar({
  children,
  className,
}: {
  children?: React.ReactNode;
  className?: string;
}) {
  return (
    <div
      className={cn(
        "flex min-h-10 flex-wrap items-center gap-2 border-b border-border px-4 py-2",
        className,
      )}
    >
      {children}
    </div>
  );
}

/** The standard bordered container a page's content lives in. */
export function PageFrame({
  children,
  className,
}: {
  children: React.ReactNode;
  className?: string;
}) {
  return (
    <div className={cn("flex min-h-0 flex-1 flex-col", className)}>{children}</div>
  );
}

/**
 * A dense data table.
 *
 * Three things make it a console table rather than a web table:
 *
 * - the header is sticky, so the column meanings survive a 200-row scroll;
 * - rows are short, so more of them are visible at once;
 * - the first column can be pinned, so a long horizontal scroll still shows
 *   which row it is.
 */
export function DataTable({
  children,
  className,
}: {
  children: React.ReactNode;
  className?: string;
}) {
  return <Table className={cn("text-[13px]", className)}>{children}</Table>;
}

/**
 * Table header cells.
 *
 * A header is a `th` with `scope="col"`, so a screen reader announces which
 * column a cell belongs to instead of reading numbers in isolation.
 */
export function DataTableHead({
  children,
  className,
  align = "left",
  ...props
}: React.ComponentProps<"th"> & { align?: "left" | "right" | "center" }) {
  return (
    <TableHead
      scope="col"
      className={cn(
        "h-8 bg-muted/60 px-3 text-[11px] font-medium tracking-wide text-muted-foreground uppercase",
        "sticky top-0 z-10 backdrop-blur-sm",
        align === "right" && "text-right",
        align === "center" && "text-center",
        className,
      )}
      {...props}
    >
      {children}
    </TableHead>
  );
}

/**
 * The pinned first column.
 *
 * `position: sticky` with an inset left edge, plus a right border that only
 * shows once the table is actually scrolled sideways. The scroll state lives on
 * the table element via a `data-scrolled` attribute, because a border on an
 * unscrolled table looks like a design mistake rather than a scroll cue.
 */
export function PinnedCell({ className, ...props }: React.ComponentProps<"td">) {
  return (
    <TableCell
      className={cn(
        "sticky left-0 z-[5] bg-card px-3 font-medium",
        "group-data-[scrolled=true]/table:shadow-[1px_0_0_var(--border)]",
        className,
      )}
      {...props}
    />
  );
}

export function DataTableCell({
  className,
  align = "left",
  ...props
}: React.ComponentProps<"td"> & { align?: "left" | "right" | "center" }) {
  return (
    <TableCell
      className={cn(
        "px-3",
        align === "right" && "text-right",
        align === "center" && "text-center",
        className,
      )}
      {...props}
    />
  );
}

/** A right-aligned numeric cell; numerals are compared, not read. */
export function NumCell({ className, ...props }: React.ComponentProps<"td">) {
  return (
    <TableCell
      className={cn("px-3 text-right tabular-nums", className)}
      {...props}
    />
  );
}

/**
 * A table row that is also the row's primary navigation.
 *
 * The label inside is a real link, so it can be opened in a new tab, copied, and
 * reached by keyboard with the same behaviour as any other link. The row itself
 * is only a hover target, never the sole affordance.
 */
export function DataTableRow({ className, ...props }: React.ComponentProps<typeof TableRow>) {
  return (
    <TableRow className={cn("group/row", className)} {...props} />
  );
}

/** The primary label inside a navigable row. */
export function RowLink({
  href,
  children,
  className,
}: {
  href: string;
  children: React.ReactNode;
  className?: string;
}) {
  return (
    <Link
      href={href}
      className={cn(
        "inline-flex items-center gap-1 hover:underline focus-visible:underline",
        "underline-offset-2",
        className,
      )}
    >
      {children}
    </Link>
  );
}

/**
 * Table skeleton rows.
 *
 * Real table geometry rather than a spinner: the column widths are already
 * known, so rendering them keeps the layout from jumping when data lands.
 */
export function TableSkeleton({
  rows = 8,
  columns = 5,
}: {
  rows?: number;
  columns?: number;
}) {
  return (
    <tbody>
      {Array.from({ length: rows }).map((_, rowIndex) => (
        <TableRow key={rowIndex}>
          {Array.from({ length: columns }).map((__, colIndex) => (
            <TableCell key={colIndex} className="px-3">
              <Skeleton
                className={cn(
                  "h-3.5",
                  colIndex === 0 ? "w-2/3" : "w-full",
                  // Vary the widths so the placeholder does not read as a
                  // repeating pattern of identical bars.
                  colIndex > 1 && rowIndex % 3 === 0 ? "w-3/4" : undefined,
                )}
              />
            </TableCell>
          ))}
        </TableRow>
      ))}
    </tbody>
  );
}

/**
 * The footer under a table: row counts on the left, paging on the right.
 *
 * Counts are phrased as what is *shown* versus the total, because "1-25 of 87"
 * is the honest answer to "how much am I looking at" while a bare "25" reads as
 * the whole set.
 */
export function TableFooter({
  shown,
  total,
  hasMore,
  children,
  className,
}: {
  shown: number;
  total?: number;
  hasMore?: boolean;
  children?: React.ReactNode;
  className?: string;
}) {
  return (
    <div
      className={cn(
        "flex flex-wrap items-center justify-between gap-2 border-t border-border px-4 py-2",
        "text-[12px] text-muted-foreground",
        className,
      )}
    >
      <p className="tabular-nums">
        {shown > 0 ? (
          <>
            <span className="text-foreground">{shown}</span>
            {total !== undefined && total > shown ? ` of ${total}` : ""}
            {hasMore ? " loaded so far" : ""}
          </>
        ) : (
          "No rows"
        )}
      </p>
      {children ? <div className="flex items-center gap-2">{children}</div> : null}
    </div>
  );
}

/**
 * A label/value pair for detail panels.
 *
 * `dt`/`dd` rather than two divs, so assistive technology reads the pair as a
 * labelled value instead of two unrelated strings.
 */
export function Field({
  label,
  children,
  className,
}: {
  label: string;
  children: React.ReactNode;
  className?: string;
}) {
  return (
    <div className={cn("min-w-0", className)}>
      <dt className="text-[11px] font-medium tracking-wide text-muted-foreground uppercase">
        {label}
      </dt>
      <dd className="mt-0.5 min-w-0 text-[13px]">{children}</dd>
    </div>
  );
}

/** A responsive grid of `Field`s. */
export function FieldGrid({
  children,
  columns = 3,
  className,
}: {
  children: React.ReactNode;
  columns?: 2 | 3 | 4;
  className?: string;
}) {
  return (
    <dl
      className={cn(
        "grid gap-x-4 gap-y-3",
        columns === 2 && "grid-cols-2",
        columns === 3 && "grid-cols-2 sm:grid-cols-3",
        columns === 4 && "grid-cols-2 sm:grid-cols-4",
        className,
      )}
    >
      {children}
    </dl>
  );
}

/**
 * A bordered panel used inside detail pages.
 *
 * Not a card: a card says "this is a thing", a panel says "this is a section of
 * the thing you are already looking at". Detail pages are one subject cut into
 * sections, so the frame has to be quieter than a card.
 */
export function Panel({
  title,
  description,
  actions,
  children,
  className,
  bodyClassName,
}: {
  title?: React.ReactNode;
  description?: React.ReactNode;
  actions?: React.ReactNode;
  children: React.ReactNode;
  className?: string;
  bodyClassName?: string;
}) {
  return (
    <section
      className={cn("rounded-md border border-border bg-card", className)}
    >
      {title ? (
        <div className="flex flex-wrap items-center justify-between gap-2 border-b border-border px-3 py-2">
          <div className="min-w-0">
            <h2 className="text-[13px] font-semibold">{title}</h2>
            {description ? (
              <p className="mt-0.5 text-[12px] text-muted-foreground">
                {description}
              </p>
            ) : null}
          </div>
          {actions ? (
            <div className="flex shrink-0 items-center gap-1.5">{actions}</div>
          ) : null}
        </div>
      ) : null}
      <div className={cn("p-3", bodyClassName)}>{children}</div>
    </section>
  );
}

/**
 * A plain `tabular-nums` figure with an optional label, for metric strips.
 *
 * A figure the system did not measure must not be rendered as a number, so
 * `value` accepts `null` and prints an em dash rather than a plausible `0`.
 */
export function Stat({
  label,
  value,
  hint,
  tone = "neutral",
  href,
  className,
}: {
  label: string;
  value: React.ReactNode;
  hint?: string;
  tone?: "neutral" | "info" | "success" | "warning" | "danger";
  href?: string;
  className?: string;
}) {
  const body = (
    <>
      <dt className="truncate text-[11px] font-medium tracking-wide text-muted-foreground uppercase">
        {label}
      </dt>
      <dd
        className={cn(
          "mt-0.5 text-[19px] leading-tight font-semibold tabular-nums",
          tone === "danger" && "text-danger-foreground",
          tone === "warning" && "text-warning-foreground",
          tone === "success" && "text-success-foreground",
          tone === "info" && "text-info-foreground",
          tone === "neutral" && "text-foreground",
        )}
      >
        {value}
      </dd>
      {hint ? (
        <p className="mt-0.5 truncate text-[11.5px] text-muted-foreground">
          {hint}
        </p>
      ) : null}
    </>
  );

  if (href) {
    return (
      <Link
        href={href}
        className={cn(
          "rounded-md border border-border bg-card px-3 py-2 transition-colors hover:bg-accent/50",
          className,
        )}
      >
        <dl className="min-w-0">{body}</dl>
      </Link>
    );
  }

  return (
    <div
      className={cn("rounded-md border border-border bg-card px-3 py-2", className)}
    >
      <dl className="min-w-0">{body}</dl>
    </div>
  );
}

/**
 * A horizontal bar used for queue depth and status ratios.
 *
 * `role="img"` with a label that states the numbers, so the bar is not a purely
 * visual cue: a screen reader hears "31 queued of 50" rather than nothing.
 */
export function Meter({
  value,
  max,
  label,
  tone = "neutral",
  className,
}: {
  value: number;
  max: number;
  label: string;
  tone?: "neutral" | "info" | "success" | "warning" | "danger";
  className?: string;
}) {
  // A zero max would divide by zero; show an empty track instead of NaN%.
  const percent = max > 0 ? Math.min(100, Math.round((value / max) * 100)) : 0;

  return (
    <div
      role="img"
      aria-label={`${label}: ${value} of ${max}`}
      className={cn("h-1.5 w-full overflow-hidden rounded-full bg-muted", className)}
    >
      <div
        className={cn(
          "h-full rounded-full",
          tone === "success" && "bg-success",
          tone === "danger" && "bg-danger",
          tone === "warning" && "bg-warning",
          tone === "info" && "bg-info",
          tone === "neutral" && "bg-primary",
        )}
        style={{ width: `${percent}%` }}
      />
    </div>
  );
}

/**
 * A compact segmented breakdown bar.
 *
 * Used for the execution-status split on the dashboard, where the useful
 * question is "what proportion of what", not "what is the absolute number".
 */
export function SplitBar({
  segments,
  className,
}: {
  segments: { label: string; value: number; tone: ToneName }[];
  className?: string;
}) {
  const total = segments.reduce((sum, s) => sum + s.value, 0);

  if (total === 0) {
    return (
      <div className={cn("h-2 rounded-full bg-muted", className)} aria-hidden />
    );
  }

  const fills: Record<ToneName, string> = {
    neutral: "bg-muted-foreground/45",
    info: "bg-info",
    success: "bg-success",
    warning: "bg-warning",
    danger: "bg-danger",
  };

  return (
    <div
      role="img"
      aria-label={segments
        .filter((s) => s.value > 0)
        .map((s) => `${s.label} ${s.value}`)
        .join(", ")}
      className={cn(
        "flex h-2 w-full overflow-hidden rounded-full bg-muted",
        className,
      )}
    >
      {segments
        .filter((s) => s.value > 0)
        .map((s) => (
          <span
            key={s.label}
            className={fills[s.tone]}
            style={{ width: `${(s.value / total) * 100}%` }}
          />
        ))}
    </div>
  );
}

export type ToneName = "neutral" | "info" | "success" | "warning" | "danger";

/**
 * A labelled swatch for a split bar's legend.
 *
 * The word carries the meaning; the swatch is decoration, so it is hidden from
 * assistive technology rather than announced as a colour name.
 */
export function LegendDot({
  tone,
  children,
  className,
}: {
  tone: ToneName;
  children: React.ReactNode;
  className?: string;
}) {
  const fills: Record<ToneName, string> = {
    neutral: "bg-muted-foreground/45",
    info: "bg-info",
    success: "bg-success",
    warning: "bg-warning",
    danger: "bg-danger",
  };
  return (
    <span
      className={cn(
        "inline-flex items-center gap-1.5 text-[12px] text-muted-foreground",
        className,
      )}
    >
      <span aria-hidden className={cn("size-1.5 rounded-full", fills[tone])} />
      {children}
    </span>
  );
}