import { ReactNode } from "react";
import { motion } from "framer-motion";
import { ArrowLeft } from "lucide-react";
import Link from "next/link";
import { Button } from "./button";

/** One entry in the resource header's tab strip. */
export interface ResourceTab {
  id: string;
  label: string;
}

/** One step of the trail above the title. */
export interface Crumb {
  label: string;
  href?: string;
}

interface ResourceShellProps {
  /** Accepts a node so a title can carry inline controls. */
  title: React.ReactNode;
  subtitle?: string;
  statusBadge?: ReactNode;
  backUrl?: string;
  actions?: ReactNode;
  children: ReactNode;
  /**
   * Tab descriptors, rendered as a strip. Accepting a descriptor array rather
   * than pre-built nodes means a caller cannot pass an unrendered array, which
   * TypeScript previously allowed and React then refused to render.
   */
  tabs?: ResourceTab[];
  /** The trail above the title; the last entry is the current page. */
  breadcrumbs?: Crumb[];
  /** Which tab is selected on first render. */
  defaultTab?: string;
}

export function ResourceShell({
  title,
  subtitle,
  statusBadge,
  backUrl,
  actions,
  children,
  tabs,
  breadcrumbs,
}: ResourceShellProps) {
  return (
    <main className="flex flex-col min-h-screen bg-background">
      <header className="px-6 py-6 border-b border-border/50 bg-card/30 backdrop-blur-md sticky top-0 z-20 shrink-0">
        <div className="flex justify-between items-start mb-4">
          <div className="flex items-center gap-4">
            {backUrl && (
              <Link href={backUrl} className="text-muted-foreground hover:text-foreground transition-colors">
                <Button variant="ghost" size="icon" className="h-8 w-8 rounded-full">
                  <ArrowLeft className="w-4 h-4" />
                </Button>
              </Link>
            )}
            <div>
              {breadcrumbs && breadcrumbs.length > 0 ? (
                <nav aria-label="Breadcrumb" className="mb-1 flex items-center gap-1 text-xs">
                  {breadcrumbs.map((crumb, index) => (
                    <span key={`${crumb.label}-${index}`} className="flex items-center gap-1">
                      {index > 0 ? (
                        <span aria-hidden className="text-muted-foreground">
                          /
                        </span>
                      ) : null}
                      {crumb.href ? (
                        <Link
                          href={crumb.href}
                          className="text-muted-foreground hover:text-foreground"
                        >
                          {crumb.label}
                        </Link>
                      ) : (
                        <span className="text-muted-foreground">{crumb.label}</span>
                      )}
                    </span>
                  ))}
                </nav>
              ) : null}
              <div className="flex items-center gap-3">
                <h1 className="text-2xl font-bold tracking-tight">{title}</h1>
                {statusBadge}
              </div>
              {subtitle && <p className="text-sm text-muted-foreground mt-1">{subtitle}</p>}
            </div>
          </div>
          <div className="flex items-center gap-2">
            {actions}
          </div>
        </div>
        {tabs && tabs.length > 0 ? (
          <nav aria-label="Sections" className="flex gap-1 border-b border-border/50">
            {tabs.map((tab) => (
              <span
                key={tab.id}
                className="border-b-2 border-primary px-3 py-2 text-sm font-medium text-foreground"
              >
                {tab.label}
              </span>
            ))}
          </nav>
        ) : null}
      </header>
      
      <div className="flex-1 p-6 overflow-auto relative">
        <motion.div
          initial={{ opacity: 0, y: 10 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ duration: 0.2 }}
        >
          {children}
        </motion.div>
      </div>
    </main>
  );
}
