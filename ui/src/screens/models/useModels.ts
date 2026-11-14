import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import type { Job, ModelChoice, ModelsView } from "../../engine/bindings";
import { type EngineError, toEngineError } from "../../engine/client";
import { useEngine, useEngineEvent } from "../../engine/context";

export const MODELS_KEY = ["models"] as const;
export const choicesKey = (job: Job) => ["models", "choices", job] as const;

/** How often a download or a re-read is asked after while one is under way, in milliseconds. */
export const MODELS_POLL_MS = 1000;

const busy = (view: ModelsView | undefined) =>
  view !== undefined &&
  ((view.change?.phase.type === "downloading" && !view.change.phase.paused) ||
    view.reindex?.phase === "running");

/**
 * The model for each job as the computer last said it, following `models-changed`. `screen` is
 * for the screens of choosing models, which ask again while a change runs.
 */
export function useModels(options: { screen?: boolean } = {}): {
  data?: ModelsView;
  error: boolean;
  /** Why the computer could not be read, when it could not. */
  failure: EngineError | null;
  refetch(): void;
} {
  const engine = useEngine();
  const client = useQueryClient();
  const query = useQuery({
    queryKey: MODELS_KEY,
    queryFn: () => engine.getModels(),
    staleTime: 5_000,
    refetchInterval: (q) => (options.screen && busy(q.state.data) ? MODELS_POLL_MS : false),
  });
  useEngineEvent("models-changed", (view) => {
    client.setQueryData(MODELS_KEY, view);
  });
  return {
    data: query.data,
    error: query.isError,
    failure: query.error ? toEngineError(query.error) : null,
    refetch: () => void query.refetch(),
  };
}

/** Every choice for a job, with its fit. Asked again whenever the view changes. */
export function useChoices(job: Job): {
  data?: ModelChoice[];
  error: boolean;
  refetch(): void;
} {
  const engine = useEngine();
  const view = useModels({ screen: true }).data;
  const query = useQuery({
    queryKey: [...choicesKey(job), view?.change?.model ?? null, view?.old?.model ?? null],
    queryFn: () => engine.listModelChoices(job),
    staleTime: 0,
  });
  return { data: query.data, error: query.isError, refetch: () => void query.refetch() };
}

type Command = "pause" | "resume" | "cancel" | "cancelReindex" | "keepOld" | "removeOld" | "reset";

/** The commands that act on a change under way, each answering with the new view. */
export function useModelsCommands() {
  const engine = useEngine();
  const client = useQueryClient();
  const run = useMutation({
    mutationFn: (command: Command) => {
      switch (command) {
        case "pause":
          return engine.pauseModelDownload();
        case "resume":
          return engine.resumeModelDownload();
        case "cancel":
          return engine.cancelModelChange();
        case "cancelReindex":
          return engine.cancelReindex();
        case "keepOld":
          return engine.keepOldModel();
        case "removeOld":
          return engine.removeOldModel();
        case "reset":
          return engine.resetModelsToTier();
      }
    },
    onSuccess: (view) => client.setQueryData(MODELS_KEY, view),
  });
  const change = useMutation({
    mutationFn: (args: { job: Job; model: string; allowDownload: boolean }) =>
      engine.changeModel(args.job, args.model, args.allowDownload),
    onSuccess: (view) => client.setQueryData(MODELS_KEY, view),
  });
  return {
    act: (command: Command) => run.mutateAsync(command),
    change: (job: Job, model: string, allowDownload: boolean) =>
      change.mutateAsync({ job, model, allowDownload }),
    pending: run.isPending || change.isPending,
  };
}
