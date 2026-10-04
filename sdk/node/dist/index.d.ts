export declare class JobContext {
    readonly executionId: string;
    readonly payload: any;
    private readonly baseUrl;
    private readonly apiKey;
    constructor(executionId: string, payload: any, baseUrl: string, apiKey: string);
    log(message: string): Promise<void>;
}
type JobHandler = (ctx: JobContext) => Promise<any> | any;
export declare class ForgeWorker {
    private readonly tenantId;
    private apiKey;
    private handlers;
    private isRunning;
    workerId: string;
    private workerHbInterval;
    private readonly baseUrl;
    constructor(baseUrl: string, tenantId?: string, apiKey?: string);
    withWorkerId(workerId: string): this;
    private request;
    register(name?: string, hostname?: string, capabilities?: string[]): Promise<any>;
    job(name: string, handler: JobHandler): void;
    start(queue: string, pollIntervalMs?: number): Promise<void>;
    stop(): void;
    private executeJob;
}
export {};
