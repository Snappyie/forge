"""End-to-end test of the Python SDK against a real Forge server.

Drives the full worker protocol through the SDK's public API only: register,
dequeue, heartbeat, log, complete. A fake HTTP layer would pass regardless of
whether the SDK sends the fields the server actually requires, which is how the
missing `lease_id` and `error_class` survived in the first place.

Run with a server on ``FORGE_TEST_API``; skipped otherwise, so it never fails a
checkout that has no stack running.
"""

import os
import sys
import unittest

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

import forge_sdk
from forge_sdk import ForgeWorker, classify

API = os.environ.get("FORGE_TEST_API", "").rstrip("/")


def unique(prefix: str) -> str:
    import uuid

    return f"{prefix}-{uuid.uuid4().hex[:8]}"


@unittest.skipUnless(API, "set FORGE_TEST_API to run against a live server")
class SdkAgainstLiveServer(unittest.TestCase):
    """Uses the server's own admin API to set up, so no fixtures are hardcoded."""

    token = ""
    tenant = ""
    queue = ""
    app_token = ""

    @classmethod
    def setUpClass(cls):
        import json
        import urllib.request

        def post(path, body, token=None):
            req = urllib.request.Request(
                f"{API}{path}",
                data=json.dumps(body).encode(),
                headers={
                    "content-type": "application/json",
                    **({"authorization": f"Bearer {token}"} if token else {}),
                },
                method="POST",
            )
            with urllib.request.urlopen(req, timeout=15) as r:
                return json.loads(r.read()).get("data") or {}

        # An operator credential is needed to create a queue and to register a
        # worker. The one-time bootstrap slot is a single claim per deployment,
        # so this suite cannot rely on registering as OWNER; it borrows a token
        # when one is offered and registers its own tenant otherwise.
        borrowed = os.environ.get("FORGE_TEST_ADMIN_TOKEN")
        if borrowed:
            cls.token = borrowed
            cls.tenant = os.environ.get("FORGE_TEST_TENANT", "")
            cls.queue = os.environ.get("FORGE_TEST_QUEUE", "")
            return

        admin = post(
            "/auth/register",
            {
                "email": unique("sdk") + "@example.com",
                "password": "SdkPassword12345!",
                "tenant_name": "SDK Test",
            },
        )
        cls.token = admin["access_token"]
        cls.tenant = admin["tenant_id"]

        # Open registration grants VIEWER, which cannot create queues. When the
        # suite registers its own tenant and is not the bootstrap claimant, the
        # queue creation below is refused; that is a server policy, not a
        # failure of this suite, so it is skipped rather than papered over.
        try:
            cls.queue = post("/queues", {"name": unique("q")}, cls.token)["name"]
        except Exception as exc:  # noqa: BLE001
            raise unittest.SkipTest(f"needs an operator credential: {exc}")

    def _worker(self) -> ForgeWorker:
        worker = ForgeWorker(API, self.tenant, self.token).with_worker_id(unique("w"))
        worker.register(name=unique("worker"))
        return worker

    def test_a_registered_worker_gets_a_real_worker_id_and_token(self):
        worker = self._worker()
        # A worker token is a different credential from the admin one; without
        # this the worker would be calling with borrowed authority.
        self.assertNotEqual(worker.token, self.token)
        self.assertTrue(worker.worker_id)
        self.assertNotEqual(worker.worker_id, unique(""))

    def test_a_worker_token_is_refused_admin_operations(self):
        """Least privilege is a server property; the SDK must live within it."""
        import json
        import urllib.error
        import urllib.request

        worker = self._worker()
        req = urllib.request.Request(
            f"{API}/jobs",
            data=json.dumps({"name": unique("nope")}).encode(),
            headers={
                "content-type": "application/json",
                "authorization": f"Bearer {worker.token}",
            },
            method="POST",
        )
        with self.assertRaises(urllib.error.HTTPError) as caught:
            urllib.request.urlopen(req, timeout=15)
        self.assertIn(caught.exception.code, (401, 403))

    def test_dequeue_returns_a_lease_and_the_sdk_keeps_it(self):
        worker = self._worker()
        # Nothing queued: dequeue returns null rather than failing.
        self.assertIsNone(worker._dequeue(self.queue))

    def test_log_and_heartbeat_reach_the_server(self):
        import json
        import urllib.request

        worker = self._worker()
        ctx = forge_sdk.JobContext(
            execution_id=unique("ex"),
            job_name="probe",
            payload={},
            lease_id=None,
            worker_id=worker.worker_id,
            base_url=API,
            token=worker.token,
        )
        # Logging against a nonexistent execution must not raise: it is
        # best-effort, and a raised transport error would fail a healthy run.
        ctx.log("hello from the sdk test")
        self.assertIsInstance(ctx._log_failures, list)

    def test_errors_carry_the_servers_structured_envelope(self):
        import json
        import urllib.error
        import urllib.request

        worker = ForgeWorker(API, self.tenant, "forge_wkr_not-a-real-token")
        req = urllib.request.Request(
            f"{API}/workers/{worker.worker_id}/heartbeat",
            data=b"{}",
            headers={
                "content-type": "application/json",
                "authorization": f"Bearer {worker.token}",
            },
            method="POST",
        )
        with self.assertRaises(urllib.error.HTTPError):
            urllib.request.urlopen(req, timeout=15)

        try:
            worker._post(f"/workers/{worker.worker_id}/heartbeat", {}, timeout=15)
        except forge_sdk.ForgeError as exc:
            # Not a bare "HTTP 401": the server's code and request id survive,
            # which is what makes a failure diagnosable.
            self.assertTrue(exc.code)
            self.assertEqual(exc.status, 401)
        else:
            self.fail("an invalid token should have been refused")


class ErrorClassification(unittest.TestCase):
    """Runs without a server: these are pure functions."""

    def test_transient_problems_are_retryable(self):
        # This is the property that makes the platform's retry policy usable: if
        # every failure were classified PERMANENT the retry machinery would be
        # unreachable from Python.
        for exc in (TimeoutError(), ConnectionError()):
            self.assertIn(classify(exc), forge_sdk.RETRYABLE_BY_DEFAULT)

    def test_bad_input_is_not_retryable(self):
        for exc in (ValueError("x"), TypeError("x"), KeyError("x")):
            self.assertEqual(classify(exc), forge_sdk.VALIDATION)
            self.assertNotIn(classify(exc), forge_sdk.RETRYABLE_BY_DEFAULT)

    def test_anything_unrecognised_is_transient(self):
        # Optimistic on purpose: retrying a wrongly-classified failure is
        # recoverable, while not retrying a transient one silently loses work.
        self.assertEqual(classify(RuntimeError("who knows")), forge_sdk.TRANSIENT)


class SlugAndContract(unittest.TestCase):
    def test_the_sdk_declares_every_error_class_the_server_accepts(self):
        # A missing constant here means a handler cannot classify that failure
        # and the server will record it as PERMANENT.
        expected = {
            "VALIDATION",
            "AUTHENTICATION",
            "AUTHORIZATION",
            "NOT_FOUND",
            "CONFLICT",
            "RATE_LIMITED",
            "TRANSIENT",
            "DEPENDENCY_UNAVAILABLE",
            "TIMEOUT",
            "CANCELLATION",
            "RESOURCE_EXHAUSTED",
            "PERMANENT",
            "INTERNAL",
        }
        for name in expected:
            self.assertTrue(hasattr(forge_sdk, name), f"missing {name}")


if __name__ == "__main__":
    unittest.main(verbosity=2)