// Core console screens: dashboard, jobs table, job builder wizard,
// schedule builder, job detail, and the "why didn't this run" diagnosis.

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

// ================================================================ dashboard

const dashboard = shell({
  active: "/",
  title: "Dashboard",
  body: `
<div class="pagehead row gap3" style="align-items:flex-start">
  <div class="flex1">
    <h1>Dashboard</h1>
    <p>Is the scheduler healthy, what needs attention, what is happening now.</p>
  </div>
  <div class="row gap2">
    <button class="btn">${icSm(`<path d="M21 12a9 9 0 1 1-3-6.7"/><path d="M21 3v6h-6"/>`)} Refresh</button>
    <button class="btn">${icSm(`<path d="M3 3v18h18"/><path d="m19 9-5 5-4-4-3 3"/>`)} Widgets</button>
    <button class="btn primary">${icSm(`<path d="M12 5v14M5 12h14"/>`)} New job</button>
  </div>
</div>

<!-- Maintenance is impossible to miss: nothing is scheduling while it is set. -->
<div class="banner warn" style="margin-bottom:16px">
  ${icSm(`<rect x="7" y="4" width="3.5" height="16" rx="1"/><rect x="13.5" y="4" width="3.5" height="16" rx="1"/>`)}
  <div class="flex1">
    <b>Maintenance window active</b> until 06:00 UTC — settlement data migration. Scheduling is held and
    dispatch is paused for the <span class="mono">critical</span> queue.
  </div>
  <button class="btn sm">Manage</button>
</div>

<!-- Every number is a link into the filtered list behind it. -->
<section class="grid g4" style="margin-bottom:16px">
  <a class="metric" href="#">
    <div class="k">${icSm(`<circle cx="12" cy="12" r="9"/><path d="M12 7v5l3 2"/>`)} Queued</div>
    <div class="v">14</div>
    <div class="d">oldest 4m 12s</div>
  </a>
  <a class="metric" href="#">
    <div class="k">${icSm(`<path d="M22 12h-4l-3 9L9 3l-3 9H2"/>`)} Running</div>
    <div class="v">3</div>
    <div class="d">of 9 workers busy</div>
  </a>
  <a class="metric" href="#">
    <div class="k">${icSm(`<path d="M20 6 9 17l-5-5"/>`)} Succeeded</div>
    <div class="v ok">1,284</div>
    <div class="d">last 24 hours</div>
  </a>
  <a class="metric" href="#">
    <div class="k">${icSm(`<path d="M12 9v4M12 17h.01"/>`)} Failed</div>
    <div class="v bad">9</div>
    <div class="d">3 dead lettered</div>
  </a>
</section>

<div class="split" style="margin-bottom:16px">
  <!-- Needs attention leads: it is the reason an operator opened this page. -->
  <div class="card">
    <div class="card-head">
      <h2>Needs attention</h2>
      <span class="pill bad">9 failing</span>
      <a class="link ml-auto" href="#" style="font-size:11.5px">Open incident</a>
    </div>
    <div class="card-body tight">
      <table>
        <thead>
          <tr><th>Execution</th><th>Job</th><th>Error</th><th>Attempt</th><th class="num">Age</th></tr>
        </thead>
        <tbody>
          <tr class="clickable">
            <td class="shrink">${statusPill("FAILED", "Failed")}</td>
            <td><div class="celltitle">settlement-webhook-relay</div><div class="cellsub mono">ex_8f2a91c4</div></td>
            <td style="max-width:210px"><div class="truncate" style="color:oklch(0.505 0.2 27)">502 Bad Gateway from partner endpoint</div><div class="cellsub">DEPENDENCY_UNAVAILABLE</div></td>
            <td class="tnum">3 of 3</td>
            <td class="num tnum muted">6m</td>
          </tr>
          <tr class="clickable">
            <td class="shrink">${statusPill("DEAD_LETTERED", "Dead lettered")}</td>
            <td><div class="celltitle">fraud-scoring</div><div class="cellsub mono">ex_3b7e04d1</div></td>
            <td style="max-width:210px"><div class="truncate" style="color:oklch(0.505 0.2 27)">attempt budget exhausted after 3 tries</div><div class="cellsub">TIMEOUT</div></td>
            <td class="tnum">3 of 3</td>
            <td class="num tnum muted">22m</td>
          </tr>
          <tr class="clickable">
            <td class="shrink">${statusPill("TIMED_OUT", "Timed out")}</td>
            <td><div class="celltitle">nightly-reconciliation</div><div class="cellsub mono">ex_9c1d77aa</div></td>
            <td style="max-width:210px"><div class="truncate" style="color:oklch(0.505 0.2 27)">exceeded 3600s timeout</div><div class="cellsub">TIMEOUT</div></td>
            <td class="tnum">1 of 2</td>
            <td class="num tnum muted">1h</td>
          </tr>
          <tr class="clickable">
            <td class="shrink">${statusPill("ABANDONED", "Abandoned")}</td>
            <td><div class="celltitle">invoice-render</div><div class="cellsub mono">ex_44ae7712</div></td>
            <td style="max-width:210px"><div class="truncate" style="color:oklch(0.505 0.2 27)">worker lease expired after 20s</div><div class="cellsub">TRANSIENT · reaped</div></td>
            <td class="tnum">2 of 5</td>
            <td class="num tnum muted">2h</td>
          </tr>
        </tbody>
      </table>
    </div>
    <div class="pager">
      <span>4 of 9 need attention</span>
      <a class="link ml-auto" href="#">See all failures</a>
    </div>
  </div>

  <div class="col gap4">
    <div class="card">
      <div class="card-head"><h3>Component health</h3><span class="sub ml-auto">checked 6s ago</span></div>
      <div class="card-body col gap2">
        <div class="statline"><span class="lbl">Scheduler</span><span class="dot ok"></span><span class="flex1" style="font-size:11.5px;color:var(--muted-fg)">tick 4s ago</span></div>
        <div class="statline"><span class="lbl">Database</span><span class="dot ok"></span><span class="flex1" style="font-size:11.5px;color:var(--muted-fg)">12ms p99</span></div>
        <div class="statline"><span class="lbl">Workers</span><span class="dot ok"></span><span class="flex1" style="font-size:11.5px;color:var(--muted-fg)">9 ready · 3 busy</span></div>
        <div class="statline"><span class="lbl">Queues</span><span class="dot wait"></span><span class="flex1" style="font-size:11.5px;color:var(--muted-fg)">1 paused</span></div>
        <div class="statline"><span class="lbl">Notifications</span><span class="dot off"></span><span class="flex1" style="font-size:11.5px;color:var(--muted-fg)">unknown</span></div>
      </div>
    </div>

    <div class="card">
      <div class="card-head"><h3>Upcoming</h3><a class="link ml-auto" href="#" style="font-size:11.5px">Calendar</a></div>
      <div class="card-body col gap2">
        <div class="row gap2" style="font-size:12px"><span class="dot wait"></span><span class="tnum">02:00</span><span class="flex1 truncate">nightly-settlement</span><span class="muted tnum" style="font-size:11px">in 18m</span></div>
        <div class="row gap2" style="font-size:12px"><span class="dot wait"></span><span class="tnum">02:15</span><span class="flex1 truncate">reconciliation</span><span class="muted tnum" style="font-size:11px">in 33m</span></div>
        <div class="row gap2" style="font-size:12px"><span class="dot run"></span><span class="tnum">02:20</span><span class="flex1 truncate">fraud-scoring</span><span class="muted tnum" style="font-size:11px">running</span></div>
        <div class="row gap2" style="font-size:12px"><span class="dot wait"></span><span class="tnum">03:00</span><span class="flex1 truncate">usage-rollup</span><span class="muted tnum" style="font-size:11px">in 1h 18m</span></div>
        <div class="row gap2" style="font-size:12px"><span class="dot wait"></span><span class="tnum">03:30</span><span class="flex1 truncate">email-digest</span><span class="muted tnum" style="font-size:11px">in 1h 48m</span></div>
      </div>
    </div>
  </div>
</div>

<div class="grid g2">
  <div class="card">
    <div class="card-head"><h3>Queues</h3><a class="link ml-auto" href="#" style="font-size:11.5px">Manage</a></div>
    <div class="card-body col gap3">
      <div>
        <div class="row gap2" style="font-size:11.5px"><span style="font-weight:550">default</span><span class="muted ml-auto tnum">4 deep</span></div>
        <div class="meter run" style="margin-top:5px"><i style="width:32%"></i></div>
      </div>
      <div>
        <div class="row gap2" style="font-size:11.5px"><span style="font-weight:550">bulk</span><span class="muted ml-auto tnum">9 deep</span></div>
        <div class="meter wait" style="margin-top:5px"><i style="width:58%"></i></div>
      </div>
      <div>
        <div class="row gap2" style="font-size:11.5px"><span style="font-weight:550">critical</span><span class="muted ml-auto tnum">1 deep</span></div>
        <div class="meter bad" style="margin-top:5px"><i style="width:12%"></i></div>
      </div>
      <div>
        <div class="row gap2" style="font-size:11.5px"><span style="font-weight:550">analytics</span><span class="pill warn" style="height:17px;font-size:10px">paused</span><span class="muted ml-auto tnum">0 deep</span></div>
        <div class="meter warn" style="margin-top:5px"><i style="width:0%"></i></div>
      </div>
    </div>
  </div>

  <div class="card">
    <div class="card-head">
      <h3>Executions</h3>
      <span class="sub ml-auto">last 24 hours · 1,310 total</span>
    </div>
    <div class="card-body col gap3">
      <div class="statline"><span class="lbl">Succeeded</span><span class="meter ok" style="flex:1"><i style="width:96%"></i></span><span class="val">1,284</span></div>
      <div class="statline"><span class="lbl">Failed</span><span class="meter bad" style="flex:1"><i style="width:5%"></i></span><span class="val">9</span></div>
      <div class="statline"><span class="lbl">Dead lettered</span><span class="meter bad" style="flex:1"><i style="width:2%"></i></span><span class="val">3</span></div>
      <div class="statline"><span class="lbl">Cancelled</span><span class="meter" style="flex:1"><i style="width:1%;background:var(--muted-fg)"></i></span><span class="val">5</span></div>
      <div class="statline"><span class="lbl">Running</span><span class="meter run" style="flex:1"><i style="width:1%"></i></span><span class="val">3</span></div>
      <div class="divider" style="margin:4px 0"></div>
      <div class="row gap3" style="font-size:11.5px;color:var(--muted-fg)">
        <span>Success rate <b style="color:var(--fg);font-weight:600">98.0%</b></span>
        <span>Mean latency <b style="color:var(--fg);font-weight:600">1m 12s</b></span>
        <span>p95 <b style="color:var(--fg);font-weight:600">4m 03s</b></span>
      </div>
    </div>
  </div>
</div>`,
});

// ================================================================ jobs list

const jobs = shell({
  active: "/jobs",
  title: "Jobs",
  body: `
<div class="pagehead row gap3" style="align-items:flex-start">
  <div class="flex1">
    <h1>Jobs</h1>
    <p>Reusable definitions of work. A job only runs once a version is published.</p>
  </div>
  <div class="row gap2">
    <button class="btn">${icSm(`<path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><path d="M7 10l5 5 5-5M12 15V3"/>`)} Import</button>
    <button class="btn">${icSm(`<path d="M12 5v14M5 12h14"/>`)} Save view</button>
    <button class="btn primary">${icSm(`<path d="M12 5v14M5 12h14"/>`)} New job</button>
  </div>
</div>

<div class="card">
  <div class="filterbar">
    <div class="finput flex1" style="max-width:260px">${I.search}<span class="ph">Search by name or key…</span></div>
    <button class="fchip on">Status: Active ${icXs(`<path d="M18 6 6 18M6 6l12 12"/>`)}</button>
    <button class="fchip">Schedule ${icXs(`<path d="M6 9l6 6 6-6"/>`)}</button>
    <button class="fchip">Queue ${icXs(`<path d="M6 9l6 6 6-6"/>`)}</button>
    <button class="fchip">Owner ${icXs(`<path d="M6 9l6 6 6-6"/>`)}</button>
    <button class="fchip">Health ${icXs(`<path d="M6 9l6 6 6-6"/>`)}</button>
    <span style="flex:1"></span>
    <span style="font-size:11.5px;color:var(--muted-fg)">24 of 24 jobs</span>
    <button class="fchip" aria-label="Columns">${icSm(`<path d="M3 3v18h18"/><path d="M9 3v18M15 3v18"/>`)}</button>
  </div>

  <table>
    <thead>
      <tr>
        <th style="width:34px"><span class="checkbox"></span></th>
        <th class="sortable">Job <span class="arrow">↑</span></th>
        <th>Status</th>
        <th>Schedule</th>
        <th>Next run</th>
        <th>Last run</th>
        <th>Success</th>
        <th>p95</th>
        <th class="sortable">Priority <span class="arrow">↓</span></th>
        <th style="width:34px"></th>
      </tr>
    </thead>
    <tbody>
      <tr class="clickable">
        <td><span class="checkbox on">${icXs(`<path d="M20 6 9 17l-5-5"/>`)}</span></td>
        <td><div class="celltitle">Nightly settlement</div><div class="cellsub mono">nightly-settlement · v4</div></td>
        <td>${statusPill("ACTIVE", "Active")}</td>
        <td><span class="mono">0 2 * * *</span><div class="cellsub">Asia/Kolkata</div></td>
        <td class="tnum">in 18m<div class="cellsub">02:00 IST</div></td>
        <td>${statusPill("SUCCEEDED", "Succeeded")}<div class="cellsub">8m 38s</div></td>
        <td><div class="row gap2"><span class="meter ok" style="width:44px"><i style="width:99%"></i></span><span class="tnum">99.4%</span></div></td>
        <td class="tnum">9m 02s</td>
        <td>${prioPill("CRITICAL")}</td>
        <td><button class="hbtn" aria-label="Row actions" style="width:24px;height:24px;padding:0;justify-content:center">${icSm(`<circle cx="5" cy="12" r="1.4"/><circle cx="12" cy="12" r="1.4"/><circle cx="19" cy="12" r="1.4"/>`)}</button></td>
      </tr>
      <tr class="clickable selected">
        <td><span class="checkbox on">${icXs(`<path d="M20 6 9 17l-5-5"/>`)}</span></td>
        <td><div class="celltitle">Settlement webhook relay</div><div class="cellsub mono">settlement-webhook-relay · v2</div></td>
        <td>${statusPill("ACTIVE", "Active")}</td>
        <td><span class="mono">*/5 * * * *</span><div class="cellsub">UTC</div></td>
        <td class="tnum">in 48s<div class="cellsub">retrying</div></td>
        <td>${statusPill("FAILED", "Failed")}<div class="cellsub">502 · 6m ago</div></td>
        <td><div class="row gap2"><span class="meter bad" style="width:44px"><i style="width:61%"></i></span><span class="tnum">61.2%</span></div></td>
        <td class="tnum">14s</td>
        <td>${prioPill("HIGH")}</td>
        <td><button class="hbtn" aria-label="Row actions" style="width:24px;height:24px;padding:0;justify-content:center">${icSm(`<circle cx="5" cy="12" r="1.4"/><circle cx="12" cy="12" r="1.4"/><circle cx="19" cy="12" r="1.4"/>`)}</button></td>
      </tr>
      <tr class="clickable">
        <td><span class="checkbox"></span></td>
        <td><div class="celltitle">Fraud scoring</div><div class="cellsub mono">fraud-scoring · v1</div></td>
        <td>${statusPill("ACTIVE", "Active")}</td>
        <td><span class="mono">15 * * * *</span><div class="cellsub">UTC</div></td>
        <td class="tnum">in 11m<div class="cellsub">paused by window</div></td>
        <td>${statusPill("DEAD_LETTERED", "Dead lettered")}<div class="cellsub">3 attempts · 22m</div></td>
        <td><div class="row gap2"><span class="meter bad" style="width:44px"><i style="width:88%"></i></span><span class="tnum">88.1%</span></div></td>
        <td class="tnum">2m 41s</td>
        <td>${prioPill("HIGH")}</td>
        <td><button class="hbtn" aria-label="Row actions" style="width:24px;height:24px;padding:0;justify-content:center">${icSm(`<circle cx="5" cy="12" r="1.4"/><circle cx="12" cy="12" r="1.4"/><circle cx="19" cy="12" r="1.4"/>`)}</button></td>
      </tr>
      <tr class="clickable">
        <td><span class="checkbox"></span></td>
        <td><div class="celltitle">Nightly reconciliation</div><div class="cellsub mono">nightly-reconciliation · v3</div></td>
        <td>${statusPill("ACTIVE", "Active")}</td>
        <td><span class="mono">15 2 * * *</span><div class="cellsub">Asia/Kolkata</div></td>
        <td class="tnum">in 33m<div class="cellsub">02:15 IST</div></td>
        <td>${statusPill("TIMED_OUT", "Timed out")}<div class="cellsub">3600s · 1h</div></td>
        <td><div class="row gap2"><span class="meter warn" style="width:44px"><i style="width:96%"></i></span><span class="tnum">96.2%</span></div></td>
        <td class="tnum">58m 12s</td>
        <td>${prioPill("NORMAL")}</td>
        <td><button class="hbtn" aria-label="Row actions" style="width:24px;height:24px;padding:0;justify-content:center">${icSm(`<circle cx="5" cy="12" r="1.4"/><circle cx="12" cy="12" r="1.4"/><circle cx="19" cy="12" r="1.4"/>`)}</button></td>
      </tr>
      <tr class="clickable">
        <td><span class="checkbox"></span></td>
        <td><div class="celltitle">Usage rollup</div><div class="cellsub mono">usage-rollup · v1</div></td>
        <td>${statusPill("ACTIVE", "Active")}</td>
        <td><span class="mono">0 3 * * *</span><div class="cellsub">UTC</div></td>
        <td class="tnum">in 1h 18m<div class="cellsub">03:00 UTC</div></td>
        <td>${statusPill("SUCCEEDED", "Succeeded")}<div class="cellsub">41s</div></td>
        <td><div class="row gap2"><span class="meter ok" style="width:44px"><i style="width:100%"></i></span><span class="tnum">100%</span></div></td>
        <td class="tnum">52s</td>
        <td>${prioPill("BACKGROUND")}</td>
        <td><button class="hbtn" aria-label="Row actions" style="width:24px;height:24px;padding:0;justify-content:center">${icSm(`<circle cx="5" cy="12" r="1.4"/><circle cx="12" cy="12" r="1.4"/><circle cx="19" cy="12" r="1.4"/>`)}</button></td>
      </tr>
      <tr class="clickable">
        <td><span class="checkbox"></span></td>
        <td><div class="celltitle">Invoice render</div><div class="cellsub mono">invoice-render · v2</div></td>
        <td>${statusPill("DRAFT", "Draft")}</td>
        <td><span class="muted mono">no schedule</span></td>
        <td class="muted">—</td>
        <td>${statusPill("ABANDONED", "Abandoned")}<div class="cellsub">lease expired · 2h</div></td>
        <td><div class="row gap2"><span class="meter warn" style="width:44px"><i style="width:91%"></i></span><span class="tnum">91.0%</span></div></td>
        <td class="tnum">1m 04s</td>
        <td>${prioPill("NORMAL")}</td>
        <td><button class="hbtn" aria-label="Row actions" style="width:24px;height:24px;padding:0;justify-content:center">${icSm(`<circle cx="5" cy="12" r="1.4"/><circle cx="12" cy="12" r="1.4"/><circle cx="19" cy="12" r="1.4"/>`)}</button></td>
      </tr>
      <tr class="clickable">
        <td><span class="checkbox"></span></td>
        <td><div class="celltitle">Email digest</div><div class="cellsub mono">email-digest · v1</div></td>
        <td>${statusPill("ARCHIVED", "Archived")}</td>
        <td><span class="mono muted">0 6 * * 1-5</span><div class="cellsub muted">UTC</div></td>
        <td class="muted">—</td>
        <td>${statusPill("CANCELLED", "Cancelled")}<div class="cellsub">9d ago</div></td>
        <td><div class="row gap2"><span class="meter" style="width:44px"><i style="width:99%;background:var(--muted-fg)"></i></span><span class="tnum t muted">99.2%</span></div></td>
        <td class="tnum muted">12s</td>
        <td>${prioPill("LOW")}</td>
        <td><button class="hbtn" aria-label="Row actions" style="width:24px;height:24px;padding:0;justify-content:center">${icSm(`<circle cx="5" cy="12" r="1.4"/><circle cx="12" cy="12" r="1.4"/><circle cx="19" cy="12" r="1.4"/>`)}</button></td>
      </tr>
    </tbody>
  </table>

  <div class="pager">
    <span>Showing 1–7 of 24 jobs</span>
    <div class="ml-auto row gap2">
      <button class="btn sm" disabled>Previous</button>
      <button class="btn sm">Next</button>
    </div>
  </div>
</div>`,
});

// ================================================================ builder

const builder = shell({
  active: "/jobs",
  title: "New job",
  flush: true,
  body: `
<div class="row" style="align-items:stretch;min-height:calc(100vh - 48px)">
  <aside class="wizrail">
    <div class="row gap2" style="margin-bottom:16px">
      <button class="hbtn" aria-label="Back to jobs" style="width:26px;height:26px;padding:0;justify-content:center">${icSm(`<path d="M15 18l-6-6 6-6"/>`)}</button>
      <div>
        <div style="font-size:12.5px;font-weight:600">New job</div>
        <div style="font-size:10.5px;color:var(--muted-fg)">Saved as a draft</div>
      </div>
    </div>

    <div class="col" style="gap:2px">
      <div class="wizstep done"><span class="wizn">${icXs(`<path d="M20 6 9 17l-5-5"/>`)}</span><span>Basics</span></div>
      <div class="wizstep on"><span class="wizn">2</span><span>Schedule</span></div>
      <div class="wizstep"><span class="wizn">3</span><span>Execution</span></div>
      <div class="wizstep"><span class="wizn">4</span><span>Retry</span></div>
      <div class="wizstep"><span class="wizn">5</span><span>Alerts</span></div>
      <div class="wizstep"><span class="wizn">6</span><span>Review</span></div>
    </div>

    <div class="divider"></div>
    <div style="font-size:11px;color:var(--muted-fg);line-height:1.55;padding:0 8px">
      A draft never runs. Publishing a version is what makes this job eligible for dispatch.
    </div>

    <div class="mt-auto" style="margin-top:auto;padding-top:14px">
      <div class="banner info" style="font-size:11px;padding:8px 10px">
        ${icSm(`<circle cx="12" cy="12" r="9"/><path d="M12 16v-4M12 8h.01"/>`)}
        <span>Draft mirrored to this browser. Server draft <span class="mono">job_7c1e…</span>.</span>
      </div>
    </div>
  </aside>

  <div class="flex1" style="padding:24px 28px 32px;max-width:920px">
    <div class="crumbs" style="margin-bottom:10px"><a href="#">Jobs</a><span style="opacity:0.5">/</span><span style="color:var(--fg);font-weight:500">New job</span></div>
    <h2 style="margin:0 0 4px;font-size:17px;font-weight:600;letter-spacing:-0.015em">When should this run?</h2>
    <p style="margin:0 0 22px;font-size:12.5px;color:var(--muted-fg)">
      Pick a pattern or type an expression. The preview below is computed by the same engine that fires the job.
    </p>

    <!-- Natural language and expression are two doors into one value. -->
    <div class="field" style="margin-bottom:20px">
      <label class="label">Describe the schedule</label>
      <div class="row gap2">
        <input class="input flex1" value="Every weekday at 2 AM" />
        <button class="btn">${icSm(`<path d="m3 21 12-12M14 5l1.5-1.5M17 8l1.5-1.5"/>`)} Interpret</button>
      </div>
      <span class="hint">Reads "every weekday at 2 AM", "hourly on the half hour", "at 14:00 on Mondays". Never applies a change without showing you the expression first.</span>
    </div>

    <div class="grid g2" style="margin-bottom:20px">
      <div class="field">
        <label class="label">Pattern</label>
        <select class="select">
          <option>Every day at a specific time</option>
          <option selected>Every weekday at a specific time</option>
          <option>Every N hours</option>
          <option>Every N minutes</option>
          <option>Weekly on specific days</option>
          <option>Custom expression</option>
        </select>
      </div>
      <div class="field">
        <label class="label">Time</label>
        <div class="row gap2">
          <input class="input mono" style="width:110px" value="02:00" />
          <select class="select flex1"><option>UTC</option><option selected>Asia/Kolkata</option><option>Europe/London</option><option>America/New_York</option></select>
        </div>
      </div>
    </div>

    <div class="field" style="margin-bottom:20px">
      <label class="label">Cron expression</label>
      <div class="row gap2">
        <input class="input mono flex1" value="0 2 * * 1-5" />
        <button class="btn" title="Copy">${icSm(`<rect x="9" y="9" width="12" height="12" rx="2"/><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"/>`)}</button>
      </div>
      <span class="hint"><b style="color:var(--fg);font-weight:550">At 02:00</b>, Monday through Friday. Five fields, weekday numbering from Sunday.</span>
    </div>

    <!-- Next-run preview: the single most valuable control on this step. -->
    <div class="card" style="margin-bottom:20px">
      <div class="card-head">
        <h3>Next 6 runs</h3>
        <span class="sub">shown in Asia/Kolkata</span>
        <span class="pill ok ml-auto">${icXs(`<path d="M20 6 9 17l-5-5"/>`)} no DST conflicts</span>
      </div>
      <div class="card-body tight">
        <table>
          <thead><tr><th style="width:150px">Local time</th><th style="width:190px">UTC</th><th>Day</th><th class="num">In</th></tr></thead>
          <tbody>
            <tr><td class="tnum" style="font-weight:550">Tue, Oct 7 · 02:00</td><td class="tnum mono muted">Mon, Oct 6 · 20:30</td><td>Tuesday</td><td class="num tnum">18m</td></tr>
            <tr><td class="tnum" style="font-weight:550">Wed, Oct 8 · 02:00</td><td class="tnum mono muted">Tue, Oct 7 · 20:30</td><td>Wednesday</td><td class="num tnum">1d 18m</td></tr>
            <tr><td class="tnum" style="font-weight:550">Thu, Oct 9 · 02:00</td><td class="tnum mono muted">Wed, Oct 8 · 20:30</td><td>Thursday</td><td class="num tnum">2d 18m</td></tr>
            <tr><td class="tnum" style="font-weight:550">Fri, Oct 10 · 02:00</td><td class="tnum mono muted">Thu, Oct 9 · 20:30</td><td>Friday</td><td class="num tnum">3d 18m</td></tr>
            <tr><td class="tnum" style="font-weight:550">Mon, Oct 13 · 02:00</td><td class="tnum mono muted">Mon, Oct 12 · 20:30</td><td>Monday</td><td class="num tnum">5d 18m</td></tr>
            <tr><td class="tnum" style="font-weight:550">Tue, Oct 14 · 02:00</td><td class="tnum mono muted">Mon, Oct 13 · 20:30</td><td>Tuesday</td><td class="num tnum">6d 18m</td></tr>
          </tbody>
        </table>
      </div>
    </div>

    <div class="banner info" style="margin-bottom:20px">
      ${icSm(`<circle cx="12" cy="12" r="9"/><path d="M12 16v-4M12 8h.01"/>`)}
      <div class="flex1">
        <b>If Forge was down at 02:00</b>, what should happen? This is the misfire policy, and it is the
        difference between a missed run and a stampede.
      </div>
    </div>

    <div class="col gap3" style="margin-bottom:22px">
      <div class="choice on">
        <span class="radio on"></span>
        <div class="flex1">
          <div class="t">Run once, late</div>
          <div class="d">One execution fires immediately when Forge recovers. Missed occurrences are skipped, not queued. <span style="color:var(--fg)">Default.</span></div>
        </div>
      </div>
      <div class="choice">
        <span class="radio"></span>
        <div class="flex1">
          <div class="t">Catch up every missed run</div>
          <div class="d">Replays each missed occurrence, oldest first, up to a limit of 100. Safe for idempotent jobs; dangerous for ones that charge a card.</div>
        </div>
      </div>
      <div class="choice">
        <span class="radio"></span>
        <div class="flex1">
          <div class="t">Skip entirely</div>
          <div class="d">The occurrence is discarded. Use when a late run is worse than no run.</div>
        </div>
      </div>
    </div>

    <div class="row gap3">
      <span style="flex:1"></span>
      <button class="btn">Back</button>
      <button class="btn primary">Continue to execution ${icSm(`<path d="M5 12h14M13 6l6 6-6 6"/>`)}</button>
    </div>
  </div>
</div>`,
});

// ================================================================ why didn't run

const why = shell({
  active: "/jobs",
  title: "Diagnosis",
  body: `
<div class="pagehead">
  ${crumb("Jobs", "Nightly settlement", "Diagnosis")}
  <div class="row gap3" style="align-items:flex-start">
    <div class="flex1">
      <h1>Why didn't nightly-settlement run on Oct 2?</h1>
      <p>Every gate that could have suppressed the occurrence, and its verdict.</p>
    </div>
    <div class="row gap2">
      <button class="btn">${icSm(`<path d="M12 2v6M15 5l-3 3-3-3"/>`)} Link to this diagnosis</button>
      <button class="btn primary">Run it now</button>
    </div>
  </div>
</div>

<div class="split">
  <div>
    <div class="card" style="margin-bottom:16px">
      <div class="card-head">
        <h2>Gate evaluation</h2>
        <span class="sub">occurrence Tue, Oct 1 · 02:00 IST → Tue, Oct 2 · 02:00 IST</span>
      </div>
      <div class="card-body">
        <div class="xlist">
          <div class="xrow">
            <span class="xn">Schedule</span>
            <span class="xs" style="color:var(--ok)">${icSm(`<path d="M20 6 9 17l-5-5"/>`)}</span>
            <span class="xv">Active</span>
            <span class="xd">enabled 41 days ago · 6 occurrences fired</span>
          </div>
          <div class="xrow">
            <span class="xn">Scheduler</span>
            <span class="xs" style="color:var(--ok)">${icSm(`<path d="M20 6 9 17l-5-5"/>`)}</span>
            <span class="xv">Healthy</span>
            <span class="xd">tick every 5s, no gap over 14h</span>
          </div>
          <div class="xrow">
            <span class="xn">Cron match</span>
            <span class="xs" style="color:var(--ok)">${icSm(`<path d="M20 6 9 17l-5-5"/>`)}</span>
            <span class="xv">Matched</span>
            <span class="xd"><span class="mono">0 2 * * *</span> in Asia/Kolkata</span>
          </div>
          <div class="xrow">
            <span class="xn">Calendar</span>
            <span class="xs" style="color:var(--bad)">${icSm(`<path d="M18 6 6 18M6 6l12 12"/>`)}</span>
            <span class="xv" style="color:var(--bad)">Excluded</span>
            <span class="xd">India Banking Calendar</span>
          </div>
          <div class="xrow">
            <span class="xn">Maintenance</span>
            <span class="xs" style="color:var(--ok)">${icSm(`<path d="M20 6 9 17l-5-5"/>`)}</span>
            <span class="xv">None active</span>
            <span class="xd">last window ended 3d ago</span>
          </div>
          <div class="xrow">
            <span class="xn">Concurrency</span>
            <span class="xs" style="color:var(--ok)">${icSm(`<path d="M20 6 9 17l-5-5"/>`)}</span>
            <span class="xv">Available</span>
            <span class="xd">1 of 4 slots in use</span>
          </div>
          <div class="xrow">
            <span class="xn">Dependency</span>
            <span class="xs" style="color:var(--ok)">${icSm(`<path d="M20 6 9 17l-5-5"/>`)}</span>
            <span class="xv">Upstream succeeded</span>
            <span class="xd"><span class="mono">extract-orders</span> finished 01:58</span>
          </div>
          <div class="xrow">
            <span class="xn">Worker</span>
            <span class="xs" style="color:var(--muted-fg)">—</span>
            <span class="xv muted">Not reached</span>
            <span class="xd">no execution was created</span>
          </div>
        </div>
      </div>
    </div>

    <div class="reason" style="margin-bottom:16px">
      <div class="rh">Reason</div>
      <div class="rb">
        <b>October 2 is excluded by "India Banking Calendar".</b>
        The schedule matched and the scheduler was healthy, but the calendar gate suppressed the
        occurrence before any execution was created. The next eligible run is
        <b>Tue, Oct 7 at 02:00 IST</b>.
      </div>
    </div>

    <div class="card">
      <div class="card-head">
        <h2>Resolved trigger</h2>
        <span class="sub">everything that produced this time</span>
      </div>
      <div class="card-body">
        <div class="grid g4" style="gap:0;text-align:center">
          <div style="padding:0 8px;border-right:1px solid var(--line)">
            <div style="font-size:10px;text-transform:uppercase;letter-spacing:0.06em;color:var(--muted-fg);font-weight:600">Trigger</div>
            <div style="margin-top:7px;font-size:12.5px;font-weight:550">Cron schedule</div>
          </div>
          <div style="padding:0 8px;border-right:1px solid var(--line)">
            <div style="font-size:10px;text-transform:uppercase;letter-spacing:0.06em;color:var(--muted-fg);font-weight:600">Expression</div>
            <div class="mono" style="margin-top:7px;font-size:13px">0 2 * * *</div>
          </div>
          <div style="padding:0 8px;border-right:1px solid var(--line)">
            <div style="font-size:10px;text-transform:uppercase;letter-spacing:0.06em;color:var(--muted-fg);font-weight:600">Timezone</div>
            <div style="margin-top:7px;font-size:12.5px;font-weight:550">Asia/Kolkata</div>
          </div>
          <div style="padding:0 8px">
            <div style="font-size:10px;text-transform:uppercase;letter-spacing:0.06em;color:var(--muted-fg);font-weight:600">Next run</div>
            <div class="tnum" style="margin-top:7px;font-size:12.5px;font-weight:550">Oct 7, 02:00</div>
          </div>
        </div>
        <div class="divider"></div>
        <div class="row gap2" style="font-size:11.5px;color:var(--muted-fg)">
          ${icSm(`<circle cx="12" cy="12" r="9"/><path d="M12 16v-4M12 8h.01"/>`)}
          <span>Timezone is never inferred from the server. A missing timezone is a validation error, not a silent fallback to UTC.</span>
        </div>
      </div>
    </div>
  </div>

  <div class="col gap4">
    <div class="card">
      <div class="card-head"><h3>Calendar</h3><a class="link ml-auto" href="#" style="font-size:11.5px">Edit</a></div>
      <div class="card-body">
        <div class="row gap2" style="font-size:12.5px;font-weight:550">
          ${icSm(`<rect x="3" y="4" width="18" height="18" rx="2"/><path d="M16 2v4M8 2v4M3 10h18"/>`)}
          India Banking Calendar
        </div>
        <div class="divider" style="margin:12px 0"></div>
        <div class="col gap2" style="font-size:11.5px">
          <div class="row gap2"><span class="dot bad"></span><span>Oct 2 — Gandhi Jayanti</span></div>
          <div class="row gap2"><span class="dot off"></span><span>Oct 15 — Diwali</span></div>
          <div class="row gap2"><span class="dot off"></span><span>Nov 1 — Diwali</span></div>
        </div>
        <a class="link" href="#" style="display:inline-block;margin-top:12px;font-size:11.5px">View all 14 exclusions</a>
      </div>
    </div>

    <div class="card">
      <div class="card-head"><h3>Schedule</h3></div>
      <div class="card-body">
        <dl class="kv" style="grid-template-columns:96px minmax(0,1fr);font-size:11.5px">
          <dt>Job</dt><dd>Nightly settlement</dd>
          <dt>Type</dt><dd>Cron</dd>
          <dt>Expression</dt><dd class="mono">0 2 * * *</dd>
          <dt>Timezone</dt><dd>Asia/Kolkata</dd>
          <dt>Enabled</dt><dd>yes, since Aug 26</dd>
          <dt>Misfire</dt><dd>Fire once</dd>
          <dt>Next run</dt><dd class="tnum">Oct 7, 02:00 IST</dd>
          <dt>Last run</dt><dd class="tnum">Oct 1, 02:00 IST</dd>
        </dl>
        <div class="btnrow" style="margin-top:14px">
          <button class="btn sm">${icSm(`<rect x="7" y="4" width="3.5" height="16" rx="1"/><rect x="13.5" y="4" width="3.5" height="16" rx="1"/>`)} Pause</button>
          <button class="btn sm">${icSm(`<path d="M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7"/><path d="M18.5 2.5a2.12 2.12 0 0 1 3 3L12 15l-4 1 1-4 9.5-9.5z"/>`)} Edit</button>
        </div>
      </div>
    </div>

    <div class="card">
      <div class="card-head"><h3>Occurrence history</h3></div>
      <div class="card-body col gap2">
        <div class="row gap2" style="font-size:11.5px"><span class="dot ok"></span><span class="tnum flex1">Oct 1 · 02:00</span><span class="muted">8m 38s</span></div>
        <div class="row gap2" style="font-size:11.5px"><span class="dot ok"></span><span class="tnum flex1">Sep 30 · 02:00</span><span class="muted">8m 51s</span></div>
        <div class="row gap2" style="font-size:11.5px"><span class="dot bad"></span><span class="tnum flex1">Oct 2 · 02:00</span><span class="muted" style="color:var(--bad)">skipped</span></div>
        <div class="row gap2" style="font-size:11.5px"><span class="dot ok"></span><span class="tnum flex1">Sep 29 · 02:00</span><span class="muted">8m 44s</span></div>
      </div>
    </div>
  </div>
</div>`,
});

const out = path.join(__dirname, "out");
fs.mkdirSync(out, { recursive: true });
fs.writeFileSync(path.join(out, "04-dashboard.html"), dashboard);
fs.writeFileSync(path.join(out, "05-jobs.html"), jobs);
fs.writeFileSync(path.join(out, "06-job-builder.html"), builder);
fs.writeFileSync(path.join(out, "07-why-didnt-run.html"), why);
console.log("wrote dashboard, jobs, builder, why-didnt-run");