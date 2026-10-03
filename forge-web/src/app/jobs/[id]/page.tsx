"use client";

import React, { useEffect, useState } from "react";
import { useParams, useRouter } from "next/navigation";
import { ResourceShell } from "@/components/ui/resource-shell";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { PlayCircle, Settings, Trash2 } from "lucide-react";
import { useToast } from "@/hooks/use-toast";
import { ScheduleBuilder } from "@/components/ui/schedule-builder";
import { GitIntegrationPanel } from "@/components/ui/git-integration";
import { ConfigurationDiffViewer } from "@/components/ui/config-diff";
import { ProductionReadinessChecklist } from "@/components/ui/production-readiness";
import { JobCloneModal } from "@/components/ui/job-clone-modal";
import { ProductionGuardrail } from "@/components/ui/production-guardrail";
import { FavoriteToggle } from "@/components/ui/favorite-toggle";
import { NaturalLanguageScheduler } from "@/components/ui/natural-language-scheduler";

export default function JobDetailsPage() {
  const { id } = useParams();
  const router = useRouter();
  const { toast } = useToast();
  
  const [job, setJob] = useState<any>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [isCloneOpen, setIsCloneOpen] = useState(false);
  const [isDeleteGuardrailOpen, setIsDeleteGuardrailOpen] = useState(false);

  useEffect(() => {
    const fetchJob = async () => {
      try {
        setIsLoading(true);
        // Note: Our mock backend doesn't have a GET /jobs/:id endpoint right now,
        // so we'll fetch all and find the right one for this prototype.
        const res = await fetch("http://localhost:3000/api/v1/jobs");
        if (res.ok) {
          const json = await res.json();
          const found = (json.data || []).find((j: any) => j.id === id);
          if (found) setJob(found);
          else router.push("/jobs");
        }
      } catch (err) {
        toast({ title: "Error", description: "Failed to fetch job details.", variant: "destructive" });
      } finally {
        setIsLoading(false);
      }
    };
    fetchJob();
  }, [id, router, toast]);

  const handleDelete = () => {
    // In a real app we'd DELETE /api/v1/jobs/:id
    toast({ title: "Job Deleted", description: `${job?.name} has been archived.` });
    router.push("/jobs");
  };

  const handleClone = (newName: string, env: string) => {
    toast({ title: "Job Cloned", description: `Created ${newName} in ${env}` });
  };

  if (isLoading) return <div className="p-8 text-center text-muted-foreground">Loading job details...</div>;
  if (!job) return <div className="p-8 text-center text-muted-foreground">Job not found</div>;

  return (
    <ResourceShell
      title={
        <div className="flex items-center gap-2">
          {job.name} <FavoriteToggle initialFavorite={false} />
        </div>
      }
      subtitle={`ID: ${job.id}`}
      statusBadge={<Badge variant="outline" className="text-indigo-600 border-indigo-200 bg-indigo-50">{job.status}</Badge>}
      backUrl="/jobs"
      actions={
        <div className="flex gap-2">
          <Button variant="outline" onClick={() => setIsCloneOpen(true)}>Clone</Button>
          <Button variant="outline" className="text-red-500 hover:bg-red-50 hover:text-red-600" onClick={() => setIsDeleteGuardrailOpen(true)}>
            <Trash2 className="w-4 h-4 mr-2" /> Delete
          </Button>
          <Button className="bg-green-600 hover:bg-green-700 text-white">
            <PlayCircle className="w-4 h-4 mr-2" /> Trigger Now
          </Button>
        </div>
      }
    >
      <div className="grid grid-cols-1 xl:grid-cols-3 gap-6">
        <div className="xl:col-span-2 space-y-6">
          <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
            <ScheduleBuilder />
            <NaturalLanguageScheduler />
          </div>
          
          <ConfigurationDiffViewer />
        </div>

        <div className="space-y-6">
          <ProductionReadinessChecklist />
          <GitIntegrationPanel />
        </div>
      </div>

      <JobCloneModal 
        open={isCloneOpen} 
        onOpenChange={setIsCloneOpen} 
        originalJobName={job.name} 
        onClone={handleClone} 
      />

      <ProductionGuardrail 
        open={isDeleteGuardrailOpen}
        onOpenChange={setIsDeleteGuardrailOpen}
        resourceName={job.name}
        actionName="DELETE"
        onConfirm={handleDelete}
      />
    </ResourceShell>
  );
}
