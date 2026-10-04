import time
import threading
import requests
import json
import traceback
import socket
from typing import Callable, Dict, Any, Optional

class JobContext:
    def __init__(self, execution_id: str, payload: dict, base_url: str, token: str):
        self.execution_id = execution_id
        self.payload = payload
        self._base_url = base_url
        self._token = token

    def log(self, message: str) -> None:
        """Stream a log message back to Forge for this execution."""
        print(f"[{self.execution_id}] {message}")
        try:
            requests.post(
                f"{self._base_url}/executions/{self.execution_id}/logs",
                headers={"Authorization": f"Bearer {self._token}"},
                json={"message": message},
                timeout=5.0
            )
        except Exception:
            pass # Best effort logging


class ForgeWorker:
    def __init__(self, base_url: str, tenant_id: str = "default-tenant", api_key: str = ""):
        self.base_url = base_url.rstrip("/")
        self.tenant_id = tenant_id
        self.token = api_key
        self.worker_id = "python-worker-1"
        self.handlers: Dict[str, Callable[[JobContext], Any]] = {}
        self._running = False
        self._stop_event = threading.Event()
        self._worker_hb_thread: Optional[threading.Thread] = None

    def with_worker_id(self, worker_id: str) -> "ForgeWorker":
        """Set the worker ID explicitly."""
        self.worker_id = worker_id
        return self

    def register(
        self,
        name: str = "python-worker",
        hostname: Optional[str] = None,
        capabilities: Optional[list] = None,
    ) -> dict:
        """Register worker with the Forge server, obtaining worker_id and a worker token."""
        hname = hostname or socket.gethostname() or "localhost"
        resp = requests.post(
            f"{self.base_url}/workers/register",
            headers={"Authorization": f"Bearer {self.token}"},
            json={
                "name": name,
                "hostname": hname,
                "capabilities": capabilities or ["*"],
            },
            timeout=10.0
        )
        if resp.status_code == 201:
            data = resp.json().get("data", {})
            if "id" in data:
                self.worker_id = str(data["id"])
            if "token" in data:
                self.token = str(data["token"])
            return data
        resp.raise_for_status()
        return {}

    def job(self, name: str):
        """Decorator to register a job handler."""
        def decorator(func: Callable[[JobContext], Any]):
            self.handlers[name] = func
            return func
        return decorator

    def _worker_heartbeat_loop(self):
        """Periodically heartbeat the worker so the server marks it ONLINE."""
        while not self._stop_event.is_set():
            try:
                requests.post(
                    f"{self.base_url}/workers/{self.worker_id}/heartbeat",
                    headers={"Authorization": f"Bearer {self.token}"},
                    timeout=5.0
                )
            except Exception:
                pass
            self._stop_event.wait(30.0)

    def _execution_heartbeat_loop(self, execution_id: str, stop_event: threading.Event):
        """Send heartbeats every 15 seconds to renew the execution lease."""
        while not stop_event.is_set():
            try:
                requests.post(
                    f"{self.base_url}/executions/{execution_id}/heartbeat",
                    headers={"Authorization": f"Bearer {self.token}"},
                    json={"worker_id": self.worker_id},
                    timeout=5.0
                )
            except Exception:
                pass
            stop_event.wait(15.0)

    def _execute(self, execution: dict):
        """Execute a single job pulled from the queue."""
        execution_id = execution["id"]
        job_name = execution.get("job_name", execution.get("type"))
        payload = execution.get("input") or {}

        handler = self.handlers.get(job_name)
        if not handler:
            try:
                requests.post(
                    f"{self.base_url}/executions/{execution_id}/fail",
                    headers={"Authorization": f"Bearer {self.token}"},
                    json={
                        "worker_id": self.worker_id,
                        "error": f"No handler registered for job: {job_name}"
                    },
                    timeout=5.0
                )
            except Exception:
                pass
            return

        ctx = JobContext(execution_id, payload, self.base_url, self.token)

        # Start execution heartbeat
        stop_event = threading.Event()
        hb_thread = threading.Thread(
            target=self._execution_heartbeat_loop,
            args=(execution_id, stop_event),
            daemon=True
        )
        hb_thread.start()

        try:
            ctx.log(f"Starting execution of {job_name}")
            result = handler(ctx)
            # Mark complete
            requests.post(
                f"{self.base_url}/executions/{execution_id}/complete",
                headers={"Authorization": f"Bearer {self.token}"},
                json={"worker_id": self.worker_id, "output": result},
                timeout=5.0
            )
            ctx.log(f"Successfully completed {job_name}")
        except Exception as e:
            error_trace = traceback.format_exc()
            ctx.log(f"Execution failed: {e}\n{error_trace}")
            try:
                requests.post(
                    f"{self.base_url}/executions/{execution_id}/fail",
                    headers={"Authorization": f"Bearer {self.token}"},
                    json={
                        "worker_id": self.worker_id,
                        "error": str(e),
                        "trace": error_trace
                    },
                    timeout=5.0
                )
            except Exception:
                pass
        finally:
            stop_event.set()
            hb_thread.join(timeout=1.0)

    def start(self, queue: str, poll_interval: float = 2.0):
        """Start worker heartbeat and poll the queue for jobs."""
        self._running = True
        self._stop_event.clear()
        print(f"ForgeWorker started. Listening on queue '{queue}' for jobs: {list(self.handlers.keys())}")

        self._worker_hb_thread = threading.Thread(target=self._worker_heartbeat_loop, daemon=True)
        self._worker_hb_thread.start()

        while self._running:
            try:
                response = requests.post(
                    f"{self.base_url}/queues/{queue}/dequeue",
                    headers={"Authorization": f"Bearer {self.token}"},
                    json={"worker_id": self.worker_id},
                    timeout=10.0
                )

                if response.status_code == 200:
                    execution = (response.json() or {}).get("data")
                    if execution:
                        self._execute(execution)
                    else:
                        time.sleep(poll_interval)
                else:
                    time.sleep(poll_interval)
            except Exception as e:
                print(f"Error polling queue {queue}: {e}")
                time.sleep(poll_interval)

    def stop(self):
        """Stop worker polling and background threads."""
        self._running = False
        self._stop_event.set()
        if self._worker_hb_thread:
            self._worker_hb_thread.join(timeout=2.0)
