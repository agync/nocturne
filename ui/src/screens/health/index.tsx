import { useQuery } from "@tanstack/react-query";
import { useRoute } from "wouter";
import { Button } from "../../components/Button/Button";
import { ScreenSheet } from "../../components/ScreenSheet/ScreenSheet";
import { t } from "../../copy/t";
import { useEngine, usePage } from "../../engine/context";
import { NotFound } from "../NotFound";
import styles from "./Health.module.css";
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
  if (!page.data || !sources.data) {
    const failed = (page.isError && !page.data) || (sources.isError && !sources.data);
    return (
      <ScreenSheet>
        <div className={styles.page}>
          {failed ? (
            <>
              <p className={styles.sentence} role="alert">
                {t("health.error")}
              </p>
              <div>
                <Button
                  variant="secondary"
                  onClick={() => {
                    void page.refetch();
                    void sources.refetch();
                  }}
                >
                  {t("button.tryAgain")}
                </Button>
              </div>
            </>
          ) : (
            <p className={styles.sentence} role="status">
              {t("health.loading")}
            </p>
          )}
        </div>
      </ScreenSheet>
    );
  }
  return <KindPage key={kind} kind={kind} today={page.data.date} phones={sources.data.phones} />;
}
