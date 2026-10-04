import axios, { AxiosInstance } from 'axios';

export class JobContext {
  constructor(
    public readonly executionId: string,
    public readonly payload: any,
    private readonly api: AxiosInstance
  ) {}

  public async log(message: string): Promise<void> {
    console.log(`[${this.executionId}] ${message}`);
    try {
      await this.api.post(`/executions/${this.executionId}/logs`, { message });
    } catch (err) {
      // Best effort logging
    }
  }
}

type JobHandler = (ctx: JobContext) => Promise<any> | any;

export class ForgeWorker {
  private api: AxiosInstance;
  private handlers: Map<string, JobHandler> = new Map();
  private isRunning: boolean = false;

  constructor(
    private readonly baseUrl: string,
    private readonly tenantId: string,
    private readonly apiKey: string
  ) {
    this.api = axios.create({
      baseURL: this.baseUrl,
      headers: { Authorization: `Bearer ${this.apiKey}` },
    });
  }

  public job(name: string, handler: JobHandler): void {
    this.handlers.set(name, handler);
  }

  public async start(queue: string, pollIntervalMs: number = 2000): Promise<void> {
    this.isRunning = true;
    console.log(`ForgeWorker started. Listening on queue '${queue}' for jobs: ${Array.from(this.handlers.keys()).join(', ')}`);

    while (this.isRunning) {
      try {
        const response = await this.api.post(`/queues/${queue}/dequeue`, {
          worker_id: 'node-worker-1'
        });

        if (response.status === 200 && response.data) {
          const execution = response.data;
          // Process asynchronously so we can continue polling
          this.executeJob(execution).catch(console.error);
        } else {
          await new Promise(resolve => setTimeout(resolve, pollIntervalMs));
        }
      } catch (error) {
        console.error(`Error polling queue ${queue}:`, error);
        await new Promise(resolve => setTimeout(resolve, pollIntervalMs));
      }
    }
  }

  public stop(): void {
    this.isRunning = false;
  }

  private async executeJob(execution: any): Promise<void> {
    const executionId = execution.id;
    const jobName = execution.job_name || execution.type;
    const payload = execution.payload || {};

    const handler = this.handlers.get(jobName);
    if (!handler) {
      try {
        await this.api.post(`/executions/${executionId}/fail`, {
          error: `No handler registered for job: ${jobName}`
        });
      } catch (e) {}
      return;
    }

    const ctx = new JobContext(executionId, payload, this.api);
    let hbInterval: NodeJS.Timeout | null = null;

    try {
      // Start heartbeat
      hbInterval = setInterval(async () => {
        try {
          await this.api.patch(`/executions/${executionId}/heartbeat`);
        } catch (e) {}
      }, 15000);

      await ctx.log(`Starting execution of ${jobName}`);
      const result = await handler(ctx);
      
      await this.api.post(`/executions/${executionId}/complete`, {
        output: result
      });
      await ctx.log(`Successfully completed ${jobName}`);

    } catch (error: any) {
      const errorMsg = error instanceof Error ? error.message : String(error);
      const stackTrace = error instanceof Error ? error.stack : undefined;
      
      await ctx.log(`Execution failed: ${errorMsg}`);
      try {
        await this.api.post(`/executions/${executionId}/fail`, {
          error: errorMsg,
          trace: stackTrace
        });
      } catch (e) {}
    } finally {
      if (hbInterval) clearInterval(hbInterval);
    }
  }
}
