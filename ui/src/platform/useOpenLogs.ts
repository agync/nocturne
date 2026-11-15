import { useToast } from "../components/Toast/Toast";
import { t } from "../copy/t";
import { usePlatform } from "./context";

/** Opens the folder of log files, and says so when it cannot. */
export function useOpenLogs(): () => void {
  const platform = usePlatform();
  const toast = useToast();
  return () => {
    platform.openLogs().catch(() => toast.show({ message: t("notice.action.failed") }));
  };
}
