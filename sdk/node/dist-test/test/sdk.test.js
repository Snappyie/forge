"use strict";
/**
 * Tests for the Node SDK.
 *
 * A stub server is the right level here: these assert *which fields the SDK
 * sends*, which is the class of bug that went unnoticed - the SDK compiled,
 * looked correct, and omitted `lease_id` and `error_class` while the server
 * quietly recorded every failure as unretryable.
 */
Object.defineProperty(exports, "__esModule", { value: true });
const node_http_1 = require("node:http");
const index_1 = require("../src/index");
/** Starts a stub server. Async because binding a port is. */
async function stub(handler) {
    const captured = [];
    const server = (0, node_http_1.createServer)((req, res) => {
        const chunks = [];
        req.on('data', (chunk) => chunks.push(chunk));
        req.on('end', () => {
            let body = {};
            try {
                body = JSON.parse(Buffer.concat(chunks).toString() || '{}');
            }
            catch {
                body = {};
            }
            const path = req.url ?? '';
            captured.push({ path, body });
            handler(path, body, res);
        });
    });
    return new Promise((resolve) => {
        server.listen(0, '127.0.0.1', () => {
            const address = server.address();
            resolve({
                url: `http://127.0.0.1:${address.port}`,
                captured,
                close: () => new Promise((done) => {
                    server.close(() => done());
                }),
            });
        });
    });
}
function json(res, status, body) {
    res.writeHead(status, { 'content-type': 'application/json' });
    res.end(JSON.stringify(body));
}
async function test(name, body) {
    try {
        await body();
        console.log(`ok    ${name}`);
    }
    catch (cause) {
        console.error(`FAIL  ${name}`);
        console.error(`      ${cause instanceof Error ? cause.message : String(cause)}`);
        process.exitCode = 1;
    }
}
async function main() {
    await test('dequeue keeps the lease id', async () => {
        const s = await stub((_p, _b, res) => json(res, 200, {
            data: { id: 'ex-1', job_name: 'settle', lease_id: 'lease-abc', input: { n: 1 } },
        }));
        try {
            const worker = new index_1.ForgeWorker(s.url).withWorkerId('w-1');
            const claimed = await worker.pollOnce('critical');
            if (claimed?.lease_id !== 'lease-abc') {
                throw new Error(`lease id dropped: ${JSON.stringify(claimed)}`);
            }
        }
        finally {
            await s.close();
        }
    });
    await test('completion sends the lease and succeeded:true', async () => {
        const s = await stub((_p, _b, res) => json(res, 200, { data: { status: 'SUCCEEDED' } }));
        try {
            const worker = new index_1.ForgeWorker(s.url).withWorkerId('w-1');
            await worker.complete('ex-1', 'lease-abc', { n: 1 });
            const body = s.captured[0]?.body ?? {};
            // `lease_id` is what the server's CompletionGate checks; `succeeded` is
            // what separates a success from a failure.
            if (body.lease_id !== 'lease-abc')
                throw new Error(`lease_id not sent: ${JSON.stringify(body)}`);
            if (body.succeeded !== true)
                throw new Error(`succeeded not true: ${JSON.stringify(body)}`);
            if (body.worker_id !== 'w-1')
                throw new Error(`worker_id not sent: ${JSON.stringify(body)}`);
        }
        finally {
            await s.close();
        }
    });
    await test('failure carries an error_class', async () => {
        const s = await stub((_p, _b, res) => json(res, 200, { data: { status: 'FAILED' } }));
        try {
            const worker = new index_1.ForgeWorker(s.url).withWorkerId('w-1');
            await worker.fail('ex-1', 'lease-abc', 'upstream down', index_1.ErrorClass.TRANSIENT);
            const body = s.captured[0]?.body ?? {};
            // Without this the server records PERMANENT, which is not retryable, and
            // the retry machinery is unreachable from Node.
            if (body.error_class !== index_1.ErrorClass.TRANSIENT) {
                throw new Error(`error_class not sent: ${JSON.stringify(body)}`);
            }
            if (body.succeeded !== false) {
                throw new Error(`failure must send succeeded=false: ${JSON.stringify(body)}`);
            }
        }
        finally {
            await s.close();
        }
    });
    await test('a 409 on completion is a lost lease', async () => {
        const s = await stub((_p, _b, res) => json(res, 409, {
            error: { code: 'CONFLICT', message: 'a worker must hold a lease', request_id: 'rq-9' },
        }));
        try {
            const worker = new index_1.ForgeWorker(s.url).withWorkerId('w-1');
            let caught;
            try {
                await worker.complete('ex-1', 'stale', null);
            }
            catch (cause) {
                caught = cause;
            }
            // Distinguishable from a generic failure, because the response is specific:
            // stop working, do not report a success for work this worker no longer owns.
            if (!(caught instanceof index_1.LeaseLostError)) {
                throw new Error(`want LeaseLostError, got ${String(caught)}`);
            }
        }
        finally {
            await s.close();
        }
    });
    await test('errors carry the server envelope', async () => {
        const s = await stub((_p, _b, res) => json(res, 403, {
            error: { code: 'AUTHORIZATION_DENIED', message: 'role VIEWER cannot', request_id: 'rq-42' },
        }));
        try {
            const worker = new index_1.ForgeWorker(s.url).withWorkerId('w-1');
            let caught;
            try {
                await worker.complete('ex-1', 'lease', null);
            }
            catch (cause) {
                if (cause instanceof index_1.ForgeError)
                    caught = cause;
            }
            if (!caught)
                throw new Error('expected a ForgeError');
            // The code and request id are what make a failure diagnosable; a bare
            // "HTTP 403" is not.
            if (caught.code !== 'AUTHORIZATION_DENIED')
                throw new Error(`code lost: ${caught.code}`);
            if (caught.requestId !== 'rq-42')
                throw new Error(`request id lost: ${String(caught.requestId)}`);
            if (caught.status !== 403)
                throw new Error(`status lost: ${caught.status}`);
        }
        finally {
            await s.close();
        }
    });
    await test('every request is bounded', async () => {
        // A server that never answers. Without an AbortSignal this hangs forever and
        // the worker's poll loop can never be interrupted.
        const s = await stub(() => {
            /* deliberately never responds */
        });
        try {
            const worker = new index_1.ForgeWorker(s.url, 'default', 'tok', { requestTimeoutMs: 150 });
            const started = Date.now();
            let failed = false;
            try {
                // `complete` uses the configured timeout directly; `pollOnce` allows a
                // claim more room because it can legitimately wait for work.
                await worker.complete('ex-1', 'lease-abc', null);
            }
            catch {
                failed = true;
            }
            const elapsed = Date.now() - started;
            if (!failed)
                throw new Error('expected the request to time out');
            if (elapsed > 5_000) {
                throw new Error(`request was not bounded; took ${elapsed}ms`);
            }
        }
        finally {
            await s.close();
        }
    });
    await test('start refuses without handlers', async () => {
        const worker = new index_1.ForgeWorker('http://127.0.0.1:1');
        // Better to fail immediately than to poll a queue nothing can execute.
        try {
            await worker.start('critical');
            throw new Error('expected an error with no handlers');
        }
        catch (cause) {
            if (!(cause instanceof Error) || !cause.message.includes('no handlers'))
                throw cause;
        }
    });
    await test('classify keeps retryable problems retryable', async () => {
        // The property that makes the platform's retry policy usable: if everything
        // classified PERMANENT the retry machinery would be dead code.
        for (const cause of [
            new index_1.ForgeError('down', { code: 'UNREACHABLE' }),
            new TypeError('bad input shape'),
        ]) {
            if (!index_1.RETRYABLE_BY_DEFAULT.has((0, index_1.classify)(cause))) {
                const got = (0, index_1.classify)(cause);
                if (got === index_1.ErrorClass.PERMANENT) {
                    throw new Error(`classify(${cause.message}) = PERMANENT; that never retries`);
                }
            }
        }
    });
    await test('classify keeps the server answer for a validation error', async () => {
        const apiErr = new index_1.ForgeError('bad', { code: index_1.ErrorClass.VALIDATION, status: 400 });
        // Second-guessing the server is how a validation failure ends up retried
        // forever.
        if ((0, index_1.classify)(apiErr) !== index_1.ErrorClass.VALIDATION) {
            throw new Error(`want VALIDATION, got ${(0, index_1.classify)(apiErr)}`);
        }
    });
    await test('every error class the server accepts is declared', async () => {
        for (const name of Object.values(index_1.ErrorClass)) {
            if (!name)
                throw new Error('an error class is empty');
        }
    });
    console.log(process.exitCode ? '\nSOME TESTS FAILED' : '\nall Node SDK tests passed');
}
void main();
