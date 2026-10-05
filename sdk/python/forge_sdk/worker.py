"""Forge worker SDK for Python.

Standard library only. The previous version imported ``requests`` at module
scope without declaring it anywhere, so ``import forge_sdk`` failed outright on
a clean machine -- the SDK was not importable at all, let alone installable.

Four behaviours here are load-bearing rather than cosmetic:

* **The lease id is captured and sent back.** ``dequeue`` returns one and the old
  code discarded it, which made ``CompletionGate`` unreachable: a completion
  after the lease expired returned 409 and the error was swallowed, so the work
  was re-dispatched and silently duplicated.
* **Failures carry an ``error_class``.** The server defaults an absent one to
  ``PERMANENT``, which is not retryable, so every failure terminated the
  execution and the whole retry/backoff machinery was unreachable from Python.
* **Every request has a timeout.** A hung API otherwise hangs the worker thread
  forever, and ``stop()`` could never interrupt it.
* **Errors are typed.** The server returns a structured envelope and it is
  surfaced rather than replaced with a bare ``HTTPError``.
"""

from __future__ import annotations

import json
import socket
import threading
import time
import traceback
import urllib.error
import urllib.request
from dataclasses import dataclass, field
from typing import Any, Callable, Dict, List, Optional

__version__ = "0.2.0"

# The error classes the server accepts. Mirrors `forge_domain::ErrorClass`;
# anything unrecognised is recorded as PERMANENT and therefore never retried.
TRANSIENT = "TRANSIENT"
DEPENDENCY_UNAVAILABLE = "DEPENDENCY_UNAVAILABLE"
TIMEOUT = "TIMEOUT"
RESOURCE_EXHAUSTED = "RESOURCE_EXHAUSTED"
RATE_LIMITED = "RATE_LIMITED"
VALIDATION = "VALIDATION"
AUTHENTICATION = "AUTHENTICATION"
AUTHORIZATION = "AUTHORIZATION"
NOT_FOUND = "NOT_FOUND"
CONFLICT = "CONFLICT"
CANCELLATION = "CANCELLATION"
PERMANENT = "PERMANENT"
INTERNAL = "INTERNAL"

#: Classes the server retries by default.
RETRYABLE_BY_DEFAULT = frozenset(
    {TRANSIENT, DEPENDENCY_UNAVAILABLE, TIMEOUT, RESOURCE_EXHAUSTED}
)


class ForgeError(Exception):
    """An error returned by the Forge API.

    Carries the server's structured envelope rather than a bare status code, so a
    handler can branch on ``code`` instead of parsing a message.
    """

    def __init__(
        self,
        message: str,
        *,
        code: str = "UNKNOWN",
        status: int = 0,
        request_id: Optional[str] = None,
        details: Optional[List[Dict[str, Any]]] = None,
    ) -> None:
        super().__init__(message)
        self.message = message
        self.code = code
        self.status = status
        self.request_id = request_id
        self.details = details or []

    def __str__(self) -> str:
        # The request id is included because it is the only handle an operator
        # has for finding this failure in a server log.
        if self.request_id:
            return f"{self.message} (code={self.code}, request_id={self.request_id})"
        return f"{self.message} (code={self.code})"


class LeaseLost(ForgeError):
    """The lease expired or was reassigned while this worker held the work.

    Distinct because the response is specific: stop working on it. Reporting
    success after losing the lease is how a side effect gets applied twice.
    """


@dataclass
class JobContext:
    """What a handler is given for one execution."""

    execution_id: str
    job_name: str
    payload: Dict[str, Any]
    lease_id: Optional[str]
    worker_id: str
    base_url: str
    token: str
    _log_failures: List[str] = field(default_factory=list)

    def log(self, message: str, stream: str = "stdout") -> None:
        """Stream a log line back to Forge.

        Best-effort by design: a log line is never worth failing an execution
        over, so a transport error is recorded locally rather than raised.
        """
        print(f"[{self.execution_id}] {message}")
        request = urllib.request.Request(
            f"{self.base_url}/executions/{self.execution_id}/logs",
            data=json.dumps({"stream": stream, "message": message}).encode("utf-8"),
            headers={
                "Authorization": f"Bearer {self.token}",
                "Content-Type": "application/json",
            },
            method="POST",
        )
        try:
            with urllib.request.urlopen(request, timeout=5.0):
                pass
        except Exception as exc:  # noqa: BLE001 - logging must not fail a run
            self._log_failures.append(str(exc))


class ForgeWorker:
    """Polls a queue and runs the registered handlers.

    Usage::

        worker = ForgeWorker(base_url, admin_token).with_worker_id("w-1")
        worker.register()
        worker.job("settle")(settle)
        worker.start("critical")
    """

    def __init__(self, base_url: str, tenant_id: str = "default", api_key: str = ""):
        self.base_url = base_url.rstrip("/")
        self.tenant_id = tenant_id
        self.token = api_key
        self.worker_id = "python-worker-1"
        self.handlers: Dict[str, Callable[[JobContext], Any]] = {}
        self._running = False
        self._stop_event = threading.Event()
        self._worker_hb_thread: Optional[threading.Thread] = None

    # -- configuration -----------------------------------------------------

    def with_worker_id(self, worker_id: str) -> "ForgeWorker":
        self.worker_id = worker_id
        return self

    def job(self, name: str) -> Callable[[Callable[[JobContext], Any]], Callable[[JobContext], Any]]:
        """Register a handler for a job name."""

        def decorator(func: Callable[[JobContext], Any]) -> Callable[[JobContext], Any]:
            self.handlers[name] = func
            return func

        return decorator

    # -- transport ---------------------------------------------------------

    def _request(
        self,
        method: str,
        path: str,
        body: Optional[Dict[str, Any]] = None,
        timeout: float = 10.0,
    ) -> Dict[str, Any]:
        """Issues one request, turning the server's envelope into a `ForgeError`.

        Every call goes through here, which makes "every request has a timeout"
        and "errors are structured" properties of the SDK rather than things
        each call site has to remember.
        """
        url = f"{self.base_url}{path}"
        data = json.dumps(body).encode("utf-8") if body is not None else None
        request = urllib.request.Request(
            url,
            data=data,
            headers={
                "Authorization": f"Bearer {self.token}",
                "Content-Type": "application/json",
            },
            method=method,
        )
        try:
            with urllib.request.urlopen(request, timeout=timeout) as response:
                raw = response.read().decode("utf-8") or "{}"
        except urllib.error.HTTPError as exc:
            raw = exc.read().decode("utf-8", "replace") or "{}"
            parsed = json.loads(raw) if raw.strip().startswith("{") else {}
            error = parsed.get("error") or {}
            raise ForgeError(
                error.get("message", f"HTTP {exc.code}"),
                code=error.get("code", "HTTP_ERROR"),
                status=exc.code,
                request_id=error.get("request_id"),
                details=error.get("details"),
            ) from exc
        except urllib.error.URLError as exc:
            raise ForgeError(
                f"could not reach Forge at {url}: {exc.reason}", code="UNREACHABLE"
            ) from exc
        except TimeoutError as exc:
            raise ForgeError(
                f"request to {url} timed out after {timeout}s", code=TIMEOUT
            ) from exc

        parsed = json.loads(raw) if raw.strip().startswith("{") else {}
        return parsed.get("data") or {}

    def _post(
        self, path: str, body: Optional[Dict[str, Any]] = None, timeout: float = 10.0
    ) -> Dict[str, Any]:
        return self._request("POST", path, body or {}, timeout)

    # -- lifecycle ---------------------------------------------------------

    def register(
        self,
        name: str = "python-worker",
        hostname: Optional[str] = None,
        capabilities: Optional[List[str]] = None,
    ) -> Dict[str, Any]:
        """Register with the server, obtaining a worker id and worker token.

        Needs an operator credential: a worker token holds only
        `workers:claim`, `workers:heartbeat` and `executions:write`, so a worker
        cannot mint itself.
        """
        data = self._post(
            "/workers/register",
            {
                "name": name,
                "hostname": hostname or socket.gethostname() or "localhost",
                "capabilities": capabilities or ["*"],
            },
            timeout=10.0,
        )
        if data.get("id"):
            self.worker_id = str(data["id"])
        if data.get("token"):
            self.token = str(data["token"])
        return data

    def start(self, queue: str, poll_interval: float = 2.0) -> None:
        """Poll the queue until `stop()` is called."""
        if not self.handlers:
            raise ValueError(
                "no handlers registered; decorate at least one function with "
                "worker.job(...)"
            )

        self._running = True
        self._stop_event.clear()
        print(
            f"ForgeWorker {self.worker_id} started. Listening on queue "
            f"'{queue}' for: {', '.join(self.handlers)}"
        )

        self._worker_hb_thread = threading.Thread(
            target=self._worker_heartbeat_loop, daemon=True
        )
        self._worker_hb_thread.start()

        while self._running:
            if self._stop_event.wait(poll_interval):
                break
            try:
                execution = self._dequeue(queue)
            except ForgeError as exc:
                # A refusal will not fix itself by polling harder, and a tight
                # loop against an auth failure buries the reason.
                print(f"error polling queue {queue}: {exc}")
                if exc.status in (401, 403):
                    self._stop_event.wait(max(poll_interval, 5.0))
                continue
            except Exception as exc:  # noqa: BLE001
                print(f"error polling queue {queue}: {exc}")
                continue

            if execution:
                self._execute(execution)
            else:
                self._stop_event.wait(poll_interval)

    def stop(self) -> None:
        """Stop polling and the background heartbeat."""
        self._running = False
        self._stop_event.set()
        if self._worker_hb_thread and self._worker_hb_thread.is_alive():
            self._worker_hb_thread.join(timeout=5.0)

    # -- worker protocol ---------------------------------------------------

    def _dequeue(self, queue: str) -> Optional[Dict[str, Any]]:
        """Claim one execution, keeping the lease id it comes back with."""
        data = self._post(
            f"/queues/{queue}/dequeue", {"worker_id": self.worker_id}, timeout=15.0
        )
        return data or None

    def _worker_heartbeat_loop(self) -> None:
        """Keep the worker itself marked alive."""
        while not self._stop_event.wait(30.0):
            try:
                self._post(f"/workers/{self.worker_id}/heartbeat", {}, timeout=5.0)
            except Exception:  # noqa: BLE001 - a missed beat is not fatal
                continue

    def _execution_heartbeat_loop(
        self, execution_id: str, lease_id: Optional[str], stop_event: threading.Event
    ) -> None:
        """Renew the lease while an execution runs.

        The lease id is included so the server can reject a heartbeat for a lease
        that has already been reassigned, rather than renewing someone else's.
        """
        while not stop_event.wait(10.0):
            try:
                self._post(
                    f"/executions/{execution_id}/heartbeat",
                    {"worker_id": self.worker_id, "lease_id": lease_id},
                    timeout=5.0,
                )
            except Exception:  # noqa: BLE001
                continue

    def _execute(self, execution: Dict[str, Any]) -> None:
        """Run one execution end to end, reporting its outcome."""
        execution_id = execution["id"]
        # `job_name` is the dispatch target; `type` is the older alias the server
        # also fills in.
        job_name = execution.get("job_name") or execution.get("type") or ""
        lease_id = execution.get("lease_id")
        payload = execution.get("input") or {}

        handler = self.handlers.get(job_name)
        if handler is None:
            self._fail(
                execution_id,
                lease_id,
                f"no handler registered for job: {job_name}",
                NOT_FOUND,
            )
            return

        ctx = JobContext(
            execution_id=execution_id,
            job_name=job_name,
            payload=payload,
            lease_id=lease_id,
            worker_id=self.worker_id,
            base_url=self.base_url,
            token=self.token,
        )

        stop_event = threading.Event()
        heartbeat = threading.Thread(
            target=self._execution_heartbeat_loop,
            args=(execution_id, lease_id, stop_event),
            daemon=True,
        )
        heartbeat.start()

        try:
            ctx.log(f"starting {job_name}")
            result = handler(ctx)
            self._complete(execution_id, lease_id, result)
            ctx.log(f"completed {job_name}")
        except LeaseLost:
            # The lease is gone, so this worker no longer owns the execution.
            # Reporting success would apply the side effect a second time; the
            # right action is to stop and let the reaper retry it.
            ctx.log(
                f"lease lost during {job_name}; abandoning without reporting "
                "an outcome"
            )
        except Exception as exc:  # noqa: BLE001 - a handler may raise anything
            error_trace = traceback.format_exc()
            ctx.log(f"{job_name} failed: {exc}")
            self._fail(
                execution_id,
                lease_id,
                str(exc),
                classify(exc),
                trace=error_trace,
            )
        finally:
            stop_event.set()
            heartbeat.join(timeout=5.0)

    def _complete(
        self,
        execution_id: str,
        lease_id: Optional[str],
        output: Any,
    ) -> None:
        try:
            self._post(
                f"/executions/{execution_id}/complete",
                {
                    "worker_id": self.worker_id,
                    "lease_id": lease_id,
                    "succeeded": True,
                    "output": output,
                },
            )
        except ForgeError as exc:
            if exc.status == 409:
                raise LeaseLost(
                    exc.message,
                    code=exc.code,
                    status=exc.status,
                    request_id=exc.request_id,
                ) from exc
            raise

    def _fail(
        self,
        execution_id: str,
        lease_id: Optional[str],
        message: str,
        error_class: str,
        trace: Optional[str] = None,
    ) -> None:
        """Report a failure with a classification.

        `error_class` is the field that decides whether the execution is
        retried. Omitting it made every failure PERMANENT, so retries never
        fired for Python workers.
        """
        try:
            self._post(
                f"/executions/{execution_id}/fail",
                {
                    "worker_id": self.worker_id,
                    "lease_id": lease_id,
                    "succeeded": False,
                    "error": message,
                    "error_class": error_class,
                    "trace": trace,
                },
            )
        except ForgeError as exc:
            # A lost lease is not a failure to report; the work will be retried.
            if exc.status != 409:
                print(f"could not report failure for {execution_id}: {exc}")


def classify(exc: BaseException) -> str:
    """Picks an error class for an exception a handler did not classify.

    Getting this roughly right is what makes the platform's retry policy usable
    from Python: an unreachable dependency or a timeout is worth another attempt,
    while a bad argument is not. Defaulting everything to PERMANENT would leave
    the retry machinery unreachable.
    """
    if isinstance(exc, TimeoutError):
        return TIMEOUT
    if isinstance(exc, ConnectionError):
        return DEPENDENCY_UNAVAILABLE
    if isinstance(exc, MemoryError):
        return RESOURCE_EXHAUSTED
    if isinstance(exc, (ValueError, TypeError, KeyError)):
        return VALIDATION
    if isinstance(exc, NotImplementedError):
        return PERMANENT
    return TRANSIENT


__all__ = [
    "ForgeWorker",
    "JobContext",
    "ForgeError",
    "LeaseLost",
    "classify",
    "RETRYABLE_BY_DEFAULT",
    "TRANSIENT",
    "DEPENDENCY_UNAVAILABLE",
    "TIMEOUT",
    "RESOURCE_EXHAUSTED",
    "RATE_LIMITED",
    "VALIDATION",
    "AUTHENTICATION",
    "AUTHORIZATION",
    "NOT_FOUND",
    "CONFLICT",
    "CANCELLATION",
    "PERMANENT",
    "INTERNAL",
    "__version__",
]