// The real desktop app, a debug build with the scripted mind and the stand-in helper, run under
// a virtual display and driven through the web view's own WebDriver (the same way a person's
// clicks reach it: through the app's commands). Used by `real-app.spec.ts` (I5, Linux).

import { type ChildProcess, spawn } from "node:child_process";
import {
  chmodSync,
  copyFileSync,
  mkdirSync,
  mkdtempSync,
  readdirSync,
  readFileSync,
  readlinkSync,
  realpathSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fakeCloudflared, HOST, writeLock } from "../interop/driver";

const sleep = (ms: number) => new Promise((done) => setTimeout(done, ms));

export function pidsRunning(under: string[]): Array<{ pid: number; exe: string }> {
  const found: Array<{ pid: number; exe: string }> = [];
  for (const entry of readdirSync("/proc")) {
    if (!/^\d+$/.test(entry)) continue;
    try {
      const exe = readlinkSync(`/proc/${entry}/exe`);
      if (under.some((u) => exe === u || exe.startsWith(`${u}/`)))
        found.push({ pid: Number(entry), exe });
    } catch {
      // Gone, or not ours to read.
    }
  }
  return found;
}

export function rssKb(pid: number): number {
  const m = /VmRSS:\s+(\d+) kB/.exec(readFileSync(`/proc/${pid}/status`, "utf8"));
  return m ? Number(m[1]) : 0;
}

function children(pid: number): number[] {
  const out: number[] = [];
  for (const entry of readdirSync("/proc")) {
    if (!/^\d+$/.test(entry)) continue;
    try {
      const stat = readFileSync(`/proc/${entry}/stat`, "utf8");
      const ppid = Number(stat.slice(stat.lastIndexOf(")") + 2).split(" ")[1]);
      if (ppid === pid) out.push(Number(entry));
    } catch {
      // Gone.
    }
  }
  return out;
}

export function treeRssKb(pid: number): number {
  return rssKb(pid) + children(pid).reduce((sum, c) => sum + treeRssKb(c), 0);
}

/** Listening sockets held by a process: TCP as `addr:port`, Unix by path. */
export function listeners(pid: number): string[] {
  const inodes = new Set<string>();
  for (const fd of readdirSync(`/proc/${pid}/fd`)) {
    try {
      const m = /^socket:\[(\d+)\]$/.exec(readlinkSync(`/proc/${pid}/fd/${fd}`));
      if (m?.[1]) inodes.add(m[1]);
    } catch {
      // Closed meanwhile.
    }
  }
  const out: string[] = [];
  for (const file of ["tcp", "tcp6"]) {
    for (const line of readFileSync(`/proc/net/${file}`, "utf8").split("\n").slice(1)) {
      const f = line.trim().split(/\s+/);
      if (f[3] === "0A" && f[9] && inodes.has(f[9])) out.push(`${file}:${f[1]}`);
    }
  }
  for (const line of readFileSync("/proc/net/unix", "utf8").split("\n").slice(1)) {
    const f = line.trim().split(/\s+/);
    // Flag 00010000 is a listening socket.
    if (f[3] === "00010000" && f[6] && inodes.has(f[6])) out.push(`unix:${f[7] ?? ""}`);
  }
  return out.sort();
}

export class RealApp {
  readonly scratch = mkdtempSync(join(tmpdir(), "away-real-"));
  readonly data = join(this.scratch, "data");
  readonly helperDir = join(this.data, "remote", "helpers", "cloudflared", "fake");
  readonly relay = join(this.scratch, "relay.bin");
  pid = 0;
  private procs: ChildProcess[] = [];
  private session = "";
  private base = "";

  readonly binary: string;

  constructor(
    binary: string,
    readonly display = `:${200 + (process.pid % 700)}`,
  ) {
    this.binary = realpathSync(binary);
  }

  private spawnGroup(cmd: string, args: string[], env: NodeJS.ProcessEnv): ChildProcess {
    const child = spawn(cmd, args, { env, detached: true, stdio: ["ignore", "pipe", "pipe"] });
    child.stdout?.resume();
    child.stderr?.resume();
    this.procs.push(child);
    return child;
  }

  async start(): Promise<void> {
    mkdirSync(this.helperDir, { recursive: true, mode: 0o700 });
    for (const p of [
      join(this.data, "remote"),
      join(this.data, "remote", "helpers"),
      join(this.data, "remote", "helpers", "cloudflared"),
      this.helperDir,
    ])
      chmodSync(p, 0o700);
    const fake = fakeCloudflared();
    copyFileSync(fake, join(this.helperDir, "cloudflared"));
    writeFileSync(
      join(this.helperDir, "fake-control.json"),
      JSON.stringify({ address: HOST, record_relay: this.relay }),
    );
    const lock = writeLock(this.scratch, fake);
    const home = join(this.scratch, "home");
    mkdirSync(join(this.scratch, "runtime"), { mode: 0o700 });
    mkdirSync(home);
    const env: NodeJS.ProcessEnv = {
      PATH: process.env.PATH,
      HOME: home,
      XDG_CONFIG_HOME: join(home, ".config"),
      XDG_DATA_HOME: join(home, ".local", "share"),
      XDG_CACHE_HOME: join(home, ".cache"),
      XDG_RUNTIME_DIR: join(this.scratch, "runtime"),
      DISPLAY: this.display,
      NOCTURNE_DATA_DIR: this.data,
      NOCTURNE_FAKE_MIND: "1",
      TAURI_WEBVIEW_AUTOMATION: "true",
      NOCTURNE_FAKE_HELPERS_LOCK: lock,
      LIBGL_ALWAYS_SOFTWARE: "1",
      WEBKIT_DISABLE_DMABUF_RENDERER: "1",
      WEBKIT_DISABLE_COMPOSITING_MODE: "1",
    };
    this.spawnGroup("Xvfb", [this.display, "-screen", "0", "1280x800x24", "-nolisten", "tcp"], env);
    await sleep(1500);
    const port = 4400 + (process.pid % 500);
    this.spawnGroup("dbus-run-session", ["--", "WebKitWebDriver", `--port=${port}`], env);
    this.base = `http://127.0.0.1:${port}`;
    for (let i = 0; i < 50; i++) {
      try {
        if ((await fetch(`${this.base}/status`)).ok) break;
      } catch {
        await sleep(200);
      }
    }
    const created = (await this.wd("POST", "/session", {
      capabilities: {
        alwaysMatch: {
          browserName: "wry",
          "webkitgtk:browserOptions": { binary: this.binary, args: [] },
        },
      },
    })) as { sessionId: string; capabilities: Record<string, unknown> };
    this.session = created.sessionId;
    await this.wd("POST", `/session/${this.session}/timeouts`, { script: 120_000 });
    this.pid = Number(created.capabilities["webkit:processID"] ?? 0);
    if (!this.pid) throw new Error("the app did not start");
  }

  private async wd(method: string, path: string, body?: unknown): Promise<unknown> {
    const r = await fetch(`${this.base}${path}`, {
      method,
      headers: { "content-type": "application/json" },
      body: body === undefined ? undefined : JSON.stringify(body),
      signal: AbortSignal.timeout(150_000),
    });
    const json = (await r.json()) as { value: unknown };
    if (!r.ok) throw new Error(`webdriver ${method} ${path}: ${JSON.stringify(json.value)}`);
    return json.value;
  }

  /** Call one of the app's commands as its own interface does. */
  async invoke(cmd: string, args: Record<string, unknown> = {}): Promise<unknown> {
    const out = (await this.wd("POST", `/session/${this.session}/execute/async`, {
      script: `const done = arguments[arguments.length - 1];
        window.__TAURI_INTERNALS__.invoke(arguments[0], arguments[1])
          .then((v) => done({ ok: v }), (e) => done({ err: JSON.stringify(e) }));`,
      args: [cmd, args],
    })) as { ok?: unknown; err?: string };
    if (out.err !== undefined) throw new Error(`${cmd}: ${out.err}`);
    return out.ok;
  }

  get logs(): string {
    const dir = join(this.data, "logs");
    try {
      return readdirSync(dir)
        .map((f) => readFileSync(join(dir, f), "utf8"))
        .join("\n");
    } catch {
      return "";
    }
  }

  get publicPort(): number {
    return Number(readFileSync(join(this.helperDir, "fake-public-port"), "utf8"));
  }

  /** Quit the app as the system would, by its exact pid, and wait for it to be gone. */
  async quit(): Promise<number> {
    const started = Date.now();
    process.kill(this.pid, "SIGTERM");
    while (Date.now() - started < 20_000 && pidsRunning([this.binary]).length > 0) await sleep(100);
    if (pidsRunning([this.binary]).length > 0) process.kill(this.pid, "SIGKILL");
    return Date.now() - started;
  }

  async close(): Promise<void> {
    for (const p of this.procs.reverse()) {
      if (p.pid) {
        try {
          process.kill(-p.pid, "SIGTERM");
        } catch {
          // Already gone.
        }
      }
    }
    await sleep(500);
    rmSync(this.scratch, { recursive: true, force: true });
  }
}
