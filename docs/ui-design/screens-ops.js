// Operational screens: executions table, execution detail (the richest screen),
// job detail, running-now, calendar, workflows, emergency.

const fs = require("fs");
const path = require("path");
const { I, icSm, icXs, shell, statusPill, prioPill } = require("./partials");

const crumb = (...items) =>
  `<div class="crumbs">${items
    .map((it, i) =>
      i === items.length - 1
        ? `<span style="color:var(--fg);font-weight:500">${it}</span>`
        : `<a href="#">${it}</a><span style="opacity:0.5">/</span>`
    )
    .join("")}</div>`;

const kbd = `<kbd>⌘K</kbd>`;

// ================================================================ executions list

const executions = shell({
  active: "/running",
  title: "Executions",
  body: `
<div class="pagehead row gap3" style="align-items:flex-start">
  <div class="flex1">
    <h1>Executions</h1>
    <p>Every run, across every job, with the reason it ended the way it did.</p>
  </div>
  <div class="row gap2">
    <button class="btn">${icSm(`<path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><path d="M7 10l5 5 5-5M12 15V3"/>`)} Export</button>
    <button class="btn">${icSm(`<path d="M5 3v18M19 3v18"/><path d="M5 8h4M15 16h4"/>`)} Compare</button>
  </div>
</div>

<div class="card">
  <div class="filterbar">
    <div class="finput flex1" style="max-width:250px">${I.search}<span class="ph">Search execution or job…</span></div>
    <button class="fchip on">Status: any ${icXs(`<path d="M6 9l6 6 6-6"/>`)}</button>
    <button class="fchip">After: 2026-10-01</button>
    <button class="fchip">Trigger: Schedule</button>
    <button class="fchip">Queue: default</button>
    <span style="flex:1"></span>
    <span class="row gap1" style="font-size:11px;color:var(--muted-fg)">${icSm(`<path d="M21 12a9 9 0 1 1-3-6.7"/><path d="M21 3v6h-6"/>`)} Live · 5s</span>
  </div>

  <table>
    <thead>
      <tr>
        <th style="width:34px"><span class="checkbox"></span></th>
        <th>Status</th>
        <th>Job</th>
        <th>Trigger</th>
        <th>Attempt</th>
        <th>Worker</th>
        <th class="sortable">Duration <span class="arrow">↓</span></th>
        <th class="sortable">Started <span class="arrow">↓</span></th>
        <th>Error</th>
        <th style="width:34px"></th>
      </tr>
    </thead>
    <tbody>
      <tr class="clickable">
        <td><span class="checkbox"></span></td>
        <td>${statusPill("RUNNING", "Running")}</td>
        <td><div class="celltitle">Nightly settlement</div><div class="cellsub mono">ex_1a4f88c2</div></td>
        <td><span class="tag">Schedule</span></td>
        <td class="tnum">2 of 3</td>
        <td class="mono" style="font-size:11px">worker-17</td>
        <td class="tnum">3m 04s</td>
        <td class="tnum">02:00:41</td>
        <td class="muted">—</td>
        <td><button class="hbtn" style="width:24px;height:24px;padding:0;justify-content:center">${icSm(`<circle cx="5" cy="12" r="1.4"/><circle cx="12" cy="12" r="1.4"/><circle cx="19" cy="12" r="1.4"/>`)}</button></td>
      </tr>
      <tr class="clickable">
        <td><span class="checkbox"></span></td>
        <td>${statusPill("FAILED", "Failed")}</td>
        <td><div class="celltitle">Settlement webhook relay</div><div class="cellsub mono">ex_8f2a91c4</div></td>
        <td><span class="tag">Schedule</span></td>
        <td class="tnum">3 of 3</td>
        <td class="mono" style="font-size:11px">worker-04</td>
        <td class="tnum">1.2s</td>
        <td class="tnum">01:55:02</td>
        <td><span class="pill bad sq monoish">DEPENDENCY_UNAVAILABLE</span></td>
        <td><button class="hbtn" style="width:24px;height:24px;padding:0;justify-content:center">${icSm(`<circle cx="5" cy="12" r="1.4"/><circle cx="12" cy="12" r="1.4"/><circle cx="19" cy="12" r="1.4"/>`)}</button></td>
      </tr>
      <tr class="clickable">
        <td><span class="checkbox"></span></td>
        <td>${statusPill("DEAD_LETTERED", "Dead lettered")}</td>
        <td><div class="celltitle">Fraud scoring</div><div class="cellsub mono">ex_3b7e04d1</div></td>
        <td><span class="tag">Schedule</span></td>
        <td class="tnum">3 of 3</td>
        <td class="mono" style="font-size:11px">worker-09</td>
        <td class="tnum">30m 00s</td>
        <td class="tnum">01:45:00</td>
        <td><span class="pill bad sq monoish">TIMEOUT</span></td>
        <td><button class="hbtn" style="width:24px;height:24px;padding:0;justify-content:center">${icSm(`<circle cx="5" cy="12" r="1.4"/><circle cx="12" cy="12" r="1.4"/><circle cx="19" cy="12" r="1.4"/>`)}</button></td>
      </tr>
      <tr class="clickable">
        <td><span class="checkbox"></span></td>
        <td>${statusPill("ABANDONED", "Abandoned")}</td>
        <td><div class="celltitle">Invoice render</div><div class="cellsub mono">ex_44ae7712</div></td>
        <td><span class="tag">Manual</span></td>
        <td class="tnum">2 of 5</td>
        <td class="mono" style="font-size:11px">worker-02</td>
        <td class="tnum">20s</td>
        <td class="tnum">00:40:12</td>
        <td><span class="pill wait sq monoish">TRANSIENT</span></td>
        <td><button class="hbtn" style="width:24px;height:24px;padding:0;justify-content:center">${icSm(`<circle cx="5" cy="12" r="1.4"/><circle cx="12" cy="12" r="1.4"/><circle cx="19" cy="12" r="1.4"/>`)}</button></td>
      </tr>
      <tr class="clickable">
        <td><span class="checkbox"></span></td>
        <td>${statusPill("SUCCEEDED", "Succeeded")}</td>
        <td><div class="celltitle">Usage rollup</div><div class="cellsub mono">ex_77bc03f5</div></td>
        <td><span class="tag">Workflow</span></td>
        <td class="tnum">1 of 3</td>
        <td class="mono" style="font-size:11px">worker-21</td>
        <td class="tnum">41s</td>
        <td class="tnum">00:00:03</td>
        <td class="muted">—</td>
        <td><button class="hbtn" style="width:24px;height:24px;padding:0;justify-content:center">${icSm(`<circle cx="5" cy="12" r="1.4"/><circle cx="12" cy="12" r="1.4"/><circle cx="19" cy="12" r="1.4"/>`)}</button></td>
      </tr>
      <tr class="clickable">
        <td><span class="checkbox"></span></td>
        <td>${statusPill("QUEUED", "Queued")}</td>
        <td><div class="celltitle">Nightly settlement</div><div class="cellsub mono">ex_9d2e1170</div></td>
        <td><span class="tag">Schedule</span></td>
        <td class="tnum">1 of 3</td>
        <td class="muted">waiting</td>
        <td class="tnum muted">4m 12s</td>
        <td class="tnum">02:00:00</td>
        <td class="muted">—</td>
        <td><button class="hbtn" style="width:24px;height:24px;padding:0;justify-content:center">${icSm(`<circle cx="5" cy="12" r="1.4"/><circle cx="12" cy="12" r="1.4"/><circle cx="19" cy="12" r="1.4"/>`)}</button></td>
      </tr>
      <tr class="clickable">
        <td><span class="checkbox"></span></td>
        <td>${statusPill("CANCEL_REQUESTED", "Cancelling")}</td>
        <td><div class="celltitle">Email digest</div><div class="cellsub mono">ex_2c8a4409</div></td>
        <td><span class="tag">Api</span></td>
        <td class="tnum">1 of 3</td>
        <td class="mono" style="font-size:11px">worker-14</td>
        <td class="tnum">18s</td>
        <td class="tnum">01:30:00</td>
        <td class="muted">—</td>
        <td><button class="hbtn" style="width:24px;height:24px;padding:0;justify-content:center">${icSm(`<circle cx="5" cy="12" r="1.4"/><circle cx="12" cy="12" r="1.4"/><circle cx="19" cy="12" r="1.4"/>`)}</button></td>
      </tr>
    </tbody>
  </table>
  <div class="pager">
    <span>Showing 1–7 of 1,310 executions</span>
    <div class="ml-auto row gap2"><button class="btn sm" disabled>Previous</button><button class="btn sm">Next</button></div>
  </div>
</div>`,
});

// ================================================================ execution detail

const executionDetail = shell({
  active: "/running",
  title: "Execution ex_8f2a91c4",
  body: `
<div class="pagehead" style="margin-bottom:14px">
  ${crumb("Executions", "Settlement webhook relay")}
  <div class="row gap3" style="align-items:flex-start">
    <div class="flex1">
      <div class="row gap2" style="margin-bottom:4px">
        <h1 style="margin:0">Settlement webhook relay</h1>
        ${statusPill("FAILED", "Failed")}
        ${prioPill("HIGH")}
      </div>
      <p class="row gap2">
        <span class="mono" style="font-size:11.5px">ex_8f2a91c4-71b3-4d2e-9a08-5c1f2b6d4e77</span>
        <button class="hbtn" aria-label="Copy ID" style="width:20px;height:20px;padding:0;justify-content:center">${icSm(`<rect x="9" y="9" width="12" height="12" rx="2"/><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"/>`)}</button>
        <span style="opacity:0.4">|</span>
        <span style="font-size:11.5px">correlation <span class="mono">schedule:8f2a91c4:1759879200</span></span>
      </p>
    </div>
    <div class="row gap2">
      <button class="btn">${icSm(`<path d="M21 12a9 9 0 1 1-3-6.7"/><path d="M21 3v6h-6"/>`)} Retry</button>
      <button class="btn">${icSm(`<path d="m4 17 6-6-6-6M12 19h8"/>`)} Replay</button>
      <button class="btn">${icSm(`<path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><path d="M7 10l5 5 5-5M12 15V3"/>`)} Logs</button>
      <button class="btn danger">${icSm(`<rect x="7" y="4" width="3.5" height="16" rx="1"/><rect x="13.5" y="4" width="3.5" height="16" rx="1"/>`)} Cancel</button>
      <button class="btn" aria-label="More actions" style="width:32px;padding:0">${icSm(`<circle cx="5" cy="12" r="1.4"/><circle cx="12" cy="12" r="1.4"/><circle cx="19" cy="12" r="1.4"/>`)}</button>
    </div>
  </div>
</div>

<div class="tabs" style="margin-bottom:16px">
  <span class="tab on">Overview</span>
  <span class="tab">Logs <span class="cnt">148</span></span>
  <span class="tab">Attempts <span class="cnt">3</span></span>
  <span class="tab">Timeline</span>
  <span class="tab">Input / output</span>
  <span class="tab">Metrics</span>
  <span class="tab">Events</span>
</div>

<div class="banner bad" style="margin-bottom:16px">
  ${icSm(`<path d="M12 9v4M12 17h.01"/><path d="M10.3 3.9 1.8 18a2 2 0 0 0 1.7 3h17a2 2 0 0 0 1.7-3L13.7 3.9a2 2 0 0 0-3.4 0z"/>`)}
  <div class="flex1">
    <b>Attempt 3 of 3 failed with <span class="mono" style="font-size:11.5px">DEPENDENCY_UNAVAILABLE</span>.</b>
    The partner endpoint returned 502 on all three attempts, spaced 1s, 2s, then 4s apart. The retry
    budget is spent, so this execution will not run again on its own.
  </div>
  <button class="btn sm">Retry now</button>
</div>

<div class="split">
  <div class="col gap4">
    <div class="card">
      <div class="card-head">
        <h2>Timeline</h2>
        <span class="sub ml-auto">total elapsed 4m 31s · queue wait 4m 12s</span>
      </div>
      <div class="card-body">
        <div class="gantt" style="margin-bottom:14px">
          <span style="font-size:11px;color:var(--muted-fg);align-self:center">Elapsed</span>
          <div>
            <div class="gantt-axis"><span>0s</span><span>1m</span><span>2m</span><span>3m</span><span>4m 31s</span></div>
            <div style="position:relative;height:16px">
              <span style="position:absolute;left:0;top:0;width:92.6%;height:16px;border-radius:3px;background:var(--wait)"></span>
              <span style="position:absolute;left:92.6%;top:0;width:1.5%;height:16px;border-radius:3px;background:var(--bad)"></span>
              <span style="position:absolute;left:94.1%;top:0;width:5.9%;height:16px;border-radius:3px;background:oklch(0.86 0 0)"></span>
            </div>
          </div>
        </div>
        <div class="tl">
          <div class="tl-item">
            <span class="tl-dot"></span>
            <div class="tl-label">Scheduled <span class="tag mono">SCHEDULE</span></div>
            <div class="tl-meta">01:55:00 · cron <span class="mono">*/5 * * * *</span> matched this occurrence</div>
          </div>
          <div class="tl-item">
            <span class="tl-dot wait"></span>
            <div class="tl-label">Queued <span class="tl-gap">+0.2s</span></div>
            <div class="tl-meta">01:55:00.214 · queue <span class="mono">default</span>, priority High</div>
          </div>
          <div class="tl-item">
            <span class="tl-dot"></span>
            <div class="tl-label">Waiting for a worker <span class="tl-gap">+4m 12s</span></div>
            <div class="tl-meta">queue <span class="mono">critical</span> was paused by the active maintenance window until 01:59:41</div>
          </div>
          <div class="tl-item">
            <span class="tl-dot run"></span>
            <div class="tl-label">Dispatched <span class="tl-gap">+0.3s</span></div>
            <div class="tl-meta">01:59:41.5 · claimed by <span class="mono">worker-04</span> · lease 20s, heartbeat 5s</div>
          </div>
          <div class="tl-item">
            <span class="tl-dot run"></span>
            <div class="tl-label">Running <span class="tl-gap">+0.9s</span></div>
            <div class="tl-meta">01:59:42.4 · execution type <span class="mono">HTTP_REQUEST</span></div>
          </div>
          <div class="tl-item">
            <span class="tl-dot bad"></span>
            <div class="tl-label">Failed <span class="tl-gap">+1.2s</span></div>
            <div class="tl-meta">01:59:43.6 · <span class="mono" style="color:var(--bad)">DEPENDENCY_UNAVAILABLE</span> · 502 Bad Gateway from partner endpoint</div>
          </div>
          <div class="tl-item">
            <span class="tl-dot wait"></span>
            <div class="tl-label">Retry scheduled <span class="tl-gap">+1.0s</span></div>
            <div class="tl-meta">exponential backoff · delay 1s (attempt 1 of 3)</div>
          </div>
        </div>
      </div>
    </div>

    <div class="card">
      <div class="card-head">
        <h2>Logs</h2>
        <span class="pill bad">attempt 3 of 3</span>
        <div class="ml-auto row gap2">
          <div class="finput" style="min-width:190px;height:26px">${I.search}<span class="ph">Filter lines…</span></div>
          <button class="fchip on" style="height:26px;font-size:11px">stderr</button>
          <button class="fchip" style="height:26px;font-size:11px">Wrap ${icXs(`<path d="M6 9l6 6 6-6"/>`)}</button>
        </div>
      </div>
      <div class="card-body tight" style="padding:9px 0">
        <div class="logs">
          <div class="logline"><span class="ln">1</span><span class="lt"><span class="ts">01:59:42.41</span>POST <span class="key">/v1/settlements</span> → https://partner.acme-payments.io/v1/settlements</span></div>
          <div class="logline"><span class="ln">2</span><span class="lt"><span class="ts">01:59:42.42</span><span class="dim2">attempt 3 of 3 · correlation 8f2a91c4</span></span></div>
          <div class="logline"><span class="ln">3</span><span class="lt"><span class="ts">01:59:42.43</span>headers: <span class="key">x-idempotency-key: s8f2a91c41759879200</span></span></div>
          <div class="logline"><span class="ln">4</span><span class="lt"><span class="ts">01:59:42.44</span>body: <span class="dim2">{amount_minor: 4821900, currency: "INR", batch_id: "b-7741"}</span></span></div>
          <div class="logline"><span class="ln">5</span><span class="lt"><span class="ts">01:59:43.05</span><span class="wrn">WARN</span> upstream latency 610ms exceeds soft budget 500ms</span></div>
          <div class="logline"><span class="ln">6</span><span class="lt"><span class="ts">01:59:43.62</span><span class="err">ERROR</span> HTTP 502 Bad Gateway — partner edge returned no response</span></div>
          <div class="logline"><span class="ln">7</span><span class="lt"><span class="ts">01:59:43.62</span><span class="dim2">response headers: server: envoy, x-envoy-upstream-service: partner-edge</span></span></div>
          <div class="logline"><span class="ln">8</span><span class="lt"><span class="ts">01:59:43.63</span><span class="err">ERROR</span> classified as <span class="key">DEPENDENCY_UNAVAILABLE</span> (retryable by policy)</span></div>
          <div class="logline hit"><span class="ln">9</span><span class="lt"><span class="ts">01:59:43.63</span><span class="err">ERROR</span> retries exhausted: <span class="key">3 of 3</span> attempts used</span></div>
          <div class="logline"><span class="ln">10</span><span class="lt"><span class="ts">01:59:43.64</span>lease released · worker-04 · duration 1197ms</span></div>
          <div class="logline"><span class="ln">11</span><span class="lt"><span class="ts">01:59:43.64</span><span class="dim2">no further attempts scheduled: attempt budget exhausted</span></span></div>
        </div>
      </div>
      <div class="pager">
        <span>148 lines · showing most recent 11</span>
        <div class="ml-auto row gap2">
          <button class="btn sm">Download</button>
          <button class="btn sm">View trace</button>
        </div>
      </div>
    </div>
  </div>

  <div class="col gap4">
    <div class="card">
      <div class="card-head"><h3>Execution</h3></div>
      <div class="card-body">
        <dl class="kv" style="font-size:11.5px">
          <dt>Status</dt><dd>${statusPill("FAILED", "Failed")}</dd>
          <dt>Job</dt><dd><a class="link" href="#">Settlement webhook relay</a></dd>
          <dt>Version</dt><dd class="mono">v2 · published</dd>
          <dt>Trigger</dt><dd>Schedule</dd>
          <dt>Attempt</dt><dd class="tnum">3 of 3</dd>
          <dt>Scheduled for</dt><dd class="tnum">01:55:00</dd>
          <dt>Dispatched</dt><dd class="tnum">01:59:41</dd>
          <dt>Duration</dt><dd class="tnum">1.2s</dd>
          <dt>Queue wait</dt><dd class="tnum">4m 12s</dd>
          <dt>Worker</dt><dd><a class="link mono" href="#">worker-04</a></dd>
          <dt>Priority</dt><dd>${prioPill("HIGH")}</dd>
          <dt>Error class</dt><dd><span class="pill bad sq monoish">DEPENDENCY_UNAVAILABLE</span></dd>
        </dl>
      </div>
    </div>

    <div class="card">
      <div class="card-head"><h3>Attempts</h3><span class="sub ml-auto">3 of 3 used</span></div>
      <div class="card-body col gap3">
        <div class="row gap2" style="font-size:12px">
          <span class="dot bad"></span><span style="font-weight:550">3</span>
          <span class="flex1 muted" style="font-size:11px">01:59:43 · 1.2s</span>
          <span class="pill bad sq monoish" style="height:18px;font-size:10px">502</span>
        </div>
        <div class="row gap2" style="font-size:12px">
          <span class="dot bad"></span><span style="font-weight:550">2</span>
          <span class="flex1 muted" style="font-size:11px">01:59:41 · 1.1s</span>
          <span class="pill bad sq monoish" style="height:18px;font-size:10px">502</span>
        </div>
        <div class="row gap2" style="font-size:12px">
          <span class="dot bad"></span><span style="font-weight:550">1</span>
          <span class="flex1 muted" style="font-size:11px">01:59:40 · 0.9s</span>
          <span class="pill bad sq monoish" style="height:18px;font-size:10px">502</span>
        </div>
        <div class="divider" style="margin:0"></div>
        <div style="font-size:11px;color:var(--muted-fg);line-height:1.5">
          Backoff was exponential from 1s, multiplier 2, capped at 1h. Delay before each retry is
          shown on the timeline.
        </div>
      </div>
    </div>

    <div class="card">
      <div class="card-head"><h3>Related</h3></div>
      <div class="card-body col gap2">
        <a class="row gap2" href="#" style="font-size:12px;text-decoration:none;color:inherit">
          ${icSm(`<rect x="8" y="2" width="8" height="4" rx="1"/><path d="M16 4h2a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2h2"/>`)}
          <span class="flex1">Settlement webhook relay</span>${icSm(`<path d="M9 18l6-6-6-6"/>`)}
        </a>
        <a class="row gap2" href="#" style="font-size:12px;text-decoration:none;color:inherit">
          ${icSm(`<path d="M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2"/><circle cx="9" cy="7" r="4"/>`)}
          <span class="flex1">worker-04</span>${icSm(`<path d="M9 18l6-6-6-6"/>`)}
        </a>
        <a class="row gap2" href="#" style="font-size:12px;text-decoration:none;color:inherit">
          ${icSm(`<path d="M12 2 2 7l10 5 10-5-10-5z"/>`)}
          <span class="flex1">default queue</span>${icSm(`<path d="M9 18l6-6-6-6"/>`)}
        </a>
        <a class="row gap2" href="#" style="font-size:12px;text-decoration:none;color:inherit">
          ${icSm(`<circle cx="12" cy="12" r="9"/><path d="M12 9v4M12 17h.01"/>`)}
          <span class="flex1">Open incident for this job</span>${icSm(`<path d="M9 18l6-6-6-6"/>`)}
        </a>
      </div>
    </div>
  </div>
</div>`,
});

// ================================================================ running now

const running = shell({
  active: "/running",
  title: "Running now",
  body: `
<div class="pagehead row gap3" style="align-items:flex-start">
  <div class="flex1">
    <div class="row gap2"><h1>Running now</h1><span class="pill run">${icXs(`<circle cx="12" cy="12" r="5"/>`)} live</span></div>
    <p>What is executing right now, on which worker, and how long it has held the slot.</p>
  </div>
  <div class="row gap2">
    <button class="btn">${icSm(`<rect x="7" y="4" width="3.5" height="16" rx="1"/><rect x="13.5" y="4" width="3.5" height="16" rx="1"/>`)} Pause queue</button>
    <button class="btn danger">${icSm(`<path d="M12 2 2 7l10 5 10-5-10-5z"/><path d="M2 17l10 5 10-5"/>`)} Emergency stop</button>
  </div>
</div>

<div class="grid g4" style="margin-bottom:16px">
  <div class="metric"><div class="k">Running</div><div class="v">3</div><div class="d">of 9 busy workers</div></div>
  <div class="metric"><div class="k">Queued</div><div class="v">14</div><div class="d">oldest 4m 12s</div></div>
  <div class="metric"><div class="k">Longest running</div><div class="v">3m 04s</div><div class="d">nightly-settlement</div></div>
  <div class="metric"><div class="k">Workers draining</div><div class="v">1</div><div class="d">finishing in-flight work</div></div>
</div>

<div class="split">
  <div class="card">
    <div class="card-head"><h2>In flight</h2><span class="sub ml-auto">refreshes every 5s · heartbeat 5s</span></div>
    <div class="card-body tight">
      <table>
        <thead><tr><th>Job</th><th>Worker</th><th>Elapsed</th><th>Lease</th><th>Status</th><th class="num">Heartbeat</th></tr></thead>
        <tbody>
          <tr class="clickable">
            <td><div class="celltitle">Nightly settlement</div><div class="cellsub mono">ex_1a4f88c2 · attempt 2 of 3</div></td>
            <td class="mono" style="font-size:11px">worker-17</td>
            <td class="tnum" style="font-weight:550">3m 04s</td>
            <td><span class="meter run" style="width:56px;display:inline-block"><i style="width:64%"></i></span></td>
            <td><div class="row gap2"><span class="dot run pulse"></span><span style="font-size:11.5px">Running</span></div></td>
            <td class="num tnum muted">2s ago</td>
          </tr>
          <tr class="clickable">
            <td><div class="celltitle">Usage rollup</div><div class="cellsub mono">ex_5f2b1190 · attempt 1 of 3</div></td>
            <td class="mono" style="font-size:11px">worker-21</td>
            <td class="tnum" style="font-weight:550">41s</td>
            <td><span class="meter run" style="width:56px;display:inline-block"><i style="width:24%"></i></span></td>
            <td><div class="row gap2"><span class="dot run pulse"></span><span style="font-size:11.5px">Running</span></div></td>
            <td class="num tnum muted">1s ago</td>
          </tr>
          <tr class="clickable">
            <td><div class="celltitle">Email digest</div><div class="cellsub mono">ex_2c8a4409 · attempt 1 of 3</div></td>
            <td class="mono" style="font-size:11px">worker-14</td>
            <td class="tnum" style="font-weight:550">18s</td>
            <td><span class="meter wait" style="width:56px;display:inline-block"><i style="width:18%"></i></span></td>
            <td><div class="row gap2"><span class="dot wait"></span><span style="font-size:11.5px">Cancelling</span></div></td>
            <td class="num tnum muted">4s ago</td>
          </tr>
        </tbody>
      </table>
    </div>
    <div class="pager"><span>3 running · 14 queued · 1 worker draining</span><a class="link ml-auto" href="#">Open queue view</a></div>
  </div>

  <div class="col gap4">
    <div class="card">
      <div class="card-head"><h3>Workers</h3><a class="link ml-auto" href="#" style="font-size:11.5px">All 9</a></div>
      <div class="card-body col gap2">
        <div class="row gap2" style="font-size:11.5px"><span class="dot run"></span><span class="mono flex1">worker-17</span><span class="muted">3 slots · busy</span></div>
        <div class="row gap2" style="font-size:11.5px"><span class="dot run"></span><span class="mono flex1">worker-21</span><span class="muted">1 slot · busy</span></div>
        <div class="row gap2" style="font-size:11.5px"><span class="dot run"></span><span class="mono flex1">worker-04</span><span class="muted">2 slots · busy</span></div>
        <div class="row gap2" style="font-size:11.5px"><span class="dot wait"></span><span class="mono flex1">worker-14</span><span class="muted">cancelling</span></div>
        <div class="row gap2" style="font-size:11.5px"><span class="dot ok"></span><span class="mono flex1">worker-02</span><span class="muted">ready</span></div>
        <div class="row gap2" style="font-size:11.5px"><span class="dot ok"></span><span class="mono flex1">worker-07</span><span class="muted">ready</span></div>
        <a class="link" href="#" style="margin-top:4px;font-size:11.5px">3 more ready</a>
      </div>
    </div>

    <div class="card">
      <div class="card-head"><h3>Queue pressure</h3></div>
      <div class="card-body col gap3">
        <div>
          <div class="row gap2" style="font-size:11.5px"><span style="font-weight:550">default</span><span class="muted ml-auto tnum">4 deep</span></div>
          <div class="meter run" style="margin-top:5px"><i style="width:30%"></i></div>
        </div>
        <div>
          <div class="row gap2" style="font-size:11.5px"><span style="font-weight:550">bulk</span><span class="muted ml-auto tnum">9 deep</span></div>
          <div class="meter wait" style="margin-top:5px"><i style="width:64%"></i></div>
        </div>
        <div>
          <div class="row gap2" style="font-size:11.5px"><span style="font-weight:550">critical</span><span class="muted ml-auto tnum">1 deep</span></div>
          <div class="meter bad" style="margin-top:5px"><i style="width:8%"></i></div>
        </div>
        <p style="margin:2px 0 0;font-size:11px;color:var(--muted-fg);line-height:1.5">
          Priority is aged in dispatch order, so a low-priority job cannot starve behind a tight loop of critical work.
        </p>
      </div>
    </div>
  </div>
</div>`,
});

// ================================================================ calendar

const calendar = shell({
  active: "/calendar",
  title: "Calendar",
  body: `
<div class="pagehead row gap3" style="align-items:flex-start">
  <div class="flex1">
    <h1>Calendar</h1>
    <p>What ran, what is running, and what is coming — in the timezone each job declares.</p>
  </div>
  <div class="row gap2">
    <div class="row" style="border:1px solid var(--border);border-radius:6px;overflow:hidden">
      <button class="btn" style="border:0;border-radius:0">${icSm(`<path d="M15 18l-6-6 6-6"/>`)}</button>
      <button class="btn" style="border:0;border-left:1px solid var(--border);border-right:1px solid var(--border);border-radius:0">October 2026</button>
      <button class="btn" style="border:0;border-radius:0">${icSm(`<path d="M9 18l6-6-6-6"/>`)}</button>
    </div>
    <div class="row" style="border:1px solid var(--border);border-radius:6px;overflow:hidden">
      <button class="btn sm" style="border:0;border-radius:0">Month</button>
      <button class="btn sm" style="border:0;border-radius:0">Week</button>
      <button class="btn sm" style="border:0;border-radius:0">Day</button>
    </div>
    <button class="btn">Today</button>
  </div>
</div>

<div class="card" style="margin-bottom:16px">
  <div class="filterbar">
    <button class="fchip on">All jobs ${icXs(`<path d="M18 6 6 18M6 6l12 12"/>`)}</button>
    <button class="fchip">Status ${icXs(`<path d="M6 9l6 6 6-6"/>`)}</button>
    <button class="fchip">Timezone ${icXs(`<path d="M6 9l6 6 6-6"/>`)}</button>
    <button class="fchip">Conflicts only</button>
    <span style="flex:1"></span>
    <span class="row gap2" style="font-size:11px;color:var(--muted-fg)">
      <span class="row gap1"><span class="dot ok"></span>succeeded</span>
      <span class="row gap1"><span class="dot bad"></span>failed</span>
      <span class="row gap1"><span class="dot run"></span>running</span>
      <span class="row gap1"><span class="dot wait"></span>upcoming</span>
    </span>
  </div>
</div>

<div class="card">
  <div class="cal">
    <div class="cal-dow">Sun</div><div class="cal-dow">Mon</div><div class="cal-dow">Tue</div><div class="cal-dow">Wed</div><div class="cal-dow">Thu</div><div class="cal-dow">Fri</div><div class="cal-dow">Sat</div>

    <div class="cal-cell out"><div class="cal-d">27</div></div>
    <div class="cal-cell out"><div class="cal-d">28</div></div>
    <div class="cal-cell out"><div class="cal-d">29</div></div>
    <div class="cal-cell out"><div class="cal-d">30</div></div>

    <div class="cal-cell"><div class="cal-d">1</div>
      <div class="cal-e"><span class="bar ok"></span>02:00 settlement</div>
      <div class="cal-e"><span class="bar ok"></span>02:15 reconciliation</div>
      <div class="cal-e"><span class="bar ok"></span>03:00 usage rollup</div>
      <div class="cal-e"><span class="bar ok"></span>06:00 email digest</div>
    </div>
    <div class="cal-cell"><div class="cal-d">2</div>
      <div class="cal-e"><span class="bar wait" style="opacity:0.45"></span>skipped · holiday</div>
    </div>
    <div class="cal-cell"><div class="cal-d">3</div>
      <div class="cal-e"><span class="bar ok"></span>02:00 settlement</div>
      <div class="cal-e"><span class="bar ok"></span>02:15 reconciliation</div>
      <div class="cal-e"><span class="bar ok"></span>03:00 usage rollup</div>
    </div>
    <div class="cal-cell"><div class="cal-d">4</div>
      <div class="cal-e"><span class="bar ok"></span>02:00 settlement</div>
      <div class="cal-e"><span class="bar ok"></span>02:15 reconciliation</div>
    </div>

    <div class="cal-cell"><div class="cal-d">5</div>
      <div class="cal-e"><span class="bar ok"></span>02:00 settlement</div>
      <div class="cal-e"><span class="bar ok"></span>02:15 reconciliation</div>
      <div class="cal-e"><span class="bar bad"></span>02:20 fraud scoring</div>
      <div class="cal-e"><span class="bar ok"></span>03:00 usage rollup</div>
    </div>
    <div class="cal-cell today"><div class="cal-d">6 <span class="cal-today">today</span></div>
      <div class="cal-e"><span class="bar ok"></span>02:00 settlement</div>
      <div class="cal-e"><span class="bar ok"></span>02:15 reconciliation</div>
      <div class="cal-e"><span class="bar ok"></span>03:00 usage rollup</div>
    </div>
    <div class="cal-cell"><div class="cal-d">7</div>
      <div class="cal-e"><span class="bar ok"></span>02:00 settlement</div>
      <div class="cal-e"><span class="bar ok"></span>02:15 reconciliation</div>
      <div class="cal-e"><span class="bar run"></span>02:20 fraud scoring</div>
      <div class="cal-e"><span class="bar wait"></span>03:30 email digest</div>
    </div>
    <div class="cal-cell"><div class="cal-d">8</div>
      <div class="cal-e"><span class="bar wait"></span>02:00 settlement</div>
      <div class="cal-e"><span class="bar wait"></span>02:15 reconciliation</div>
    </div>
    <div class="cal-cell"><div class="cal-d">9</div>
      <div class="cal-e"><span class="bar wait"></span>02:00 settlement</div>
    </div>
    <div class="cal-cell"><div class="cal-d">10</div>
      <div class="cal-e"><span class="bar wait"></span>02:00 settlement</div>
      <div class="cal-e"><span class="bar wait"></span>03:00 usage rollup</div>
    </div>
    <div class="cal-cell"><div class="cal-d">11</div>
      <div class="cal-e"><span class="bar wait"></span>02:00 settlement</div>
    </div>

    <div class="cal-cell"><div class="cal-d">12</div>
      <div class="cal-e"><span class="bar wait"></span>02:00 settlement</div>
      <div class="cal-e"><span class="bar wait"></span>03:00 usage rollup</div>
    </div>
    <div class="cal-cell"><div class="cal-d">13</div>
      <div class="cal-e"><span class="bar wait"></span>02:00 settlement</div>
    </div>
    <div class="cal-cell"><div class="cal-d">14</div>
      <div class="cal-e"><span class="bar wait"></span>02:00 settlement</div>
      <div class="cal-e"><span class="bar wait"></span>02:15 reconciliation</div>
      <div class="cal-e"><span class="bar wait"></span>03:00 usage rollup</div>
    </div>
    <div class="cal-cell"><div class="cal-d">15</div>
      <div class="cal-e"><span class="bar wait"></span>02:00 settlement</div>
    </div>
    <div class="cal-cell"><div class="cal-d">16</div>
      <div class="cal-e"><span class="bar wait"></span>02:00 settlement</div>
      <div class="cal-e"><span class="bar wait"></span>03:00 usage rollup</div>
    </div>
    <div class="cal-cell"><div class="cal-d">17</div></div>
    <div class="cal-cell"><div class="cal-d">18</div></div>
  </div>
</div>`,
});

// ================================================================ workflow designer

const workflow = shell({
  active: "/workflows",
  title: "Workflows",
  body: `
<div class="pagehead row gap3" style="align-items:flex-start">
  <div class="flex1">
    <h1>Workflows</h1>
    <p>Multi-step work with dependencies, approvals, and conditional edges.</p>
  </div>
  <div class="row gap2">
    <button class="btn">Validate</button>
    <button class="btn">Publish version</button>
    <button class="btn primary">Trigger run</button>
  </div>
</div>

<div class="split-wide">
  <div class="card">
    <div class="card-head">
      <h2>Settlement pipeline</h2>
      <span class="pill ok">Published v3</span>
      <div class="ml-auto row gap2">
        <button class="btn sm">${icSm(`<path d="M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7"/><path d="M18.5 2.5a2.12 2.12 0 0 1 3 3L12 15l-4 1 1-4 9.5-9.5z"/>`)} Edit</button>
      </div>
    </div>
    <div class="card-body tight dag-grid" style="padding:0;overflow:hidden">
      <svg viewBox="0 0 860 380" style="width:100%;height:auto;display:block" role="img" aria-label="Workflow graph with six nodes">
        <defs>
          <marker id="ar" markerWidth="7" markerHeight="7" refX="6" refY="3.5" orient="auto">
            <path d="M0 0 L7 3.5 L0 7 z" fill="oklch(0.78 0 0)"/>
          </marker>
        </defs>

        <!-- edges -->
        <path class="dagedge hl" d="M180 90 L262 90" marker-end="url(#ar)"/>
        <path class="dagedge hl" d="M180 90 C 220 90, 220 200, 262 200" marker-end="url(#ar)"/>
        <path class="dagedge" d="M402 200 L482 200" marker-end="url(#ar)"/>
        <path class="dagedge" d="M402 200 C 440 200, 440 90, 482 90" marker-end="url(#ar)"/>
        <path class="dagedge" d="M402 200 C 440 200, 440 310, 482 310" marker-end="url(#ar)"/>
        <path class="dagedge" d="M622 90 L700 90" marker-end="url(#ar)"/>
        <path class="dagedge" d="M622 90 C 660 90, 660 200, 700 200" marker-end="url(#ar)"/>

        <!-- start -->
        <g>
          <rect x="40" y="66" width="34" height="34" rx="17" fill="var(--bg)" stroke="var(--fg)" stroke-width="1.5"/>
          <circle cx="57" cy="83" r="6" fill="none" stroke="var(--fg)" stroke-width="1.6"/>
        </g>
        <text x="40" y="118" font-size="9" fill="oklch(0.6 0 0)" font-family="Inter">Trigger</text>

        <!-- node 1 -->
        <g class="dagnode sel">
          <rect x="74" y="60" width="106" height="46" rx="7"/>
          <circle cx="90" cy="77" r="5" fill="var(--ok)"/>
          <text x="102" y="81">extract</text>
          <text x="90" y="95" class="sub">Job · succeeded</text>
        </g>

        <!-- node 2 -->
        <g class="dagnode">
          <rect x="262" y="60" width="140" height="46" rx="7"/>
          <circle cx="278" cy="77" r="5" fill="var(--ok)"/>
          <text x="290" y="81">fraud-check</text>
          <text x="278" y="95" class="sub">Job · succeeded</text>
        </g>

        <!-- node 3 -->
        <g class="dagnode">
          <rect x="262" y="174" width="140" height="46" rx="7"/>
          <circle cx="278" cy="191" r="5" fill="var(--run)"/>
          <text x="290" y="195">manual approval</text>
          <text x="278" y="209" class="sub">Awaiting · role Operator</text>
        </g>

        <!-- node 4 (condition) -->
        <g class="dagnode">
          <rect x="482" y="64" width="140" height="46" rx="7"/>
          <rect x="494" y="76" width="12" height="12" rx="3" fill="none" stroke="var(--muted-fg)" stroke-width="1.4"/>
          <text x="514" y="81">amount &gt; 1M?</text>
          <text x="494" y="100" class="sub">Condition</text>
        </g>

        <!-- node 5 (delay) -->
        <g class="dagnode">
          <rect x="482" y="284" width="140" height="46" rx="7"/>
          <circle cx="498" cy="301" r="6" fill="none" stroke="var(--muted-fg)" stroke-width="1.5"/>
          <path d="M498 297 v4 l2.5 2" stroke="var(--muted-fg)" stroke-width="1.3" fill="none" stroke-linecap="round"/>
          <text x="512" y="305">hold 2 hours</text>
          <text x="498" y="319" class="sub">Delay · resumes 04:20</text>
        </g>

        <!-- node 6 -->
        <g class="dagnode">
          <rect x="482" y="174" width="140" height="46" rx="7"/>
          <circle cx="498" cy="191" r="5" fill="var(--wait)"/>
          <text x="510" y="195">settle-payment</text>
          <text x="498" y="209" class="sub">Pending</text>
        </g>

        <!-- node 7 final -->
        <g class="dagnode">
          <rect x="700" y="174" width="120" height="46" rx="7"/>
          <circle cx="716" cy="191" r="5" fill="oklch(0.8 0 0)"/>
          <text x="728" y="195">notify-risk</text>
          <text x="716" y="209" class="sub">Pending</text>
        </g>

        <g>
          <rect x="700" y="64" width="120" height="46" rx="7" fill="oklch(0.985 0 0)" stroke="var(--border)" stroke-dasharray="3 3"/>
          <text x="714" y="85">manual review</text>
          <text x="714" y="100" font-size="9.5" fill="oklch(0.6 0 0)" font-family="Inter">skipped · condition false</text>
        </g>

        <!-- edge labels -->
        <text x="410" y="194" class="dagelabel" text-anchor="middle">all succeeded</text>
        <text x="440" y="150" class="dagelabel" text-anchor="middle">any succeeded</text>
        <text x="440" y="262" class="dagelabel" text-anchor="middle">timed out</text>
        <text x="658" y="144" class="dagelabel" text-anchor="middle">yes</text>
        <text x="658" y="248" class="dagelabel" text-anchor="middle">no</text>
      </svg>
    </div>
    <div class="pager">
      <span>6 nodes · 7 edges · acyclic</span>
      <div class="ml-auto row gap2">
        <span class="row gap1" style="font-size:11px;color:var(--muted-fg)">${icSm(`<path d="M12 2v6M15 5l-3 3-3-3"/>`)} Fit</span>
        <button class="btn sm">Validate</button>
      </div>
    </div>
  </div>

  <div class="col gap4">
    <div class="card">
      <div class="card-head"><h3>Run wf_3c8a91</h3>${statusPill("RUNNING", "Running")}</div>
      <div class="card-body">
        <div class="tl">
          <div class="tl-item"><span class="tl-dot ok"></span><div class="tl-label">extract</div><div class="tl-meta">succeeded · 41s</div></div>
          <div class="tl-item"><span class="tl-dot ok"></span><div class="tl-label">fraud-check</div><div class="tl-meta">succeeded · 12s</div></div>
          <div class="tl-item"><span class="tl-dot wait"></span><div class="tl-label">manual approval</div><div class="tl-meta">awaiting Operator · 18m elapsed</div></div>
          <div class="tl-item"><span class="tl-dot"></span><div class="tl-label">settle-payment</div><div class="tl-meta">blocked · waiting on approval</div></div>
        </div>
        <div class="banner warn" style="margin-top:14px;font-size:11.5px">
          ${icSm(`<rect x="7" y="4" width="3.5" height="16" rx="1"/><rect x="13.5" y="4" width="3.5" height="16" rx="1"/>`)}
          <span>A run has been waiting on approval for 18 minutes.</span>
        </div>
        <div class="btnrow" style="margin-top:12px">
          <button class="btn sm primary" style="flex:1">Approve</button>
          <button class="btn sm danger" style="flex:1">Reject</button>
        </div>
      </div>
    </div>

    <div class="card">
      <div class="card-head"><h3>Graph</h3></div>
      <div class="card-body col gap2" style="font-size:11.5px">
        <div class="row gap2"><span class="dot ok"></span><span class="flex1">Job nodes</span><span class="muted">3</span></div>
        <div class="row gap2"><span class="dot wait"></span><span class="flex1">Approval nodes</span><span class="muted">1</span></div>
        <div class="row gap2"><span class="dot off"></span><span class="flex1">Condition nodes</span><span class="muted">1</span></div>
        <div class="row gap2"><span class="dot off"></span><span class="flex1">Delay nodes</span><span class="muted">1</span></div>
        <div class="divider" style="margin:6px 0"></div>
        <div class="row gap2"><span style="flex1">Edge condition</span><span class="muted">all succeeded</span></div>
      </div>
    </div>
  </div>
</div>`,
});

// ================================================================ emergency

const emergency = shell({
  active: "/",
  title: "Emergency",
  health: "degraded",
  body: `
<div class="pagehead">
  <h1>Emergency controls</h1>
  <p>Hold scheduling, drain capacity, or stop everything in flight. Every action here is audited and requires a reason.</p>
</div>

<div class="banner warn" style="margin-bottom:16px">
  ${icSm(`<circle cx="12" cy="12" r="9"/><path d="M12 16v-4M12 8h.01"/>`)}
  <div class="flex1">
    <b>Nothing here is reversible automatically.</b>
    Cancelling running executions is cooperative — workers are asked to stop, and a worker that
    ignores the request keeps its lease until it expires.
  </div>
</div>

<div class="split">
  <div class="col gap4">
    <div class="card">
      <div class="card-head"><h2>Current state</h2><span class="pill run ml-auto">${icXs(`<circle cx="12" cy="12" r="5"/>`)} live</span></div>
      <div class="card-body">
        <div class="grid g4" style="gap:0;text-align:center">
          <div style="padding:0 8px;border-right:1px solid var(--line)">
            <div style="font-size:10px;text-transform:uppercase;letter-spacing:0.06em;color:var(--muted-fg);font-weight:600">Running</div>
            <div class="tnum" style="margin-top:6px;font-size:22px;font-weight:600">3</div>
          </div>
          <div style="padding:0 8px;border-right:1px solid var(--line)">
            <div style="font-size:10px;text-transform:uppercase;letter-spacing:0.06em;color:var(--muted-fg);font-weight:600">Queued</div>
            <div class="tnum" style="margin-top:6px;font-size:22px;font-weight:600">14</div>
          </div>
          <div style="padding:0 8px;border-right:1px solid var(--line)">
            <div style="font-size:10px;text-transform:uppercase;letter-spacing:0.06em;color:var(--muted-fg);font-weight:600">Paused queues</div>
            <div class="tnum" style="margin-top:6px;font-size:22px;font-weight:600">1</div>
          </div>
          <div style="padding:0 8px">
            <div style="font-size:10px;text-transform:uppercase;letter-spacing:0.06em;color:var(--muted-fg);font-weight:600">Paused schedules</div>
            <div class="tnum" style="margin-top:6px;font-size:22px;font-weight:600">2</div>
          </div>
        </div>
        <div class="divider"></div>
        <div class="col gap2">
          <div class="row gap2" style="font-size:12px"><span class="dot wait"></span><span style="font-weight:550">Maintenance window</span><span class="ml-auto muted" style="font-size:11px">active until 06:00 UTC · settlement data migration</span></div>
          <div class="row gap2" style="font-size:12px"><span class="dot wait"></span><span style="font-weight:550">analytics queue</span><span class="ml-auto muted" style="font-size:11px">paused 2h ago by n.kapoor</span></div>
          <div class="row gap2" style="font-size:12px"><span class="dot wait"></span><span style="font-weight:550">Fraud scoring schedule</span><span class="ml-auto muted" style="font-size:11px">paused 40m ago by n.kapoor</span></div>
        </div>
      </div>
    </div>

    <div class="card">
      <div class="card-head"><h2>Stop everything</h2></div>
      <div class="card-body">
        <div class="banner bad" style="margin-bottom:14px">
          ${icSm(`<path d="M12 9v4M12 17h.01"/><path d="M10.3 3.9 1.8 18a2 2 0 0 0 1.7 3h17a2 2 0 0 0 1.7-3L13.7 3.9a2 2 0 0 0-3.4 0z"/>`)}
          <span>Cancels all 3 running executions and records <span class="mono" style="font-size:11.5px">CANCEL_REQUESTED</span>. Workers observe it cooperatively.</span>
        </div>
        <div class="field" style="margin-bottom:14px">
          <label class="label" for="r">Reason (required, written to the audit log)</label>
          <input class="input" id="r" value="Partner outage — halting settlement until their status page clears" />
        </div>
        <div class="btnrow">
          <button class="btn danger">Cancel all running executions</button>
          <button class="btn">Pause every schedule</button>
        </div>
      </div>
    </div>
  </div>

  <div class="col gap4">
    <div class="card">
      <div class="card-head"><h3>Recover</h3></div>
      <div class="card-body col gap3">
        <button class="btn" style="justify-content:flex-start">Lift maintenance window</button>
        <button class="btn" style="justify-content:flex-start">Resume analytics queue</button>
        <button class="btn" style="justify-content:flex-start">Resume Fraud scoring schedule</button>
        <div class="divider" style="margin:2px 0"></div>
        <a class="link" href="#" style="font-size:11.5px">View all 14 audit entries from today</a>
      </div>
    </div>

    <div class="card">
      <div class="card-head"><h3>What happens after a stop</h3></div>
      <div class="card-body col gap2" style="font-size:11.5px;line-height:1.55;color:var(--muted-fg)">
        <div class="row gap2" style="align-items:flex-start">${icXs(`<path d="M20 6 9 17l-5-5"/>`)}<span>Queued executions stay queued. They are not dropped.</span></div>
        <div class="row gap2" style="align-items:flex-start">${icXs(`<path d="M20 6 9 17l-5-5"/>`)}<span>Each cancelled execution keeps its attempt history and logs.</span></div>
        <div class="row gap2" style="align-items:flex-start">${icXs(`<path d="M20 6 9 17l-5-5"/>`)}<span>Retries already scheduled are cancelled with it.</span></div>
        <div class="row gap2" style="align-items:flex-start">${icXs(`<path d="M18 6 6 18M6 6l12 12"/>`)}<span>A running worker that ignores the request loses its lease in 20s and is reaped.</span></div>
      </div>
    </div>
  </div>
</div>`,
});

const out = path.join(__dirname, "out");
fs.mkdirSync(out, { recursive: true });
fs.writeFileSync(path.join(out, "08-executions.html"), executions);
fs.writeFileSync(path.join(out, "09-execution-detail.html"), executionDetail);
fs.writeFileSync(path.join(out, "10-running-now.html"), running);
fs.writeFileSync(path.join(out, "11-calendar.html"), calendar);
fs.writeFileSync(path.join(out, "12-workflows.html"), workflow);
fs.writeFileSync(path.join(out, "13-emergency.html"), emergency);
console.log("wrote executions, execution-detail, running-now, calendar, workflows, emergency");