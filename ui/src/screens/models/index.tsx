import { useRoute } from "wouter";
import type { CopyId } from "../../copy/t";
import { Placeholder } from "../Placeholder";

const TITLES: Record<string, CopyId> = {
  answering: "screen.models.answering.title",
  recordings: "screen.models.recordings.title",
  aloud: "screen.models.aloud.title",
  finding: "screen.models.finding.title",
};

/** Serves #/settings/models and the page for each job. */
export default function Models() {
  const [, params] = useRoute("/settings/models/:job");
  const title = (params && TITLES[params.job]) || "screen.models.title";
  return <Placeholder title={title} />;
}
