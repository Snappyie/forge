// Brand-register screens: marketing landing, register, sign in.
// These are the only screens where the interface *is* the experience.

const fs = require("fs");
const path = require("path");
const { I, icSm } = require("./partials");

const page = (title, body, bodyClass = "land") => `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>${title}</title>
<link rel="preconnect" href="https://fonts.googleapis.com">
<link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
<link href="https://fonts.googleapis.com/css2?family=Inter:wght@400;450;500;600;700&family=JetBrains+Mono:wght@400;500&display=swap" rel="stylesheet">
<link rel="stylesheet" href="forge.css">
</head>
<body class="${bodyClass}">
${body}
</body>
</html>`;

// ---------------------------------------------------------------- landing

const landing = page("Forge — distributed job orchestration", `
<div class="hero">
  <nav class="land-top">
    <span class="row gap2" style="color:#fff;font-size:14px;font-weight:600">
      <span class="brand-mark" style="width:24px;height:24px;font-size:11px">F</span>Forge
    </span>
    <a href="#product">Product</a>
    <a href="#explain">Why didn't it run?</a>
    <a href="#spec">Docs</a>
    <a href="#">Changelog</a>
    <span class="ml-auto row gap2">
      <a href="#">Sign in</a>
      <a href="#" style="background:#fff;color:var(--ink);padding:6px 13px;border-radius:6px;font-weight:550">Start free</a>
    </span>
  </nav>

  <div class="hero-in">
    <span class="eyebrow">${icSm(`<circle cx="12" cy="12" r="9"/>`)} Rust · self-hosted · Apache-2.0</span>
    <h1>Every scheduled job answers: why did it run, and why not?</h1>
    <p class="lede">
      Forge is a distributed job orchestration platform built for operators who cannot afford a
      silent miss. Cron, retries with real backoff, leases, misfire policy, and an execution
      history you can audit — without leaving the browser.
    </p>
    <div class="cta">
      <a class="btn pri" href="#">Start scheduling</a>
      <a class="btn sec" href="#">${icSm(`<path d="m4 17 6-6-6-6M12 19h8"/>`)} Read the docs</a>
      <span style="font-size:12.5px;color:oklch(0.6 0 0);margin-left:6px">docker compose up --build</span>
    </div>

    <!-- Proof object: the artifact of the domain, not a generic dashboard.
         A real execution timeline with its four stages and real durations. -->
    <div class="proofstrip" style="grid-template-columns:1.55fr 1fr">
      <div>
        <div class="row gap2" style="margin-bottom:11px">
          <span class="dot ok"></span>
          <span class="pk">nightly-settlement</span>
          <span class="tag mono" style="border-color:oklch(1 0 0 / 0.16);background:oklch(1 0 0 / 0.06);color:oklch(0.78 0 0)">0 2 * * *</span>
          <span class="pv ml-auto" style="margin:0">Asia/Kolkata</span>
        </div>
        <div style="display:grid;grid-template-columns:76px minmax(0,1fr);gap:0 10px;align-items:center">
          <span class="pv" style="margin:0;font-size:10.5px">Scheduled</span>
          <span style="display:block;height:8px;border-radius:3px;background:oklch(1 0 0 / 0.13)"></span>
          <span class="pv" style="margin:0;font-size:10.5px">Queued</span>
          <span style="display:block;height:8px;border-radius:3px;background:oklch(1 0 0 / 0.13);width:16%"></span>
          <span class="pv" style="margin:0;font-size:10.5px">Running</span>
          <span style="display:block;height:8px;border-radius:3px;background:var(--ok);width:74%"></span>
          <span class="pv" style="margin:0;font-size:10.5px">Completed</span>
          <span style="display:block;height:8px;border-radius:3px;background:oklch(1 0 0 / 0.13);width:6%"></span>
        </div>
        <div class="row gap3" style="margin-top:13px">
          <span class="pv" style="margin:0">Duration <b style="color:#fff;font-weight:600">8m 38s</b></span>
          <span class="pv" style="margin:0">Attempt <b style="color:#fff;font-weight:600">1 of 3</b></span>
          <span class="pv" style="margin:0">Worker <b style="color:#fff;font-weight:600">worker-17</b></span>
        </div>
      </div>
      <div>
        <div class="pk" style="margin-bottom:11px">Why didn't Oct 2 run?</div>
        <div class="col gap2" style="font-size:11px">
          <span class="row gap2"><span style="color:var(--ok)">${icSm(`<path d="M20 6 9 17l-5-5"/>`)}</span><span class="pv" style="margin:0">Schedule active</span></span>
          <span class="row gap2"><span style="color:var(--ok)">${icSm(`<path d="M20 6 9 17l-5-5"/>`)}</span><span class="pv" style="margin:0">Scheduler healthy</span></span>
          <span class="row gap2"><span style="color:var(--bad)">${icSm(`<path d="M18 6 6 18M6 6l12 12"/>`)}</span><span class="pv" style="margin:0">Calendar</span></span>
        </div>
        <div style="margin-top:11px;padding:8px 10px;border-radius:6px;background:oklch(0.577 0.245 27.325 / 0.16);border:1px solid oklch(0.577 0.245 27.325 / 0.35);font-size:11px;line-height:1.5;color:#f7d9d4">
          October 2 is excluded by <b>India Banking Calendar</b>.
        </div>
      </div>
    </div>
  </div>
</div>

<section class="lsec" id="product">
  <div class="lsec-in">
    <div class="eyebrow2">The operator's console</div>
    <h2>Designed around the three questions an on-call engineer actually asks</h2>
    <p class="sub">
      Most schedulers answer "what failed". Forge answers what failed, why it failed, and what
      will happen next — from one screen, with the evidence attached.
    </p>
    <div class="featuregrid">
      <div class="feature">
        <div class="fi">${icSm(`<circle cx="11" cy="11" r="7"/><path d="m21 21-4.3-4.3"/>`)}</div>
        <h3>Is it healthy?</h3>
        <p>Queue depth, oldest queued age, worker liveness and success rate on one screen. Component health reports <span class="mono">unknown</span> rather than a misleading green.</p>
      </div>
      <div class="feature">
        <div class="fi">${icSm(`<circle cx="12" cy="12" r="9"/><path d="M12 16v-4M12 8h.01"/>`)}</div>
        <h3>Why did that happen?</h3>
        <p>Every execution carries a timeline, attempt history, error class, and a plain-English reason. No cross-system log hunt to explain a miss.</p>
      </div>
      <div class="feature">
        <div class="fi">${icSm(`<path d="M12 2v6M15 5l-3 3-3-3"/><path d="M4 12v6a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-6"/>`)}</div>
        <h3>What happens next?</h3>
        <p>Next-run preview with timezone and DST resolution before you save. Misfire and catch-up policy are explicit choices, not defaults you discover in production.</p>
      </div>
      <div class="feature">
        <div class="fi">${icSm(`<rect x="3" y="11" width="18" height="11" rx="2"/><path d="M7 11V7a5 5 0 0 1 10 0v4"/>`)}</div>
        <h3>Is it safe to change?</h3>
        <p>Published versions are immutable. Production changes create a new version with a diff. Job status changes are undoable for 24 hours.</p>
      </div>
      <div class="feature">
        <div class="fi">${icSm(`<path d="M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2"/><circle cx="9" cy="7" r="4"/>`)}</div>
        <h3>Who did it?</h3>
        <p>Six roles, tenant isolation enforced before read, and an append-only audit trail for every security-sensitive action.</p>
      </div>
      <div class="feature">
        <div class="fi">${icSm(`<path d="M13 2 3 14h8l-1 8 10-12h-8l1-8z"/>`)}</div>
        <h3>Will it scale with me?</h3>
        <p>Workers claim work over HTTP with leases and heartbeats. A dead worker's execution is reaped and retried rather than lost.</p>
      </div>
    </div>
  </div>
</section>

<section class="lsec lsec-lite" id="explain">
  <div class="lsec-in split" style="grid-template-columns:1fr 1fr;gap:56px;align-items:center">
    <div>
      <div class="eyebrow2">Signature capability</div>
      <h2 style="font-size:27px">"Why didn't this job run?" is a feature, not a forum post</h2>
      <p class="sub" style="margin-bottom:0">
        Forge evaluates every gate that could have suppressed an occurrence — schedule state,
        calendar, concurrency, worker availability, dependencies, maintenance windows — and shows
        the verdict with the evidence attached.
      </p>
      <ul class="col gap3" style="list-style:none;padding:0;margin:24px 0 0;font-size:13px;color:var(--muted-fg);line-height:1.55">
        <li class="row gap2" style="align-items:flex-start">${icSm(`<path d="M20 6 9 17l-5-5"/>`)}<span>Checks that passed are shown too, so a wrong guess is visibly wrong</span></li>
        <li class="row gap2" style="align-items:flex-start">${icSm(`<path d="M20 6 9 17l-5-5"/>`)}<span>Renders the exact cron, timezone, and DST decision that produced the time</span></li>
        <li class="row gap2" style="align-items:flex-start">${icSm(`<path d="M20 6 9 17l-5-5"/>`)}<span>Links straight to the schedule, queue, or maintenance window responsible</span></li>
      </ul>
    </div>
    <div class="card" style="box-shadow:0 14px 40px oklch(0 0 0 / 0.08)">
      <div class="card-head">
        <h3>Diagnosis</h3>
        <span class="sub ml-auto">nightly-settlement · Oct 2, 02:00 IST</span>
      </div>
      <div class="card-body">
        <div class="xlist">
          <div class="xrow"><span class="xn">Schedule</span><span class="xs" style="color:var(--ok)">${icSm(`<path d="M20 6 9 17l-5-5"/>`)}</span><span class="xv">Active</span><span class="xd">enabled 41d ago</span></div>
          <div class="xrow"><span class="xn">Scheduler</span><span class="xs" style="color:var(--ok)">${icSm(`<path d="M20 6 9 17l-5-5"/>`)}</span><span class="xv">Healthy</span><span class="xd">tick 5s ago</span></div>
          <div class="xrow"><span class="xn">Calendar</span><span class="xs" style="color:var(--bad)">${icSm(`<path d="M18 6 6 18M6 6l12 12"/>`)}</span><span class="xv">Excluded</span><span class="xd">India Banking Calendar</span></div>
          <div class="xrow"><span class="xn">Concurrency</span><span class="xs" style="color:var(--ok)">${icSm(`<path d="M20 6 9 17l-5-5"/>`)}</span><span class="xv">Available</span><span class="xd">1 of 4 slots</span></div>
          <div class="xrow"><span class="xn">Worker</span><span class="xs" style="color:var(--ok)">${icSm(`<path d="M20 6 9 17l-5-5"/>`)}</span><span class="xv">Available</span><span class="xd">9 ready</span></div>
        </div>
        <div class="reason" style="margin-top:14px">
          <div class="rh">Reason</div>
          <div class="rb">October 2 is excluded by <b>India Banking Calendar</b>. The next eligible occurrence is <b>October 3 at 02:00 IST</b>.</div>
        </div>
      </div>
    </div>
  </div>
</section>

<section class="lsec" id="spec">
  <div class="lsec-in">
    <div class="eyebrow2">Built to be operated</div>
    <h2>The parts that matter when something goes wrong at 3am</h2>
    <div class="featuregrid" style="margin-top:30px">
      <div class="feature">
        <h3>Cooperative cancellation</h3>
        <p style="margin-bottom:12px">Cancel records intent and lets the worker observe it, so a half-finished write is never silently abandoned.</p>
        <div class="row gap2" style="font-size:11.5px;color:var(--muted-fg)"><span class="tag mono">CANCEL_REQUESTED</span><span>→</span><span class="tag mono">CANCELLED</span></div>
      </div>
      <div class="feature">
        <h3>Leases, not hope</h3>
        <p style="margin-bottom:12px">A worker holds a 20-second lease and heartbeats every 5. Expire it and the reaper requeues or dead-letters the attempt.</p>
        <div class="row gap2" style="font-size:11.5px;color:var(--muted-fg)"><span class="tag mono">ABANDONED</span><span>→</span><span class="tag mono">QUEUED</span></div>
      </div>
      <div class="feature">
        <h3>Idempotent by default</h3>
        <p style="margin-bottom:12px">Deterministic occurrence keys, an idempotency-key header, and checksum dedupe mean a retry never doubles a run.</p>
        <div class="row gap2" style="font-size:11.5px;color:var(--muted-fg)"><span class="tag mono">Idempotency-Key</span><span class="tag mono">409 on drift</span></div>
      </div>
      <div class="feature">
        <h3>Errors that mean something</h3>
        <p style="margin-bottom:12px">Thirteen error classes drive retry. Four are retryable by default; the rest need an explicit allow-list.</p>
        <div class="row gap2" style="font-size:11.5px;color:var(--muted-fg)"><span class="tag mono">TRANSIENT</span><span class="tag mono">TIMEOUT</span><span class="tag mono">DEPENDENCY_UNAVAILABLE</span></div>
      </div>
      <div class="feature">
        <h3>Maintenance you can see</h3>
        <p style="margin-bottom:12px">Hold scheduling globally or per queue, with a required reason, and an emergency stop for everything in flight.</p>
        <div class="row gap2" style="font-size:11.5px;color:var(--muted-fg)"><span class="tag mono">MAINTENANCE</span><span class="tag mono">PAUSE QUEUE</span><span class="tag mono">CANCEL RUNNING</span></div>
      </div>
      <div class="feature">
        <h3>Your data stays yours</h3>
        <p style="margin-bottom:12px">Self-hosted, PostgreSQL, six roles, tenant isolation, and outbound URLs screened against SSRF before any call.</p>
        <div class="row gap2" style="font-size:11.5px;color:var(--muted-fg)"><span class="tag mono">MIT</span><span class="tag mono">Apache-2.0</span><span class="tag mono">No SaaS required</span></div>
      </div>
    </div>
  </div>
</section>

<footer class="land-foot">
  <div style="max-width:1120px;margin:0 auto;display:flex;gap:40px;flex-wrap:wrap">
    <div style="min-width:200px">
      <div class="row gap2" style="color:#fff;font-weight:600;font-size:13px;margin-bottom:9px">
        <span class="brand-mark" style="width:22px;height:22px;font-size:10px">F</span>Forge
      </div>
      <p style="margin:0;line-height:1.6;max-width:30ch">Distributed job scheduling and workflow orchestration, written in Rust.</p>
    </div>
    <div>
      <div style="color:#fff;font-weight:600;font-size:11.5px;margin-bottom:9px">Product</div>
      <div class="col gap2"><a href="#">Jobs</a><a href="#">Workflows</a><a href="#">Workers</a><a href="#">Alerts</a></div>
    </div>
    <div>
      <div style="color:#fff;font-weight:600;font-size:11.5px;margin-bottom:9px">Developers</div>
      <div class="col gap2"><a href="#">Quick start</a><a href="#">API reference</a><a href="#">CLI</a><a href="#">Self-hosting</a></div>
    </div>
    <div>
      <div style="color:#fff;font-weight:600;font-size:11.5px;margin-bottom:9px">Project</div>
      <div class="col gap2"><a href="#">Roadmap</a><a href="#">Security</a><a href="#">Contributing</a><a href="#">License</a></div>
    </div>
  </div>
  <div style="max-width:1120px;margin:32px auto 0;padding-top:20px;border-top:1px solid oklch(1 0 0 / 0.1);font-size:11.5px">
    The <span class="mono">docs/</span> directory is normative. Implementation and documentation disagreeing is a bug in one of them.
  </div>
</footer>
`);

const register = `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>Create your tenant — Forge</title>
<link rel="preconnect" href="https://fonts.googleapis.com">
<link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
<link href="https://fonts.googleapis.com/css2?family=Inter:wght@400;450;500;600;700&family=JetBrains+Mono:wght@400;500&display=swap" rel="stylesheet">
<link rel="stylesheet" href="forge.css">
</head>
<body>
<div class="auth">
  <div class="auth-form">
    <div class="auth-box">
      <div class="row gap2" style="margin-bottom:26px">
        <span class="brand-mark">F</span>
        <span style="font-size:14px;font-weight:600">Forge</span>
      </div>
      <h1 style="margin:0 0 5px;font-size:21px;font-weight:600;letter-spacing:-0.02em">Create your tenant</h1>
      <p style="margin:0 0 24px;font-size:13px;color:var(--muted-fg);line-height:1.55">
        A tenant is an isolated workspace. Every job, worker, and execution belongs to exactly one.
      </p>

      <form class="col gap4">
        <div class="field">
          <label class="label" for="t">Tenant name</label>
          <input class="input" id="t" value="Acme Payments" />
          <span class="hint">Your team or environment. You can add more tenants later.</span>
        </div>
        <div class="field">
          <label class="label" for="e">Work email</label>
          <input class="input" id="e" type="email" value="neel@acme.com" />
        </div>
        <div class="field">
          <label class="label" for="p">Password</label>
          <input class="input" id="p" type="password" value="correct-horse-battery" />
          <div class="row gap2">
            <span class="meter ok" style="flex:1"><i style="width:100%"></i></span>
            <span style="font-size:11px;color:oklch(0.44 0.13 163);font-weight:550">Strong</span>
          </div>
          <span class="hint">At least 12 characters. Stored with argon2id, never recoverable.</span>
        </div>
        <button class="btn primary lg" style="width:100%" type="button">Create tenant and continue</button>
      </form>

      <p style="margin:18px 0 0;font-size:12px;color:var(--muted-fg);text-align:center">
        Already have an account? <a class="link" href="#">Sign in</a>
      </p>
    </div>
  </div>

  <aside class="auth-side" style="justify-content:center">
    <div style="max-width:400px">
      <div style="font-size:11px;font-weight:600;letter-spacing:0.08em;text-transform:uppercase;color:oklch(0.62 0 0);margin-bottom:18px">What happens next</div>
      <h2 style="margin:0 0 26px;font-size:26px;font-weight:600;letter-spacing:-0.024em;line-height:1.2">
        Three steps to your first execution
      </h2>
      <div class="steps">
        <div class="step" style="border-color:oklch(1 0 0 / 0.13)">
          <span class="step-num" style="border-color:oklch(1 0 0 / 0.25);color:oklch(0.75 0 0)">1</span>
          <div>
            <div class="step-t">Name a job</div>
            <div class="step-d" style="color:oklch(0.66 0 0)">A reusable definition of work: what to run, how often, and what happens when it fails.</div>
          </div>
        </div>
        <div class="step" style="border-color:oklch(1 0 0 / 0.13)">
          <span class="step-num" style="border-color:oklch(1 0 0 / 0.25);color:oklch(0.75 0 0)">2</span>
          <div>
            <div class="step-t">Publish a version</div>
            <div class="step-d" style="color:oklch(1 0 0 / 0.62)">Drafts never run. Publishing freezes a version so a change can never alter work already dispatched.</div>
          </div>
        </div>
        <div class="step" style="border-color:oklch(1 0 0 / 0.13)">
          <span class="step-num" style="border-color:oklch(1 0 0 / 0.25);color:oklch(0.75 0 0)">3</span>
          <div>
            <div class="step-t">Run it, or let it run</div>
            <div class="step-d" style="color:oklch(1 0 0 / 0.62)">Trigger once by hand, or attach a schedule and a worker and let Forge do it for you.</div>
          </div>
        </div>
      </div>
      <p style="margin:30px 0 0;font-size:11.5px;color:oklch(0.58 0 0);line-height:1.6">
        No card, no cluster. Runs on one Postgres container to start.
      </p>
    </div>
  </aside>
</div>
</body>
</html>`;

// ---------------------------------------------------------------- sign in

const login = `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>Sign in — Forge</title>
<link rel="preconnect" href="https://fonts.googleapis.com">
<link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
<link href="https://fonts.googleapis.com/css2?family=Inter:wght@400;450;500;600;700&family=JetBrains+Mono:wght@400;500&display=swap" rel="stylesheet">
<link rel="stylesheet" href="forge.css">
</head>
<body>
<div class="auth" style="grid-template-columns:1fr 1fr">
  <div class="auth-form">
    <div class="auth-box">
      <div class="row gap2" style="margin-bottom:26px">
        <span class="brand-mark">F</span>
        <span style="font-size:14px;font-weight:600">Forge</span>
      </div>
      <h1 style="margin:0 0 5px;font-size:21px;font-weight:600;letter-spacing:-0.02em">Sign in</h1>
      <p style="margin:0 0 24px;font-size:13px;color:var(--muted-fg)">Access the console for your tenant.</p>

      <div class="banner bad" style="margin-bottom:16px">
        ${icSm(`<path d="M12 9v4M12 17h.01"/><path d="M10.3 3.9 1.8 18a2 2 0 0 0 1.7 3h17a2 2 0 0 0 1.7-3L13.7 3.9a2 2 0 0 0-3.4 0z"/>`)}
        <div>
          <b>Email or password is incorrect.</b>
          <div style="margin-top:2px;opacity:0.85">Check the address, or sign in with a personal access token.</div>
        </div>
      </div>

      <form class="col gap4">
        <div class="field">
          <label class="label" for="e">Work email</label>
          <input class="input" id="e" type="email" value="neel@acme.com" />
        </div>
        <div class="field">
          <div class="row">
            <label class="label" for="p">Password</label>
            <a class="link ml-auto" href="#" style="font-size:11.5px">Forgot password?</a>
          </div>
          <input class="input" id="p" type="password" value="wrong-password" />
        </div>
        <button class="btn primary lg" style="width:100%" type="button">Sign in</button>
      </form>

      <div class="row gap3" style="margin:20px 0">
        <span style="flex:1;height:1px;background:var(--border)"></span>
        <span style="font-size:11px;color:var(--muted-fg)">or</span>
        <span style="flex:1;height:1px;background:var(--border)"></span>
      </div>

      <div class="field">
        <label class="label" for="t">Personal access token</label>
        <input class="input mono" id="t" placeholder="forge_••••••••••••••••" />
        <span class="hint">Create a token under Administration → API keys. Tokens are shown once.</span>
      </div>
      <button class="btn lg" style="width:100%" type="button">${icSm(`<path d="M21 2l-2 2m-7.6 7.6a5.5 5.5 0 1 1-7.8 7.8 5.5 5.5 0 0 1 7.8-7.8zm0 0L15.5 7.5m0 0 3 3L22 7l-3-3m-3.5 3.5L19 4"/>`)} Sign in with token</button>

      <p style="margin:20px 0 0;font-size:12px;color:var(--muted-fg);text-align:center">
        New to Forge? <a class="link" href="#">Create a tenant</a>
      </p>
    </div>
  </div>

  <aside class="auth-side" style="justify-content:center">
    <div style="max-width:410px">
      <div style="font-size:11px;font-weight:600;letter-spacing:0.08em;text-transform:uppercase;color:oklch(0.62 0 0);margin-bottom:18px">Production status</div>
      <div class="card" style="background:oklch(0.205 0 0);border-color:oklch(1 0 0 / 0.13)">
        <div class="card-head" style="border-color:oklch(1 0 0 / 0.1)">
          <h3 style="color:#fff">acme-prod</h3>
          <span class="tag" style="margin-left:auto;border-color:oklch(1 0 0 / 0.18);background:oklch(1 0 0 / 0.06);color:oklch(0.8 0 0)">eu-west-1</span>
        </div>
        <div class="card-body col gap3">
          <div class="row gap2" style="font-size:12px;color:oklch(0.82 0 0)"><span class="dot ok"></span> Scheduler <span class="ml-auto mono" style="font-size:11px;color:oklch(0.6 0 0)">tick 4s ago</span></div>
          <div class="row gap2" style="font-size:12px;color:oklch(0.82 0 0)"><span class="dot ok"></span> Workers <span class="ml-auto mono" style="font-size:11px;color:oklch(0.6 0 0)">9 ready</span></div>
          <div class="row gap2" style="font-size:12px;color:oklch(0.82 0 0)"><span class="dot wait"></span> Queues <span class="ml-auto mono" style="font-size:11px;color:oklch(0.6 0 0)">1 paused</span></div>
        </div>
      </div>
      <p style="margin:26px 0 0;font-size:12.5px;color:oklch(0.66 0 0);line-height:1.65">
        You are signing in to a live production tenant. Destructive actions require a typed reason and
        are written to the audit log.
      </p>
    </div>
  </aside>
</div>
</body>
</html>`;

const out = path.join(__dirname, "out");
fs.mkdirSync(out, { recursive: true });
fs.writeFileSync(path.join(out, "01-landing.html"), landing);
fs.writeFileSync(path.join(out, "02-register.html"), register);
fs.writeFileSync(path.join(out, "03-sign-in.html"), login);
console.log("wrote landing, register, sign-in");