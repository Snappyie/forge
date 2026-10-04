#!/usr/bin/env python3
"""Checks that the demo covers every feature group the API serves.

Run it after changing either the demo or the API:

    python3 verify_demo.py --base-url http://127.0.0.1:3000/api/v1

It reads the OpenAPI document to learn which groups exist, then checks the
demo's client and script for a call into each one. It also runs the demo and
confirms every numbered step completed, stripping terminal colour first so the
count cannot be fooled by escape sequences.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
import urllib.request

HERE = os.path.dirname(os.path.abspath(__file__))

# Terminal colour codes sit between the checkmark and the step number, so any
# naive "✓ N." pattern silently matches nothing. Strip them first.
ANSI = re.compile(r"\x1b\[[0-9;]*m")

# The endpoint path fragment that stands for each documented group.
PROBE = {
    "jobs": "/jobs",
    "executions": "/executions",
    "schedules": "/schedules",
    "queues": "/queues",
    "workers": "/workers",
    "workflows": "/workflows",
    "alerts": "/alert",
    "incidents": "/incidents",
    "notifications": "/notification",
    "webhooks": "/webhooks",
    "integrations": "/integration",
    "savedViews": "/saved-views",
    "undo": "/undo",
    "dashboard": "/dashboard",
    "search": "/search",
    "assistant": "/assistant",
    "system": "/system/health",
    "apiKeys": "/api-keys",
    "admin": "/admin",
    "auth": "/auth/",
    "public": "/openapi.json",
    "users": "/users",
    "audit": "/audit-events",
}


def served_groups(base_url: str) -> list[str]:
    request = urllib.request.Request(f"{base_url.rstrip('/')}/openapi.json")
    with urllib.request.urlopen(request, timeout=15) as response:
        document = json.load(response)
    groups = set()
    for item in document.get("paths", {}).values():
        for method, operation in item.items():
            if method in ("get", "post", "put", "patch", "delete"):
                groups.add(operation["tags"][0])
    return sorted(groups)


def source_text() -> str:
    parts = []
    for name in ("demo.py", "forge_demo/client.py"):
        with open(os.path.join(HERE, name), encoding="utf-8") as handle:
            parts.append(handle.read())
    return "\n".join(parts)


def check_coverage(groups: list[str]) -> bool:
    source = source_text()
    print(f"feature groups served: {len(groups)}")
    missing = []
    for group in groups:
        probe = PROBE.get(group, group)
        present = probe in source
        print(f"  {group:<14} {'example present' if present else 'MISSING'}")
        if not present:
            missing.append(group)
    if missing:
        print(f"\n{len(missing)} group(s) have no example: {missing}")
    return not missing


def check_step_numbering() -> bool:
    """The steps must be 1..N with no gaps and no repeats."""
    with open(os.path.join(HERE, "demo.py"), encoding="utf-8") as handle:
        numbers = [
            int(m.group(1))
            for m in re.finditer(r'^        step\((\d+), "', handle.read(), re.M)
        ]
    expected = list(range(1, len(numbers) + 1))
    if numbers == expected:
        print(f"\nstep numbering: contiguous 1..{len(numbers)}")
        return True
    print(f"\nstep numbering is wrong:\n  found    {numbers}\n  expected {expected}")
    return False


def check_run(base_url: str, email: str, password: str, invite: str | None) -> bool:
    command = [sys.executable, os.path.join(HERE, "demo.py"),
               "--base-url", base_url, "--email", email, "--password", password]
    if invite:
        command += ["--invite", invite]
    result = subprocess.run(command, capture_output=True, text=True, timeout=300)
    output = ANSI.sub("", result.stdout)

    completed = [int(m.group(1)) for m in re.finditer(r"^✓ (\d+)\.", output, re.M)]

    if result.returncode != 0:
        print("\ndemo failed:")
        print(output[-1500:])
        return False

    with open(os.path.join(HERE, "demo.py"), encoding="utf-8") as handle:
        expected = len(re.findall(r'^        step\(\d+, "', handle.read(), re.M))

    print(f"\nsteps completed: {len(completed)} of {expected}")
    if len(completed) != expected:
        print("NOT every step ran")
        return False
    if completed != list(range(1, expected + 1)):
        print(f"steps out of order or gapped: {completed}")
        return False
    print("every declared step ran, in order, with no gaps")
    return True


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:3000/api/v1")
    parser.add_argument("--email", default="verify@example.com")
    parser.add_argument("--password", default="correct-horse-battery")
    parser.add_argument("--invite", default=None)
    parser.add_argument("--skip-run", action="store_true",
                        help="only check coverage and numbering")
    args = parser.parse_args()

    groups = served_groups(args.base_url)
    ok = check_coverage(groups)
    ok = check_step_numbering() and ok

    if not args.skip_run:
        ok = check_run(args.base_url, args.email, args.password, args.invite) and ok

    print("\n" + ("all checks passed" if ok else "CHECKS FAILED"))
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
