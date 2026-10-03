"use client";

import { motion } from "framer-motion";
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Settings2, Webhook, Key, Database, RefreshCw, Download, GitBranch } from "lucide-react";

export default function SettingsPage() {
  return (
    <main className="p-8 relative min-h-screen bg-[radial-gradient(ellipse_at_top_right,_var(--tw-gradient-stops))] from-slate-50/50 via-background to-background dark:from-slate-900/10 dark:via-background dark:to-background">
      <header className="flex justify-between items-center mb-8">
        <div>
          <motion.h1 
            initial={{ y: -20, opacity: 0 }}
            animate={{ y: 0, opacity: 1 }}
            className="text-4xl font-bold tracking-tight mb-2 text-foreground flex items-center gap-3"
          >
            <Settings2 className="h-8 w-8 text-slate-500" /> Platform Settings
          </motion.h1>
          <p className="text-muted-foreground">Manage global configurations, integrations, and security.</p>
        </div>
      </header>

      <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
        <motion.div initial={{ opacity: 0, y: 20 }} animate={{ opacity: 1, y: 0 }} transition={{ delay: 0.1 }}>
          <Card className="bg-card/50 backdrop-blur-sm border-border/50 h-full">
            <CardHeader>
              <CardTitle className="flex items-center gap-2"><Key className="w-5 h-5 text-amber-500" /> API Keys & Service Accounts</CardTitle>
              <CardDescription>Manage programmatic access to the Forge API.</CardDescription>
            </CardHeader>
            <CardContent className="space-y-4">
              <div className="flex justify-between items-center p-3 border rounded-md">
                <div>
                  <div className="font-medium">Production CI/CD Deployer</div>
                  <div className="text-xs text-muted-foreground">Created 2 months ago • Last used 4 hours ago</div>
                </div>
                <Button variant="outline" size="sm">Revoke</Button>
              </div>
              <Button className="w-full" variant="outline">Generate New API Key</Button>
            </CardContent>
          </Card>
        </motion.div>

        <motion.div initial={{ opacity: 0, y: 20 }} animate={{ opacity: 1, y: 0 }} transition={{ delay: 0.2 }}>
          <Card className="bg-card/50 backdrop-blur-sm border-border/50 h-full">
            <CardHeader>
              <CardTitle className="flex items-center gap-2"><Webhook className="w-5 h-5 text-indigo-500" /> Webhooks</CardTitle>
              <CardDescription>Configure outgoing webhooks for external integrations.</CardDescription>
            </CardHeader>
            <CardContent className="space-y-4">
              <div className="flex justify-between items-center p-3 border rounded-md">
                <div>
                  <div className="font-medium">Slack Pager</div>
                  <div className="text-xs text-muted-foreground">Triggers on: Job Failure, SLA Violation</div>
                </div>
                <Button variant="outline" size="sm">Edit</Button>
              </div>
              <Button className="w-full" variant="outline">Add Webhook</Button>
            </CardContent>
          </Card>
        </motion.div>

        <motion.div initial={{ opacity: 0, y: 20 }} animate={{ opacity: 1, y: 0 }} transition={{ delay: 0.3 }}>
          <Card className="bg-card/50 backdrop-blur-sm border-border/50 h-full">
            <CardHeader>
              <CardTitle className="flex items-center gap-2"><GitBranch className="w-5 h-5 text-teal-500" /> Git / IaC Integration</CardTitle>
              <CardDescription>Sync jobs directly from your version control.</CardDescription>
            </CardHeader>
            <CardContent className="space-y-4">
              <div className="p-4 bg-muted/20 border rounded-md text-sm">
                Connected to GitHub repository <strong>acmecorp/forge-jobs</strong>. Main branch is currently synced.
              </div>
              <Button className="w-full" variant="outline"><RefreshCw className="w-4 h-4 mr-2" /> Force Sync Repository</Button>
            </CardContent>
          </Card>
        </motion.div>

        <motion.div initial={{ opacity: 0, y: 20 }} animate={{ opacity: 1, y: 0 }} transition={{ delay: 0.4 }}>
          <Card className="bg-card/50 backdrop-blur-sm border-border/50 h-full">
            <CardHeader>
              <CardTitle className="flex items-center gap-2"><Database className="w-5 h-5 text-rose-500" /> System & Data</CardTitle>
              <CardDescription>Database maintenance and data export.</CardDescription>
            </CardHeader>
            <CardContent className="space-y-4">
              <div className="flex flex-col gap-2">
                <Button variant="outline" className="justify-start"><Download className="w-4 h-4 mr-2" /> Export All Jobs (JSON)</Button>
                <Button variant="outline" className="justify-start"><Download className="w-4 h-4 mr-2" /> Export Audit Logs (CSV)</Button>
                <Button variant="destructive" className="justify-start mt-4">Enable Maintenance Mode</Button>
              </div>
            </CardContent>
          </Card>
        </motion.div>
      </div>
    </main>
  );
}
