"""Drives one real execution through the Python SDK against a live server.

Run against a server started with registration closed so the first account
claims the one-time bootstrap slot and becomes OWNER:

    python3 examples/e2e.py
"""
import json
import os
import time
import sys
import urllib.request

sys.path.insert(0, os.path.abspath(os.path.join(os.path.dirname(__file__), "..")))

from forge_sdk import ForgeWorker

API = os.environ.get("FORGE_TEST_API", "http://localhost:3000/api/v1").rstrip("/")


def post(path: str, body: dict, token: str = "") -> dict:
    req = urllib.request.Request(
        API + path,
        data=json.dumps(body).encode(),
        headers={
            "content-type": "application/json",
            **({"authorization": f"Bearer {token}"} if token else {}),
        },
        method="POST",
    )
    with urllib.request.urlopen(req, timeout=15) as response:
        return json.loads(response.read()).get("data") or {}


def get(path: str, token: str) -> dict:
    req = urllib.request.Request(API + path, headers={"authorization": f"Bearer {token}"})
    with urllib.request.urlopen(req, timeout=15) as response:
        return json.loads(response.read()).get("data") or {}


def main() -> int:
    # Registration is closed for this run, so the script's own account claimed
    # the one-time bootstrap slot and became OWNER. Reuse its token: registering
    # again would be refused, and workers:register is all a worker needs.
    token = os.environ["FORGE_TEST_TOKEN"]
    tenant = os.environ["FORGE_TEST_TENANT"]
    # The queue is created once by the verification script and passed in: only
    # the first account to claim the one-time bootstrap slot becomes OWNER, so
    # each probe cannot create its own.
    queue = {
        "id": os.environ["FORGE_TEST_QUEUE_ID"],
        "name": os.environ["FORGE_TEST_QUEUE_NAME"],
    }

    worker = ForgeWorker(API, tenant, token).with_worker_id("python-e2e-1")
    worker.register(name="python-e2e")
    print(f"  registered: worker_id={worker.worker_id[:8]}... token acquired=True")

    @worker.job("Settle")
    def settle(ctx):
        ctx.log("settling payments from Python")
        return {"settled": 11}

    job = post(
        "/jobs",
        {
            "name": "Settle",
            "key": f"python-settle-{int(time.time())}",
            "default_queue_id": queue["id"],
        },
        token,
    )
    version = post(f"/jobs/{job['id']}/versions", {"execution_type": "WORKER_TASK"}, token)
    post(f"/jobs/{job['id']}/versions/{version['id']}/publish", {}, token)
    post(f"/jobs/{job['id']}/trigger", {}, token)

    claimed = worker._dequeue(queue["name"])
    if not claimed:
        print("  FAILED: dequeue returned nothing")
        return 1
    print(f"  claimed execution={claimed['id'][:8]}... lease_id={claimed['lease_id'][:8]}...")

    worker._execute(claimed)

    final = get(f"/executions/{claimed['id']}", token)
    print(f"  final execution status: {final.get('status')}")
    if final.get("status") != "SUCCEEDED":
        print(f"  UNEXPECTED: {final}")
        return 1
    print("  RESULT: the Python SDK executed a real job and the server recorded success")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
