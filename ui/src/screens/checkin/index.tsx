import { type FormEvent, useEffect, useRef, useState } from "react";
import { useQueryParam } from "../../app/query";
import { BackLink } from "../../components/BackLink/BackLink";
import { Button } from "../../components/Button/Button";
import { Disclosure } from "../../components/Disclosure/Disclosure";
import { FeelingField, middleOf } from "../../components/FeelingField/FeelingField";
import { ScreenSheet } from "../../components/ScreenSheet/ScreenSheet";
import { TextInput } from "../../components/TextInput/TextInput";
import { t } from "../../copy/t";
import type { CheckIn, FeelingPart, FeelingPoint } from "../../engine/bindings";
import { useEngine } from "../../engine/context";
import { PART_ORDER, partOf, WORDS } from "../../journal/feelings";
import styles from "./Checkin.module.css";
import { Done } from "./Done";
import { feelingOf, toggleWord } from "./selection";
import { useCheckInToChange } from "./useCheckInToChange";
import { FieldWords, ListWords, Meaning, useMeaning } from "./Words";

type Mode = "field" | "list";

const asPart = (value: string | null): FeelingPart | null =>
  PART_ORDER.find((part) => part === value) ?? null;

/** Serves #/journal/check-in, also with `change=<check-in>` or `part=<part>`. */
export default function Checkin() {
  const engine = useEngine();
  const changeId = useQueryParam("change");
  const startPart = asPart(useQueryParam("part"));
  const loaded = useCheckInToChange(changeId);

  const [mode, setMode] = useState<Mode>("field");
  const [point, setPoint] = useState<FeelingPoint | null>(startPart ? middleOf(startPart) : null);
  const [words, setWords] = useState<string[]>([]);
  const [line, setLine] = useState("");
  const [urge, setUrge] = useState("");
  const [replaces, setReplaces] = useState<string | null>(changeId);
  /** An urge written earlier shows at once; a new one waits behind "More". */
  const [urgeOpen, setUrgeOpen] = useState(false);
  const [saved, setSaved] = useState<CheckIn | null>(null);
  const [working, setWorking] = useState(false);
  const [error, setError] = useState(false);
  const filled = useRef(false);
  const meaning = useMeaning();

  useEffect(() => {
    const found = loaded.data;
    if (!found || filled.current) return;
    filled.current = true;
    setPoint(found.feeling.point);
    setWords(found.feeling.words);
    setLine(found.line ?? "");
    setUrge(found.urge ?? "");
    setUrgeOpen(Boolean(found.urge));
  }, [loaded.data]);

  const feeling = feelingOf(mode, point, words);

  const place = (next: FeelingPoint) => {
    const part = partOf(next);
    // Words belong to a part, so the ones left behind go when the point crosses into another.
    setWords((now) => now.filter((w) => WORDS[part].some((entry) => entry.word === w)));
    setPoint(next);
  };

  const switchTo = (next: Mode) => {
    if (next === "field" && feeling) setPoint(feeling.point);
    setMode(next);
  };

  const save = async (event: FormEvent) => {
    event.preventDefault();
    if (!feeling || working) return;
    setWorking(true);
    setError(false);
    try {
      const made = await engine.keepCheckIn({
        feeling,
        line: line.trim() || null,
        urge: urge.trim() || null,
        replaces,
      });
      setSaved(made);
    } catch {
      setError(true);
    } finally {
      setWorking(false);
    }
  };

  const change = () => {
    if (!saved) return;
    setReplaces(saved.id);
    setMode("field");
    setPoint(saved.feeling.point);
    setWords(saved.feeling.words);
    setUrgeOpen(urge !== "");
    setSaved(null);
  };

  if (saved) {
    return (
      <ScreenSheet>
        <BackLink to="/journal" label={t("screen.journal.title")} />
        <Done checkin={saved} onChange={change} />
      </ScreenSheet>
    );
  }

  const part = point ? partOf(point) : null;
  const urgeField = (
    <TextInput
      label={t("journal.checkin.urge")}
      value={urge}
      onChange={setUrge}
      autoComplete="off"
    />
  );

  return (
    <ScreenSheet>
      <BackLink to="/journal" label={t("screen.journal.title")} />
      <div className={styles.head}>
        <div className={styles.headText}>
          <h1 className="t-page-title">{t("journal.checkin.title")}</h1>
          <p className={styles.lead}>{t("journal.checkin.lead")}</p>
        </div>
        <Button variant="quiet" onClick={() => switchTo(mode === "field" ? "list" : "field")}>
          {t(mode === "field" ? "journal.checkin.showList" : "journal.checkin.showField")}
        </Button>
      </div>

      <form className={styles.form} onSubmit={save}>
        <div className={styles.checkin} data-mode={mode}>
          {mode === "field" ? <FeelingField point={point} onChange={place} /> : null}
          <div className={styles.panel}>
            {mode === "list" ? (
              <>
                <ListWords
                  words={words}
                  onToggle={(w) => setWords(toggleWord(words, w))}
                  bind={meaning.bind}
                />
                <Meaning word={meaning.shown} />
                <p className={styles.small}>{t("journal.checkin.listHint")}</p>
              </>
            ) : part && point ? (
              <>
                <FieldWords
                  key={part}
                  part={part}
                  point={point}
                  words={words}
                  onToggle={(w) => setWords(toggleWord(words, w))}
                  bind={meaning.bind}
                />
                <Meaning word={meaning.shown} />
              </>
            ) : (
              <p className={styles.pick}>{t("journal.checkin.pick")}</p>
            )}

            <TextInput
              label={t("journal.checkin.going")}
              value={line}
              onChange={setLine}
              placeholder={t("journal.checkin.goingPlaceholder")}
              autoComplete="off"
            />
            {urgeOpen ? (
              urgeField
            ) : (
              <Disclosure label={t("journal.checkin.urgeMore")}>{urgeField}</Disclosure>
            )}
            {error ? (
              <p className={styles.error} role="alert">
                {t("bar.kept.error")}
              </p>
            ) : null}
            <div className={styles.footer}>
              <Button variant="primary" type="submit" disabled={!feeling || working}>
                {t("journal.checkin.save")}
              </Button>
            </div>
          </div>
        </div>
      </form>
    </ScreenSheet>
  );
}
