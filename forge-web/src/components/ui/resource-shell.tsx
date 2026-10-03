import { ReactNode } from "react";
import { motion } from "framer-motion";
import { ArrowLeft } from "lucide-react";
import Link from "next/link";
import { Button } from "./button";

interface ResourceShellProps {
  title: string;
  subtitle?: string;
  statusBadge?: ReactNode;
  backUrl?: string;
  actions?: ReactNode;
  children: ReactNode;
  tabs?: ReactNode;
}

export function ResourceShell({ title, subtitle, statusBadge, backUrl, actions, children, tabs }: ResourceShellProps) {
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
        {tabs}
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
