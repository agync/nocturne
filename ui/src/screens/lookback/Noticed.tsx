import { duration, num } from "../../components/health/sentence";
import { type CopyId, t } from "../../copy/t";
import type { Noticed } from "../../engine/bindings";
import styles from "./Lookback.module.css";
import { Chips, useActiveReceipt } from "./Receipts";
import { partName } from "./text";

/** Steps are said to the nearest hundred: "about 3,000 more". */
const aboutSteps = (steps: number): string => num(Math.max(100, Math.round(steps / 100) * 100));

const topicKey = (item: Noticed): string =>
  item.topic.type === "name"
    ? item.topic.with.entityId
    : item.topic.type === "felt"
      ? item.topic.part
      : item.topic.type;

const SENTENCE = {
  "sleep.name": "journal.lookback.noticed.sleep.name",
  "sleep.felt": "journal.lookback.noticed.sleep.felt",
  "sleep.good": "journal.lookback.noticed.sleep.good",
  "steps.name": "journal.lookback.noticed.steps.name",
  "steps.felt": "journal.lookback.noticed.steps.felt",
  "steps.good": "journal.lookback.noticed.steps.good",
  "workout.felt": "journal.lookback.noticed.workout.felt",
} as const satisfies Record<string, CopyId>;

/** What went with what, in counts and the person's own units. Never why. */
export function noticedLine(item: Noticed): string {
  const topic = item.topic;
  const what = topic.type === "felt" ? partName(topic.part).toLowerCase() : "";
  const name = topic.type === "name" ? topic.with.name : "";
  const difference =
    item.measure === "sleep" ? duration(item.difference) : aboutSteps(item.difference);
  const id = SENTENCE[`${item.measure}.${topic.type}` as keyof typeof SENTENCE];
  return t(id, {
    days: item.days,
    held: item.held,
    difference,
    direction: item.more ? "more" : "less",
    name,
    what,
  });
}

/** The sentences about what sleep, steps or workouts went with, each with both sides cited. */
export function NoticedLines({
  items,
  period,
}: {
  items: readonly Noticed[];
  period: "week" | "month" | "year";
}) {
  const receipt = useActiveReceipt();
  const withDay = period === "week" ? "weekday" : "date";
  return (
    <>
      {items.map((item) => (
        <div key={`${item.measure}-${topicKey(item)}`} className={styles.togetherBlock}>
          <p className={styles.together}>{noticedLine(item)}</p>
          <div className={styles.chipPair}>
            <Chips receipts={item.body} receipt={receipt} withDay={withDay} most={2} />
            <Chips receipts={item.journal} receipt={receipt} withDay={withDay} most={2} />
          </div>
        </div>
      ))}
    </>
  );
}
