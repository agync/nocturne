// Starts the computer's side (the real app on the mock runtime, the scripted mind, the stand-in
// helper) as `src-tauri/tests/away_driver.rs` and talks to it over standard input and output.

import { type ChildProcessWithoutNullStreams, spawn } from "node:child_process";
import { createHash } from "node:crypto";
import {
  createWriteStream,
  mkdtempSync,
  readdirSync,
  readFileSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

export const repo = resolve(dirname(fileURLToPath(import.meta.url)), "..", "..");
export const HOST = "amber-otter-lamp-quiet.trycloudflare.com";

/** The newest built test program named `name`, or `AWAY_<NAME>` when set. */
export function builtTest(name: string): string {
  const fromEnv = process.env[`AWAY_${name.toUpperCase()}`];
  if (fromEnv) return fromEnv;
  const deps = join(repo, "target", "debug", "deps");
  const found = readdirSync(deps)
    .filter((f) => f.startsWith(`${name}-`) && !f.includes("."))
    .map((f) => join(deps, f))
    .sort((a, b) => statSync(b).mtimeMs - statSync(a).mtimeMs);
  if (!found[0])
    throw new Error(`${name} is not built: cargo test --features fake-helpers --no-run`);
  return found[0];
}

export function fakeCloudflared(): string {
  return process.env.AWAY_FAKE_CLOUDFLARED ?? join(repo, "target", "debug", "fake-cloudflared");
}

/** A lock that pins the stand-in, as the network check writes one. */
export function writeLock(dir: string, binary: string): string {
  const sha = createHash("sha256").update(readFileSync(binary)).digest("hex");
  let text = `[[helpers]]\nkind = "cloudflared"\nversion = "fake"\nlicence = "Apache-2.0"\npinned_at = "2000-01-01"\nbase_url = "https://example.invalid"\n`;
  for (const platform of ["linux-x86_64", "linux-aarch64", "macos-aarch64", "windows-x86_64"]) {
    text += `\n[helpers.assets.${platform}]\nfile = "fake"\nsha256 = "${sha}"\n`;
  }
  const path = join(dir, "fake.lock");
  writeFileSync(path, text);
  return path;
}

type Reply = { id: number; ok?: unknown; error?: string };

export class Driver {
  private next = 1;
  private waiting = new Map<number, (r: Reply) => void>();
  private buffer = "";
  readonly exited: Promise<number | null>;
  info!: { host: string; publicPort: number; socket: string; dataDir: string; state: string };
  relay!: string;

  private constructor(private readonly child: ChildProcessWithoutNullStreams) {
    child.stdout.setEncoding("utf8");
    child.stdout.on("data", (chunk: string) => {
      this.buffer += chunk;
      for (;;) {
        const at = this.buffer.indexOf("\n");
        if (at < 0) break;
        const line = this.buffer.slice(0, at);
        this.buffer = this.buffer.slice(at + 1);
        const from = line.indexOf("AWAYDRV ");
        if (from < 0) continue;
        const reply = JSON.parse(line.slice(from + 8)) as Reply;
        this.waiting.get(reply.id)?.(reply);
        this.waiting.delete(reply.id);
      }
    });
    this.exited = new Promise((done) => child.on("exit", (code) => done(code)));
  }

  /**
   * `env.AWAY_REAL` starts the real helper instead of the stand-in (the owner-approved trials in
   * `e2e-trials`); `wrap` runs the driver under another program (strace for a network trace);
   * `on: false` leaves Away from home off until the caller says "on".
   */
  static async start(
    env: Record<string, string> = {},
    opts: { wrap?: string[]; on?: boolean } = {},
  ): Promise<Driver> {
    const scratch = mkdtempSync(join(tmpdir(), "away-"));
    const real = env.AWAY_REAL !== undefined;
    const binary = fakeCloudflared();
    const stand: Record<string, string> = real
      ? {}
      : { AWAY_FAKE_CLOUDFLARED: binary, NOCTURNE_FAKE_HELPERS_LOCK: writeLock(scratch, binary) };
    const args = ["--ignored", "--nocapture", "--exact", "drive", "--test-threads=1"];
    const [program, ...before] = [...(opts.wrap ?? []), builtTest("away_driver")];
    const child = spawn(program ?? "", [...before, ...args], {
      cwd: join(repo, "src-tauri"),
      env: { ...process.env, ...stand, ...env },
      stdio: ["pipe", "pipe", "pipe"],
    });
    const log = createWriteStream(join(scratch, "driver.log"));
    child.stderr.pipe(log);
    const d = new Driver(child);
    const ready = new Promise<Reply>((done, fail) => {
      d.waiting.set(0, done);
      d.exited.then((code) => fail(new Error(`the driver exited early (${code}); see ${scratch}`)));
    });
    const first = await ready;
    d.relay = String((first.ok as { relay: string }).relay);
    if (opts.on !== false) await d.call("on");
    d.info = (await d.call("hello")) as typeof d.info;
    return d;
  }

  call(cmd: string, args: Record<string, unknown> = {}): Promise<unknown> {
    const id = this.next++;
    this.child.stdin.write(`${JSON.stringify({ id, cmd, ...args })}\n`);
    return new Promise((done, fail) => {
      this.waiting.set(id, (r) => (r.error !== undefined ? fail(new Error(r.error)) : done(r.ok)));
    });
  }

  get pid(): number | undefined {
    return this.child.pid;
  }

  async stop(): Promise<void> {
    try {
      await Promise.race([this.call("quit"), this.exited]);
    } catch {
      // The driver is already gone.
    }
    const timer = setTimeout(() => this.child.kill("SIGKILL"), 10_000);
    await this.exited;
    clearTimeout(timer);
  }
}
