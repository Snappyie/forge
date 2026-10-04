"use client";

/**
 * Breadcrumbs (UI.md section 1).
 *
 * The spec lists breadcrumbs and a back button as part of the shell. Detail
 * pages replace their hand-rolled "All jobs" link with this so the trail reads
 * the same everywhere and reflects where the record actually sits.
 */

import Link from "next/link";
import { ChevronRight } from "lucide-react";

import {
  Breadcrumb,
  BreadcrumbItem,
  BreadcrumbLink,
  BreadcrumbList,
  BreadcrumbPage,
  BreadcrumbSeparator,
} from "@/components/ui/breadcrumb";

export interface Crumb {
  label: string;
  /** Omitted on the current page, which is rendered as the leaf. */
  href?: string;
}

export function PageBreadcrumb({ items }: { items: Crumb[] }) {
  if (items.length === 0) return null;

  return (
    <Breadcrumb>
      <BreadcrumbList>
        {items.map((item, index) => {
          const last = index === items.length - 1;
          return (
            <BreadcrumbItem key={`${item.label}-${index}`}>
              {last || !item.href ? (
                <BreadcrumbPage>{item.label}</BreadcrumbPage>
              ) : (
                <>
                  <BreadcrumbLink render={<Link href={item.href} />}>
                    {item.label}
                  </BreadcrumbLink>
                  <BreadcrumbSeparator />
                </>
              )}
            </BreadcrumbItem>
          );
        })}
      </BreadcrumbList>
    </Breadcrumb>
  );
}

/** A back link that matches the breadcrumb's first hop. */
export function BackLink({ href, label }: { href: string; label: string }) {
  return (
    <Link
      href={href}
      className="inline-flex items-center gap-1 text-xs text-muted-foreground hover:text-foreground"
    >
      <ChevronRight className="size-3 rotate-180" aria-hidden />
      {label}
    </Link>
  );
}
