"use client";

import { useEffect, useState } from "react";
import { useRouter } from "next/navigation";
import { motion } from "framer-motion";
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from "@/components/ui/card";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { PlusCircle, Search, Settings2 } from "lucide-react";
import { Checkbox } from "@/components/ui/checkbox";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
  DialogFooter
} from "@/components/ui/dialog";
import { useToast } from "@/hooks/use-toast";
import { PowerfulFilterBar } from "@/components/ui/filter-bar";
import { BulkOperationsBar } from "@/components/ui/bulk-operations";

export default function JobsPage() {
  const router = useRouter();
  const { toast } = useToast();
  
  const [jobs, setJobs] = useState<any[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [newJobName, setNewJobName] = useState("");
  const [isDialogOpen, setIsDialogOpen] = useState(false);
  const [selectedJobs, setSelectedJobs] = useState<Set<string>>(new Set());

  const fetchJobs = async () => {
    try {
      setIsLoading(true);
      const res = await fetch("http://localhost:3000/api/v1/jobs");
      if (res.ok) {
        const json = await res.json();
        setJobs(json.data || []);
      }
    } catch (err) {
      console.error("Failed to fetch jobs:", err);
      toast({ title: "Error", description: "Failed to fetch jobs.", variant: "destructive" });
    } finally {
      setIsLoading(false);
    }
  };

  useEffect(() => {
    fetchJobs();
  }, []);

  const handleCreateJob = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!newJobName) return;
    
    try {
      const res = await fetch("http://localhost:3000/api/v1/jobs", {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
        },
        body: JSON.stringify({
          name: newJobName,
          tenant_id: "00000000-0000-0000-0000-000000000000" // Mock tenant for now
        }),
      });
      
      if (res.ok) {
        setNewJobName("");
        setIsDialogOpen(false);
        toast({ title: "Success", description: "Job created successfully." });
        fetchJobs(); // Refresh list
      } else {
        toast({ title: "Error", description: "Failed to create job.", variant: "destructive" });
      }
    } catch (err) {
      console.error("Failed to create job:", err);
      toast({ title: "Error", description: "Failed to create job.", variant: "destructive" });
    }
  };

  const toggleSelectAll = () => {
    if (selectedJobs.size === jobs.length) {
      setSelectedJobs(new Set());
    } else {
      setSelectedJobs(new Set(jobs.map(j => j.id)));
    }
  };

  const toggleSelectJob = (id: string, e: React.MouseEvent) => {
    e.stopPropagation();
    const newSelected = new Set(selectedJobs);
    if (newSelected.has(id)) {
      newSelected.delete(id);
    } else {
      newSelected.add(id);
    }
    setSelectedJobs(newSelected);
  };

  return (
    <main className="p-8 relative min-h-screen bg-[radial-gradient(ellipse_at_top_right,_var(--tw-gradient-stops))] from-indigo-50/50 via-background to-background dark:from-indigo-900/10 dark:via-background dark:to-background">
      <header className="flex justify-between items-center mb-8">
        <div>
          <motion.h1 
            initial={{ y: -20, opacity: 0 }}
            animate={{ y: 0, opacity: 1 }}
            className="text-4xl font-bold tracking-tight mb-2 text-foreground"
          >
            Job Definitions
          </motion.h1>
          <p className="text-muted-foreground">Manage templates and automation routines.</p>
        </div>
        
        <Dialog open={isDialogOpen} onOpenChange={setIsDialogOpen}>
          <DialogTrigger className="bg-indigo-600 hover:bg-indigo-700 text-white shadow-md transition-all hover:-translate-y-0.5 inline-flex shrink-0 items-center justify-center rounded-lg text-sm font-medium h-9 px-4 py-2">
            <PlusCircle className="mr-2 h-4 w-4" /> Create Job
          </DialogTrigger>
          <DialogContent className="sm:max-w-[425px]">
            <form onSubmit={handleCreateJob}>
              <DialogHeader>
                <DialogTitle>Create New Job</DialogTitle>
                <DialogDescription>
                  Define a new automation routine in your tenant.
                </DialogDescription>
              </DialogHeader>
              <div className="grid gap-4 py-4">
                <div className="flex flex-col space-y-2">
                  <label htmlFor="name" className="text-sm font-medium leading-none">Job Name</label>
                  <input 
                    id="name" 
                    value={newJobName}
                    onChange={(e) => setNewJobName(e.target.value)}
                    className="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-indigo-500" 
                    placeholder="e.g. data-etl-pipeline" 
                  />
                </div>
              </div>
              <DialogFooter>
                <Button type="submit" className="bg-indigo-600 hover:bg-indigo-700 text-white">Create Job</Button>
              </DialogFooter>
            </form>
          </DialogContent>
        </Dialog>
      </header>

      <PowerfulFilterBar />

      <motion.div
        initial={{ opacity: 0, y: 20 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ delay: 0.1 }}
      >
        <Card className="bg-card/50 backdrop-blur-sm shadow-sm border-border/50">
          <CardHeader>
            <div className="flex items-center justify-between">
              <div>
                <CardTitle className="text-foreground">Configured Jobs</CardTitle>
                <CardDescription className="text-muted-foreground">
                  View and edit existing job definitions.
                </CardDescription>
              </div>
              <div className="flex space-x-2">
                <Button variant="outline" size="sm"><Settings2 className="mr-2 h-4 w-4" />View</Button>
              </div>
            </div>
          </CardHeader>
          <CardContent>
            <Table>
              <TableHeader>
                <TableRow className="hover:bg-transparent bg-muted/20">
                  <TableHead className="w-12 text-center">
                     <Checkbox 
                        checked={jobs.length > 0 && selectedJobs.size === jobs.length}
                        onCheckedChange={toggleSelectAll} 
                     />
                  </TableHead>
                  <TableHead className="font-medium text-muted-foreground">ID</TableHead>
                  <TableHead className="font-medium text-muted-foreground">Name</TableHead>
                  <TableHead className="font-medium text-muted-foreground">Version</TableHead>
                  <TableHead className="font-medium text-muted-foreground">Status</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {isLoading ? (
                  <TableRow>
                    <TableCell colSpan={5} className="text-center py-8 text-muted-foreground">
                      Loading jobs from database...
                    </TableCell>
                  </TableRow>
                ) : jobs.length === 0 ? (
                  <TableRow>
                    <TableCell colSpan={5} className="text-center py-8 text-muted-foreground">
                      No jobs found. Create your first job above!
                    </TableCell>
                  </TableRow>
                ) : (
                  jobs.map(job => (
                    <TableRow 
                      key={job.id} 
                      className="transition-colors group hover:bg-muted/50 cursor-pointer"
                      onClick={() => router.push(`/jobs/${job.id}`)}
                    >
                      <TableCell className="text-center" onClick={(e) => toggleSelectJob(job.id, e)}>
                        <Checkbox checked={selectedJobs.has(job.id)} />
                      </TableCell>
                      <TableCell className="font-mono text-xs text-muted-foreground group-hover:text-foreground transition-colors">
                        {job.id.substring(0, 8)}...
                      </TableCell>
                      <TableCell className="font-medium">{job.name}</TableCell>
                      <TableCell className="text-muted-foreground">v1</TableCell>
                      <TableCell>
                        <Badge variant="outline" className="text-indigo-600 dark:text-indigo-400 border-indigo-200 dark:border-indigo-900 bg-indigo-50 dark:bg-indigo-900/20">
                          {job.status}
                        </Badge>
                      </TableCell>
                    </TableRow>
                  ))
                )}
              </TableBody>
            </Table>
          </CardContent>
        </Card>
      </motion.div>

      <BulkOperationsBar 
        selectedCount={selectedJobs.size} 
        onClear={() => setSelectedJobs(new Set())} 
      />
    </main>
  );
}
