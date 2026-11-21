// Frames for the phone app and health mockups. ?theme=dark or ?theme=light picks the theme.
// <body data-frame="duo"> draws #content twice, in an Android and an iPhone frame side by side;
// <x data-os="android"> or data-os="ios" shows in one frame only. <body data-frame="app"> draws the
// computer's rail and keep bar. <x data-state="a b"> shows only in states a and b; ?state= picks one.

const ICON = {
  lock: '<rect width="18" height="11" x="3" y="11" rx="2" ry="2"/><path d="M7 11V7a5 5 0 0 1 10 0v4"/>',
  paperclip:
    '<path d="m16 6-8.414 8.586a2 2 0 0 0 2.829 2.829l8.414-8.586a4 4 0 1 0-5.657-5.657l-8.379 8.551a6 6 0 1 0 8.485 8.485l8.379-8.551"/>',
  mic: '<path d="M12 19v3"/><path d="M19 10v2a7 7 0 0 1-14 0v-2"/><rect x="9" y="2" width="6" height="13" rx="3"/>',
  "chevron-down": '<path d="m6 9 6 6 6-6"/>',
  "chevron-right": '<path d="m9 18 6-6-6-6"/>',
  x: '<path d="M18 6 6 18"/><path d="m6 6 12 12"/>',
  check: '<path d="M20 6 9 17l-5-5"/>',
  image:
    '<rect width="18" height="18" x="3" y="3" rx="2" ry="2"/><circle cx="9" cy="9" r="2"/><path d="m21 15-3.086-3.086a2 2 0 0 0-2.828 0L6 21"/>',
  sun: '<circle cx="12" cy="12" r="4"/><path d="M12 2v2"/><path d="M12 20v2"/><path d="m4.93 4.93 1.41 1.41"/><path d="m17.66 17.66 1.41 1.41"/><path d="M2 12h2"/><path d="M20 12h2"/><path d="m6.34 17.66-1.41 1.41"/><path d="m19.07 4.93-1.41 1.41"/>',
  play: '<polygon points="6 3 20 12 6 21 6 3"/>',
  calendar:
    '<path d="M8 2v4"/><path d="M16 2v4"/><rect width="18" height="18" x="3" y="4" rx="2"/><path d="M3 10h18"/>',
  user: '<path d="M19 21v-2a4 4 0 0 0-4-4H9a4 4 0 0 0-4 4v2"/><circle cx="12" cy="7" r="4"/>',
  pin: '<path d="M20 10c0 4.993-5.539 10.193-7.399 11.799a1 1 0 0 1-1.202 0C9.539 20.193 4 14.993 4 10a8 8 0 0 1 16 0"/><circle cx="12" cy="10" r="3"/>',
  thing:
    '<path d="m7.5 4.27 9 5.15"/><path d="M21 8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16Z"/><path d="m3.3 7 8.7 5 8.7-5"/><path d="M12 22V12"/>',
  plus: '<path d="M5 12h14"/><path d="M12 5v14"/>',
  file: '<path d="M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z"/><path d="M14 2v4a2 2 0 0 0 2 2h4"/><path d="M10 9H8"/><path d="M16 13H8"/><path d="M16 17H8"/>',
  phone: '<rect width="14" height="20" x="5" y="2" rx="2" ry="2"/><path d="M12 18h.01"/>',
  pen: '<path d="M12 20h9"/><path d="M16.376 3.622a1 1 0 0 1 3.002 3.002L7.368 18.635a2 2 0 0 1-.855.506l-2.872.838a.5.5 0 0 1-.62-.62l.838-2.872a2 2 0 0 1 .506-.854z"/>',
  more: '<circle cx="12" cy="12" r="1"/><circle cx="19" cy="12" r="1"/><circle cx="5" cy="12" r="1"/>',
  listen:
    '<path d="M11 4.702a.705.705 0 0 0-1.203-.498L6.413 7.587A1.4 1.4 0 0 1 5.416 8H3a1 1 0 0 0-1 1v6a1 1 0 0 0 1 1h2.416a1.4 1.4 0 0 1 .997.413l3.383 3.384A.705.705 0 0 0 11 19.298z"/><path d="M16 9a5 5 0 0 1 0 6"/>',
  print:
    '<path d="M6 18H4a2 2 0 0 1-2-2v-5a2 2 0 0 1 2-2h16a2 2 0 0 1 2 2v5a2 2 0 0 1-2 2h-2"/><path d="M6 9V3a1 1 0 0 1 1-1h10a1 1 0 0 1 1 1v6"/><rect x="6" y="14" width="12" height="8" rx="1"/>',
  menu: '<path d="M4 6h16"/><path d="M4 12h16"/><path d="M4 18h16"/>',
  moon: '<path d="M12 3a6 6 0 0 0 9 9 9 9 0 1 1-9-9Z"/>',
  camera:
    '<path d="M14.5 4h-5L7 7H4a2 2 0 0 0-2 2v9a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2V9a2 2 0 0 0-2-2h-3l-2.5-3z"/><circle cx="12" cy="13" r="3"/>',
  ask: '<path d="M7.9 20A9 9 0 1 0 4 16.1L2 22Z"/>',
  book:
    '<path d="M12 7v14"/><path d="M3 18a1 1 0 0 1-1-1V4a1 1 0 0 1 1-1h5a4 4 0 0 1 4 4 4 4 0 0 1 4-4h5a1 1 0 0 1 1 1v13a1 1 0 0 1-1 1h-6a3 3 0 0 0-3 3 3 3 0 0 0-3-3z"/>',
  // The four parts of the feelings field: a shape each, so colour never carries the meaning alone.
  wound: '<path d="M3 15l4.5-6 4.5 6 4.5-6 4.5 6"/>',
  lit: '<circle cx="12" cy="12" r="3.5"/><path d="M12 2.5v3.5M12 18v3.5M2.5 12H6M18 12h3.5"/>',
  low: '<path d="M4 6l16 11"/><path d="M13 17h7v-6"/>',
  settled: '<path d="M2.5 13c3-3.5 6.5 3.5 9.5 0s6.5 3.5 9.5 0"/>',
  heart:
    '<path d="M19 14c1.49-1.46 3-3.21 3-5.5A5.5 5.5 0 0 0 16.5 3c-1.76 0-3 .5-4.5 2-1.5-1.5-2.74-2-4.5-2A5.5 5.5 0 0 0 2 8.5c0 2.3 1.5 4.05 3 5.5l7 7Z"/>',
  steps:
    '<path d="M4 16v-2.38C4 11.5 2.97 10.5 3 8c.03-2.72 1.49-6 4.5-6C9.37 2 10 3.8 10 5.5c0 3.11-2 5.66-2 8.68V16a2 2 0 1 1-4 0Z"/><path d="M20 20v-2.38c0-2.12 1.03-3.12 1-5.62-.03-2.72-1.49-6-4.5-6C14.63 6 14 7.8 14 9.5c0 3.11 2 5.66 2 8.68V20a2 2 0 1 0 4 0Z"/>',
  walk: '<circle cx="13" cy="4" r="2"/><path d="m9 20 3-6 3 3v4"/><path d="m6 12 3-4 4 1 3 3 3 1"/>',
  swim: '<path d="M2 18c1.5 1 3 1 4.5 0s3-1 4.5 0 3 1 4.5 0 3-1 4.5 0"/><circle cx="16" cy="6" r="2"/><path d="m6 13 4-4 3 3 3-1"/>',
  scale: '<path d="M12 3v18"/><path d="M7 21h10"/><path d="M3 7h18"/><path d="m3 7 3 8 3-8"/><path d="m15 7 3 8 3-8"/>',
  gauge: '<path d="m12 14 4-4"/><path d="M3.34 19a10 10 0 1 1 17.32 0"/>',
  settings:
    '<path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z"/><circle cx="12" cy="12" r="3"/>',
  scan: '<path d="M3 7V5a2 2 0 0 1 2-2h2"/><path d="M17 3h2a2 2 0 0 1 2 2v2"/><path d="M21 17v2a2 2 0 0 1-2 2h-2"/><path d="M7 21H5a2 2 0 0 1-2-2v-2"/><path d="M7 12h10"/>',
  keyboard:
    '<rect width="20" height="16" x="2" y="4" rx="2"/><path d="M6 8h.01"/><path d="M10 8h.01"/><path d="M14 8h.01"/><path d="M18 8h.01"/><path d="M8 12h.01"/><path d="M12 12h.01"/><path d="M16 12h.01"/><path d="M7 16h10"/>',
  "chevron-left": '<path d="m15 18-6-6 6-6"/>',
  "arrow-left": '<path d="m12 19-7-7 7-7"/><path d="M19 12H5"/>',
  share: '<path d="M4 12v8a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-8"/><path d="m16 6-4-4-4 4"/><path d="M12 2v13"/>',
  refresh:
    '<path d="M3 12a9 9 0 0 1 9-9 9.75 9.75 0 0 1 6.74 2.74L21 8"/><path d="M21 3v5h-5"/><path d="M21 12a9 9 0 0 1-9 9 9.75 9.75 0 0 1-6.74-2.74L3 16"/><path d="M8 16H3v5"/>',
  info: '<circle cx="12" cy="12" r="10"/><path d="M12 16v-4"/><path d="M12 8h.01"/>',
  trash: '<path d="M3 6h18"/><path d="M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6"/><path d="M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2"/>',
  laptop:
    '<path d="M20 16V7a2 2 0 0 0-2-2H6a2 2 0 0 0-2 2v9m16 0H4m16 0 1.28 2.55a1 1 0 0 1-.9 1.45H3.62a1 1 0 0 1-.9-1.45L4 16"/>',
  download: '<path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><path d="m7 10 5 5 5-5"/><path d="M12 15V3"/>',
  list: '<path d="M3 12h.01"/><path d="M3 18h.01"/><path d="M3 6h.01"/><path d="M8 12h13"/><path d="M8 18h13"/><path d="M8 6h13"/>',
  shield:
    '<path d="M20 13c0 5-3.5 7.5-7.66 8.95a1 1 0 0 1-.67-.01C7.5 20.5 4 18 4 13V6a1 1 0 0 1 1-1c2 0 4.5-1.2 6.24-2.72a1.17 1.17 0 0 1 1.52 0C14.51 3.81 17 5 19 5a1 1 0 0 1 1 1z"/>',
};

function icon(name, size = 20) {
  return `<svg width="${size}" height="${size}" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="${(1.5 * 24) / size}" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${ICON[name] || ""}</svg>`;
}

function mark(size = 20) {
  const stroke = (1.5 * 24) / size;
  return `<svg class="mark" viewBox="0 0 24 24" width="${size}" height="${size}" aria-hidden="true"><path d="M 0.75 12 A 11.25 11.25 0 0 0 23.25 12 Z" fill="currentColor"/><circle cx="12" cy="12" r="11.25" fill="none" stroke="currentColor" stroke-width="${stroke}"/></svg>`;
}

const params = new URLSearchParams(location.search);
const states = (document.body.dataset.states || "").split(" ").filter(Boolean);
const state = params.get("state") || states[0];
for (const el of document.querySelectorAll("[data-state]")) {
  if (!el.dataset.state.split(" ").includes(state)) el.remove();
}
const theme = params.get("theme");
if (theme === "dark" || theme === "light") document.documentElement.dataset.theme = theme;

const frame = document.body.dataset.frame;
const content = document.getElementById("content");

// Per-state frame settings: <script type="application/json" id="frames">{"state": {...}}</script>
// bar: "brand" (mark and title), "top" (title, no back), "back" (back and title), "modal" (close and title), "none".
// title, back: words for the bar. tab: "Today", "Journal", "Ask" or "" for no bottom bar. keep: true shows Keep.
// action: true shows This phone in the bar. dim: true draws the screen dimmed under a system prompt.
function settings() {
  const node = document.getElementById("frames");
  const all = node ? JSON.parse(node.textContent) : {};
  return Object.assign({ bar: "brand", title: "Today", tab: "Today", keep: false, action: true }, all["*"] || {}, all[state] || {});
}

const TABS = [
  ["Today", "file"],
  ["Journal", "book"],
  ["Ask", "ask"],
];

function statusBar(os) {
  const sig = `<svg width="17" height="11" viewBox="0 0 17 11" aria-hidden="true"><rect x="0" y="7" width="3" height="4" rx="1" fill="currentColor"/><rect x="4.5" y="5" width="3" height="6" rx="1" fill="currentColor"/><rect x="9" y="2.5" width="3" height="8.5" rx="1" fill="currentColor"/><rect x="13.5" y="0" width="3" height="11" rx="1" fill="currentColor"/></svg>`;
  const bat = `<svg width="25" height="12" viewBox="0 0 25 12" aria-hidden="true"><rect x="0.5" y="0.5" width="21" height="11" rx="3" fill="none" stroke="currentColor" opacity=".45"/><rect x="2" y="2" width="15" height="8" rx="1.6" fill="currentColor"/><rect x="22.5" y="4" width="1.6" height="4" rx=".8" fill="currentColor" opacity=".45"/></svg>`;
  if (os === "ios") return `<div class="sys"><span class="sysTime">9:41</span><span class="island"></span><span class="sysIcons">${sig}${bat}</span></div>`;
  return `<div class="sys"><span class="sysTime">9:41</span><span class="sysIcons">${sig}${bat}</span></div>`;
}

function topBar(os, s) {
  if (s.bar === "none") return "";
  const action = s.action ? (os === "ios" ? `<span class="navAction">This phone</span>` : `<span class="iconBtn" aria-label="This phone">${icon("settings", 22)}</span>`) : "";
  if (s.bar === "brand") {
    if (os === "ios") return `<header class="nav navLarge"><div class="navRow"><span></span>${action}</div><h1 class="largeTitle">${s.title}</h1></header>`;
    return `<header class="appbar"><span class="appbarBrand">${mark(22)}<span>${s.title}</span></span>${action}</header>`;
  }
  if (s.bar === "top") {
    if (os === "ios") return `<header class="nav navLarge"><div class="navRow"><span></span>${action}</div><h1 class="largeTitle">${s.title}</h1></header>`;
    return `<header class="appbar"><span class="appbarTitle">${s.title}</span>${action}</header>`;
  }
  if (s.bar === "modal") {
    if (os === "ios") return `<header class="nav"><span class="navBack">${s.back || "Cancel"}</span><span class="navTitle">${s.title}</span><span class="navAction">${s.done || ""}</span></header>`;
    return `<header class="appbar"><span class="iconBtn" aria-label="Close">${icon("x", 22)}</span><span class="appbarTitle">${s.title}</span><span class="appbarText">${s.done || ""}</span></header>`;
  }
  if (os === "ios") return `<header class="nav"><span class="navBack">${icon("chevron-left", 22)}${s.back || "Back"}</span><span class="navTitle">${s.title}</span><span class="navAction">${s.done || ""}</span></header>`;
  return `<header class="appbar"><span class="iconBtn" aria-label="Back">${icon("arrow-left", 22)}</span><span class="appbarTitle">${s.title}</span>${s.done ? `<span class="appbarText">${s.done}</span>` : ""}</header>`;
}

function bottomBar(os, s) {
  const items = TABS.map(([name, ic]) => `<span class="tab"${name === s.tab ? ' aria-current="page"' : ""}><span class="tabIcon">${icon(ic, 22)}</span><span class="tabLabel">${name}</span></span>`).join("");
  const nav = s.tab ? `<nav class="tabs" aria-label="Main">${items}</nav>` : "";
  return nav + `<div class="homebar"><span></span></div>`;
}

function keepButton(os) {
  return os === "ios"
    ? `<span class="keepBtn">${icon("plus", 20)}<span>Keep</span></span>`
    : `<span class="keepBtn">${icon("plus", 22)}<span>Keep</span></span>`;
}

if (frame === "duo") {
  const s = settings();
  const wrap = document.createElement("div");
  wrap.className = "duo";
  for (const os of ["android", "ios"]) {
    const clone = content.cloneNode(true);
    clone.removeAttribute("id");
    for (const el of clone.querySelectorAll("[data-os]")) if (el.dataset.os !== os) el.remove();
    const over = clone.querySelector(".over");
    if (over) over.remove();
    const device = document.createElement("figure");
    device.className = "device";
    device.dataset.os = os;
    device.innerHTML = `<figcaption class="osLabel">${os === "ios" ? "iPhone" : "Android"}</figcaption>
      <div class="screen${s.tab ? " hasTabs" : ""}">${statusBar(os)}${topBar(os, s)}<div class="scroll"></div>${s.keep ? keepButton(os) : ""}${bottomBar(os, s)}</div>`;
    clone.classList.add("pmain");
    device.querySelector(".scroll").appendChild(clone);
    if (over) device.querySelector(".screen").appendChild(over);
    wrap.appendChild(device);
  }
  content.remove();
  document.body.prepend(wrap);
} else if (frame === "app") {
  const section = document.body.dataset.section || "Today";
  const main = [
    ["Today", '<span class="mono">Fri</span>'],
    ["Journal", ""],
    ["Needs you", '<span class="count">1</span>'],
    ["What Nocturne remembers", ""],
    ["Recently kept", ""],
    ["Fading &amp; let go", ""],
    ["Past pages", ""],
  ];
  const link = ([name, extra]) =>
    `<a class="link" href="#"${name === section ? ' aria-current="page"' : ""}><span>${name}</span>${extra}</a>`;
  const keepbar = (narrow) =>
    `<div class="keepbar"><span>${narrow ? "Keep or ask…" : "Keep something, or ask…"}</span>${narrow ? "" : '<span class="kbd">Ctrl Shift Space</span>'}<span class="round">${icon("paperclip")}</span><span class="round">${icon("mic")}</span></div>`;
  const shell = document.createElement("div");
  shell.className = "shell";
  shell.innerHTML = `
    <nav class="rail" aria-label="Main">
      <div class="brand">${mark(20)}<span class="wordmark">Nocturne</span></div>
      <div class="links">${main.map(link).join("")}</div>
      <div class="railFoot">
        <div class="status">
          <span class="statusState">${mark(14)} Resting</span>
          <span><span class="statusLabel">Uses the internet for</span><br><span class="statusValue">Away from home</span></span>
        </div>
        <div class="secondary">${link(["Privacy", ""])}${link(["Settings", ""])}</div>
      </div>
    </nav>
    <div class="body">
      <div class="topbar">${keepbar(false)}<span class="dropHint">or drop files anywhere</span></div>
      <main class="frame"></main>
    </div>`;
  shell.querySelector("main").appendChild(content);
  document.body.prepend(shell);
}

for (const el of document.querySelectorAll("[data-icon]")) {
  el.outerHTML = icon(el.dataset.icon, Number(el.dataset.size || 20));
}
for (const el of document.querySelectorAll("[data-mark]")) {
  el.outerHTML = mark(Number(el.dataset.mark || 20));
}
for (const el of document.querySelectorAll(".feel[data-part]")) {
  if (!el.querySelector("svg") && !el.querySelector("[data-icon]")) el.insertAdjacentHTML("afterbegin", icon(el.dataset.part, 14));
}

// A stand-in code: finder squares and fixed pseudo-random modules, not a real code.
function qrSvg(size) {
  const n = 25;
  let seed = 7;
  const rnd = () => ((seed = (seed * 9301 + 49297) % 233280) / 233280);
  const cells = [];
  const finder = (x, y) => x < 7 && y < 7;
  for (let y = 0; y < n; y++) for (let x = 0; x < n; x++) {
    const f = finder(x, y) || finder(n - 1 - x, y) || finder(x, n - 1 - y);
    if (f) {
      const fx = x < 7 ? x : n - 1 - x, fy = y < 7 ? y : n - 1 - y;
      const ring = Math.max(Math.abs(fx - 3), Math.abs(fy - 3));
      if (ring !== 2) cells.push([x, y]);
    } else if (rnd() > 0.52) cells.push([x, y]);
  }
  const rects = cells.map(([x, y]) => `<rect x="${x + 2}" y="${y + 2}" width="1" height="1"/>`).join("");
  return `<svg width="${size}" height="${size}" viewBox="0 0 ${n + 4} ${n + 4}" shape-rendering="crispEdges" aria-hidden="true"><rect width="${n + 4}" height="${n + 4}" fill="#fff"/><g fill="#000">${rects}</g></svg>`;
}
for (const el of document.querySelectorAll("[data-qr]")) el.outerHTML = qrSvg(Number(el.dataset.qr || 160));

const overlay = document.getElementById("overlay");
if (overlay) document.body.appendChild(overlay);
