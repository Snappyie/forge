"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.ForgeWorker = exports.JobContext = void 0;
class JobContext {
    executionId;
    payload;
    baseUrl;
    apiKey;
    constructor(executionId, payload, baseUrl, apiKey) {
        this.executionId = executionId;
        this.payload = payload;
        this.baseUrl = baseUrl;
        this.apiKey = apiKey;
    }
    async log(message) {
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
        }
        catch (err) {
            // Best effort logging
        }
    }
}
exports.JobContext = JobContext;
class ForgeWorker {
    tenantId;
    apiKey;
    handlers = new Map();
    isRunning = false;
    workerId = 'node-worker-1';
    workerHbInterval = null;
    baseUrl;
    constructor(baseUrl, tenantId = 'default-tenant', apiKey = '') {
        this.tenantId = tenantId;
        this.apiKey = apiKey;
        this.baseUrl = baseUrl.replace(/\/+$/, '');
    }
    withWorkerId(workerId) {
        this.workerId = workerId;
        return this;
    }
    async request(method, path, body) {
        const headers = {
            'Authorization': `Bearer ${this.apiKey}`,
            'Content-Type': 'application/json',
        };
        const init = {
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
    async register(name = 'node-worker', hostname = 'localhost', capabilities = ['*']) {
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
    job(name, handler) {
        this.handlers.set(name, handler);
    }
    async start(queue, pollIntervalMs = 2000) {
        this.isRunning = true;
        console.log(`ForgeWorker started. Listening on queue '${queue}' for jobs: ${Array.from(this.handlers.keys()).join(', ')}`);
        // Worker heartbeat loop
        this.workerHbInterval = setInterval(async () => {
            try {
                await this.request('POST', `/workers/${this.workerId}/heartbeat`);
            }
            catch (e) { }
        }, 30000);
        while (this.isRunning) {
            try {
                const execution = await this.request('POST', `/queues/${queue}/dequeue`, {
                    worker_id: this.workerId,
                });
                if (execution && execution.id) {
                    this.executeJob(execution).catch(console.error);
                }
                else {
                    await new Promise(resolve => setTimeout(resolve, pollIntervalMs));
                }
            }
            catch (error) {
                console.error(`Error polling queue ${queue}:`, error);
                await new Promise(resolve => setTimeout(resolve, pollIntervalMs));
            }
        }
    }
    stop() {
        this.isRunning = false;
        if (this.workerHbInterval) {
            clearInterval(this.workerHbInterval);
            this.workerHbInterval = null;
        }
    }
    async executeJob(execution) {
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
            }
            catch (e) { }
            return;
        }
        const ctx = new JobContext(executionId, payload, this.baseUrl, this.apiKey);
        let hbInterval = null;
        try {
            // Start execution heartbeat
            hbInterval = setInterval(async () => {
                try {
                    await this.request('POST', `/executions/${executionId}/heartbeat`, {
                        worker_id: this.workerId,
                    });
                }
                catch (e) { }
            }, 15000);
            await ctx.log(`Starting execution of ${jobName}`);
            const result = await handler(ctx);
            await this.request('POST', `/executions/${executionId}/complete`, {
                worker_id: this.workerId,
                output: result,
            });
            await ctx.log(`Successfully completed ${jobName}`);
        }
        catch (error) {
            const errorMsg = error instanceof Error ? error.message : String(error);
            const stackTrace = error instanceof Error ? error.stack : undefined;
            await ctx.log(`Execution failed: ${errorMsg}`);
            try {
                await this.request('POST', `/executions/${executionId}/fail`, {
                    worker_id: this.workerId,
                    error: errorMsg,
                    trace: stackTrace,
                });
            }
            catch (e) { }
        }
        finally {
            if (hbInterval)
                clearInterval(hbInterval);
        }
    }
}
exports.ForgeWorker = ForgeWorker;
