import { useQuery } from "@tanstack/react-query";
import { useRoute } from "wouter";
import { ScreenSheet } from "../../components/ScreenSheet/ScreenSheet";
import { useEngine, usePage } from "../../engine/context";
import { NotFound } from "../NotFound";
import { KindPage } from "./KindPage";
import { kindOfSlug } from "./kinds";

export default function Health() {
  const engine = useEngine();
  const [, params] = useRoute("/journal/health/:kind");
  const kind = kindOfSlug(params?.kind);
  const page = usePage();
  const sources = useQuery({
    queryKey: ["health", "phones"],
    queryFn: () => engine.getPhoneSources(),
  });

  if (kind === null) return <NotFound />;
  if (!page.data || !sources.data) return <ScreenSheet>{null}</ScreenSheet>;
  return <KindPage key={kind} kind={kind} today={page.data.date} phones={sources.data.phones} />;
}
