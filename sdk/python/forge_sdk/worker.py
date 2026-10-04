import time
import threading
import requests
import json
import traceback
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
                json={"message": message}
            )
        except Exception:
            pass # Best effort logging


class ForgeWorker:
    def __init__(self, base_url: str, tenant_id: str, api_key: str):
        self.base_url = base_url
        self.tenant_id = tenant_id
        self.token = api_key # Assuming api_key is used as Bearer token for simplicity
        self.handlers: Dict[str, Callable[[JobContext], Any]] = {}
        self._running = False

    def job(self, name: str):
        """Decorator to register a job handler."""
        def decorator(func: Callable[[JobContext], Any]):
            self.handlers[name] = func
            return func
        return decorator

    def _heartbeat_loop(self, execution_id: str, stop_event: threading.Event):
        """Send heartbeats every 15 seconds to prevent execution timeout."""
        while not stop_event.is_set():
            try:
                requests.patch(
                    f"{self.base_url}/executions/{execution_id}/heartbeat",
                    headers={"Authorization": f"Bearer {self.token}"}
                )
            except Exception:
                pass
            stop_event.wait(15.0)

    def _execute(self, execution: dict):
        """Execute a single job pulled from the queue."""
        execution_id = execution["id"]
        job_name = execution.get("job_name", execution.get("type")) # Dependent on API payload
        payload = execution.get("payload", {})

        handler = self.handlers.get(job_name)
        if not handler:
            # Reject or fail if we don't know how to handle this job
            try:
                requests.post(
                    f"{self.base_url}/executions/{execution_id}/fail",
                    headers={"Authorization": f"Bearer {self.token}"},
                    json={"error": f"No handler registered for job: {job_name}"}
                )
            except Exception:
                pass
            return

        ctx = JobContext(execution_id, payload, self.base_url, self.token)
        
        # Start heartbeat
        stop_event = threading.Event()
        hb_thread = threading.Thread(target=self._heartbeat_loop, args=(execution_id, stop_event))
        hb_thread.start()

        try:
            ctx.log(f"Starting execution of {job_name}")
            result = handler(ctx)
            # Mark complete
            requests.post(
                f"{self.base_url}/executions/{execution_id}/complete",
                headers={"Authorization": f"Bearer {self.token}"},
                json={"output": result}
            )
            ctx.log(f"Successfully completed {job_name}")
        except Exception as e:
            error_trace = traceback.format_exc()
            ctx.log(f"Execution failed: {e}\n{error_trace}")
            requests.post(
                f"{self.base_url}/executions/{execution_id}/fail",
                headers={"Authorization": f"Bearer {self.token}"},
                json={"error": str(e), "trace": error_trace}
            )
        finally:
            stop_event.set()
            hb_thread.join()

    def start(self, queue: str, poll_interval: float = 2.0):
        """Start polling the queue for jobs to execute."""
        self._running = True
        print(f"ForgeWorker started. Listening on queue '{queue}' for jobs: {list(self.handlers.keys())}")
        
        while self._running:
            try:
                response = requests.post(
                    f"{self.base_url}/queues/{queue}/dequeue",
                    headers={"Authorization": f"Bearer {self.token}"},
                    json={"worker_id": "python-worker-1"} # Unique identifier for this worker instance
                )
                
                if response.status_code == 200:
                    execution = response.json()
                    if execution:
                        # Process in a new thread or inline. We'll do inline for simplicity in this example.
                        self._execute(execution)
                    else:
                        time.sleep(poll_interval)
                else:
                    time.sleep(poll_interval)
            except Exception as e:
                print(f"Error polling queue {queue}: {e}")
                time.sleep(poll_interval)
