// The world around the phone: the computer (the real app on the mock runtime, through the
// stand-in helper), and a TLS front that plays Cloudflare's edge. Chromium is told that the
// computer's address is this front, so the phone app runs at its real `https://<address>` origin
// with real WebCrypto, and the stand-in helper records every byte it relays.

import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync } from "node:fs";
import net from "node:net";
import { tmpdir } from "node:os";
import { join } from "node:path";
import tls from "node:tls";
import { Driver, HOST } from "../interop/driver";

export { HOST };

export class Front {
  private server: tls.Server | null = null;
  private sockets = new Set<net.Socket>();
  /** Where the stand-in helper's public port is now. */
  target = 0;
  /** Client-to-computer bytes per second, or 0 for no limit. */
  rate = 0;
  port = 0;

  async start(): Promise<void> {
    const dir = mkdtempSync(join(tmpdir(), "away-front-"));
    const key = join(dir, "key.pem");
    const cert = join(dir, "cert.pem");
    execFileSync(
      "openssl",
      [
        "req",
        "-x509",
        "-newkey",
        "ec",
        "-pkeyopt",
        "ec_paramgen_curve:prime256v1",
        "-nodes",
        "-days",
        "2",
        "-subj",
        `/CN=${HOST}`,
        "-addext",
        `subjectAltName=DNS:${HOST}`,
        "-keyout",
        key,
        "-out",
        cert,
      ],
      { stdio: "ignore" },
    );
    const server = tls.createServer(
      { key: readFileSync(key), cert: readFileSync(cert), ALPNProtocols: ["http/1.1"] },
      (client) => {
        this.sockets.add(client);
        client.on("close", () => this.sockets.delete(client));
        client.on("error", () => client.destroy());
        const upstream = net.connect(this.target, "127.0.0.1");
        this.sockets.add(upstream);
        upstream.on("close", () => {
          this.sockets.delete(upstream);
          client.destroy();
        });
        upstream.on("error", () => client.destroy());
        client.on("close", () => upstream.destroy());
        client.on("data", (data: Buffer) => {
          upstream.write(data);
          if (this.rate > 0) {
            client.pause();
            setTimeout(() => client.resume(), (data.length / this.rate) * 1000);
          }
        });
        upstream.pipe(client);
      },
    );
    this.server = server;
    await new Promise<void>((done) => server.listen(0, "127.0.0.1", done));
    this.port = (server.address() as net.AddressInfo).port;
  }

  /** What Chromium needs to reach the computer's address through this front. */
  get args(): string[] {
    return [
      `--host-resolver-rules=MAP ${HOST} 127.0.0.1:${this.port}`,
      "--ignore-certificate-errors",
    ];
  }

  dropAll(): void {
    for (const s of this.sockets) s.destroy();
  }

  async close(): Promise<void> {
    this.dropAll();
    await new Promise<void>((done) => (this.server ? this.server.close(() => done()) : done()));
  }
}

export interface Computer {
  driver: Driver;
  front: Front;
  /** Everything the stand-in helper has relayed so far, as bytes. */
  relayed(): Buffer;
}

export async function startComputer(front: Front): Promise<Computer> {
  const driver = await Driver.start();
  front.target = driver.info.publicPort;
  return {
    driver,
    front,
    relayed: () => {
      try {
        return readFileSync(driver.relay);
      } catch {
        return Buffer.alloc(0);
      }
    },
  };
}
