/**
 * Forge worker SDK for Node and TypeScript.
 *
 * Three behaviours are load-bearing rather than cosmetic, and all three were
 * absent before:
 *
 * - **The lease id is captured and sent back.** `dequeue` returns one and the
 *   previous code discarded it, which made the server's CompletionGate
 *   unreachable: a completion after the lease expired returned 409, the error
 *   was swallowed, and the work was re-dispatched with the side effect applied
 *   twice.
 * - **Failures carry an `errorClass`.** The server records an absent one as
 *   PERMANENT, which is not retryable, so every failure terminated the execution
 *   and the platform's retry machinery was unreachable from Node.
 * - **Every request is bounded.** `fetch` was called with no `AbortSignal`, so
 *   a hung API hung the worker forever and `stop()` could never interrupt it.
 */
/** Error classes the server accepts. Mirrors `forge_domain::ErrorClass`. */
export declare const ErrorClass: {
    readonly TRANSIENT: "TRANSIENT";
    readonly DEPENDENCY_UNAVAILABLE: "DEPENDENCY_UNAVAILABLE";
    readonly TIMEOUT: "TIMEOUT";
    readonly RESOURCE_EXHAUSTED: "RESOURCE_EXHAUSTED";
    readonly RATE_LIMITED: "RATE_LIMITED";
    readonly VALIDATION: "VALIDATION";
    readonly AUTHENTICATION: "AUTHENTICATION";
    readonly AUTHORIZATION: "AUTHORIZATION";
    readonly NOT_FOUND: "NOT_FOUND";
    readonly CONFLICT: "CONFLICT";
    readonly CANCELLATION: "CANCELLATION";
    readonly PERMANENT: "PERMANENT";
    readonly INTERNAL: "INTERNAL";
};
export type ErrorClassName = (typeof ErrorClass)[keyof typeof ErrorClass];
/** Classes the server retries by default. */
export declare const RETRYABLE_BY_DEFAULT: ReadonlySet<string>;
/** One execution as the server returns it from dequeue. */
export interface ClaimedExecution {
    id: string;
    /** The dispatch target: the job name. `type` is the older alias. */
    job_name?: string;
    type?: string;
    /** The lease that must accompany every heartbeat and completion. */
    lease_id?: string;
    input?: Record<string, unknown>;
}
/** An error returned by the Forge API, carrying its structured envelope. */
export declare class ForgeError extends Error {
    readonly code: string;
    readonly status: number;
    readonly requestId?: string;
    readonly details?: Array<Record<string, unknown>>;
    constructor(message: string, options?: {
        code?: string;
        status?: number;
        requestId?: string;
        details?: Array<Record<string, unknown>>;
    });
    /** Whether retrying could plausibly succeed. */
    get temporary(): boolean;
}
/**
 * The lease expired or was reassigned while this worker held the work.
 *
 * Distinct because the response is specific: stop working on it. Reporting
 * success after losing the lease is how a side effect gets applied twice.
 */
export declare class LeaseLostError extends ForgeError {
    constructor(message: string);
}
export interface JobContextOptions {
    baseUrl: string;
    token: string;
    /** Sends one log line. Bound by the worker so it shares its transport. */
    post: (path: string, body: Record<string, unknown>, timeoutMs: number) => Promise<unknown>;
    /** Records a log line the server refused, so a caller can notice. */
    recordLogFailure?: (reason: string) => void;
}
/** What a handler is given for one execution. */
export declare class JobContext {
    readonly executionId: string;
    readonly jobName: string;
    readonly payload: Record<string, unknown>;
    /** Travels with the execution and must be sent back on every report. */
    readonly leaseId?: string;
    readonly workerId: string;
    private readonly options;
    constructor(executionId: string, jobName: string, payload: Record<string, unknown>, leaseId: string | undefined, workerId: string, options: JobContextOptions);
    /**
     * Streams a log line back to Forge.
     *
     * Best-effort by design: a log line is never worth failing an execution over,
     * so a transport failure is recorded rather than thrown.
     */
    log(message: string, stream?: string): Promise<void>;
    logf(format: string, ...args: unknown[]): Promise<void>;
}
export type JobHandler = (ctx: JobContext) => unknown | Promise<unknown>;
export interface ForgeWorkerOptions {
    /** Overrides the default per-request timeout. */
    requestTimeoutMs?: number;
}
/**
 * Polls a queue and runs the registered handlers.
 *
 * ```ts
 * const worker = new ForgeWorker(baseUrl, 'tenant', adminToken).withWorkerId('w-1');
 * await worker.register();
 * worker.job('settle', async ctx => settle(ctx.payload));
 * await worker.start('critical');
 * ```
 */
export declare class ForgeWorker {
    workerId: string;
    /** Replaced by the worker token when `register()` succeeds. */
    private apiKey;
    private readonly baseUrl;
    private readonly tenantId;
    private readonly handlers;
    private readonly requestTimeoutMs;
    private running;
    private stopping;
    private workerHeartbeat?;
    constructor(baseUrl: string, tenantId?: string, apiKey?: string, options?: ForgeWorkerOptions);
    withWorkerId(workerId: string): this;
    /** The credential in use. A worker token after `register()`. */
    get token(): string;
    /** Associates a handler with a job name. */
    job(name: string, handler: JobHandler): this;
    /**
     * Registers with the server, acquiring a worker id and worker token.
     *
     * Needs an operator credential: a worker token cannot register itself.
     */
    register(name?: string, hostname?: string, capabilities?: string[]): Promise<Record<string, unknown>>;
    /**
     * Polls the queue until `stop()` is called.
     *
     * Rejects rather than looping when no handler is registered: polling a queue
     * nothing can execute only produces failures.
     */
    start(queue: string, pollIntervalMs?: number): Promise<void>;
    /**
     * Claims a single execution, or `null` when the queue is empty.
     *
     * Separate from `start` because a serverless or event-driven worker wants one
     * claim per invocation, and a test wants to drive the protocol deterministically.
     */
    pollOnce(queue: string): Promise<ClaimedExecution | null>;
    /**
     * Runs a claimed execution to completion and reports its outcome.
     *
     * Separate from `start` for the same reason as `pollOnce`: the caller decides
     * the lifecycle.
     */
    execute(execution: ClaimedExecution): Promise<void>;
    /**
     * Reports a successful execution.
     *
     * Throws {@link LeaseLostError} when the lease has gone, so the caller can
     * distinguish "someone else owns this now" from "the network broke".
     */
    complete(executionId: string, leaseId: string | undefined, output: unknown): Promise<void>;
    /**
     * Reports a failure with a classification.
     *
     * `errorClass` decides whether the execution is retried; omitting it makes
     * every failure PERMANENT, which is what made retries unreachable from Node.
     */
    fail(executionId: string, leaseId: string | undefined, message: string, errorClass: string, trace?: string): Promise<void>;
    /** Stops polling and the heartbeat. */
    stop(): void;
    /**
     * Issues one request and unwraps the standard envelope.
     *
     * Every call goes through here, which makes "every request is bounded" and
     * "errors are structured" properties of the SDK rather than things each call
     * site has to remember.
     */
    private request;
}
/**
 * Picks an error class for an error a handler did not classify.
 *
 * Getting this roughly right is what makes the platform's retry policy usable
 * from Node: an unreachable dependency or a timeout is worth another attempt,
 * while bad input is not. Defaulting everything to PERMANENT would leave the
 * retry machinery unreachable.
 */
export declare function classify(cause: unknown): ErrorClassName;
export declare const VERSION = "0.2.0";
