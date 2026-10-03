"use client";

import { motion } from "framer-motion";
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from "@/components/ui/card";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { Badge } from "@/components/ui/badge";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Button } from "@/components/ui/button";
import { Key, Lock, Users, PlusCircle, Trash2, Eye, EyeOff, MoreHorizontal } from "lucide-react";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
  DialogFooter
} from "@/components/ui/dialog";

export default function SettingsPage() {
  return (
    <main className="p-8 relative min-h-screen bg-[radial-gradient(ellipse_at_top_right,_var(--tw-gradient-stops))] from-indigo-50/50 via-background to-background dark:from-indigo-900/10 dark:via-background dark:to-background">
      <header className="flex justify-between items-center mb-8">
        <div>
          <motion.h1 
            initial={{ y: -20, opacity: 0 }}
            animate={{ y: 0, opacity: 1 }}
            className="text-4xl font-bold tracking-tight mb-2 text-foreground"
          >
            Workspace Settings
          </motion.h1>
          <p className="text-muted-foreground">Manage access, secrets, and billing for Acme Corp.</p>
        </div>
      </header>

      <motion.div
        initial={{ opacity: 0, y: 20 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ delay: 0.1 }}
      >
        <Tabs defaultValue="api-keys" className="w-full">
          <TabsList className="mb-6 bg-card/50 backdrop-blur-sm border border-border/50 p-1">
            <TabsTrigger value="api-keys" className="data-[state=active]:bg-indigo-600 data-[state=active]:text-white">
              <Key className="w-4 h-4 mr-2" /> API Keys
            </TabsTrigger>
            <TabsTrigger value="secrets" className="data-[state=active]:bg-indigo-600 data-[state=active]:text-white">
              <Lock className="w-4 h-4 mr-2" /> Secrets Vault
            </TabsTrigger>
            <TabsTrigger value="team" className="data-[state=active]:bg-indigo-600 data-[state=active]:text-white">
              <Users className="w-4 h-4 mr-2" /> Team Members
            </TabsTrigger>
          </TabsList>
          
          {/* API KEYS TAB */}
          <TabsContent value="api-keys">
            <Card className="bg-card/50 backdrop-blur-sm shadow-sm border-border/50">
              <CardHeader className="flex flex-row items-center justify-between">
                <div>
                  <CardTitle>API Keys</CardTitle>
                  <CardDescription>
                    Manage programmatic access to your tenant.
                  </CardDescription>
                </div>
                <Dialog>
                  <DialogTrigger 
                    render={
                      <Button className="bg-indigo-600 hover:bg-indigo-700 text-white" />
                    }
                  >
                    <PlusCircle className="mr-2 h-4 w-4" /> Generate Key
                  </DialogTrigger>
                  <DialogContent className="sm:max-w-[425px]">
                    <DialogHeader>
                      <DialogTitle>Generate API Key</DialogTitle>
                      <DialogDescription>
                        Create a new key. The secret will only be shown once.
                      </DialogDescription>
                    </DialogHeader>
                    <div className="grid gap-4 py-4">
                      <div className="flex flex-col space-y-2">
                        <label className="text-sm font-medium">Key Description</label>
                        <input className="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-indigo-500" placeholder="e.g. Production CI/CD Bot" />
                      </div>
                    </div>
                    <DialogFooter>
                      <Button className="bg-indigo-600 text-white">Generate</Button>
                    </DialogFooter>
                  </DialogContent>
                </Dialog>
              </CardHeader>
              <CardContent>
                <div className="rounded-md border border-border/50">
                  <Table>
                    <TableHeader>
                      <TableRow className="hover:bg-transparent bg-muted/20">
                        <TableHead>Description</TableHead>
                        <TableHead>Prefix</TableHead>
                        <TableHead>Created</TableHead>
                        <TableHead>Last Used</TableHead>
                        <TableHead className="text-right">Actions</TableHead>
                      </TableRow>
                    </TableHeader>
                    <TableBody>
                      <TableRow className="group">
                        <TableCell className="font-medium">Production CI/CD Bot</TableCell>
                        <TableCell className="font-mono text-muted-foreground">forge_prod_***</TableCell>
                        <TableCell className="text-muted-foreground">Oct 1, 2026</TableCell>
                        <TableCell className="text-muted-foreground">2 hours ago</TableCell>
                        <TableCell className="text-right">
                          <Button variant="ghost" size="icon" className="text-rose-500 opacity-0 group-hover:opacity-100 transition-opacity">
                            <Trash2 className="h-4 w-4" />
                          </Button>
                        </TableCell>
                      </TableRow>
                      <TableRow className="group">
                        <TableCell className="font-medium">Local Development (Neel)</TableCell>
                        <TableCell className="font-mono text-muted-foreground">forge_dev_***</TableCell>
                        <TableCell className="text-muted-foreground">Sep 15, 2026</TableCell>
                        <TableCell className="text-muted-foreground">Never</TableCell>
                        <TableCell className="text-right">
                          <Button variant="ghost" size="icon" className="text-rose-500 opacity-0 group-hover:opacity-100 transition-opacity">
                            <Trash2 className="h-4 w-4" />
                          </Button>
                        </TableCell>
                      </TableRow>
                    </TableBody>
                  </Table>
                </div>
              </CardContent>
            </Card>
          </TabsContent>

          {/* SECRETS VAULT TAB */}
          <TabsContent value="secrets">
            <Card className="bg-card/50 backdrop-blur-sm shadow-sm border-border/50">
              <CardHeader className="flex flex-row items-center justify-between">
                <div>
                  <CardTitle>Secrets Vault</CardTitle>
                  <CardDescription>
                    Securely inject environment variables into your executed jobs.
                  </CardDescription>
                </div>
                <Dialog>
                  <DialogTrigger 
                    render={
                      <Button className="bg-indigo-600 hover:bg-indigo-700 text-white" />
                    }
                  >
                    <PlusCircle className="mr-2 h-4 w-4" /> Add Secret
                  </DialogTrigger>
                  <DialogContent className="sm:max-w-[425px]">
                    <DialogHeader>
                      <DialogTitle>Add Secret</DialogTitle>
                      <DialogDescription>
                        Key-value pair that will be encrypted at rest.
                      </DialogDescription>
                    </DialogHeader>
                    <div className="grid gap-4 py-4">
                      <div className="flex flex-col space-y-2">
                        <label className="text-sm font-medium">Key Name</label>
                        <input className="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm uppercase font-mono" placeholder="DATABASE_URL" />
                      </div>
                      <div className="flex flex-col space-y-2">
                        <label className="text-sm font-medium">Value</label>
                        <input type="password" className="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm" placeholder="••••••••••••" />
                      </div>
                    </div>
                    <DialogFooter>
                      <Button className="bg-indigo-600 text-white">Save Secret</Button>
                    </DialogFooter>
                  </DialogContent>
                </Dialog>
              </CardHeader>
              <CardContent>
                <div className="rounded-md border border-border/50">
                  <Table>
                    <TableHeader>
                      <TableRow className="hover:bg-transparent bg-muted/20">
                        <TableHead>Key</TableHead>
                        <TableHead>Value</TableHead>
                        <TableHead>Updated</TableHead>
                        <TableHead className="text-right">Actions</TableHead>
                      </TableRow>
                    </TableHeader>
                    <TableBody>
                      <TableRow className="group">
                        <TableCell className="font-mono text-sm font-medium">AWS_ACCESS_KEY_ID</TableCell>
                        <TableCell className="font-mono text-muted-foreground text-xs">•••••••••••••••••</TableCell>
                        <TableCell className="text-muted-foreground text-sm">Oct 2, 2026</TableCell>
                        <TableCell className="text-right">
                          <Button variant="ghost" size="icon" className="opacity-0 group-hover:opacity-100 transition-opacity">
                            <MoreHorizontal className="h-4 w-4" />
                          </Button>
                        </TableCell>
                      </TableRow>
                      <TableRow className="group">
                        <TableCell className="font-mono text-sm font-medium">STRIPE_SECRET_KEY</TableCell>
                        <TableCell className="font-mono text-muted-foreground text-xs">•••••••••••••••••</TableCell>
                        <TableCell className="text-muted-foreground text-sm">Sep 28, 2026</TableCell>
                        <TableCell className="text-right">
                          <Button variant="ghost" size="icon" className="opacity-0 group-hover:opacity-100 transition-opacity">
                            <MoreHorizontal className="h-4 w-4" />
                          </Button>
                        </TableCell>
                      </TableRow>
                    </TableBody>
                  </Table>
                </div>
              </CardContent>
            </Card>
          </TabsContent>

          {/* TEAM TAB */}
          <TabsContent value="team">
            <Card className="bg-card/50 backdrop-blur-sm shadow-sm border-border/50">
              <CardHeader className="flex flex-row items-center justify-between">
                <div>
                  <CardTitle>Team Members</CardTitle>
                  <CardDescription>
                    Manage users and their permissions within this workspace.
                  </CardDescription>
                </div>
                <Button className="bg-indigo-600 hover:bg-indigo-700 text-white">
                  <PlusCircle className="mr-2 h-4 w-4" /> Invite Member
                </Button>
              </CardHeader>
              <CardContent>
                <div className="rounded-md border border-border/50">
                  <Table>
                    <TableHeader>
                      <TableRow className="hover:bg-transparent bg-muted/20">
                        <TableHead>User</TableHead>
                        <TableHead>Email</TableHead>
                        <TableHead>Role</TableHead>
                        <TableHead className="text-right">Actions</TableHead>
                      </TableRow>
                    </TableHeader>
                    <TableBody>
                      <TableRow className="group">
                        <TableCell className="font-medium flex items-center gap-2">
                          <div className="h-6 w-6 rounded-full bg-gradient-to-tr from-indigo-500 to-purple-500 flex items-center justify-center text-[10px] font-bold text-white shadow-inner">
                            ND
                          </div>
                          Neel D.
                        </TableCell>
                        <TableCell className="text-muted-foreground">neel@example.com</TableCell>
                        <TableCell>
                          <Badge className="bg-indigo-100 text-indigo-700 dark:bg-indigo-500/10 dark:text-indigo-400 border-none shadow-none">Owner</Badge>
                        </TableCell>
                        <TableCell className="text-right">
                           <Button variant="ghost" size="icon" disabled>
                            <MoreHorizontal className="h-4 w-4 text-muted-foreground/50" />
                          </Button>
                        </TableCell>
                      </TableRow>
                    </TableBody>
                  </Table>
                </div>
              </CardContent>
            </Card>
          </TabsContent>

        </Tabs>
      </motion.div>
    </main>
  );
}
