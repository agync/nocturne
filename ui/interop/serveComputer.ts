// The phone app's own test computer (`FakeComputer`, written in TypeScript against WebCrypto)
// behind a real loopback socket, so the Rust test phone can talk to it over HTTP and WebSocket.
// It adds only the wire: the sealed channel is the TypeScript implementation's own.

import { createHash } from "node:crypto";
import { createServer, type IncomingMessage, type Server } from "node:http";
import type { AddressInfo, Socket } from "node:net";
import type { FakeComputer } from "../src/phone/testing/fakeComputer";

const GUID = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

function wsFrame(opcode: number, payload: Buffer): Buffer {
  const n = payload.length;
  const head =
    n < 126
      ? Buffer.from([0x80 | opcode, n])
      : n < 65536
        ? Buffer.from([0x80 | opcode, 126, n >> 8, n & 255])
        : (() => {
            const b = Buffer.alloc(10);
            b[0] = 0x80 | opcode;
            b[1] = 127;
            b.writeBigUInt64BE(BigInt(n), 2);
            return b;
          })();
  return Buffer.concat([head, payload]);
}

/** Parse the masked frames a client sent; returns what is left over. */
function parseFrames(buf: Buffer, on: (opcode: number, payload: Buffer) => void): Buffer {
  let at = 0;
  for (;;) {
    if (buf.length - at < 2) break;
    const first = buf[at] ?? 0;
    const second = buf[at + 1] ?? 0;
    const opcode = first & 0x0f;
    const masked = (second & 0x80) !== 0;
    let len = second & 0x7f;
    let head = 2;
    if (len === 126) {
      if (buf.length - at < 4) break;
      len = buf.readUInt16BE(at + 2);
      head = 4;
    } else if (len === 127) {
      if (buf.length - at < 10) break;
      len = Number(buf.readBigUInt64BE(at + 2));
      head = 10;
    }
    const total = head + (masked ? 4 : 0) + len;
    if (buf.length - at < total) break;
    const mask = masked ? buf.subarray(at + head, at + head + 4) : null;
    const payload = Buffer.from(buf.subarray(at + head + (masked ? 4 : 0), at + total));
    if (mask)
      for (let i = 0; i < payload.length; i++) payload[i] = (payload[i] ?? 0) ^ (mask[i % 4] ?? 0);
    on(opcode, payload);
    at += total;
  }
  return Buffer.from(buf.subarray(at));
}

export interface Served {
  port: number;
  /** WebSocket upgrades accepted so far. */
  upgrades: number;
  server: Server;
  close(): Promise<void>;
}

export async function serveComputer(computer: FakeComputer): Promise<Served> {
  const sockets = new Set<Socket>();
  const served = { upgrades: 0 };
  const server = createServer(async (req, res) => {
    const chunks: Buffer[] = [];
    for await (const c of req) chunks.push(c as Buffer);
    const ac = new AbortController();
    res.on("close", () => ac.abort());
    try {
      const response = await computer.fetch(`https://${computer.host}${req.url}`, {
        method: req.method,
        body: req.method === "POST" ? Buffer.concat(chunks) : undefined,
        signal: ac.signal,
      });
      const body = Buffer.from(await response.arrayBuffer());
      res.writeHead(response.status, { "content-length": body.length });
      res.end(body);
    } catch {
      res.destroy();
    }
  });
  server.on("connection", (s) => {
    sockets.add(s);
    s.on("close", () => sockets.delete(s));
  });
  server.on("upgrade", (req: IncomingMessage, socket: Socket) => {
    served.upgrades += 1;
    const key = req.headers["sec-websocket-key"];
    if (typeof key !== "string") return socket.destroy();
    const Ctor = computer.env.WebSocket;
    if (!Ctor) return socket.destroy();
    const peer = new Ctor(`wss://${computer.host}${req.url}`);
    peer.binaryType = "arraybuffer";
    const accept = createHash("sha1")
      .update(key + GUID)
      .digest("base64");
    let open = false;
    let pending: Buffer = Buffer.alloc(0);
    const toPeer: Buffer[] = [];
    peer.onopen = () => {
      open = true;
      for (const m of toPeer.splice(0)) peer.send(new Uint8Array(m));
    };
    peer.onmessage = (ev) => {
      socket.write(wsFrame(2, Buffer.from(ev.data as ArrayBuffer)));
    };
    peer.onclose = (ev) => {
      const code = (ev as { code?: number }).code ?? 1000;
      const payload = Buffer.alloc(2);
      payload.writeUInt16BE(code);
      socket.write(wsFrame(8, payload));
      socket.end();
    };
    socket.write(
      `HTTP/1.1 101 Switching Protocols\r\nupgrade: websocket\r\nconnection: Upgrade\r\nsec-websocket-accept: ${accept}\r\n\r\n`,
    );
    socket.on("data", (data: Buffer) => {
      pending = parseFrames(Buffer.concat([pending, data]), (opcode, payload) => {
        if (opcode === 2) {
          if (open) peer.send(new Uint8Array(payload));
          else toPeer.push(payload);
        } else if (opcode === 9) socket.write(wsFrame(10, payload));
        else if (opcode === 8) {
          peer.close();
          socket.end();
        }
      });
    });
    socket.on("close", () => peer.close());
    socket.on("error", () => peer.close());
    sockets.add(socket);
    socket.on("close", () => sockets.delete(socket));
  });
  await new Promise<void>((done) => server.listen(0, "127.0.0.1", done));
  return {
    get upgrades() {
      return served.upgrades;
    },
    port: (server.address() as AddressInfo).port,
    server,
    close: () =>
      new Promise<void>((done) => {
        computer.dropAll();
        for (const s of sockets) s.destroy();
        server.close(() => done());
      }),
  };
}
