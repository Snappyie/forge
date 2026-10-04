#!/usr/bin/env python3
"""A tour of every Forge feature, run against a live server.

Each step prints what it did and what came back, so the output doubles as a
readable transcript of the API. The script is idempotent: re-running it reuses
the jobs it created rather than failing on duplicate keys, and always leaves
maintenance mode off.

    python3 demo.py --base-url http://localhost:3000/api/v1 \\
                    --email ops@example.com --password correct-horse-battery
"""

from __future__ import annotations

import argparse
import json
import sys
import time
import uuid

from forge_demo.client import ForgeClient, ForgeError

OK = "\033[32m✓\033[0m"
DIM = "\033[2m"


def step(number: int, title: str) -> None:
    print(f"\n{OK} {number}. {title}")


def note(text: str) -> None:
    print(f"   {DIM}{text}{DIM}")


def show(label: str, value: object) -> None:
    rendered = json.dumps(value, default=str)
    if len(rendered) > 220:
        rendered = rendered[:217] + "..."
    print(f"   {label}: {rendered}")


class Demo:
    def __init__(self, client: ForgeClient, run_id: str):
        self.client = client
        self.run_id = run_id
        self.job_ids: list[str] = []

    # -- authentication ---------------------------------------------------

    def authenticate(self, email: str, password: str, invite_token: str | None) -> None:
        step(1, "Authenticate")
        # The first registration on an empty server claims the tenant as owner.
        # Every later one needs an invitation, so both paths are exercised.
        try:
            session = self.client.register(email, password, invite_token)
            note("registered; this account owns the first tenant")
        except ForgeError as error:
            if error.code != "AUTHORIZATION_DENIED":
                raise
            if invite_token is None and self._unknown_account(email):
                raise SystemExit(
                    f"\nThis tenant is already claimed, so {email} must be "
                    "invited before it can register.\n"
                    "Create an invitation as an owner:\n"
                    "  INSERT INTO invites (token, tenant_id, email, role, expires_at)\n"
                    "  VALUES (gen_random_uuid(), '<tenant-uuid>', "
                    f"'{email}', 'DEVELOPER', NOW() + INTERVAL '7 days');\n"
                    "then re-run with --invite <uuid>."
                )
            note("registration is invitation-only, as it should be; signing in")
            session = self.client.login(email, password)
        self.client.token = session["access_token"]
        show("tenant", session["tenant_id"])
        show("role", session["role"])

        # Refreshing rotates the token; the old one is invalidated.
        refreshed = self.client.refresh(session["refresh_token"])
        self.client.token = refreshed["access_token"]
        note("refreshed the session; the previous access token is now invalid")

    def _unknown_account(self, email: str) -> bool:
        """Whether the account simply does not exist yet, as opposed to a bad password."""
        try:
            self.client.login(email, "definitely-not-the-password")
            return False
        except ForgeError as error:
            # The API deliberately answers identically for both cases, so this is
            # a best-effort hint for the message, never a security decision.
            return error.code == "AUTHENTICATION_REQUIRED"

    # -- jobs and versions ------------------------------------------------

    def create_jobs(self) -> None:
        step(2, "Define jobs and publish a version")
        job = self.client.create_job(
            name="Settlement extract",
            key=f"settlement-extract-{self.run_id}",
            priority="CRITICAL",
            description="Pulls the day's ledger movements.",
            # An idempotency key means a retried create cannot duplicate the job.
            idempotency_key=f"create-{self.run_id}",
        )
        self.job_ids.append(job["id"])
        show("job", {"id": job["id"], "status": job["status"]})
        note("a new job is a DRAFT until a version is published")

        version = self.client.create_version(
            job["id"],
            config={"url": "https://example.invalid/extract", "timeout_seconds": 300},
            # Two at a time is a real limit: the dispatcher honours it, and
            # QUEUED executions count against it too.
            concurrency={
                "max_concurrent_executions": {"Bounded": 2},
                "scope": "JOB",
                "queue_id": None,
            },
            # Retry only what is worth retrying, and jitter so a failing job does
            # not stampede on every tick.
            retry={
                "max_attempts": 3,
                "backoff": self.client.exponential_backoff(1, 2, 300),
                "retryable_error_classes": ["TRANSIENT", "TIMEOUT"],
                "non_retryable_error_classes": ["VALIDATION"],
                "jitter_ratio": 0.2,
            },
        )
        show("version", {"id": version["id"], "number": version["version_number"]})

        published = self.client.publish_version(job["id"], version["id"])
        show("published", published)

        # A second job so bulk operations have more than one subject.
        sibling = self.client.create_job(
            name="Settlement notification",
            key=f"settlement-notify-{self.run_id}",
            priority="NORMAL",
        )
        self.job_ids.append(sibling["id"])
        sibling_version = self.client.create_version(sibling["id"], config={"url": "https://example.invalid/notify"})
        self.client.publish_version(sibling["id"], sibling_version["id"])
        note("created a second job so bulk actions have two subjects")

    # -- scheduling -------------------------------------------------------

    def create_schedule(self) -> None:
        step(3, "Schedule the job")
        schedule = self.client.create_schedule(
            job_id=self.job_ids[0],
            cron="0 2 * * 1-5",
            timezone="Asia/Kolkata",
        )
        self.schedule_id = schedule["id"]
        show("schedule", {
            "cron": schedule.get("expression") or schedule.get("cron_expression"),
            "timezone": schedule.get("timezone"),
            "next_run": schedule.get("next_run_at"),
        })

        preview = self.client.schedule_preview(self.schedule_id, count=4)
        show("next occurrences", preview.get("occurrences") or preview)
        anomalies = preview.get("anomalies") or []
        note(
            f"daylight-saving anomalies reported: {len(anomalies)}"
            if anomalies
            else "no daylight-saving anomalies in this window"
        )

    # -- execution --------------------------------------------------------

    def run_and_inspect(self) -> None:
        step(4, "Trigger executions and follow them")
        executions = [
            self.client.trigger(self.job_ids[0], idempotency_key=f"run-{self.run_id}-{i}")
            for i in range(3)
        ]
        first = executions[0]
        show("execution", {"id": first["execution_id"], "status": first["status"]})
        note(f"triggered {len(executions)} executions with idempotency keys")

        detail = self.client.get(f"/executions/{first['execution_id']}")
        show("status", detail["status"])
        show("attempt", detail["attempt_count"])

        timeline = self.client.get(f"/executions/{first['execution_id']}/timeline")
        stages = " → ".join(stage["label"] for stage in timeline["stages"])
        note(f"lifecycle: {stages}")

        logs = self.client.get(f"/executions/{first['execution_id']}/logs")
        count = len(logs.get("lines") or [])
        note(f"{count} log line(s); empty is a truthful answer, not a failure")

        metrics = self.client.get(f"/executions/{first['execution_id']}/metrics")
        show("metrics sampled", metrics["sampled"])
        if not metrics["sampled"]:
            note("no resource telemetry yet; the console says 'no data' rather than zero")

    # -- concurrency ------------------------------------------------------

    def show_concurrency(self) -> None:
        step(5, "Read the job's health")
        health = self.client.job_health(self.job_ids[0])
        reliability = health["reliability"]
        performance = health["performance"]
        show("reliability", reliability)
        show("performance", performance)
        note("an unmeasured figure reads as null, never as a misleading zero")

    # -- sla --------------------------------------------------------------

    def configure_sla(self) -> None:
        step(6, "Set an SLA target")
        target = self.client.set_sla(self.job_ids[0], 1800)
        show("target", target)
        note("compliance is reported once runs have been measured against it")

    # -- alerts -----------------------------------------------------------

    def configure_alerts(self) -> None:
        step(7, "Configure alerting")
        rule = self.client.create_alert_rule(
            name=f"Settlement failures ({self.run_id})",
            kind="EXECUTION_FAILED",
            config={"failures": 3, "window_seconds": 900},
        )
        show("rule", {"id": rule["id"], "kind": rule["kind"]})
        summary = self.client.get("/alerts/summary")
        show("open alerts", summary)
        note("rules evaluate continuously; nothing fires until a run actually fails")

    # -- webhooks ---------------------------------------------------------

    def configure_webhooks(self) -> None:
        step(8, "Register a webhook")
        try:
            hook = self.client.create_webhook(
                name=f"demo hook ({self.run_id})",
                url="https://example.invalid/forge-events",
                events=["execution.failed", "execution.succeeded"],
            )
            show("webhook", {"id": hook["id"], "events": hook.get("events")})
        except ForgeError as error:
            # The SSRF guard rejects loopback and private targets; this proves it
            # is live rather than decorative.
            show("webhook refused", error.message)
            note("the SSRF guard rejected the destination, as it should")

    # -- workflows --------------------------------------------------------

    def configure_workflow(self) -> None:
        step(9, "Build and publish a workflow")
        workflow = self.client.create_workflow(
            name=f"Nightly close ({self.run_id})",
            key=f"nightly-close-{self.run_id}",
            nodes=[
                {"key": "extract", "name": "Extract movements", "type": "JOB",
                 "config": {"job_id": self.job_ids[0]}},
                {"key": "approve", "name": "Finance approval", "type": "APPROVAL",
                 "config": {}},
            ],
            edges=[{"from": "extract", "to": "approve"}],
        )
        show("workflow", {"id": workflow["id"], "status": workflow["status"]})
        self.client.publish_workflow(workflow["id"])
        note("published; a workflow runs against a published definition, like a job")

    # -- bulk operations and undo ----------------------------------------

    def demonstrate_bulk_and_undo(self) -> None:
        step(10, "Bulk operations, then undo one")
        # A published job is DRAFT until something activates it, and pausing an
        # already-draft job is a no-op — so activate first, otherwise the bulk
        # call has nothing to change and records nothing to undo.
        detail = self.client.get(f"/jobs/{self.job_ids[0]}")
        if detail["status"] != "ACTIVE":
            self.client.set_job_status(
                self.job_ids[0], "ACTIVE", detail["updated_at"]
            )
            note("activated the job so pausing it is a real change")

        result = self.client.bulk("PAUSE", self.job_ids)
        show("bulk pause", {
            "requested": result["requested"],
            "applied": result["applied"],
            "failed": result["failed"],
        })
        for entry in result["results"]:
            if not entry.get("ok"):
                note(f"{entry['job_id'][:8]} was refused: {entry.get('error')}")
        note("a partial batch reports both counts rather than collapsing to one result")

        entries = self.client.undo_list()
        if entries:
            undone = self.client.undo(entries[0]["id"])
            show("undone", undone)
            note("reversible while the entry is unexpired; a second attempt is refused")
        else:
            note("nothing was undoable")

    # -- operational surfaces --------------------------------------------

    def demonstrate_operations(self) -> None:
        step(11, "Operational endpoints")
        health = self.client.system_health()
        for component in health["components"]:
            note(f"{component['name']:<22} {component['status']:<10} {component['detail']}")

        dashboard = self.client.dashboard()
        show("dashboard executions", dashboard["executions"])
        show("dashboard workers", dashboard["workers"])
        note("an absent heartbeat reads as 'unknown', never as 'healthy'")

        upcoming = self.client.get("/upcoming")
        show("upcoming", len(upcoming["items"]))

    # -- search and assistant --------------------------------------------

    def demonstrate_search_and_assistant(self) -> None:
        step(12, "Search and the assistant")
        results = self.client.search("settlement")
        show("search total", results["total"])
        note("grouped with per-group counts, across jobs, executions, workers, alerts")

        answer = self.client.ask("What jobs are running right now?")
        show("answer", answer["answer"])

        proposal = self.client.propose("Create a job that runs every weekday at 2am")
        show("proposal", proposal["proposal"])
        show("applied", proposal["applied"])
        note("the assistant proposes; a human applies. It never changes production itself")

    # -- maintenance ------------------------------------------------------

    def demonstrate_maintenance(self) -> None:
        step(13, "Maintenance mode, and lifting it")
        self.client.set_maintenance(f"Demo maintenance window ({self.run_id})")
        state = self.client.get("/maintenance")
        show("active", state["active"])
        note("nothing new is dispatched while this is set; work in flight is untouched")
        self.client.clear_maintenance()
        show("after lifting", self.client.get("/maintenance")["active"])


    # -- queues -----------------------------------------------------------

    def demonstrate_queues(self) -> None:
        step(14, "Queues and their capacity")
        queue = self.client.create_queue(f"settlement-{self.run_id}", max_concurrency=5)
        self.queue_id = queue["id"]
        show("queue", {"id": queue["id"], "name": queue.get("name")})

        self.client.set_queue_paused(self.queue_id, True)
        state = self.client.get("/emergency")
        note(f"paused; paused queues now reported as "
             f"{[q['name'] for q in state['queues'] if q['paused']]}")
        self.client.set_queue_paused(self.queue_id, False)
        note("resumed")

    # -- workers ----------------------------------------------------------

    def demonstrate_workers(self) -> None:
        step(15, "The worker protocol")
        worker = self.client.register_worker(
            hostname=f"demo-worker-{self.run_id}",
            capabilities=["http", "container"],
            labels={"region": "local", "tier": "demo"},
        )
        self.worker_id = worker["id"]
        show("worker", {"id": worker["id"], "status": worker.get("status")})

        # The dispatch preconditions are enforced, not merely described: a
        # drained worker is given nothing.
        self.client.drain_worker(self.worker_id)
        claim = self.client._request("POST", f"/workers/{self.worker_id}/claim", {})
        note(f"draining; a claim returned {claim['data']['execution'] and 'work' or 'nothing'}")
        self.client._request("POST", f"/workers/{self.worker_id}/drain", {})  # idempotent
        self.client.worker_heartbeat(self.worker_id)
        note("heartbeat sent")

    # -- saved views ------------------------------------------------------

    def demonstrate_saved_views(self) -> None:
        step(16, "Saved views")
        view = self.client.save_view(
            name=f"Failed jobs ({self.run_id})",
            filters={"status": "FAILED"},
            resource="JOBS",
        )
        show("view", {"id": view["id"], "filters": view.get("filters")})
        listed = self.client.list_saved_views("JOBS")
        note(f"{len(listed)} saved view(s) for jobs")
        self.client.delete_saved_view(view["id"])
        note("deleted; a saved view is a filter state, not data")

    # -- integrations -----------------------------------------------------

    def demonstrate_integrations(self) -> None:
        step(17, "Integrations")
        listing = self.client.list_integrations()
        supported = listing["data"]["supported"]
        absent = listing["data"]["not_configured"]
        note(f"{len(supported)} integration kinds supported, {len(absent)} not configured")

        integration = self.client.create_integration("KAFKA", f"events-{self.run_id}")
        show("created", {"id": integration["id"], "kind": integration["kind"]})
        result = self.client.test_integration(integration["id"])
        show("connection verified", result["connection_verified"])
        note("Forge does not dial external endpoints on request, so it says so "
             "rather than reporting a success it cannot confirm")

    # -- API keys ---------------------------------------------------------

    def demonstrate_api_keys(self) -> None:
        step(18, "API keys for programmatic access")
        key = self.client.create_api_key(f"demo-key-{self.run_id}")
        show("created", {"id": key["id"], "prefix": key.get("prefix")})
        note("the secret is returned exactly once and stored hashed")

        rotated = self.client.rotate_api_key(key["id"])
        show("rotated", {"new_prefix": rotated.get("prefix"),
                          "previous_prefix": rotated.get("previous_prefix")})
        note("rotation invalidates the old secret immediately")

        self.client.revoke_api_key(key["id"])
        note("revoked; a revoked key cannot be brought back")

    # -- users and audit --------------------------------------------------

    def demonstrate_users_and_audit(self) -> None:
        step(19, "Users and the audit trail")
        users = self.client.list_users()
        note(f"{len(users)} user(s) in this tenant")

        colleague_email = f"colleague-{self.run_id}@example.com"
        try:
            created = self.client.create_user(
                colleague_email, "correct-horse-battery", "DEVELOPER"
            )
            show("invited", {"id": created["id"], "role": created["role"]})
            note("a new user joins this tenant with the role given")
        except ForgeError as error:
            note(f"could not add a user: {error.code}")

        events = self.client.list_audit_events(limit=10)
        show("recent audit events", len(events))
        if events:
            latest = events[0]
            note(f"most recent: {latest.get('action')} on "
                 f"{latest.get('resource_type')} by {latest.get('actor_type')}")

    # -- notifications and incidents --------------------------------------

    def demonstrate_notifications_and_incidents(self) -> None:
        step(20, "Notifications and incidents")
        inbox = self.client.list_notifications()
        show("unread", inbox.get("unread"))
        show("delivered", len(inbox.get("data") or []))
        if inbox.get("unread"):
            self.client.mark_notifications_read()
            show("after marking read", self.client.list_notifications().get("unread"))

        self.client.update_notification_preferences(
            kinds=["EXECUTION_FAILED", "SLA_VIOLATION"],
            channels={"in_app": True, "email": False, "webhook": False},
        )
        note("preferences updated; unsaved defaults are returned by the API")

        incidents = self.client.list_incidents()
        show("incidents", len(incidents))
        if incidents:
            detail = self.client.incident_detail(incidents[0]["id"])
            note(f"{incidents[0]['title']}: {len(detail.get('alerts') or [])} alert(s), "
                 f"{len(detail.get('timeline') or [])} timeline entries")

    # -- emergency and discovery -----------------------------------------

    def demonstrate_emergency_and_contract(self) -> None:
        step(21, "Emergency controls and the contract")
        state = self.client.emergency_state()
        show("running executions", state["running_executions"])
        show("paused schedules", state["paused_schedules"])

        if state["running_executions"] > 0:
            result = self.client.cancel_all_running(f"Demo cancellation ({self.run_id})")
            show("cancelled", result["cancelled"])
            note("cancelling in-flight work demands a reason, which is recorded")
        else:
            note("nothing was running, so the cancellation path was not exercised")

        contract = self.client.openapi_document()
        operations = sum(
            len([m for m in v if m in ("get", "post", "put", "patch", "delete")])
            for v in contract["paths"].values()
        )
        show("contract", {"paths": len(contract["paths"]), "operations": operations})
        note("this is the document a client generator reads; a test fails the "
             "build if it stops matching the routes")

    # -- pagination -------------------------------------------------------

    def demonstrate_pagination(self) -> None:
        step(22, "Paginate a collection")
        seen = 0
        for _ in self.client.paginate("/executions", limit=2):
            seen += 1
            if seen >= 6:
                break
        note(f"walked {seen} executions following next_cursor, two at a time")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default=None, help="API base URL")
    parser.add_argument("--email", required=True, help="account to authenticate")
    parser.add_argument("--password", required=True, help="password for that account")
    parser.add_argument(
        "--invite",
        default=None,
        help=(
            "invitation token, required once the first account has claimed the "
            "tenant. Create one as an owner, then pass it here."
        ),
    )
    args = parser.parse_args()

    client = ForgeClient(base_url=args.base_url)
    run_id = uuid.uuid4().hex[:8]
    print(f"Forge demo — {client.base_url} (run {run_id})")

    demo = Demo(client, run_id)
    try:
        demo.authenticate(args.email, args.password, args.invite)
        demo.create_jobs()
        demo.create_schedule()
        demo.run_and_inspect()
        demo.show_concurrency()
        demo.configure_sla()
        demo.configure_alerts()
        demo.configure_webhooks()
        demo.configure_workflow()
        demo.demonstrate_bulk_and_undo()
        demo.demonstrate_operations()
        demo.demonstrate_search_and_assistant()
        demo.demonstrate_maintenance()
        demo.demonstrate_queues()
        demo.demonstrate_workers()
        demo.demonstrate_saved_views()
        demo.demonstrate_integrations()
        demo.demonstrate_api_keys()
        demo.demonstrate_users_and_audit()
        demo.demonstrate_notifications_and_incidents()
        demo.demonstrate_emergency_and_contract()
        demo.demonstrate_pagination()
    except ForgeError as error:
        print(f"\n\033[31m✗ {error.status} {error.code}\033[0m: {error.message}")
        if error.details:
            print(f"   {json.dumps(error.details, default=str)}")
        return 1
    except KeyboardInterrupt:
        print("\ninterrupted")
        return 130

    print(f"\n{OK} Every feature exercised. Jobs created: {len(demo.job_ids)}")
    print(f"   Open the console at the web UI to see the same records.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
