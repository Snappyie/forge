// Shared shell partials for the Forge console mockups.
// Icons are inline 16px stroke SVGs so the pages are self-contained and the
// screenshots need no network or icon font.

const ic = (d, extra = "") =>
  `<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"${extra}>${d}</svg>`;

const icSm = (d) =>
  `<svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${d}</svg>`;

const icXs = (d) =>
  `<svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${d}</svg>`;

const I = {
  gauge: ic(`<path d="M12 14l4-4"/><circle cx="12" cy="14" r="1"/><path d="M3.3 18a10 10 0 1 1 17.4 0"/>`),
  clip: ic(`<rect x="8" y="2" width="8" height="4" rx="1"/><path d="M16 4h2a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2h2"/><path d="M9 12h6M9 16h4"/>`),
  activity: ic(`<path d="M22 12h-4l-3 9L9 3l-3 9H2"/>`),
  workflow: ic(`<rect x="3" y="3" width="6" height="6" rx="1.5"/><rect x="15" y="15" width="6" height="6" rx="1.5"/><path d="M6 9v4a2 2 0 0 0 2 2h7"/>`),
  calendar: ic(`<rect x="3" y="4" width="18" height="18" rx="2"/><path d="M16 2v4M8 2v4M3 10h18"/>`),
  users: ic(`<path d="M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2"/><circle cx="9" cy="7" r="4"/><path d="M22 21v-2a4 4 0 0 0-3-3.87"/>`),
  layers: ic(`<path d="M12 2 2 7l10 5 10-5-10-5z"/><path d="M2 17l10 5 10-5"/><path d="M2 12l10 5 10-5"/>`),
  clock: ic(`<circle cx="12" cy="12" r="9"/><path d="M12 7v5l3 2"/>`),
  dot: ic(`<circle cx="12" cy="12" r="5"/>`),
  spark: ic(`<path d="M12 2l1.9 5.6L19.5 9l-5.6 1.9L12 16.5l-1.9-5.6L4.5 9l5.6-1.4L12 2z"/><path d="M19 15l.8 2.2L22 18l-2.2.8L19 21l-.8-2.2L16 18l2.2-.8L19 15z"/>`),
  bell: ic(`<path d="M6 8a6 6 0 1 1 12 0c0 7 3 9 3 9H3s3-2 3-9"/><path d="M10.3 21a1.94 1.94 0 0 0 3.4 0"/>`),
  shield: ic(`<path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z"/>`),
  shieldAlert: ic(`<path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z"/><path d="M12 8v4M12 16h.01"/>`),
  list: ic(`<path d="M8 6h13M8 12h13M8 18h13M3 6h.01M3 12h.01M3 18h.01"/>`),
  plug: ic(`<path d="M9 2v6M15 2v6M6 8h12v3a6 6 0 0 1-6 6 6 6 0 0 1-6-6V8zM12 17v5"/>`),
  webhook: ic(`<path d="M18 16.98h-5.99c-1.1 0-1.95.94-2.48 1.9A4 4 0 0 1 2 17c.01-.7.2-1.4.57-2"/><path d="m6 17 3.13-5.78c.53-.97.1-2.18-.5-3.1a4 4 0 1 1 6.89-4.06"/><path d="m12 6 3.13 5.73C15.66 12.7 16.7 13.86 17 15"/>`),
  book: ic(`<path d="M4 19.5A2.5 2.5 0 0 1 6.5 17H20"/><path d="M6.5 2H20v20H6.5A2.5 2.5 0 0 1 4 19.5v-15A2.5 2.5 0 0 1 6.5 2z"/>`),
  settings: ic(`<circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 1 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 1 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06A1.65 1.65 0 0 0 9 4.6h.09A1.65 1.65 0 0 0 10 3.09V3a2 2 0 1 1 4 0v.09A1.65 1.65 0 0 0 15 4.6a1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9v.09a1.65 1.65 0 0 0 1.51 1H21a2 2 0 1 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z"/>`),
  search: ic(`<circle cx="11" cy="11" r="7"/><path d="m21 21-4.3-4.3"/>`),
  play: ic(`<path d="M6 4l13 8-13 8V4z"/>`),
  pause: ic(`<rect x="7" y="4" width="3.5" height="16" rx="1"/><rect x="13.5" y="4" width="3.5" height="16" rx="1"/>`),
  check: ic(`<path d="M20 6 9 17l-5-5"/>`),
  x: ic(`<path d="M18 6 6 18M6 6l12 12"/>`),
  chevR: ic(`<path d="M9 18l6-6-6-6"/>`),
  chevD: ic(`<path d="M6 9l6 6 6-6"/>`),
  refresh: ic(`<path d="M21 12a9 9 0 1 1-3-6.7"/><path d="M21 3v6h-6"/>`),
  more: ic(`<circle cx="5" cy="12" r="1.4"/><circle cx="12" cy="12" r="1.4"/><circle cx="19" cy="12" r="1.4"/>`),
  plus: ic(`<path d="M12 5v14M5 12h14"/>`),
  alert: ic(`<path d="M12 9v4M12 17h.01"/><path d="M10.3 3.9 1.8 18a2 2 0 0 0 1.7 3h17a2 2 0 0 0 1.7-3L13.7 3.9a2 2 0 0 0-3.4 0z"/>`),
  info: ic(`<circle cx="12" cy="12" r="9"/><path d="M12 16v-4M12 8h.01"/>`),
  filter: ic(`<path d="M3 4h18l-7 8v7l-4 2v-9L3 4z"/>`),
  download: ic(`<path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><path d="M7 10l5 5 5-5M12 15V3"/>`),
  copy: ic(`<rect x="9" y="9" width="12" height="12" rx="2"/><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"/>`),
  trash: ic(`<path d="M3 6h18M8 6V4a1 1 0 0 1 1-1h6a1 1 0 0 1 1 1v2M19 6l-1 14a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2L5 6"/>`),
  star: ic(`<path d="m12 2 3.1 6.3 6.9 1-5 4.9 1.2 6.9-6.2-3.3-6.2 3.3L7 14.2l-5-4.9 6.9-1L12 2z"/>`),
  eye: ic(`<path d="M2 12s3.6-7 10-7 10 7 10 7-3.6 7-10 7-10-7-10-7z"/><circle cx="12" cy="12" r="3"/>`),
  edit: ic(`<path d="M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7"/><path d="M18.5 2.5a2.12 2.12 0 0 1 3 3L12 15l-4 1 1-4 9.5-9.5z"/>`),
  zap: ic(`<path d="M13 2 3 14h8l-1 8 10-12h-8l1-8z"/>`),
  sun: ic(`<circle cx="12" cy="12" r="4"/><path d="M12 2v2M12 20v2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M2 12h2M20 12h2M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4"/>`),
  moon: ic(`<path d="M21 12.8A9 9 0 1 1 11.2 3a7 7 0 0 0 9.8 9.8z"/>`),
  help: ic(`<circle cx="12" cy="12" r="9"/><path d="M9.1 9a3 3 0 0 1 5.8 1c0 2-3 3-3 3M12 17h.01"/>`),
  inbox: ic(`<path d="M22 12h-6l-2 3h-4l-2-3H2"/><path d="M5.4 5.1 2 12v6a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2v-6l-3.4-6.9A2 2 0 0 0 16.8 4H7.2a2 2 0 0 0-1.8 1.1z"/>`),
  terminal: ic(`<path d="m4 17 6-6-6-6M12 19h8"/>`),
  file: ic(`<path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><path d="M14 2v6h6"/>`),
  arrowRight: ic(`<path d="M5 12h14M13 6l6 6-6 6"/>`),
  arrowU: ic(`<path d="M12 19V5M6 11l6-6 6 6"/>`),
  save: ic(`<path d="M19 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h11l5 5v11a2 2 0 0 1-2 2z"/><path d="M17 21v-8H7v8M7 3v5h8"/>`),
  server: ic(`<rect x="2" y="3" width="20" height="8" rx="2"/><rect x="2" y="13" width="20" height="8" rx="2"/><path d="M6 7h.01M6 17h.01"/>`),
  key: ic(`<circle cx="7.5" cy="15.5" r="4.5"/><path d="m11 12 9-9 2 2-2 2 2 2-3 1-2-2-2 2"/>`),
  git: ic(`<circle cx="12" cy="18" r="3"/><circle cx="6" cy="6" r="3"/><path d="M18 9a3 3 0 1 0-3-3"/><path d="M6 9v6a3 3 0 0 0 3 3h6"/>`),
  clock3: ic(`<circle cx="12" cy="12" r="9"/><path d="M12 7v5l3 2"/>`),
  compare: ic(`<path d="M5 3v18M19 3v18"/><path d="M5 8h4M15 16h4M9 8c0 3 2 4 6 4M15 16c0-3-2-4-6-4"/>`),
  wand: ic(`<path d="m3 21 12-12M14 5l1.5-1.5M17 8l1.5-1.5M14 11l1.5-1.5M20 4l1 1"/><path d="M3 3v4M1 5h4M6 17v4M4 19h4"/>`),
  bell2: ic(`<path d="M6 8a6 6 0 0 1 12 0c0 7 3 9 3 9H3s3-2 3-9"/><path d="M10.3 21a1.94 1.94 0 0 0 3.4 0"/>`),
  map: ic(`<path d="m3 6 6-3 6 3 6-3v15l-6 3-6-3-6 3V6z"/><path d="M9 3v15M15 6v15"/>`),
};

// Navigation. Grouped by work pattern so the sidebar teaches the mental model:
// Monitor (what is happening), Operate (define and control work), Govern (why
// and who). This replaces the current flat 20-item list.
const NAV_GROUPS = [
  {
    label: "Monitor",
    items: [
      { href: "/", label: "Dashboard", icon: "gauge" },
      { href: "/running", label: "Running now", icon: "dot", count: "3" },
      { href: "/upcoming", label: "Upcoming", icon: "clock" },
      { href: "/alerts", label: "Alerts", icon: "bell", count: "4", tone: "bad" },
      { href: "/incidents", label: "Incidents", icon: "shieldAlert", count: "1", tone: "wait" },
    ],
  },
  {
    label: "Define",
    items: [
      { href: "/jobs", label: "Jobs", icon: "clip", count: "24" },
      { href: "/schedules", label: "Schedules", icon: "calendar" },
      { href: "/workflows", label: "Workflows", icon: "workflow", count: "6" },
      { href: "/calendar", label: "Calendar", icon: "clock3" },
    ],
  },
  {
    label: "Capacity",
    items: [
      { href: "/queues", label: "Queues", icon: "layers", count: "4" },
      { href: "/workers", label: "Workers", icon: "users", count: "9" },
    ],
  },
  {
    label: "Govern",
    items: [
      { href: "/audit", label: "Audit log", icon: "list" },
      { href: "/integrations", label: "Integrations", icon: "plug" },
      { href: "/admin", label: "Administration", icon: "settings" },
    ],
  },
];

const navCount = (n) =>
  n.count
    ? `<span class="navcount${n.tone ? " " + n.tone : ""}">${n.count}</span>`
    : "";

function sidebar(active) {
  const groups = NAV_GROUPS.map((g) => {
    const items = g.items
      .map((n) => {
        const on = n.href === active ? ' aria-current="page"' : "";
        return `<a class="navlink" href="${n.href}"${on}>${I[n.icon]}<span class="truncate">${n.label}</span>${navCount(n)}</a>`;
      })
      .join("\n        ");
    return `<div class="navgroup"><div class="navgroup-label">${g.label}</div>\n        ${items}\n      </div>`;
  }).join("\n      ");

  return `<nav class="sidebar" aria-label="Main">
      <div class="brand">
        <span class="brand-mark">F</span>
        <span class="brand-name">Forge</span>
        <button class="hbtn ml-auto" aria-label="Collapse sidebar" style="width:24px;height:24px;padding:0;justify-content:center">${icSm(`<path d="M15 6l-6 6 6 6"/>`)}</button>
      </div>
      ${groups}
      <div class="sidebar-foot">
        <a class="navlink" href="/assistant">${I.spark}<span class="truncate">Assistant</span></a>
        <a class="navlink" href="/docs">${I.book}<span class="truncate">Developers</span></a>
      </div>
    </nav>`;
}

function header(opts = {}) {
  const {
    env = "production",
    envLabel = "production",
    search = "Search jobs, executions, workers…",
    health = "ok",
    notif = 3,
  } = opts;

  const healthEl =
    health === "ok"
      ? `<span class="dot ok" role="status" aria-label="System healthy"></span>`
      : health === "degraded"
      ? `<span class="dot wait" role="status" aria-label="System degraded"></span>`
      : `<span class="dot bad" role="status" aria-label="System unhealthy"></span>`;

  return `<header class="header">
      <span class="env ${env}">${envLabel}</span>
      <button class="searchbtn" aria-label="Search">${I.search}<span class="truncate">${search}</span><kbd class="ml-auto">/</kbd></button>
      <button class="hbtn bordered" aria-label="Command palette">${I.terminal}<span>Commands</span><kbd>⌘K</kbd></button>
      <span class="tenant">${I.server}<span>acme-prod</span></span>
      ${healthEl}
      <span class="row" style="position:relative">
        <button class="hbtn" aria-label="Notifications, ${notif} unread" style="width:28px;padding:0;justify-content:center">${I.bell}</button>
        ${notif > 0 ? `<span style="position:absolute;top:1px;right:1px;min-width:14px;height:14px;border-radius:8px;background:var(--bad);color:#fff;font-size:9px;font-weight:600;display:grid;place-items:center;padding:0 3px">${notif}</span>` : ""}
      </span>
      <button class="hbtn" aria-label="Theme" style="width:28px;padding:0;justify-content:center">${I.sun}</button>
      <button class="hbtn" aria-label="Help" style="width:28px;padding:0;justify-content:center">${I.help}</button>
      <span class="row gap2" style="padding-left:10px;margin-left:2px;border-left:1px solid var(--border)">
        <span style="font-size:11.5px;color:var(--muted-fg)">Owner</span>
        <span class="avatar">NK</span>
      </span>
    </header>`;
}

/** Full console page. `body` is the main content; `flush` removes content padding. */
function shell({ active, title, body, env, envLabel, search, health, notif, flush }) {
  return `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>${title} — Forge</title>
<link rel="preconnect" href="https://fonts.googleapis.com">
<link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
<link href="https://fonts.googleapis.com/css2?family=Inter:wght@400;450;500;600;700&family=JetBrains+Mono:wght@400;500&display=swap" rel="stylesheet">
<link rel="stylesheet" href="forge.css">
</head>
<body>
<div class="shell">
  ${sidebar(active)}
  <div class="main">
    ${header({ env, envLabel, search, health, notif })}
    <main class="content${flush ? " flush" : ""}">
${body}
    </main>
  </div>
</div>
</body>
</html>`;
}

const statusPill = (status, label) => {
  const map = {
    SUCCEEDED: "ok",
    ACTIVE: "ok",
    READY: "ok",
    RUNNING: "run",
    DISPATCHED: "run",
    BUSY: "run",
    QUEUED: "wait",
    SCHEDULED: "wait",
    RETRY_SCHEDULED: "wait",
    CANCEL_REQUESTED: "wait",
    FAILED: "bad",
    TIMED_OUT: "bad",
    DEAD_LETTERED: "bad",
    ABANDONED: "bad",
    DRAFT: "",
    CANCELLED: "",
    ARCHIVED: "",
    OFFLINE: "",
    DRAINING: "warn",
  };
const tone = map[status] ?? "";
  // Neutral tones carry no glyph; without the fallback this prints "undefined".
  const glyph =
    {
      ok: icXs(`<path d="M20 6 9 17l-5-5"/>`),
      bad: icXs(`<circle cx="12" cy="12" r="9"/><path d="M12 8v5M12 16h.01"/>`),
    }[tone] ?? "";
  // An unknown status renders neutrally rather than being guessed at.
  const text =
    label || String(status ?? "unknown").replace(/_/g, " ").toLowerCase();
  return `<span class="pill ${tone}">${glyph}${text}</span>`;
};

const prioPill = (p) => {
  const map = {
    CRITICAL: "bad",
    HIGH: "warn",
    NORMAL: "",
    LOW: "",
    BACKGROUND: "",
  };
  return `<span class="pill ${map[p] ?? ""}">${p.charAt(0) + p.slice(1).toLowerCase()}</span>`;
};

module.exports = { I, icSm, icXs, ic, shell, sidebar, header, statusPill, prioPill, NAV_GROUPS };