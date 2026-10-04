"""A dependency-free Forge API client.

Uses only the standard library so the example runs anywhere Python does, with
no `pip install` step. It covers the whole surface the demo exercises:
authentication, idempotent writes, cursor pagination, and the operational
endpoints.
"""

from __future__ import annotations

import json
import os
import urllib.error
import urllib.parse
import urllib.request
from dataclasses import dataclass
from typing import Any, Iterator


class ForgeError(RuntimeError):
    """An error the API returned, with its machine-readable code preserved."""

    def __init__(self, status: int, code: str, message: str, details: Any = None):
        super().__init__(f"{code}: {message}")
        self.status = status
        self.code = code
        self.message = message
        self.details = details


@dataclass
class Page:
    """One page of a list response."""

    data: list[Any]
    next_cursor: str | None
    has_more: bool


class ForgeClient:
    """Talks to a Forge server over HTTP.

    The base URL defaults to a local server, so the example works with no
    configuration beyond an access token.
    """

    def __init__(
        self,
        base_url: str | None = None,
        token: str | None = None,
        timeout: float = 30.0,
    ):
        self.base_url = (base_url or os.environ.get("FORGE_BASE_URL")
                         or "http://localhost:3000/api/v1").rstrip("/")
        self.token = token or os.environ.get("FORGE_TOKEN")
        self.timeout = timeout

    # -- plumbing ---------------------------------------------------------

    def _request(
        self,
        method: str,
        path: str,
        body: Any = None,
        *,
        query: dict[str, Any] | None = None,
        idempotency_key: str | None = None,
        authenticated: bool = True,
    ) -> Any:
        url = f"{self.base_url}{path}"
        if query:
            clean = {k: v for k, v in query.items() if v is not None}
            if clean:
                url = f"{url}?{urllib.parse.urlencode(clean)}"

        payload = None
        headers = {"Accept": "application/json"}
        if body is not None:
            payload = json.dumps(body).encode()
            headers["Content-Type"] = "application/json"
        if authenticated:
            if not self.token:
                raise ForgeError(0, "NO_TOKEN", "set FORGE_TOKEN or pass token=")
            headers["Authorization"] = f"Bearer {self.token}"
        if idempotency_key:
            headers["Idempotency-Key"] = idempotency_key

        request = urllib.request.Request(url, data=payload, headers=headers, method=method)
        try:
            with urllib.request.urlopen(request, timeout=self.timeout) as response:
                raw = response.read()
                return json.loads(raw) if raw else None
        except urllib.error.HTTPError as error:
            raw = error.read()
            try:
                envelope = json.loads(raw)
            except json.JSONDecodeError:
                raise ForgeError(error.code, "HTTP_ERROR", raw.decode(errors="replace"))
            body_ = envelope.get("error", {})
            raise ForgeError(
                error.code,
                body_.get("code", "UNKNOWN"),
                body_.get("message", "request failed"),
                body_.get("details"),
            ) from None
        except urllib.error.URLError as error:
            raise ForgeError(0, "UNREACHABLE", str(error.reason)) from None

    def _envelope(self, payload: Any) -> Any:
        """Unwraps the `{data, ...}` envelope the API always returns."""
        return payload.get("data") if isinstance(payload, dict) else payload

    # -- authentication ---------------------------------------------------

    def register(self, email: str, password: str, invite_token: str | None = None) -> dict:
        body: dict[str, Any] = {"email": email, "password": password}
        if invite_token:
            body["invite_token"] = invite_token
        return self._envelope(self._request("POST", "/auth/register", body, authenticated=False))

    def login(self, email: str, password: str) -> dict:
        return self._envelope(
            self._request(
                "POST", "/auth/login", {"email": email, "password": password},
                authenticated=False,
            )
        )

    def refresh(self, refresh_token: str) -> dict:
        return self._envelope(
            self._request(
                "POST", "/auth/refresh", {"refresh_token": refresh_token},
                authenticated=False,
            )
        )

    # -- reads ------------------------------------------------------------

    def get(self, path: str, **query: Any) -> Any:
        return self._envelope(self._request("GET", path, query=query or None))

    def dashboard(self) -> Any:
        return self.get("/dashboard")

    def system_health(self) -> Any:
        return self.get("/system/health")

    def job_health(self, job_id: str) -> Any:
        return self.get(f"/jobs/{job_id}/health")

    def search(self, query: str) -> Any:
        return self.get("/search", q=query)

    def schedule_preview(self, schedule_id: str, count: int = 5) -> Any:
        """Preview the next occurrences.

        This is a POST rather than a GET: it takes an explicit body so the
        result can depend on a hypothetical expression, not only the stored
        one.
        """
        return self._envelope(
            self._request("POST", f"/schedules/{schedule_id}/preview", {"count": count})
        )

    def paginate(self, path: str, limit: int = 50) -> Iterator[Any]:
        """Walks every page of a cursor-paginated list."""
        cursor = None
        while True:
            payload = self._request("GET", path, query={"limit": limit, "cursor": cursor})
            page = Page(
                data=(payload.get("data") or []),
                next_cursor=(payload.get("page") or {}).get("next_cursor"),
                has_more=(payload.get("page") or {}).get("has_more", False),
            )
            yield from page.data
            if not page.has_more or not page.next_cursor:
                return
            cursor = page.next_cursor

    # -- writes -----------------------------------------------------------

    def create_job(self, name: str, key: str | None = None, priority: str = "NORMAL",
                   description: str | None = None, idempotency_key: str | None = None) -> Any:
        body: dict[str, Any] = {"name": name, "priority": priority}
        if key:
            body["key"] = key
        if description:
            body["description"] = description
        return self._envelope(
            self._request("POST", "/jobs", body, idempotency_key=idempotency_key)
        )

    def create_version(self, job_id: str, config: dict, concurrency: dict | None = None) -> Any:
        body: dict[str, Any] = {"config": config}
        if concurrency:
            body["concurrency_policy"] = concurrency
        return self._envelope(self._request("POST", f"/jobs/{job_id}/versions", body))

    def publish_version(self, job_id: str, version_id: str) -> Any:
        return self._envelope(
            self._request("POST", f"/jobs/{job_id}/versions/{version_id}/publish", {})
        )

    def trigger(self, job_id: str, payload: dict | None = None,
                idempotency_key: str | None = None) -> Any:
        return self._envelope(
            self._request("POST", f"/jobs/{job_id}/trigger", payload or {},
                          idempotency_key=idempotency_key)
        )

    def set_job_status(self, job_id: str, status: str, expected_updated_at: str) -> Any:
        return self._envelope(
            self._request(
                "PATCH", f"/jobs/{job_id}",
                {"status": status, "expected_updated_at": expected_updated_at},
            )
        )

    def cancel_execution(self, execution_id: str, reason: str = "Cancelled by the demo") -> Any:
        return self._envelope(
            self._request("POST", f"/executions/{execution_id}/cancel", {"reason": reason})
        )

    def retry_execution(self, execution_id: str) -> Any:
        return self._envelope(self._request("POST", f"/executions/{execution_id}/retry", {}))

    def create_schedule(self, job_id: str, cron: str, timezone: str = "UTC",
                        misfire_policy: str = "FIRE_ONCE",
                        target_type: str = "JOB") -> Any:
        """Attaches a cron schedule to a job or workflow.

        The API names the subject `target_id` because a schedule can point at
        either a job or a workflow.
        """
        return self._envelope(
            self._request(
                "POST", "/schedules",
                {
                    "target_id": job_id,
                    "target_type": target_type,
                    "expression": cron,
                    "schedule_type": "CRON",
                    "timezone": timezone,
                    "misfire_policy": misfire_policy,
                },
            )
        )

    def create_alert_rule(self, name: str, kind: str, config: dict) -> Any:
        return self._envelope(
            self._request("POST", "/alert-rules", {"name": name, "kind": kind, "config": config})
        )

    def acknowledge(self, alert_id: str) -> Any:
        return self._envelope(self._request("POST", f"/alerts/{alert_id}/acknowledge", {}))

    def set_sla(self, job_id: str, seconds: int) -> Any:
        return self._envelope(
            self._request("PUT", f"/jobs/{job_id}/sla", {"target_duration_seconds": seconds})
        )

    def create_webhook(self, name: str, url: str, events: list[str] | None = None) -> Any:
        body: dict[str, Any] = {"name": name, "url": url}
        if events:
            body["events"] = events
        return self._envelope(self._request("POST", "/webhooks", body))

    def create_workflow(self, name: str, key: str, nodes: list[dict],
                        edges: list[dict] | None = None) -> Any:
        return self._envelope(
            self._request(
                "POST", "/workflows",
                {"name": name, "key": key,
                 "definition": {"nodes": nodes, "edges": edges or []}},
            )
        )

    def publish_workflow(self, workflow_id: str) -> Any:
        return self._envelope(self._request("POST", f"/workflows/{workflow_id}/publish", {}))

    def trigger_workflow(self, workflow_id: str) -> Any:
        return self._envelope(self._request("POST", f"/workflows/{workflow_id}/trigger", {}))

    def bulk(self, action: str, job_ids: list[str]) -> Any:
        return self._envelope(
            self._request("POST", "/jobs/bulk", {"action": action, "job_ids": job_ids})
        )

    def undo_list(self) -> list[Any]:
        return self.get("/undo") or []

    def undo(self, entry_id: str) -> Any:
        return self._envelope(self._request("POST", f"/undo/{entry_id}", {}))

    def set_maintenance(self, reason: str) -> Any:
        return self._envelope(self._request("POST", "/maintenance", {"reason": reason}))

    def clear_maintenance(self) -> Any:
        return self._envelope(self._request("DELETE", "/maintenance"))

    def ask(self, question: str) -> Any:
        return self._envelope(self._request("POST", "/assistant/ask", {"question": question}))

    def propose(self, question: str) -> Any:
        return self._envelope(
            self._request("POST", "/assistant/propose", {"question": question})
        )

    def export_jobs(self) -> Any:
        return self.get("/jobs/export")
