export class JobContext {
  constructor(
    public readonly executionId: string,
    public readonly payload: any,
    private readonly baseUrl: string,
    private readonly apiKey: string
  ) {}

  public async log(message: string): Promise<void> {
    console.log(`[${this.executionId}] ${message}`);
    try {
      await fetch(`${this.baseUrl}/executions/${this.executionId}/logs`, {
        method: 'POST',
        headers: {
          'Authorization': `Bearer ${this.apiKey}`,
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({ message }),
      });
    } catch (err) {
      // Best effort logging
    }
  }
}

type JobHandler = (ctx: JobContext) => Promise<any> | any;

export class ForgeWorker {
  private handlers: Map<string, JobHandler> = new Map();
  private isRunning: boolean = false;
  public workerId: string = 'node-worker-1';
  private workerHbInterval: any = null;
  private readonly baseUrl: string;

  constructor(
    baseUrl: string,
    private readonly tenantId: string = 'default-tenant',
    private apiKey: string = ''
  ) {
    this.baseUrl = baseUrl.replace(/\/+$/, '');
  }

  public withWorkerId(workerId: string): this {
    this.workerId = workerId;
    return this;
  }

  private async request(method: string, path: string, body?: any): Promise<any> {
    const headers: Record<string, string> = {
      'Authorization': `Bearer ${this.apiKey}`,
      'Content-Type': 'application/json',
    };
    const init: RequestInit = {
      method,
      headers,
    };
    if (body !== undefined) {
      init.body = JSON.stringify(body);
    }
    const resp = await fetch(`${this.baseUrl}${path}`, init);
    if (!resp.ok && resp.status !== 200 && resp.status !== 201) {
      throw new Error(`Request to ${path} failed with HTTP ${resp.status}`);
    }
    const json = await resp.json().catch(() => null);
    return json?.data !== undefined ? json.data : json;
  }

  public async register(
    name: string = 'node-worker',
    hostname: string = 'localhost',
    capabilities: string[] = ['*']
  ): Promise<any> {
    const data = await this.request('POST', '/workers/register', {
      name,
      hostname,
      capabilities,
    });
    if (data?.id) {
      this.workerId = String(data.id);
    }
    if (data?.token) {
      this.apiKey = String(data.token);
    }
    return data;
  }

  public job(name: string, handler: JobHandler): void {
    this.handlers.set(name, handler);
  }

  public async start(queue: string, pollIntervalMs: number = 2000): Promise<void> {
    this.isRunning = true;
    console.log(`ForgeWorker started. Listening on queue '${queue}' for jobs: ${Array.from(this.handlers.keys()).join(', ')}`);

    // Worker heartbeat loop
    this.workerHbInterval = setInterval(async () => {
      try {
        await this.request('POST', `/workers/${this.workerId}/heartbeat`);
      } catch (e) {}
    }, 30000);

    while (this.isRunning) {
      try {
        const execution = await this.request('POST', `/queues/${queue}/dequeue`, {
          worker_id: this.workerId,
        });

        if (execution && execution.id) {
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
    if (this.workerHbInterval) {
      clearInterval(this.workerHbInterval);
      this.workerHbInterval = null;
    }
  }

  private async executeJob(execution: any): Promise<void> {
    const executionId = execution.id;
    const jobName = execution.job_name || execution.type;
    const payload = execution.input || {};

    const handler = this.handlers.get(jobName);
    if (!handler) {
      try {
        await this.request('POST', `/executions/${executionId}/fail`, {
          worker_id: this.workerId,
          error: `No handler registered for job: ${jobName}`,
        });
      } catch (e) {}
      return;
    }

    const ctx = new JobContext(executionId, payload, this.baseUrl, this.apiKey);
    let hbInterval: any = null;

    try {
      // Start execution heartbeat
      hbInterval = setInterval(async () => {
        try {
          await this.request('POST', `/executions/${executionId}/heartbeat`, {
            worker_id: this.workerId,
          });
        } catch (e) {}
      }, 15000);

      await ctx.log(`Starting execution of ${jobName}`);
      const result = await handler(ctx);

      await this.request('POST', `/executions/${executionId}/complete`, {
        worker_id: this.workerId,
        output: result,
      });
      await ctx.log(`Successfully completed ${jobName}`);

    } catch (error: any) {
      const errorMsg = error instanceof Error ? error.message : String(error);
      const stackTrace = error instanceof Error ? error.stack : undefined;

      await ctx.log(`Execution failed: ${errorMsg}`);
      try {
        await this.request('POST', `/executions/${executionId}/fail`, {
          worker_id: this.workerId,
          error: errorMsg,
          trace: stackTrace,
        });
      } catch (e) {}
    } finally {
      if (hbInterval) clearInterval(hbInterval);
    }
  }
}
