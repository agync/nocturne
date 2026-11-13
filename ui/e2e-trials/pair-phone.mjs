// A trial pairing for a real phone (plan 12, D10 phone checks): the computer's side with demo
// data and the real cloudflared, a pairing code in this terminal and as a page in the trials
// folder, and the check words to compare before allowing the phone. Ctrl+C ends it and stops
// the helper. Nothing here touches the person's own Nocturne data.
//
//   TRIALS_DIR=<folder> node ui/e2e-trials/pair-phone.mjs

import { spawn } from "node:child_process";
import { Resolver } from "node:dns/promises";
import { mkdirSync, readdirSync, readlinkSync, statSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { createInterface } from "node:readline";
import { fileURLToPath } from "node:url";

const repo = resolve(dirname(fileURLToPath(import.meta.url)), "..", "..");
const trials = process.env.TRIALS_DIR;
if (!trials) throw new Error("TRIALS_DIR names the folder for the code page and the data");
const scratch = join(trials, "tmp-phone");
mkdirSync(scratch, { recursive: true, mode: 0o700 });

const deps = join(repo, "target", "debug", "deps");
const driver = readdirSync(deps)
  .filter((f) => f.startsWith("away_driver-") && !f.includes("."))
  .map((f) => join(deps, f))
  .sort((a, b) => statSync(b).mtimeMs - statSync(a).mtimeMs)[0];
if (!driver)
  throw new Error(
    "build it first: cargo test -p nocturne-app --features fake-helpers --test away_driver --no-run",
  );

const child = spawn(driver, ["--ignored", "--nocapture", "--exact", "drive", "--test-threads=1"], {
  cwd: join(repo, "src-tauri"),
  env: { ...process.env, AWAY_REAL: "simple", TMPDIR: scratch },
  stdio: ["pipe", "pipe", "ignore"],
  // Its own process group, so Ctrl+C reaches only this script, which then asks it to quit: a
  // driver ended by the signal would leave cloudflared running.
  detached: true,
});
let next = 1;
let buffer = "";
const waiting = new Map();
child.stdout.setEncoding("utf8");
child.stdout.on("data", (chunk) => {
  buffer += chunk;
  for (let at = buffer.indexOf("\n"); at >= 0; at = buffer.indexOf("\n")) {
    const line = buffer.slice(0, at);
    buffer = buffer.slice(at + 1);
    const from = line.indexOf("AWAYDRV ");
    if (from < 0) continue;
    const reply = JSON.parse(line.slice(from + 8));
    waiting.get(reply.id)?.(reply);
    waiting.delete(reply.id);
  }
});
const call = (cmd, args = {}) => {
  const id = next++;
  child.stdin.write(`${JSON.stringify({ id, cmd, ...args })}\n`);
  return new Promise((done, fail) =>
    waiting.set(id, (r) => (r.error !== undefined ? fail(new Error(r.error)) : done(r.ok))),
  );
};
await new Promise((done) => waiting.set(0, done));

function terminalQr(qr) {
  const n = qr.size;
  const at = (x, y) => x >= 0 && y >= 0 && x < n && y < n && qr.modules[y * n + x] === "1";
  const lines = [];
  for (let y = -2; y < n + 2; y += 2) {
    let row = "";
    for (let x = -2; x < n + 2; x++) {
      const top = at(x, y);
      const bottom = at(x, y + 1);
      row += top && bottom ? " " : top ? "▄" : bottom ? "▀" : "█";
    }
    lines.push(row);
  }
  return lines.join("\n");
}

function pageQr(qr, hint) {
  const n = qr.size;
  let rects = "";
  for (let y = 0; y < n; y++)
    for (let x = 0; x < n; x++)
      if (qr.modules[y * n + x] === "1")
        rects += `<rect x="${x + 4}" y="${y + 4}" width="1" height="1"/>`;
  return `<!doctype html><meta charset="utf-8"><title>Nocturne trial pairing</title>
<body style="font-family:sans-serif;background:#fff;color:#000;text-align:center">
<h1>Scan with the phone's camera</h1>
<svg viewBox="0 0 ${n + 8} ${n + 8}" width="420" height="420" shape-rendering="crispEdges">${rects}</svg>
<p>${hint}</p>
<p>The code works once and for five minutes. Press Enter in the terminal for a new one.</p></body>`;
}

const rl = createInterface({ input: process.stdin });
const lines = [];
rl.on("line", (l) => lines.push(l.trim().toLowerCase()));
const nextLine = async () => {
  while (!stopping && lines.length === 0) await new Promise((d) => setTimeout(d, 200));
  return lines.shift() ?? "";
};
let stopping = false;
async function stop() {
  if (stopping) return;
  stopping = true;
  rl.close();
  console.log("\nStopping…");
  // The driver answers in order: the quit waits behind a pending request, then it shuts the
  // helper down and exits.
  child.stdin.end(`${JSON.stringify({ id: 999_999, cmd: "quit" })}\n`);
  const exited = await Promise.race([
    new Promise((d) => child.once("exit", () => d(true))),
    new Promise((d) => setTimeout(() => d(false), 60_000)),
  ]);
  if (!exited) child.kill("SIGTERM");
  console.log(
    exited ? "Stopped. Nothing is left running." : "It did not stop by itself, so it was ended.",
  );
  process.exit(0);
}
process.on("SIGINT", () => {
  stop();
});

await call("seed_pdf");
console.log("Starting the connection (this downloads the pinned cloudflared once)…");
await call("on");
const host = String(await call("wait_on", { secs: 300 }));
const r = new Resolver();
r.setServers(["1.1.1.1"]);
for (let i = 0; i < 180; i++) {
  try {
    if ((await r.resolve4(host)).length) break;
  } catch {
    await new Promise((d) => setTimeout(d, 1000));
  }
}

const { dataDir } = await call("hello");

/** Give the computer a new address, as a restart of the connection does, and show its code. */
async function newAddress() {
  const helpers = join(dataDir, "remote", "helpers");
  for (const entry of readdirSync("/proc")) {
    if (!/^\d+$/.test(entry)) continue;
    try {
      if (readlinkSync(`/proc/${entry}/exe`).startsWith(`${helpers}/`))
        process.kill(Number(entry), "SIGKILL");
    } catch {
      // Not ours.
    }
  }
  for (let i = 0; i < 300; i++) {
    const s = await call("remote_state");
    if (s.type === "needsRelink") {
      writeFileSync(
        join(trials, "PAIRING-CODE.html"),
        pageQr(s.qr, `New address. Or type the words <b style="font-size:1.6em">${s.words}</b>`),
        {
          mode: 0o600,
        },
      );
      console.log(`\n${terminalQr(s.qr)}\n`);
      console.log(
        `New address. On the phone choose "Scan the new code", or type the words: ${s.words}`,
      );
      return;
    }
    await new Promise((d) => setTimeout(d, 1000));
  }
  console.log("The new address did not come up.");
}

while (!stopping) {
  const offer = await call("offer");
  const page = join(trials, "PAIRING-CODE.html");
  writeFileSync(
    page,
    pageQr(
      offer.qr,
      `Or open <b>https://${offer.typedAddress}</b> on the phone and type<br><b style="font-size:1.6em">${offer.typedCode}</b>`,
    ),
    { mode: 0o600 },
  );
  console.log(`\n${terminalQr(offer.qr)}\n`);
  console.log(`A bigger code: open ${page} in a browser on this computer.`);
  console.log(`Or type on the phone: https://${offer.typedAddress}  code ${offer.typedCode}`);
  console.log("Waiting for the phone (Enter for a new code, Ctrl+C to stop)…");
  lines.length = 0;
  let asked = null;
  while (!stopping && !asked && lines.length === 0) asked = await call("request").catch(() => null);
  if (stopping) break;
  if (!asked) {
    lines.shift();
    continue;
  }
  console.log(
    `\n"${asked.deviceName}" wants to connect. Its screen should show: ${asked.words.join(", ")}`,
  );
  process.stdout.write("Do the words match? Allow it? [y/N] ");
  const yes = (await nextLine()) === "y";
  await call("decide", { pairingId: asked.pairingId, allow: yes });
  console.log(
    yes
      ? "Allowed. Use the phone now. Enter adds another phone; a then Enter gives the computer a new address; Ctrl+C stops."
      : "Refused.",
  );
  while (yes && !stopping) {
    if ((await nextLine()) !== "a") break;
    await newAddress();
    console.log("Enter adds another phone; a then Enter gives another new address; Ctrl+C stops.");
  }
}
