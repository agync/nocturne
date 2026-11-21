// Draws the health forms into <x data-chart="kind" data-args='{...}' data-say="...">.
// Runs before ../phone-apps/shell.js, so the phone frames copy finished charts.
// The forms are fixed: one colour, no good or bad colours, the person's own usual as a band.

const NS = "http://www.w3.org/2000/svg";
const AXIS_HOURS = [["10 pm", 2], ["12 am", 4], ["2 am", 6], ["4 am", 8], ["6 am", 10], ["8 am", 12], ["10 am", 14]];

// Hours after 8 pm, so a night runs left to right from 0 to 16.
function h(t) {
  const [hh, mm] = t.split(":").map(Number);
  const v = hh + mm / 60 - 20;
  return v < 0 ? v + 24 : v;
}

function svg(w, hgt, body, say) {
  return `<svg class="hfSvg" viewBox="0 0 ${w} ${hgt}" role="img" aria-label="${say || ""}" xmlns="${NS}">${body}</svg>`;
}

const DRAW = {
  night(a, say) {
    const w = a.w || 520, x0 = a.x0 || 0, span = w - x0, px = span / 16, y = 8, bh = a.thick || 12;
    const x = (t) => x0 + h(t) * px;
    let b = `<rect class="h-track" x="${x0}" y="${y}" width="${span}" height="${bh}" rx="${bh / 2}"/>`;
    const parts = [];
    let start = a.bed;
    for (const [g0, g1] of a.gaps || []) { parts.push([start, g0]); start = g1; }
    parts.push([start, a.wake]);
    for (const [s, e] of parts) b += `<rect class="h-bar" x="${x(s)}" y="${y}" width="${x(e) - x(s)}" height="${bh}" rx="3"/>`;
    if (a.usual) for (const t of a.usual) b += `<line class="h-usual" x1="${x(t)}" y1="${y - 4}" x2="${x(t)}" y2="${y + bh + 4}"/>`;
    if (a.axis !== false) {
      const step = a.compact ? 2 : 1;
      AXIS_HOURS.filter((_, i) => i % step === 0).forEach(([l, v]) => (b += `<text class="h-axis" x="${x0 + v * px}" y="${y + bh + 16}" text-anchor="middle">${l}</text>`));
    }
    return svg(w, y + bh + (a.axis === false ? 6 : 20), b, say);
  },

  nights(a, say) {
    const w = a.w || 520, x0 = 52, span = w - x0, px = span / 16, rh = a.rh || 24, top = 4;
    const x = (t) => x0 + h(t) * px;
    const n = a.rows.length;
    let b = "";
    if (a.usual) b += `<rect class="h-band" x="${x(a.usual[0])}" y="0" width="${x(a.usual[1]) - x(a.usual[0])}" height="${n * rh + top}" rx="4"/>`;
    a.rows.forEach((r, i) => {
      const y = top + i * rh + rh / 2 - 5;
      b += `<text class="h-axis" x="0" y="${y + 9}">${r.d}</text>`;
      b += `<rect class="h-track" x="${x0}" y="${y}" width="${span}" height="10" rx="5"/>`;
      let start = r.bed;
      const parts = [];
      for (const [g0, g1] of r.gaps || []) { parts.push([start, g0]); start = g1; }
      parts.push([start, r.wake]);
      for (const [s, e] of parts) b += `<rect class="h-bar${r.mark ? " ink" : ""}" x="${x(s)}" y="${y}" width="${x(e) - x(s)}" height="10" rx="3"/>`;
      if (r.entry) b += `<circle class="h-entry" cx="${w - 4}" cy="${y + 5}" r="2.5"/>`;
    });
    const ay = top + n * rh + 14;
    AXIS_HOURS.filter((_, i) => i % 2 === 0).forEach(([l, v]) => (b += `<text class="h-axis" x="${x0 + v * px}" y="${ay}" text-anchor="middle">${l}</text>`));
    return svg(w, ay + 4, b, say);
  },

  line(a, say) {
    const w = a.w || 520, hgt = a.h || 110, pad = 8, bottom = hgt - 22, topY = 14;
    const n = a.values.length, lo = a.min, hi = a.max;
    const X = (i) => pad + (i * (w - pad * 2)) / (n - 1);
    const Y = (v) => bottom - ((v - lo) / (hi - lo)) * (bottom - topY);
    let b = "";
    if (a.band) b += `<rect class="h-band" x="${pad}" y="${Y(a.band[1])}" width="${w - pad * 2}" height="${Y(a.band[0]) - Y(a.band[1])}" rx="3"/>`;
    const pts = a.values.map((v, i) => (v == null ? null : `${X(i)},${Y(v)}`)).filter(Boolean).join(" ");
    b += `<polyline class="h-line" points="${pts}"/>`;
    for (const i of a.entries || []) b += `<line class="h-tick" x1="${X(i)}" y1="${bottom + 4}" x2="${X(i)}" y2="${bottom + 9}"/>`;
    const at = a.at == null ? n - 1 : a.at;
    b += `<circle class="h-dot" cx="${X(at)}" cy="${Y(a.values[at])}" r="4"/>`;
    if (a.ring != null) b += `<circle class="h-ring" cx="${X(a.ring)}" cy="${Y(a.values[a.ring])}" r="7"/>`;
    b += `<text class="h-val" x="${X(at) - 8}" y="${Y(a.values[at]) - 9}" text-anchor="end">${a.values[at]}</text>`;
    if (a.band) b += `<text class="h-axis" x="${pad + 4}" y="${Y(a.band[1]) - 4}">your usual</text>`;
    if (a.labels) a.labels.forEach(([i, l]) => (b += `<text class="h-axis" x="${X(i)}" y="${hgt - 2}" text-anchor="middle">${l}</text>`));
    return svg(w, hgt, b, say);
  },

  range(a, say) {
    const w = a.w || 520, x0 = 0, min = a.min || 40, max = a.max || 180;
    const X = (v) => x0 + ((v - min) / (max - min)) * (w - x0);
    let b = `<rect class="h-track" x="${x0}" y="6" width="${w - x0}" height="12" rx="6"/>`;
    if (a.band) b += `<rect class="h-band" x="${X(a.band[0])}" y="2" width="${X(a.band[1]) - X(a.band[0])}" height="20" rx="4"/>`;
    b += `<rect class="h-bar" x="${X(a.lo)}" y="6" width="${X(a.hi) - X(a.lo)}" height="12" rx="4"/>`;
    b += `<line class="h-mid" x1="${X(a.mid)}" y1="4" x2="${X(a.mid)}" y2="20"/>`;
    [min, 80, 120, 160].forEach((v) => (b += `<text class="h-axis" x="${X(v)}" y="36" text-anchor="${v === min ? "start" : "middle"}">${v}</text>`));
    return svg(w, 40, b, say);
  },

  hours(a, say) {
    const w = a.w || 520, hgt = a.h || 170, bottom = hgt - 18, topY = 6, min = a.min || 40, max = a.max || 160;
    const n = a.rows.length, step = w / n;
    const Y = (v) => bottom - ((v - min) / (max - min)) * (bottom - topY);
    let b = "";
    a.rows.forEach(([lo, hi, mid], i) => {
      if (lo == null) return;
      const x = i * step + step / 2;
      b += `<rect class="h-bar${a.workout && a.workout.includes(i) ? " ink" : ""}" x="${x - 3}" y="${Y(hi)}" width="6" height="${Y(lo) - Y(hi)}" rx="3"/>`;
      b += `<line class="h-mid" x1="${x - 4}" y1="${Y(mid)}" x2="${x + 4}" y2="${Y(mid)}"/>`;
    });
    [[0, "12 am"], [6, "6 am"], [12, "12 pm"], [18, "6 pm"]].forEach(([i, l]) => (b += `<text class="h-axis" x="${i * step + step / 2}" y="${hgt - 2}" text-anchor="middle">${l}</text>`));
    return svg(w, hgt, b, say);
  },

  bp(a, say) {
    const w = a.w || 520, hgt = a.h || 150, bottom = hgt - 20, topY = 8, min = a.min || 60, max = a.max || 150;
    const n = a.days, X = (i) => 36 + (i * (w - 50)) / (n - 1);
    const Y = (v) => bottom - ((v - min) / (max - min)) * (bottom - topY);
    let b = "";
    for (const band of [a.bandTop, a.bandBottom]) if (band) b += `<rect class="h-band" x="0" y="${Y(band[1])}" width="${w}" height="${Y(band[0]) - Y(band[1])}" rx="3"/>`;
    for (const r of a.readings) {
      b += `<line class="h-pair" x1="${X(r.d)}" y1="${Y(r.top)}" x2="${X(r.d)}" y2="${Y(r.bottom)}"/>`;
      b += `<circle class="h-dot" cx="${X(r.d)}" cy="${Y(r.top)}" r="3.5"/><circle class="h-dot" cx="${X(r.d)}" cy="${Y(r.bottom)}" r="3.5"/>`;
      if (r.ring) b += `<circle class="h-ring" cx="${X(r.d)}" cy="${Y(r.top)}" r="7"/>`;
    }
    for (const i of a.entries || []) b += `<line class="h-tick" x1="${X(i)}" y1="${bottom + 4}" x2="${X(i)}" y2="${bottom + 9}"/>`;
    if (a.labels) a.labels.forEach(([i, l]) => (b += `<text class="h-axis" x="${X(i)}" y="${hgt - 2}" text-anchor="middle">${l}</text>`));
    [80, 120].forEach((v) => (b += `<text class="h-axis" x="0" y="${Y(v) + 3}">${v}</text>`));
    return svg(w, hgt, b, say);
  },

  bars(a, say) {
    const w = a.w || 520, hgt = a.h || 90, bottom = hgt - 18, topY = 16, max = a.max;
    const n = a.values.length, step = w / n, bw = Math.min(28, step * 0.62);
    const Y = (v) => bottom - (v / max) * (bottom - topY);
    const today = a.at == null ? n - 1 : a.at;
    let b = "";
    a.values.forEach((v, i) => {
      const x = i * step + (step - bw) / 2;
      b += `<rect class="h-bar ${i === today ? "today" : "soft"}" x="${x}" y="${Y(v)}" width="${bw}" height="${bottom - Y(v)}" rx="3"/>`;
      if ((a.entries || []).includes(i)) b += `<circle class="h-entry" cx="${x + bw / 2}" cy="${bottom + 6}" r="2"/>`;
      if (a.labels) b += `<text class="h-axis" x="${x + bw / 2}" y="${hgt - 2}" text-anchor="middle">${a.labels[i]}</text>`;
    });
    b += `<text class="h-val" x="${today * step + step / 2}" y="${Y(a.values[today]) - 5}" text-anchor="middle">${a.values[today].toLocaleString("en-GB")}</text>`;
    return svg(w, hgt, b, say);
  },
};

for (const el of document.querySelectorAll("[data-chart]")) {
  const args = JSON.parse(el.dataset.args || "{}");
  const tmp = document.createElement("div");
  tmp.innerHTML = DRAW[el.dataset.chart](args, el.dataset.say || "");
  const node = tmp.firstElementChild;
  for (const k of ["state", "os"]) if (el.dataset[k]) node.dataset[k] = el.dataset[k];
  el.replaceWith(node);
}
