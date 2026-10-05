"use strict";
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
Object.defineProperty(exports, "__esModule", { value: true });
exports.VERSION = exports.ForgeWorker = exports.JobContext = exports.LeaseLostError = exports.ForgeError = exports.RETRYABLE_BY_DEFAULT = exports.ErrorClass = void 0;
exports.classify = classify;
/** Error classes the server accepts. Mirrors `forge_domain::ErrorClass`. */
exports.ErrorClass = {
    TRANSIENT: 'TRANSIENT',
    DEPENDENCY_UNAVAILABLE: 'DEPENDENCY_UNAVAILABLE',
    TIMEOUT: 'TIMEOUT',
    RESOURCE_EXHAUSTED: 'RESOURCE_EXHAUSTED',
    RATE_LIMITED: 'RATE_LIMITED',
    VALIDATION: 'VALIDATION',
    AUTHENTICATION: 'AUTHENTICATION',
    AUTHORIZATION: 'AUTHORIZATION',
    NOT_FOUND: 'NOT_FOUND',
    CONFLICT: 'CONFLICT',
    CANCELLATION: 'CANCELLATION',
    PERMANENT: 'PERMANENT',
    INTERNAL: 'INTERNAL',
};
/** Classes the server retries by default. */
exports.RETRYABLE_BY_DEFAULT = new Set([
    exports.ErrorClass.TRANSIENT,
    exports.ErrorClass.DEPENDENCY_UNAVAILABLE,
    exports.ErrorClass.TIMEOUT,
    exports.ErrorClass.RESOURCE_EXHAUSTED,
]);
/** An error returned by the Forge API, carrying its structured envelope. */
class ForgeError extends Error {
    code;
    status;
    requestId;
    details;
    constructor(message, options = {}) {
        // The request id is included because it is the only handle an operator has
        // for finding this failure in a server log.
        super(options.requestId
            ? `${message} (code=${options.code ?? 'UNKNOWN'}, request_id=${options.requestId})`
            : `${message} (code=${options.code ?? 'UNKNOWN'})`);
        this.name = 'ForgeError';
        this.code = options.code ?? 'UNKNOWN';
        this.status = options.status ?? 0;
        this.requestId = options.requestId;
        this.details = options.details;
    }
    /** Whether retrying could plausibly succeed. */
    get temporary() {
        return (this.status >= 500 ||
            this.status === 429 ||
            this.code === exports.ErrorClass.DEPENDENCY_UNAVAILABLE ||
            this.code === 'UNREACHABLE');
    }
}
exports.ForgeError = ForgeError;
/**
 * The lease expired or was reassigned while this worker held the work.
 *
 * Distinct because the response is specific: stop working on it. Reporting
 * success after losing the lease is how a side effect gets applied twice.
 */
class LeaseLostError extends ForgeError {
    constructor(message) {
        super(message, { code: exports.ErrorClass.CONFLICT, status: 409 });
        this.name = 'LeaseLostError';
    }
}
exports.LeaseLostError = LeaseLostError;
/** What a handler is given for one execution. */
class JobContext {
    executionId;
    jobName;
    payload;
    /** Travels with the execution and must be sent back on every report. */
    leaseId;
    workerId;
    options;
    constructor(executionId, jobName, payload, leaseId, workerId, options) {
        this.executionId = executionId;
        this.jobName = jobName;
        this.payload = payload;
        this.leaseId = leaseId;
        this.workerId = workerId;
        this.options = options;
    }
    /**
     * Streams a log line back to Forge.
     *
     * Best-effort by design: a log line is never worth failing an execution over,
     * so a transport failure is recorded rather than thrown.
     */
    async log(message, stream = 'stdout') {
        console.log(`[${this.executionId}] ${message}`);
        try {
            await this.options.post(`/executions/${this.executionId}/logs`, { stream, message }, 5_000);
        }
        catch (cause) {
            this.options.recordLogFailure?.(cause instanceof Error ? cause.message : String(cause));
        }
    }
    logf(format, ...args) {
        return this.log(format.replace(/%s/g, String(args[0] ?? '')));
    }
}
exports.JobContext = JobContext;
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
class ForgeWorker {
    workerId = 'node-worker-1';
    /** Replaced by the worker token when `register()` succeeds. */
    apiKey;
    baseUrl;
    tenantId;
    handlers = new Map();
    requestTimeoutMs;
    running = false;
    stopping = false;
    workerHeartbeat;
    constructor(baseUrl, tenantId = 'default', apiKey = '', options = {}) {
        this.baseUrl = baseUrl.replace(/\/+$/, '');
        this.tenantId = tenantId;
        this.apiKey = apiKey;
        this.requestTimeoutMs = options.requestTimeoutMs ?? 15_000;
    }
    withWorkerId(workerId) {
        this.workerId = workerId;
        return this;
    }
    /** The credential in use. A worker token after `register()`. */
    get token() {
        return this.apiKey;
    }
    /** Associates a handler with a job name. */
    job(name, handler) {
        this.handlers.set(name, handler);
        return this;
    }
    /**
     * Registers with the server, acquiring a worker id and worker token.
     *
     * Needs an operator credential: a worker token cannot register itself.
     */
    async register(name = 'node-worker', hostname = 'localhost', capabilities = ['*']) {
        const data = await this.request('POST', '/workers/register', { name, hostname, capabilities }, 15_000);
        if (typeof data.id === 'string' && data.id)
            this.workerId = data.id;
        if (typeof data.token === 'string' && data.token)
            this.apiKey = data.token;
        return data;
    }
    /**
     * Polls the queue until `stop()` is called.
     *
     * Rejects rather than looping when no handler is registered: polling a queue
     * nothing can execute only produces failures.
     */
    async start(queue, pollIntervalMs = 2_000) {
        if (this.handlers.size === 0) {
            throw new Error('ForgeWorker: no handlers registered; call job(...) before start()');
        }
        this.running = true;
        this.stopping = false;
        console.log(`Forge worker ${this.workerId} listening on queue "${queue}" for ` +
            `${Array.from(this.handlers.keys()).join(', ')}`);
        // A worker that stops reporting is marked OFFLINE and is filtered out of
        // dispatch, so the worker itself must heartbeat, not only its executions.
        this.workerHeartbeat = setInterval(() => {
            void this.request('POST', `/workers/${this.workerId}/heartbeat`, {}, 5_000).catch(() => undefined);
        }, 30_000);
        const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
        try {
            while (this.running && !this.stopping) {
                try {
                    const execution = await this.pollOnce(queue);
                    if (execution) {
                        await this.execute(execution);
                        continue;
                    }
                }
                catch (cause) {
                    const error = toForgeError(cause);
                    console.error(`ForgeWorker: error polling queue ${queue}: ${error.message}`);
                    if (error.status === 401 || error.status === 403) {
                        // A credential problem will not fix itself by polling harder, and a
                        // tight loop against it buries the reason.
                        throw error;
                    }
                }
                await sleep(pollIntervalMs);
            }
        }
        finally {
            this.running = false;
            this.stop();
        }
    }
    /**
     * Claims a single execution, or `null` when the queue is empty.
     *
     * Separate from `start` because a serverless or event-driven worker wants one
     * claim per invocation, and a test wants to drive the protocol deterministically.
     */
    async pollOnce(queue) {
        const data = await this.request('POST', `/queues/${queue}/dequeue`, { worker_id: this.workerId }, 
        // A claim can legitimately wait for work, so it gets more room than an
        // ordinary call - but not a hardcoded one, which would ignore a caller's
        // configured timeout entirely and leave a hung queue inescapable.
        Math.max(this.requestTimeoutMs, 20_000));
        if (!data || typeof data.id !== 'string')
            return null;
        // The lease travels with the work; dropping it is what made a completion
        // after lease expiry an unrecoverable 409.
        return data;
    }
    /**
     * Runs a claimed execution to completion and reports its outcome.
     *
     * Separate from `start` for the same reason as `pollOnce`: the caller decides
     * the lifecycle.
     */
    async execute(execution) {
        const executionId = execution.id;
        const jobName = execution.job_name ?? execution.type ?? '';
        const leaseId = execution.lease_id;
        const payload = execution.input ?? {};
        const handler = this.handlers.get(jobName);
        const logFailures = [];
        const ctx = new JobContext(executionId, jobName, payload, leaseId, this.workerId, {
            baseUrl: this.baseUrl,
            token: this.apiKey,
            post: (path, body, timeoutMs) => this.request('POST', path, body, timeoutMs),
            // `push` returns a length; the callback is typed as void, so the result is
            // discarded explicitly rather than widening the signature.
            recordLogFailure: (reason) => {
                logFailures.push(reason);
            },
        });
        if (!handler) {
            await this.fail(executionId, leaseId, `no handler registered for job "${jobName}"`, exports.ErrorClass.NOT_FOUND).catch(() => undefined);
            return;
        }
        // The lease is renewed while the handler runs; without it a long handler
        // outlives its lease and the work is re-dispatched underneath it.
        const heartbeat = setInterval(() => {
            void this.request('POST', `/executions/${executionId}/heartbeat`, { worker_id: this.workerId, lease_id: leaseId }, 5_000).catch(() => undefined);
        }, 10_000);
        try {
            await ctx.log(`starting ${jobName}`);
            const result = await handler(ctx);
            await this.complete(executionId, leaseId, result);
            await ctx.log(`completed ${jobName}`);
        }
        catch (cause) {
            if (cause instanceof LeaseLostError) {
                // The lease is gone, so this worker no longer owns the execution.
                // Reporting a success would apply the side effect a second time; the
                // right action is to stop and let the reaper retry it.
                await ctx.log(`lease lost during ${jobName}; abandoning without reporting an outcome`);
                return;
            }
            if (this.stopping && cause instanceof Error && cause.name === 'AbortError') {
                // Shutting down mid-execution. Reporting nothing leaves the lease to
                // expire and the work to be retried, which is correct: reporting a
                // failure would look like the work had been attempted.
                await ctx.log(`abandoning ${jobName}: worker shutting down`);
                return;
            }
            const error = toForgeError(cause);
            await ctx.log(`${jobName} failed: ${error.message}`);
            await this.fail(executionId, leaseId, error.message, classify(cause), cause instanceof Error ? cause.stack : undefined).catch(() => undefined);
        }
        finally {
            clearInterval(heartbeat);
            if (logFailures.length > 0) {
                // Surfaced rather than swallowed: a worker whose every log line is
                // refused is invisible to an operator debugging a job.
                console.warn(`ForgeWorker: ${logFailures.length} log line(s) could not be delivered`);
            }
        }
    }
    /**
     * Reports a successful execution.
     *
     * Throws {@link LeaseLostError} when the lease has gone, so the caller can
     * distinguish "someone else owns this now" from "the network broke".
     */
    async complete(executionId, leaseId, output) {
        await this.request('POST', `/executions/${executionId}/complete`, {
            worker_id: this.workerId,
            lease_id: leaseId,
            succeeded: true,
            output,
        }, this.requestTimeoutMs);
    }
    /**
     * Reports a failure with a classification.
     *
     * `errorClass` decides whether the execution is retried; omitting it makes
     * every failure PERMANENT, which is what made retries unreachable from Node.
     */
    async fail(executionId, leaseId, message, errorClass, trace) {
        const body = {
            worker_id: this.workerId,
            lease_id: leaseId,
            succeeded: false,
            error: message,
            error_class: errorClass,
        };
        if (trace)
            body.trace = trace;
        await this.request('POST', `/executions/${executionId}/fail`, body, this.requestTimeoutMs);
    }
    /** Stops polling and the heartbeat. */
    stop() {
        this.stopping = true;
        this.running = false;
        if (this.workerHeartbeat) {
            clearInterval(this.workerHeartbeat);
            this.workerHeartbeat = undefined;
        }
    }
    /**
     * Issues one request and unwraps the standard envelope.
     *
     * Every call goes through here, which makes "every request is bounded" and
     * "errors are structured" properties of the SDK rather than things each call
     * site has to remember.
     */
    async request(method, path, body, timeoutMs = this.requestTimeoutMs) {
        const controller = new AbortController();
        // Without this a hung server hangs the worker forever: `fetch` has no
        // default timeout, so an unresponsive Forge would leave every pending
        // promise alive indefinitely.
        const timer = setTimeout(() => controller.abort(), timeoutMs);
        let response;
        try {
            response = await fetch(`${this.baseUrl}${path}`, {
                method,
                headers: {
                    Authorization: `Bearer ${this.apiKey}`,
                    'Content-Type': 'application/json',
                },
                body: body === undefined ? undefined : JSON.stringify(body),
                signal: controller.signal,
            });
        }
        catch (cause) {
            const aborted = controller.signal.aborted;
            throw aborted
                ? new ForgeError(`request to ${path} timed out after ${timeoutMs}ms`, {
                    code: exports.ErrorClass.TIMEOUT,
                })
                : new ForgeError(`could not reach Forge at ${path}: ${cause instanceof Error ? cause.message : String(cause)}`, { code: 'UNREACHABLE' });
        }
        finally {
            clearTimeout(timer);
        }
        const raw = await response.text();
        if (!response.ok) {
            let parsed = {};
            try {
                parsed = JSON.parse(raw);
            }
            catch {
                parsed = {};
            }
            const error = parsed.error ?? {};
            // A 409 on a completion or a failure means the lease is gone. Surfaced
            // as its own type so the caller stops rather than reporting a success.
            if (response.status === 409) {
                throw new LeaseLostError(typeof error.message === 'string'
                    ? error.message
                    : `the lease for ${path} is no longer held`);
            }
            throw new ForgeError(typeof error.message === 'string'
                ? error.message
                : `Forge returned HTTP ${response.status} for ${path}`, {
                code: typeof error.code === 'string' ? error.code : 'HTTP_ERROR',
                status: response.status,
                requestId: typeof error.request_id === 'string' ? error.request_id : undefined,
                details: Array.isArray(error.details)
                    ? error.details
                    : undefined,
            });
        }
        if (raw.trim() === '')
            return {};
        try {
            const parsed = JSON.parse(raw);
            return parsed.data ?? {};
        }
        catch {
            throw new ForgeError(`could not decode the response from ${path}`, {
                code: 'BAD_RESPONSE',
                status: response.status,
            });
        }
    }
}
exports.ForgeWorker = ForgeWorker;
/** Coerces anything thrown into a `ForgeError`. */
function toForgeError(cause) {
    if (cause instanceof ForgeError)
        return cause;
    return new ForgeError(cause instanceof Error ? cause.message : String(cause));
}
/**
 * Picks an error class for an error a handler did not classify.
 *
 * Getting this roughly right is what makes the platform's retry policy usable
 * from Node: an unreachable dependency or a timeout is worth another attempt,
 * while bad input is not. Defaulting everything to PERMANENT would leave the
 * retry machinery unreachable.
 */
function classify(cause) {
    if (cause instanceof LeaseLostError)
        return exports.ErrorClass.CONFLICT;
    if (cause instanceof ForgeError) {
        // The server already classified it; keep its answer rather than
        // second-guessing it, which is how a validation failure ends up retried.
        if (Object.values(exports.ErrorClass).includes(cause.code)) {
            return cause.code;
        }
        switch (cause.status) {
            case 400:
                return exports.ErrorClass.VALIDATION;
            case 401:
                return exports.ErrorClass.AUTHENTICATION;
            case 403:
                return exports.ErrorClass.AUTHORIZATION;
            case 404:
                return exports.ErrorClass.NOT_FOUND;
            case 409:
                return exports.ErrorClass.CONFLICT;
            case 429:
                return exports.ErrorClass.RATE_LIMITED;
            default:
                return cause.temporary ? exports.ErrorClass.TRANSIENT : exports.ErrorClass.PERMANENT;
        }
    }
    if (cause instanceof TypeError)
        return exports.ErrorClass.VALIDATION;
    if (cause instanceof RangeError)
        return exports.ErrorClass.VALIDATION;
    if (cause instanceof Error &&
        (cause.name === 'AbortError' || cause.name === 'TimeoutError')) {
        return exports.ErrorClass.TIMEOUT;
    }
    // Optimistic on purpose: retrying a wrongly-classified failure is recoverable,
    // while not retrying a transient one silently loses work.
    return exports.ErrorClass.TRANSIENT;
}
exports.VERSION = '0.2.0';
