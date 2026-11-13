import { useRoute } from "wouter";
import { NotFound } from "../NotFound";
import { isJob } from "./jobs";
import { ModelsPage } from "./ModelsPage";
import { Picker } from "./Picker";

/** Serves #/settings/models and the page for each job. */
export default function Models() {
  const [, params] = useRoute("/settings/models/:job");
  if (!params) return <ModelsPage />;
  return isJob(params.job) ? <Picker job={params.job} /> : <NotFound />;
}
