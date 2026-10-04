// Remaining screens: first-run onboarding, the state catalogue
// (empty/loading/error/permission-denied/degraded), and the command palette.

const fs = require("fs");
const path = require("path");
const { I, icSm, icXs, shell, statusPill } = require("./partials");

const kbd = `<kbd>⌘K</kbd>`;

// ================================================================ onboarding

const onboarding = shell({
  active: "/",
  title: "Getting started",
  env: "stage",
  envLabel: "staging",
  body: `
<div class="pagehead" style="margin-bottom:18px">
  <div class="row gap2">
    <span class="pill warn sq">${icXs(`<path d="M12 2v6M15 5l-3 3-3-3"/>`)} First run</span>
    <span style="font-size:11.5px;color:var(--muted-fg)">3 of 4 steps complete</span>
  </div>
  <h1 style="margin-top:10px">Forge is running. Give it something to do.</h1>
  <p>Four steps to a job that fires on its own. Nothing here is required in order, but each one depends on the last.</p>
</div>

<div class="split">
  <div class="card">
    <div class="card-body">
      <div class="steps">
        <div class="step">
          <span class="step-num done">${icXs(`<path d="M20 6 9 17l-5-5"/>`)}</span>
          <div class="flex1">
            <div class="row gap2">
              <span class="step-t">Create your tenant</span>
              <span class="pill ok" style="height:18px;font-size:10px">done</span>
            </div>
            <div class="step-d">acme-payments, created just now. Every job, worker and execution belongs to this tenant.</div>
          </div>
        </div>

        <div class="step">
          <span class="step-num done">${icXs(`<path d="M20 6 9 17l-5-5"/>`)}</span>
          <div class="flex1">
            <div class="row gap2">
              <span class="step-t">Define your first job</span>
              <span class="pill ok" style="height:18px;font-size:10px">done</span>
            </div>
            <div class="step-d">
              <span class="mono">nightly-settlement</span> — daily at 02:00 Asia/Kolkata, HTTP request to the
              settlement endpoint, 3 attempts with exponential backoff.
            </div>
            <a class="link" href="#" style="display:inline-block;margin-top:6px;font-size:11.5px">Open job</a>
          </div>
        </div>

        <div class="step">
          <span class="step-num done">${icXs(`<path d="M20 6 9 17l-5-5"/>`)}</span>
          <div class="flex1">
            <div class="row gap2">
              <span class="step-t">Publish a version</span>
              <span class="pill ok" style="height:18px;font-size:10px">done</span>
            </div>
            <div class="step-d">Version 1 is published and immutable. Only a published version is eligible for dispatch — a draft never runs.</div>
          </div>
        </div>

        <div class="step">
          <span class="step-num now">4</span>
          <div class="flex1">
            <div class="row gap2">
              <span class="step-t">Connect a worker</span>
              <span class="pill run" style="height:18px;font-size:10px">in progress</span>
            </div>
            <div class="step-d">
              A worker claims work over HTTP, holds a lease, and heartbeats. Without one, executions queue and
              nothing runs. Run the example worker, or point your own at the claim endpoint.
            </div>

            <div class="tabs" style="margin:14px 0 12px;border-radius:6px 6px 0 0;border:1px solid var(--border)">
              <span class="tab on" style="padding:7px 11px">Example worker</span>
              <span class="tab" style="padding:7px 11px">Python SDK</span>
              <span class="tab" style="padding:7px 11px">Bring your own</span>
            </div>

            <div class="codeblock" style="border-radius:0 0 6px 6px">
              <span class="c"># start the bundled example worker</span><br>
              docker compose up -d ecommerce-worker<br><br>
              <span class="c"># or register one of your own</span><br>
              curl -X POST <span class="k">$FORGE_URL</span>/api/v1/workers/register \<br>
                -H <span class="s">"authorization: Bearer $FORGE_TOKEN"</span> \<br>
                -d <span class="s">'{"name":"worker-01","capabilities":["http","worker_task"]}'</span>
            </div>

            <div class="row gap2" style="margin-top:14px">
              <button class="btn primary">${icSm(`<path d="m4 17 6-6-6-6M12 19h8"/>`)} Copy claim command</button>
              <button class="btn">Read worker guide</button>
            </div>
          </div>
        </div>

        <div class="step" style="opacity:0.55">
          <span class="step-num">5</span>
          <div class="flex1">
            <div class="step-t">Run it once by hand</div>
            <div class="step-d">Trigger the job directly and watch the execution timeline. Once you have seen one succeed, let the schedule do it.</div>
          </div>
        </div>
      </div>
    </div>
  </div>

  <div class="col gap4">
    <div class="card">
      <div class="card-head"><h3>Progress</h3><span class="sub ml-auto">3 of 5</span></div>
      <div class="card-body">
        <div class="meter ok" style="height:7px"><i style="width:60%"></i></div>
        <p style="margin:10px 0 0;font-size:11.5px;color:var(--muted-fg);line-height:1.5">
          Two steps left. Most people finish in about four minutes.
        </p>
      </div>
    </div>

    <div class="card">
      <div class="card-head"><h3>What is already working</h3></div>
      <div class="card-body col gap2" style="font-size:11.5px">
        <div class="row gap2">${icXs(`<path d="M20 6 9 17l-5-5"/>`)}<span class="flex1">Scheduler loop</span><span class="muted">ticking every 5s</span></div>
        <div class="row gap2">${icXs(`<path d="M20 6 9 17l-5-5"/>`)}<span class="flex1">Database and migrations</span><span class="muted">13 applied</span></div>
        <div class="row gap2">${icXs(`<path d="M20 6 9 17l-5-5"/>`)}<span class="flex1">Lease reaper</span><span class="muted">sweeping 5s</span></div>
        <div class="row gap2">${icXs(`<path d="M20 6 9 17l-5-5"/>`)}<span class="flex1">Outbox publisher</span><span class="muted">draining</span></div>
        <div class="row gap2">${icXs(`<path d="M20 6 9 17l-5-5"/>`)}<span class="flex1">Auth and RBAC</span><span class="muted">you are Owner</span></div>
        <div class="row gap2" style="opacity:0.55">${icXs(`<path d="M12 16v-4M12 8h.01"/>`)}<span class="flex1">Workers</span><span class="muted">none registered</span></div>
        <div class="row gap2" style="opacity:0.55">${icXs(`<path d="M12 16v-4M12 8h.01"/>`)}<span class="flex1">Queues</span><span class="muted">default only</span></div>
      </div>
    </div>

    <div class="card">
      <div class="card-head"><h3>Self-hosted, by design</h3></div>
      <div class="card-body">
        <p style="margin:0;font-size:11.5px;color:var(--muted-fg);line-height:1.6">
          Forge has no control plane and no outbound calls you did not configure. Your job data stays in your
          PostgreSQL, and this checklist disappears once you have a worker.
        </p>
      </div>
    </div>
  </div>
</div>`,
});

// ================================================================ state catalogue

const states = shell({
  active: "/jobs",
  title: "States",
  body: `
<div class="pagehead" style="margin-bottom:18px">
  <h1>The six states every Forge page can be in</h1>
  <p>AsyncBoundary enforces these structurally, so a page cannot quietly render half a state.
  A missing value says <span class="mono">unknown</span>, never a confident green.</p>
</div>

<div class="grid g2" style="margin-bottom:16px">
  <div class="card">
    <div class="card-head"><h3>Empty — nothing here yet</h3><span class="pill ml-auto">teaches the space</span></div>
    <div class="card-body tight">
      <div class="empty">
        <div class="empty-icon">${icSm(`<path d="M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2"/><circle cx="9" cy="7" r="4"/>`)}</div>
        <h3>No workers registered</h3>
        <p>Workers claim executions over HTTP. Until one registers, jobs queue and nothing runs.</p>
        <button class="btn primary">Connect a worker</button>
      </div>
    </div>
  </div>

  <div class="card">
    <div class="card-head"><h3>Empty — filtered to nothing</h3><span class="pill ml-auto">distinct from empty</span></div>
    <div class="card-body tight">
      <div class="empty">
        <div class="empty-icon">${icSm(`<path d="M3 4h18l-7 8v7l-4 2v-9L3 4z"/>`)}</div>
        <h3>No jobs match this filter</h3>
        <p>You are viewing <span class="mono">Status: Failed</span> and <span class="mono">after 2026-10-04</span>. Clear one to widen the results.</p>
        <button class="btn">Clear filters</button>
      </div>
    </div>
  </div>

  <div class="card">
    <div class="card-head"><h3>Loading — names the work</h3></div>
    <div class="card-body tight">
      <table>
        <tbody>
          <tr><td style="width:34px"><span class="skel" style="display:block;width:14px;height:14px"></span></td>
            <td><div class="skel" style="height:11px;width:62%;margin-bottom:5px"></div><div class="skel" style="height:8px;width:38%"></div></td>
            <td><div class="skel" style="height:16px;width:58px;border-radius:10px"></div></td>
            <td><div class="skel" style="height:10px;width:74px"></div></td>
            <td><div class="skel" style="height:10px;width:52px"></div></td></tr>
          <tr><td><span class="skel" style="display:block;width:14px;height:14px"></span></td>
            <td><div class="skel" style="height:11px;width:48%;margin-bottom:5px"></div><div class="skel" style="height:8px;width:44%"></div></td>
            <td><div class="skel" style="height:16px;width:58px;border-radius:10px"></div></td>
            <td><div class="skel" style="height:10px;width:66px"></div></td>
            <td><div class="skel" style="height:10px;width:48px"></div></td></tr>
          <tr><td><span class="skel" style="display:block;width:14px;height:14px"></span></td>
            <td><div class="skel" style="height:11px;width:70%;margin-bottom:5px"></div><div class="skel" style="height:8px;width:34%"></div></td>
            <td><div class="skel" style="height:16px;width:58px;border-radius:10px"></div></td>
            <td><div class="skel" style="height:10px;width:80px"></div></td>
            <td><div class="skel" style="height:10px;width:56px"></div></td></tr>
        </tbody>
      </table>
      <div class="pager"><span>Loading 24 jobs…</span></div>
    </div>
  </div>

  <div class="card">
    <div class="card-head"><h3>Error — a recovery path</h3></div>
    <div class="card-body tight">
      <div class="empty">
        <div class="empty-icon" style="background:var(--bad-bg);color:var(--bad)">${icSm(`<path d="M12 9v4M12 17h.01"/><path d="M10.3 3.9 1.8 18a2 2 0 0 0 1.7 3h17a2 2 0 0 0 1.7-3L13.7 3.9a2 2 0 0 0-3.4 0z"/>`)}</div>
        <h3>Cannot reach the server</h3>
        <p>The request did not complete. This is usually the network or the API container, not your input.
        Request <span class="mono">req_7f2a91c4</span> is in the server log if you need it.</p>
        <button class="btn">Retry</button>
      </div>
    </div>
  </div>

  <div class="card">
    <div class="card-head"><h3>Permission denied — says what to do</h3></div>
    <div class="card-body tight">
      <div class="empty">
        <div class="empty-icon">${icSm(`<path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z"/>`)}</div>
        <h3>You need the Operator role to retry executions</h3>
        <p>Your role is <span class="mono">Auditor</span>, which is read-only. Ask an owner to grant
        <span class="mono">executions:retry</span>, or ask someone with that role to run it.</p>
        <button class="btn">Request access</button>
      </div>
    </div>
  </div>

  <div class="card">
    <div class="card-head"><h3>Degraded — partial truth, labelled</h3></div>
    <div class="card-body">
      <div class="banner warn" style="margin-bottom:12px">
        ${icSm(`<circle cx="12" cy="12" r="9"/><path d="M12 16v-4M12 8h.01"/>`)}
        <div><b>Notification delivery is degraded.</b> The outbox publisher is running, but no delivery
        transport is configured, so alerts are recorded and not sent.</div>
      </div>
      <div class="xlist">
        <div class="xrow"><span class="xn">Scheduler</span><span class="xs" style="color:var(--ok)">${icSm(`<path d="M20 6 9 17l-5-5"/>`)}</span><span class="xv">Healthy</span><span class="xd">tick 4s ago</span></div>
        <div class="xrow"><span class="xn">Workers</span><span class="xs" style="color:var(--ok)">${icSm(`<path d="M20 6 9 17l-5-5"/>`)}</span><span class="xv">Healthy</span><span class="xd">9 ready</span></div>
        <div class="xrow"><span class="xn">Notifications</span><span class="xs" style="color:var(--muted-fg)">—</span><span class="xv muted">Unknown</span><span class="xd">no transport configured</span></div>
        <div class="xrow"><span class="xn">Webhooks</span><span class="xs" style="color:var(--muted-fg)">—</span><span class="xv muted">Unknown</span><span class="xd">sink logs only</span></div>
      </div>
      <p style="margin:12px 0 0;font-size:11px;color:var(--muted-fg);line-height:1.5">
        Components report <span class="mono">unknown</span> when there is no evidence either way. A missing
        heartbeat is not a healthy worker.
      </p>
    </div>
  </div>
</div>

<div class="card">
  <div class="card-head"><h3>Copy rules these screens follow</h3></div>
  <div class="card-body">
    <div class="grid g3" style="gap:20px">
      <div>
        <div style="font-size:11px;font-weight:600;margin-bottom:7px">Errors are recovery paths</div>
        <p style="margin:0;font-size:11.5px;color:var(--muted-fg);line-height:1.55">
          Say what broke, why when it matters, and what comes next. Never frame the operator as the cause.
        </p>
      </div>
      <div>
        <div style="font-size:11px;font-weight:600;margin-bottom:7px">Empty states teach the space</div>
        <p style="margin:0;font-size:11.5px;color:var(--muted-fg);line-height:1.55">
          Say what belongs here, why it matters, and the one action that fills it. A label with no direction is not a state.
        </p>
      </div>
      <div>
        <div style="font-size:11px;font-weight:600;margin-bottom:7px">Loading names the work</div>
        <p style="margin:0;font-size:11.5px;color:var(--muted-fg);line-height:1.55">
          "Loading" names nothing. "Loading 24 jobs" tells you what is arriving and whether it is stuck.
        </p>
      </div>
    </div>
  </div>
</div>`,
});

// ================================================================ command palette

const palette = `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>Command palette — Forge</title>
<link rel="preconnect" href="https://fonts.googleapis.com">
<link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
<link href="https://fonts.googleapis.com/css2?family=Inter:wght@400;450;500;600;700&family=JetBrains+Mono:wght@400;500&display=swap" rel="stylesheet">
<link rel="stylesheet" href="forge.css">
</head>
<body style="background:oklch(0.975 0 0)">
<div class="shell" style="grid-template-columns:232px minmax(0,1fr);min-height:100vh;filter:saturate(0.6)">
  <nav class="sidebar" aria-hidden="true">
    <div class="brand"><span class="brand-mark">F</span><span class="brand-name">Forge</span></div>
  </nav>
  <div class="main">
    <header class="header" aria-hidden="true">
      <span class="env prod">production</span>
      <div class="searchbtn flex1" style="max-width:420px">${I.search}<span>Search jobs, executions, workers…</span><kbd class="ml-auto">/</kbd></div>
      <span style="flex:1"></span>
    </header>
    <main class="content" aria-hidden="true">
      <div class="skel" style="height:22px;width:200px;margin-bottom:20px"></div>
      <div class="grid g4" style="margin-bottom:20px">
        <div class="skel" style="height:74px"></div><div class="skel" style="height:74px"></div>
        <div class="skel" style="height:74px"></div><div class="skel" style="height:74px"></div>
      </div>
      <div class="skel" style="height:280px"></div>
    </main>
  </div>
</div>

<div class="scrim"></div>
<div class="palette" role="dialog" aria-modal="true" aria-label="Command palette">
  <div class="palette-in">
    ${I.search}
    <input value="fail" aria-label="Command" />
    <kbd>esc</kbd>
  </div>

  <div style="max-height:440px;overflow:hidden">
    <div class="palette-sec">Actions</div>
    <div class="palette-row on">${icSm(`<path d="M21 12a9 9 0 1 1-3-6.7"/><path d="M21 3v6h-6"/>`)}<span>Open failed executions</span><span class="kind">${icSm(`<path d="M5 12h14M13 6l6 6-6 6"/>`)}</span></div>
    <div class="palette-row">${icSm(`<circle cx="12" cy="12" r="9"/><path d="M12 9v4M12 17h.01"/>`)}<span>Show what failed in the last 24 hours</span><span class="kind">${icSm(`<path d="M5 12h14M13 6l6 6-6 6"/>`)}</span></div>
    <div class="palette-row">${icSm(`<path d="M12 2 2 7l10 5 10-5-10-5z"/>`)}<span>Pause the bulk queue</span><span class="kind">${icSm(`<path d="M5 12h14M13 6l6 6-6 6"/>`)}</span></div>

    <div class="palette-sec">Jobs</div>
    <div class="palette-row"><span class="dot bad"></span><span>Settlement webhook relay</span><span class="kind">job · failing</span></div>
    <div class="palette-row"><span class="dot bad"></span><span>Fraud scoring</span><span class="kind">job · dead lettered</span></div>
    <div class="palette-row"><span class="dot ok"></span><span>Nightly settlement</span><span class="kind">job</span></div>

    <div class="palette-sec">Executions</div>
    <div class="palette-row"><span class="dot bad"></span><span class="mono" style="font-size:11.5px">ex_8f2a91c4</span><span class="muted" style="font-size:11.5px">502 · 6m ago</span><span class="kind">execution</span></div>
    <div class="palette-row"><span class="dot bad"></span><span class="mono" style="font-size:11.5px">ex_3b7e04d1</span><span class="muted" style="font-size:11.5px">timeout · 22m ago</span><span class="kind">execution</span></div>
  </div>

  <div class="pager" style="background:oklch(0.985 0 0)">
    <span class="row gap2">${kbd}<span>to navigate</span></span>
    <span class="row gap2">${kbd}<span>to select</span></span>
    <span class="ml-auto row gap2"><kbd>/</kbd><span>search</span></span>
  </div>
</div>
</body>
</html>`;

// ================================================================ job detail

const jobDetail = shell({
  active: "/jobs",
  title: "Nightly settlement",
  body: `
<div class="pagehead" style="margin-bottom:14px">
  <div class="crumbs"><a href="#">Jobs</a><span style="opacity:0.5">/</span><span style="color:var(--fg);font-weight:500">Nightly settlement</span></div>
  <div class="row gap3" style="align-items:flex-start">
    <div class="flex1">
      <div class="row gap2" style="margin-bottom:4px">
        <h1 style="margin:0">Nightly settlement</h1>
        ${statusPill("ACTIVE", "Active")}
      </div>
      <p style="display:flex;gap:14px;flex-wrap:wrap">
        <span class="mono" style="font-size:11.5px">nightly-settlement</span>
        <span style="font-size:11.5px">Owner <b style="color:var(--fg)">n.kapoor</b></span>
        <span style="font-size:11.5px">Payments</span>
        <span style="font-size:11.5px">Updated 4 days ago</span>
      </p>
    </div>
    <div class="row gap2">
      <button class="btn primary">${icSm(`<path d="M6 4l13 8-13 8V4z"/>`)} Run now</button>
      <button class="btn">${icSm(`<rect x="7" y="4" width="3.5" height="16" rx="1"/><rect x="13.5" y="4" width="3.5" height="16" rx="1"/>`)} Pause</button>
      <button class="btn">${icSm(`<path d="M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7"/><path d="M18.5 2.5a2.12 2.12 0 0 1 3 3L12 15l-4 1 1-4 9.5-9.5z"/>`)} Edit</button>
      <button class="btn" aria-label="More" style="width:32px;padding:0">${icSm(`<circle cx="5" cy="12" r="1.4"/><circle cx="12" cy="12" r="1.4"/><circle cx="19" cy="12" r="1.4"/>`)}</button>
    </div>
  </div>
</div>

<div class="tabs" style="margin-bottom:16px">
  <span class="tab on">Overview</span>
  <span class="tab">Executions <span class="cnt">412</span></span>
  <span class="tab">Schedule</span>
  <span class="tab">Versions <span class="cnt">4</span></span>
  <span class="tab">Dependencies</span>
  <span class="tab">Health</span>
  <span class="tab">Audit</span>
</div>

<div class="grid g4" style="margin-bottom:16px">
  <div class="metric"><div class="k">Success rate</div><div class="v ok">99.4%</div><div class="d">last 30 days · 412 runs</div></div>
  <div class="metric"><div class="k">Average duration</div><div class="v">8m 51s</div><div class="d">p95 9m 02s</div></div>
  <div class="metric"><div class="k">Next run</div><div class="v" style="font-size:19px">in 18m</div><div class="d">Oct 7, 02:00 IST</div></div>
  <div class="metric"><div class="k">SLA</div><div class="v ok">100%</div><div class="d">target under 15m</div></div>
</div>

<div class="split">
  <div class="col gap4">
    <div class="card">
      <div class="card-head"><h2>Schedule</h2><a class="link ml-auto" href="#" style="font-size:11.5px">Edit schedule</a></div>
      <div class="card-body">
        <div class="row gap3" style="align-items:flex-start">
          <div class="mono" style="font-size:19px;font-weight:500;padding:6px 12px;background:var(--muted);border-radius:6px">0 2 * * *</div>
          <div class="flex1">
            <div style="font-size:13px;font-weight:550">Every day at 02:00</div>
            <div style="font-size:11.5px;color:var(--muted-fg);margin-top:2px">Asia/Kolkata · fire once if missed · catch-up limit 100</div>
            <div class="row gap2" style="margin-top:9px">
              <span class="tag mono">enabled</span>
              <span class="tag">queue: critical</span>
              <span class="tag">timeout 3600s</span>
            </div>
          </div>
        </div>
        <div class="divider"></div>
        <div class="row gap2" style="font-size:11px;color:var(--muted-fg)">
          ${icSm(`<circle cx="12" cy="12" r="9"/><path d="M12 16v-4M12 8h.01"/>`)}
          <span>Last run Oct 1 at 02:00 IST succeeded in 8m 38s. Next eligible run Oct 7 at 02:00 IST.</span>
        </div>
      </div>
    </div>

    <div class="card">
      <div class="card-head">
        <h2>Recent executions</h2>
        <a class="link ml-auto" href="#" style="font-size:11.5px">See all 412</a>
      </div>
      <div class="card-body tight">
        <table>
          <thead><tr><th>Status</th><th>Scheduled</th><th>Attempt</th><th>Worker</th><th class="num">Duration</th><th>Note</th></tr></thead>
          <tbody>
            <tr class="clickable"><td>${statusPill("SUCCEEDED", "Succeeded")}</td><td class="tnum">Oct 1, 02:00</td><td class="tnum">1 of 3</td><td class="mono" style="font-size:11px">worker-17</td><td class="num tnum">8m 38s</td><td class="muted">—</td></tr>
            <tr class="clickable"><td>${statusPill("SUCCEEDED", "Succeeded")}</td><td class="tnum">Sep 30, 02:00</td><td class="tnum">1 of 3</td><td class="mono" style="font-size:11px">worker-12</td><td class="num tnum">8m 51s</td><td class="muted">—</td></tr>
            <tr class="clickable"><td>${statusPill("SUCCEEDED", "Succeeded")}</td><td class="tnum">Sep 29, 02:00</td><td class="tnum">2 of 3</td><td class="mono" style="font-size:11px">worker-17</td><td class="num tnum">9m 14s</td><td><span class="pill wait" style="height:18px;font-size:10px">retried once</span></td></tr>
            <tr class="clickable"><td>${statusPill("SUCCEEDED", "Succeeded")}</td><td class="tnum">Sep 28, 02:00</td><td class="tnum">1 of 3</td><td class="mono" style="font-size:11px">worker-04</td><td class="num tnum">8m 42s</td><td class="muted">—</td></tr>
            <tr class="clickable"><td>${statusPill("SUCCEEDED", "Succeeded")}</td><td class="tnum">Sep 27, 02:00</td><td class="tnum">1 of 3</td><td class="mono" style="font-size:11px">worker-12</td><td class="num tnum">8m 39s</td><td class="muted">—</td></tr>
          </tbody>
        </table>
      </div>
    </div>

    <div class="card">
      <div class="card-head"><h2>Versions</h2><span class="sub ml-auto">published versions are immutable</span></div>
      <div class="card-body tight">
        <table>
          <thead><tr><th>Version</th><th>Published</th><th>Execution type</th><th>Retry</th><th>Changes</th><th></th></tr></thead>
          <tbody>
            <tr><td><span class="celltitle">v4</span> <span class="pill ok" style="height:17px;font-size:9.5px">current</span></td><td class="tnum">Sep 22</td><td class="mono" style="font-size:11px">HTTP_REQUEST</td><td class="mono" style="font-size:11px">3 · exp</td><td class="muted">timeout 3600s</td><td><button class="btn sm">Diff</button></td></tr>
            <tr><td><span class="celltitle">v3</span></td><td class="tnum">Aug 14</td><td class="mono" style="font-size:11px">HTTP_REQUEST</td><td class="mono" style="font-size:11px">3 · exp</td><td class="muted">retryable classes narrowed</td><td><button class="btn sm">Diff</button></td></tr>
            <tr><td><span class="celltitle">v2</span></td><td class="tnum">Jul 2</td><td class="mono" style="font-size:11px">CONTAINER_COMMAND</td><td class="mono" style="font-size:11px">2 · fixed</td><td class="muted">moved off in-process</td><td><button class="btn sm">Diff</button></td></tr>
          </tbody>
        </table>
      </div>
    </div>
  </div>

  <div class="col gap4">
    <div class="card">
      <div class="card-head"><h3>Health</h3><span class="pill ok ml-auto">${icXs(`<path d="M20 6 9 17l-5-5"/>`)} healthy</span></div>
      <div class="card-body col gap3">
        <div class="statline"><span class="lbl">Success</span><span class="meter ok" style="flex:1"><i style="width:99%"></i></span><span class="val">99.4%</span></div>
        <div class="statline"><span class="lbl">Retries</span><span class="meter wait" style="flex:1"><i style="width:6%"></i></span><span class="val">6</span></div>
        <div class="statline"><span class="lbl">Timeouts</span><span class="meter" style="flex:1"><i style="width:1%;background:var(--muted-fg)"></i></span><span class="val">0</span></div>
        <div class="statline"><span class="lbl">Dead letters</span><span class="meter" style="flex:1"><i style="width:0%"></i></span><span class="val">0</span></div>
        <div class="divider" style="margin:2px 0"></div>
        <div style="font-size:11px;color:var(--muted-fg);line-height:1.55">
          Measured from 412 recorded durations. No score is invented when there is not enough data.
        </div>
      </div>
    </div>

    <div class="card">
      <div class="card-head"><h3>Production readiness</h3></div>
      <div class="card-body col gap2">
        <div class="row gap2" style="font-size:11.5px">${icXs(`<path d="M20 6 9 17l-5-5"/>`)}<span class="flex1">SLA target configured</span></div>
        <div class="row gap2" style="font-size:11.5px">${icXs(`<path d="M20 6 9 17l-5-5"/>`)}<span class="flex1">Alert rule on repeated failure</span></div>
        <div class="row gap2" style="font-size:11.5px">${icXs(`<path d="M20 6 9 17l-5-5"/>`)}<span class="flex1">Retry budget reviewed</span></div>
        <div class="row gap2" style="font-size:11.5px">${icXs(`<path d="M20 6 9 17l-5-5"/>`)}<span class="flex1">Owner assigned</span></div>
        <div class="row gap2" style="font-size:11.5px;color:var(--muted-fg)">${icXs(`<path d="M12 16v-4M12 8h.01"/>`)}<span class="flex1">Runbook linked</span></div>
        <div class="row gap2" style="font-size:11.5px;color:var(--muted-fg)">${icXs(`<path d="M12 16v-4M12 8h.01"/>`)}<span class="flex1">On-call rotation set</span></div>
      </div>
    </div>

    <div class="card">
      <div class="card-head"><h3>Depends on</h3></div>
      <div class="card-body col gap2" style="font-size:12px">
        <a class="row gap2" href="#" style="text-decoration:none;color:inherit">
          ${icSm(`<rect x="8" y="2" width="8" height="4" rx="1"/><path d="M16 4h2a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2h2"/>`)}
          <span class="flex1">extract-orders</span><span class="pill ok" style="height:17px;font-size:9.5px">succeeded</span>
        </a>
        <div style="font-size:11px;color:var(--muted-fg);padding-left:22px">must succeed before this job runs</div>
      </div>
    </div>
  </div>
</div>`,
});

const out = path.join(__dirname, "out");
fs.mkdirSync(out, { recursive: true });
fs.writeFileSync(path.join(out, "14-onboarding.html"), onboarding);
fs.writeFileSync(path.join(out, "15-states.html"), states);
fs.writeFileSync(path.join(out, "16-command-palette.html"), palette);
fs.writeFileSync(path.join(out, "17-job-detail.html"), jobDetail);
console.log("wrote onboarding, states, command-palette, job-detail");