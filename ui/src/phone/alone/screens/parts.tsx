import { Children, type ReactNode } from "react";
import { type CopyId, type CopyValues, t, tParts } from "../../../copy/t";
import { Icon } from "../../screens/ui";

/** A control named in a sentence, drawn as the control's own label in a box. */
export function Chip({ children }: { children: ReactNode }) {
  return <span className="ph-cc">{children}</span>;
}

/** A sentence with its `{thisPhone}`, `{keep}` and `{addAPhone}` drawn as chips. */
export function chips(id: CopyId, values?: CopyValues): ReactNode {
  return Children.toArray(
    tParts(id, {
      ...values,
      thisPhone: <Chip>{t("phone.sheet.title")}</Chip>,
      keep: <Chip>{t("phone.home.keep.title")}</Chip>,
      addAPhone: <Chip>{t("away.phones.add")}</Chip>,
    }) as ReactNode[],
  );
}

/** The same sentence as plain words. */
export function chipsPlain(id: CopyId, values?: CopyValues): string {
  return t(id, {
    ...values,
    thisPhone: t("phone.sheet.title"),
    keep: t("phone.home.keep.title"),
    addAPhone: t("away.phones.add"),
  });
}

/** "This phone is nearly full.": on Today and on Keep while less than 500 MB is free. */
export function NearlyFull() {
  return (
    <div className="ph-banner" role="status">
      <span className="ph-banner-icon" aria-hidden="true">
        <Icon name="phone" size={22} />
      </span>
      <span>
        <strong>{t("phone.alone.full.title")}</strong> {t("phone.alone.full.body")}
      </span>
    </div>
  );
}
