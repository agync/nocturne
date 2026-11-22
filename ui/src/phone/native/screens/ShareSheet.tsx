import { useEffect, useRef, useState } from "react";
import { formatBytes } from "../../../copy/format";
import { t } from "../../../copy/t";
import type { Feeling } from "../../../engine/bindings";
import { sniffMime } from "../../crypto";
import {
  type Connection,
  ConnectionError,
  type PendingKeep,
  type PhoneEngine,
  PhoneRequestError,
  type Queue,
} from "../../engine";
import { MAX_CAPTION, MAX_ITEM, MAX_TEXT } from "../../protocol";
import { withKeepLock } from "../../queue";
import { BottomSheet } from "../../screens/Dialog";
import { FeelingPick, feelingOfPick, type Pick } from "../../screens/Feeling";
import { clientId, WAIT_CODES } from "../../screens/Keep";
import { Button, Icon } from "../../screens/ui";
import { type Shared, type SharedFile, sharedDone, sharedFile } from "../launch";

type Problem = "big" | "kind" | "failed";
type State =
  | { name: "compose" }
  | { name: "sending" }
  | { name: "kept" }
  /** `newer`: it waits until the computer has a newer Nocturne. */
  | { name: "waiting"; newer?: boolean }
  | { name: "problem"; kind: Problem };

const PROBLEM = {
  big: ["phone.keep.big.title", "phone.keep.big.body"],
  kind: ["phone.keep.kind.title", "phone.keep.kind.body"],
  failed: ["phone.keep.failed.title", "phone.keep.failed.body"],
} as const;

/** A shared page or link: the computer keeps it as untrusted web content. */
export const isWeb = (shared: Shared): boolean =>
  /\bhttps?:\/\/\S/i.test(`${shared.subject ?? ""} ${shared.text ?? ""}`);

/** What a shared file is kept as, from its first bytes; null when Nocturne can't keep it. */
async function kindOfBytes(blob: Blob): Promise<"photo" | "recording" | null> {
  const mime = sniffMime(new Uint8Array(await blob.slice(0, 32).arrayBuffer()));
  if (!mime) return null;
  return mime.startsWith("audio/") ? "recording" : "photo";
}

const encoder = new TextEncoder();

function kindOf(file: SharedFile): string {
  if (file.mime.startsWith("image/")) return t("kind.image");
  if (file.mime.startsWith("audio/")) return t("kind.audio");
  return t("kind.document");
}

/** The text with the note before it, so a shared link keeps what the person said about it. */
function textToKeep(shared: Shared, note: string): string {
  const parts = [note.trim(), shared.subject?.trim() ?? "", shared.text ?? ""].filter(
    (part, i, all) => part !== "" && !(i === 1 && all[2]?.includes(part)),
  );
  return parts.join("\n\n");
}

/**
 * What a share keeps, each file checked for its kind and size first, one at a time; or why
 * nothing can be kept. Throws when a file can no longer be read.
 */
export async function sharedKeeps(
  shared: Shared,
  note: string,
  feeling: Feeling | undefined,
  now: () => number,
): Promise<PendingKeep[] | Problem> {
  const web = shared.files.length === 0 && isWeb(shared);
  const marks = { ...(feeling ? { feeling } : {}), ...(web ? { web } : {}) };
  if (shared.files.length === 0) {
    const text = textToKeep(shared, note);
    const size = encoder.encode(text).length;
    if (size > MAX_TEXT) return "big";
    return [{ clientId: clientId(), kind: "text", text, ...marks, size, createdAt: now() }];
  }
  const caption = note.trim().slice(0, MAX_CAPTION) || undefined;
  const keeps: PendingKeep[] = [];
  for (const file of shared.files) {
    if (file.size > MAX_ITEM) return "big";
    const blob = await sharedFile(file);
    if (blob.size > MAX_ITEM) return "big";
    const kind = await kindOfBytes(blob);
    if (!kind) return "kind";
    keeps.push({
      clientId: clientId(),
      kind,
      blob,
      caption: kind === "photo" ? caption : undefined,
      ...marks,
      size: blob.size,
      createdAt: now(),
    });
  }
  return keeps;
}

/**
 * A share that arrives while this phone is not paired, or was removed: it waits on the phone
 * like any keep, and nothing opens over the screen.
 */
export async function queueShared(shared: Shared, queue: Queue, now: () => number) {
  const made = await sharedKeeps(shared, "", undefined, now).catch(() => "failed" as const);
  if (typeof made !== "string") for (const k of made) await queue.add(k);
  await sharedDone();
}

/**
 * "Keep in Nocturne": what another app shared, with a note and how it feels. It waits on this
 * phone when the computer is away, as everything kept here does.
 */
export function ShareSheet({
  shared,
  engine,
  queue,
  connection,
  now,
  onQueued,
  onClose,
  alone = false,
}: {
  shared: Shared;
  engine: PhoneEngine;
  queue: Queue;
  connection: Connection;
  now(): number;
  onQueued(): void;
  onClose(): void;
  /** A phone alone: kept into its own memory, with no computer to read it. */
  alone?: boolean;
}) {
  const [state, setState] = useState<State>({ name: "compose" });
  const [note, setNote] = useState("");
  const [feel, setFeel] = useState<Pick | null>(null);
  const [preview, setPreview] = useState<string | null>(null);
  const heading = useRef<HTMLHeadingElement>(null);
  const first = shared.files[0];

  useEffect(() => {
    if (!first?.mime.startsWith("image/")) return;
    let url: string | null = null;
    let gone = false;
    sharedFile(first).then(
      (blob) => {
        if (gone) return;
        url = URL.createObjectURL(blob);
        setPreview(url);
      },
      () => undefined,
    );
    return () => {
      gone = true;
      if (url) URL.revokeObjectURL(url);
    };
  }, [first]);

  useEffect(() => {
    if (state.name !== "compose" && state.name !== "sending") heading.current?.focus();
  }, [state.name]);

  const close = () => {
    void sharedDone();
    onClose();
  };

  const keep = async () => {
    if (state.name !== "compose") return;
    setState({ name: "sending" });
    const feeling = feel ? feelingOfPick(feel) : undefined;
    const web = shared.files.length === 0 && isWeb(shared);
    const marks = feeling || web ? { feeling, web: web || undefined } : undefined;
    let keeps: PendingKeep[];
    try {
      const made = await sharedKeeps(shared, note, feeling, now);
      if (typeof made === "string") return setState({ name: "problem", kind: made });
      keeps = made;
    } catch {
      return setState({ name: "problem", kind: "failed" });
    }
    // Each keep is written first, so nothing is lost if the phone closes the app part-way.
    for (const k of keeps) await queue.add(k);
    onQueued();
    void sharedDone();
    if (connection.kind !== "connected") return setState({ name: "waiting" });
    let sent = 0;
    let newer = false;
    for (const k of keeps) {
      try {
        await withKeepLock(() =>
          k.kind === "text"
            ? engine.keepText(k.text ?? "", k.clientId, marks)
            : k.kind === "recording"
              ? engine.keepRecording(k.blob as Blob, k.clientId, marks)
              : engine.keepBytes(k.blob as Blob, k.clientId, k.caption, marks),
        );
        await queue.remove(k.clientId);
        sent += 1;
      } catch (e) {
        if (e instanceof ConnectionError) break;
        if (e instanceof PhoneRequestError && WAIT_CODES.has(e.code)) {
          newer = e.code === "computerOutdated";
          break;
        }
        // The computer will never take it: it does not wait on the phone either.
        await queue.remove(k.clientId);
        onQueued();
        const kind: Problem =
          e instanceof PhoneRequestError && e.code === "tooLarge"
            ? "big"
            : e instanceof PhoneRequestError && e.code === "invalid"
              ? "kind"
              : "failed";
        return setState({ name: "problem", kind });
      }
    }
    onQueued();
    setState(sent === keeps.length ? { name: "kept" } : { name: "waiting", newer });
  };

  const done = state.name === "kept" || state.name === "waiting" || state.name === "problem";
  return (
    <>
      <div className="ph-scrim" aria-hidden="true" onClick={close} />
      <BottomSheet label={t("phone.app.share.title")} onClose={close} className="ph-share">
        <div className="ph-row-between">
          <h2 tabIndex={-1} ref={heading}>
            {state.name === "kept"
              ? t("phone.keep.kept")
              : state.name === "waiting"
                ? t("phone.keep.waiting")
                : state.name === "problem"
                  ? t(PROBLEM[state.kind][0])
                  : t("phone.app.share.title")}
          </h2>
          <span className="ph-mark" aria-hidden="true" />
        </div>
        {done ? (
          <>
            <p className="ph-body" role="status">
              {state.name === "kept"
                ? alone
                  ? t("phone.alone.share.kept")
                  : t("phone.journal.keep.body")
                : state.name === "waiting" && state.newer
                  ? t("phone.app.value.needsNewer")
                  : t("phone.keep.waiting.body")}
            </p>
            <Button kind="primary" onClick={close}>
              {t("phone.done")}
            </Button>
          </>
        ) : (
          <>
            {first ? (
              <div className="ph-share-preview">
                {preview ? (
                  <img className="ph-share-thumb" src={preview} alt="" />
                ) : (
                  <span className="ph-share-thumb" aria-hidden="true">
                    <Icon name={first.mime.startsWith("image/") ? "image" : "document"} size={18} />
                  </span>
                )}
                <span className="ph-dim">
                  {t("phone.app.share.file", {
                    kind: kindOf(first),
                    size: formatBytes(first.size),
                  })}
                  <br />
                  {first.shown}
                  {shared.files.length > 1 ? (
                    <>
                      <br />
                      {t("phone.app.share.more", { count: shared.files.length - 1 })}
                    </>
                  ) : null}
                </span>
              </div>
            ) : (
              <p className="ph-share-text">{shared.text}</p>
            )}
            <div className="ph-field">
              <label htmlFor="ph-share-note" className="ph-lbl">
                {t("phone.app.share.note")}
              </label>
              <textarea
                id="ph-share-note"
                className="ph-input ph-share-note"
                rows={2}
                value={note}
                onChange={(e) => setNote(e.target.value)}
              />
            </div>
            <FeelingPick pick={feel} onPick={setFeel} />
            <Button kind="primary" disabled={state.name === "sending"} onClick={() => void keep()}>
              {t("phone.home.keep.title")}
            </Button>
          </>
        )}
      </BottomSheet>
    </>
  );
}
