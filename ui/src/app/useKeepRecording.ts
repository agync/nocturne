import { useCallback } from "react";
import { useToast } from "../components/Toast/Toast";
import { formatDuration } from "../copy/format";
import { t } from "../copy/t";
import { useEngine, useRefresh } from "../engine/context";
import { useRecorder } from "./useRecorder";

/**
 * Hold to talk, then keep the voice note. If keeping fails the recording is not thrown away: the
 * toast offers Try again with the same audio.
 */
export function useKeepRecording() {
  const engine = useEngine();
  const toast = useToast();
  const refresh = useRefresh();

  const save = useCallback(
    async (bytes: Uint8Array, mime: string, durationMs: number): Promise<void> => {
      try {
        await engine.keepRecording(bytes, mime);
        toast.show({
          message: t("capture.kept.toast", {
            kind: t("kind.audio"),
            duration: formatDuration(durationMs),
          }),
        });
        refresh("items", "status", "page");
      } catch {
        toast.show({
          message: t("capture.recording.failed"),
          action: {
            label: t("button.tryAgain"),
            onAction: () => void save(bytes, mime, durationMs),
          },
        });
      }
    },
    [engine, toast, refresh],
  );

  return useRecorder({
    onFinished: (bytes, mime, durationMs) => void save(bytes, mime, durationMs),
    onFailed: (reason) =>
      toast.show({
        message: t(
          reason === "unsupported"
            ? "capture.mic.unsupported"
            : reason === "denied"
              ? "notice.micDenied.text"
              : "capture.mic.failed",
        ),
        duration: 9000,
      }),
  });
}
