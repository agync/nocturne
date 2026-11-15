// Wraps a mockup's content in the app's frame. ?theme=dark or ?theme=light picks the theme.
// <body data-frame="app" data-section="Privacy"> draws the rail and keep bar around the sheet;
// <body data-frame="window"> centres a card the way setup does; data-night forces dark.

const ICON = {
  lock: '<rect width="18" height="11" x="3" y="11" rx="2" ry="2"/><path d="M7 11V7a5 5 0 0 1 10 0v4"/>',
  paperclip:
    '<path d="m16 6-8.414 8.586a2 2 0 0 0 2.829 2.829l8.414-8.586a4 4 0 1 0-5.657-5.657l-8.379 8.551a6 6 0 1 0 8.485 8.485l8.379-8.551"/>',
  mic: '<path d="M12 19v3"/><path d="M19 10v2a7 7 0 0 1-14 0v-2"/><rect x="9" y="2" width="6" height="13" rx="3"/>',
  "chevron-down": '<path d="m6 9 6 6 6-6"/>',
  "chevron-right": '<path d="m9 18 6-6-6-6"/>',
  x: '<path d="M18 6 6 18"/><path d="m6 6 12 12"/>',
  check: '<path d="M20 6 9 17l-5-5"/>',
};

function icon(name, size = 20) {
  return `<svg width="${size}" height="${size}" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="${(1.5 * 24) / size}" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${ICON[name]}</svg>`;
}

function mark(size = 20, horizon = 0.5) {
  const r = 11.25;
  const y = 24 * horizon;
  const dy = y - 12;
  const w = Math.sqrt(Math.max(0, r * r - dy * dy));
  const large = y < 12 ? 1 : 0;
  const stroke = (1.5 * 24) / size;
  return `<svg class="mark" viewBox="0 0 24 24" width="${size}" height="${size}" aria-hidden="true"><path d="M ${12 - w} ${y} A ${r} ${r} 0 ${large} 0 ${12 + w} ${y} Z" fill="currentColor"/><circle cx="12" cy="12" r="${r}" fill="none" stroke="currentColor" stroke-width="${stroke}"/></svg>`;
}

const params = new URLSearchParams(location.search);

// <x data-state="a b"> shows only in states a and b. ?state= picks one; the first listed is the default.
const states = (document.body.dataset.states || "").split(" ").filter(Boolean);
const state = params.get("state") || states[0];
for (const el of document.querySelectorAll("[data-state]")) {
  if (!el.dataset.state.split(" ").includes(state)) el.remove();
}

const theme = params.get("theme");
if (document.body.dataset.night !== undefined) {
  document.documentElement.dataset.theme = "dark";
} else if (theme === "dark" || theme === "light") {
  document.documentElement.dataset.theme = theme;
}

for (const el of document.querySelectorAll("[data-icon]")) {
  el.outerHTML = icon(el.dataset.icon, Number(el.dataset.size || 20));
}
for (const el of document.querySelectorAll("[data-mark]")) {
  el.outerHTML = mark(Number(el.dataset.size || 20), Number(el.dataset.mark || 0.5));
}

const frame = document.body.dataset.frame;
const content = document.getElementById("content");

if (frame === "app") {
  const section = document.body.dataset.section || "Privacy";
  const main = [
    ["Today", '<span class="mono">Sun</span>'],
    ["Needs you", '<span class="count">2</span>'],
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
}

// A dialog mockup shows its panel over the frame.
const dialog = document.getElementById("dialog");
if (dialog) document.body.appendChild(dialog);
