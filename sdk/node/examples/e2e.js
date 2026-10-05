// Drives one real execution through the Node SDK against a live server.
const API = process.env.FORGE_TEST_API;

async function post(path, body, token) {
  const res = await fetch(API + path, {
    method: 'POST',
    headers: {
      'content-type': 'application/json',
      ...(token ? { authorization: `Bearer ${token}` } : {}),
    },
    body: JSON.stringify(body),
  });
  const raw = await res.text();
  if (!res.ok) throw new Error(`${path} -> ${res.status} ${raw}`);
  const parsed = JSON.parse(raw);
  return parsed.data ?? {};
}

async function get(path, token) {
  const res = await fetch(API + path, { headers: { authorization: `Bearer ${token}` } });
  const parsed = JSON.parse(await res.text());
  return parsed.data ?? {};
}

(async () => {
  const { ForgeWorker } = require(process.env.FORGE_SDK);

  // Registration is closed for this run, so the script's own account claimed
  // the one-time bootstrap slot and became OWNER. Reuse its token.
  const token = process.env.FORGE_TEST_TOKEN;
  const tenant = process.env.FORGE_TEST_TENANT;

  // The queue is created once by the verification script and passed in: only the
  // first account to claim the one-time bootstrap slot becomes OWNER, so each
  // probe cannot create its own.
  const q = { id: process.env.FORGE_TEST_QUEUE_ID, name: process.env.FORGE_TEST_QUEUE_NAME };
  const qid = q.id;
  const qname = q.name;

  const w = new ForgeWorker(API, tenant, token).withWorkerId('node-e2e-1');
  await w.register();
  console.log(`  registered: worker_id=${w.workerId.slice(0, 8)}... token replaced=${w.token !== token}`);

  w.job('Settle', async (ctx) => {
    await ctx.log('settling payments from Node');
    return { settled: 3 };
  });

  const job = await post('/jobs', { name: 'Settle', key: `node-settle-${Date.now()}`, default_queue_id: qid }, token);
  const ver = await post(`/jobs/${job.id}/versions`, { execution_type: 'WORKER_TASK' }, token);
  await post(`/jobs/${job.id}/versions/${ver.id}/publish`, {}, token);
  await post(`/jobs/${job.id}/trigger`, {}, token);

  const claimed = await w.pollOnce(qname);
  if (!claimed) throw new Error('dequeue returned nothing');
  console.log(`  claimed execution=${claimed.id.slice(0, 8)}... lease_id=${String(claimed.lease_id).slice(0, 8)}...`);

  await w.execute(claimed);

  const final = await get(`/executions/${claimed.id}`, token);
  console.log(`  final execution status: ${final.status}`);
  if (final.status !== 'SUCCEEDED') {
    console.log('  UNEXPECTED:', JSON.stringify(final));
    process.exit(1);
  }
  console.log('  RESULT: the Node SDK executed a real job and the server recorded success');
})().catch((e) => {
  console.error('  FAILED:', e.message);
  process.exit(1);
});
