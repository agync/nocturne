import { t } from "../../copy/t";
import type { ForgetReport } from "../../engine/bindings";
import { Button } from "../Button/Button";
import { Dialog } from "../Dialog/Dialog";

export interface ForgetDialogProps {
  memoryIds: string[];
  itemIds?: string[];
  defaultIncludeOriginals?: boolean;
  onClose: () => void;
  onDone: (report: ForgetReport) => void;
}

/**
 * The shape every screen imports today. The memories lane replaces the body with the real
 * preview and confirm flow (focus on Keep them, Enter never forgets); the props stay as they are.
 */
export function ForgetDialog({ memoryIds, onClose }: ForgetDialogProps) {
  return (
    <Dialog title={t("forget.title", { count: memoryIds.length })} onClose={onClose}>
      <p>{t("forget.stub")}</p>
      <Button variant="secondary" onClick={onClose}>
        {t("forget.keep")}
      </Button>
    </Dialog>
  );
}
