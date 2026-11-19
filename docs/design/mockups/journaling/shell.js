// Wraps a mockup's content in the app's frame. ?theme=dark or ?theme=light picks the theme.
// <body data-frame="app" data-section="Journal"> draws the rail and keep bar around the sheet;
// <body data-frame="window"> centres a card the way quick capture does.
// <x data-state="a b"> shows only in states a and b; ?state= picks one, the first listed is the default.

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
};

function icon(name, size = 20) {
  return `<svg width="${size}" height="${size}" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="${(1.5 * 24) / size}" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${ICON[name]}</svg>`;
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

for (const el of document.querySelectorAll("[data-icon]")) {
  el.outerHTML = icon(el.dataset.icon, Number(el.dataset.size || 20));
}

// Colour never carries the meaning alone: every feeling dot and word bar carries its part's shape.
for (const el of document.querySelectorAll(".dot[data-part], .mdot[data-part]")) {
  if (!el.querySelector("svg")) {
    const small = el.classList.contains("small") || el.classList.contains("mdot");
    el.innerHTML = icon(el.dataset.part, small ? 10 : 14);
  }
}
for (const el of document.querySelectorAll(".counts li[data-part] > span:first-child")) {
  el.innerHTML = icon(el.parentElement.dataset.part, 14) + "<span>" + el.textContent + "</span>";
}

const frame = document.body.dataset.frame;
const content = document.getElementById("content");

if (frame === "app") {
  const section = document.body.dataset.section || "Journal";
  const main = [
    ["Today", '<span class="mono">Thu</span>'],
    ["Journal", ""],
    ["Needs you", '<span class="count">2</span>'],
    ["What Nocturne remembers", ""],
    ["Recently kept", ""],
    ["Fading &amp; let go", ""],
    ["Past pages", ""],
  ];
  const link = ([name, extra]) =>
    `<a class="link${name === "Journal" ? " new" : ""}" href="#"${name === section ? ' aria-current="page"' : ""}><span>${name}</span>${extra}</a>`;
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
          <span class="statusState">${mark(14)} ${state === "notready" ? "Getting ready" : "Resting"}</span>
          <span><span class="statusLabel">Uses the internet for</span><br><span class="statusValue">Downloads you agreed to</span></span>
        </div>
        <div class="secondary">${link(["Privacy", ""])}${link(["Settings", ""])}</div>
      </div>
    </nav>
    <header class="narrowHeader">${mark(20)}<span>${section}</span><span class="chev">${icon("chevron-down", 16)}</span></header>
    <div class="body">
      <div class="topbar">${keepbar(false)}<span class="dropHint">or drop files anywhere</span></div>
      <main class="frame"></main>
      <div class="bottomBar">${keepbar(true)}</div>
    </div>`;
  shell.querySelector("main").appendChild(content);
  document.body.prepend(shell);
} else if (frame === "window") {
  const win = document.createElement("main");
  win.className = "window";
  win.appendChild(content);
  document.body.prepend(win);
} else if (frame === "phone") {
  // The phone app's own frame (Away from home): a 60px bar, then the screen.
  // <body data-frame="phone" data-back="Journal"> draws a back bar; without it, the brand bar.
  const back = document.body.dataset.back;
  const screen = document.createElement("div");
  screen.className = "phone";
  const bar = document.createElement("header");
  bar.className = back ? "pbar pbarBack" : "pbar";
  bar.innerHTML = back
    ? `<span class="pbarBtn" aria-label="Back">←</span><span class="pbarTitle">${back}</span>`
    : `<span class="pbrand">${mark(20)}<span>Nocturne</span></span><span class="pbarBtn" aria-label="Menu">${icon("menu", 22)}</span>`;
  screen.appendChild(bar);
  screen.appendChild(content);
  document.body.prepend(screen);
}

const overlay = document.getElementById("overlay");
if (overlay) document.body.appendChild(overlay);
