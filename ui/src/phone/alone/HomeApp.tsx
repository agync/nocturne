import {
  type ReactNode,
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { readingDay } from "../../components/ReceiptChip/receiptPath";
import { journalDay } from "../../copy/format";
import { t } from "../../copy/t";
import type {
  FeelingPart,
  HomeStats,
  PhoneSource,
  PutAsideMemory,
  Receipt,
  SameNameQuestion,
} from "../../engine/bindings";
import type { NativeChannel } from "../native/api";
import { type LaunchRoute, type Shared, watchLaunch } from "../native/launch";
import { openAppSettings } from "../native/scanner";
import { CalendarConnect } from "../native/screens/CalendarConnect";
import { KeepButton, NavBar, type Tab, TabBar } from "../native/screens/chrome";
import { HealthConnect } from "../native/screens/HealthConnect";
import { Offer } from "../native/screens/Offer";
import { ShareSheet } from "../native/screens/ShareSheet";
import type { NativeServices } from "../native/services";
import { Ask, useAskSession } from "../screens/Ask";
import { CheckIn } from "../screens/CheckIn";
import { Journal } from "../screens/Journal";
import { Keep } from "../screens/Keep";
import { Original } from "../screens/Original";
import { Which } from "../screens/Which";
import "./home.css";
import { listen } from "@tauri-apps/api/event";
import { HomeRequestError, NEARLY_FULL } from "./api";
import {
  chosenHost,
  createMindApi,
  type MindChosen,
  type MindReading,
  type MindStatus,
} from "./mind";
import { forgetReminder, reminderDone, reminderDue, VOICE_ASKED } from "./reminder";
import { Restore, SaveBackup } from "./screens/Backup";
import { DeleteEverything } from "./screens/DeleteEverything";
import { DeleteKept } from "./screens/DeleteKept";
import { HomeFind } from "./screens/Find";
import { ForgetItem } from "./screens/Forget";
import {
  ChooseMind,
  MindAgree,
  MindDetails,
  type MindDraft,
  MindLimitStep,
  MindReadingScreen,
  MindWhat,
  NEW_DRAFT,
} from "./screens/Mind";
import { MindSentList } from "./screens/MindSent";
import { Permissions } from "./screens/Permissions";
import { PutAside } from "./screens/PutAside";
import { HomeThisPhone } from "./screens/ThisPhone";
import { HomeToday } from "./screens/Today";
import { VoiceAsk } from "./screens/VoiceAsk";
import { GiveMind } from "./screens/Welcome";

type View =
  | { name: "tab"; tab: Tab; above?: true }
  | { name: "offer" }
  | { name: "thisPhone" }
  | { name: "calendar" }
  | { name: "health" }
  | { name: "mind" }
  | { name: "mindComputer" }
  /** `depth`: the entries this way in pushed, so "Not now" leaves them all. */
  | { name: "mindOnline"; step: 1 | 2 | 3 | 4; depth: number }
  | { name: "mindReading"; depth: number; first: MindReading }
  | { name: "mindSent" }
  | { name: "backup" }
  | { name: "restore" }
  | { name: "delete" }
  | { name: "putAside"; memory: PutAsideMemory; withTime: boolean }
  | { name: "deleteKept"; source: PhoneSource; choose: boolean }
  | { name: "forget"; id: string; withOriginal: boolean }
  | { name: "permissions" }
  | { name: "keep"; recording?: boolean }
  | { name: "checkin"; part?: FeelingPart }
  | { name: "original"; id: string }
  | { name: "journal"; day: string; today: string | null }
  | { name: "which"; id: string; day: string; today: string | null };

const TODAY: View = { name: "tab", tab: "today" };

const OWN: ReadonlySet<string> = new Set([
  "tab",
  "offer",
  "thisPhone",
  "calendar",
  "health",
  "mind",
  "mindComputer",
  "mindOnline",
  "mindReading",
  "mindSent",
  "backup",
  "restore",
  "delete",
  "putAside",
  "deleteKept",
  "forget",
  "permissions",
  "keep",
  "checkin",
  "original",
  "journal",
  "which",
]);

/** A view from the history entry, when it is one of this app's; a companion's is not. */
function ownView(state: unknown): View | null {
  const view = (state as { view?: { name?: unknown } } | null)?.view;
  if (!view || typeof view.name !== "string" || !OWN.has(view.name)) return null;
  if (view.name === "journal" && !(view as { day?: unknown }).day)
    return { name: "tab", tab: "journal" };
  return view as View;
}

/** Views live in the history entries, so the phone's back gesture steps back through the app. */
function useView(first: View) {
  const [view, setView] = useState<View>(() => {
    if (first.name === "offer") return first;
    return ownView(history.state) ?? first;
  });
  // Recorded as the screen first commits, before a tap can push the next entry: a later
  // record would overwrite that entry and send the back gesture to the wrong screen.
  useLayoutEffect(() => {
    history.replaceState({ view }, "");
    const onPop = (e: PopStateEvent) => {
      depth.current = Math.max(0, depth.current - 1);
      setView(ownView(e.state) ?? TODAY);
    };
    window.addEventListener("popstate", onPop);
    return () => window.removeEventListener("popstate", onPop);
    // The first entry is recorded once.
  }, []);
  // Entries this app pushed and has not stepped back from, so Delete everything can unwind them.
  const depth = useRef(0);
  const go = useCallback((next: View) => {
    history.pushState({ view: next }, "");
    depth.current += 1;
    setView(next);
  }, []);
  const unwind = useCallback(() => {
    if (depth.current > 0) history.go(-depth.current);
    depth.current = 0;
  }, []);
  const replace = useCallback((next: View) => {
    history.replaceState({ view: next }, "");
    setView(next);
  }, []);
  /** Steps back over `n` entries at once. */
  const leave = useCallback((n: number) => {
    if (n <= 0) return;
    // The one popstate this makes takes off one more.
    depth.current = Math.max(0, depth.current - (n - 1));
    history.go(-n);
  }, []);
  const back = useCallback(() => {
    if (history.length > 1 && history.state?.view?.name !== "tab") history.back();
    else replace(TODAY);
  }, [replace]);
  return { view, go, back, replace, unwind, leave };
}

/** The phone's language for reading voice notes, as the web view knows it. */
const language = () => navigator.language || "en-GB";

/**
 * A phone alone: Today, Journal and Ask from this phone's own memory, Keep floating over Today
 * and Journal, and This phone in the top bar, with its backup and Delete everything.
 */
export function HomeApp({
  services,
  channel,
  offer,
  name,
  onLost,
  onDeleted,
  onScan,
}: {
  services: NativeServices;
  channel: NativeChannel;
  /** Just started: "Bring more from this phone?" comes first. */
  offer: boolean;
  /** This phone's name. */
  name: string;
  /** The memory could not be opened: where it stands is asked again. */
  onLost(): void;
  /** Everything was deleted: Welcome comes next, saying so. */
  onDeleted(): void;
  /** "Scan the code" in Give Nocturne a mind: pairing a computer starts the move. */
  onScan(): void;
}) {
  const { home } = services;
  const engine = useMemo(() => services.engineFor(() => ""), [services]);
  useEffect(() => () => engine.close(), [engine]);
  const conn = engine.connection();
  const { view, go, back, replace, unwind, leave } = useView(offer ? { name: "offer" } : TODAY);
  const mind = useMemo(() => services.mind ?? createMindApi(), [services]);
  const [mindChosen, setMindChosen] = useState<MindChosen | null>(null);
  const [mindStatus, setMindStatus] = useState<MindStatus | null>(null);
  const ask = useAskSession(engine);
  // The form, its key included, lives here and never in the history entries.
  const draft = useRef<MindDraft>(NEW_DRAFT);
  const host = chosenHost(mindChosen);
  const shown = useRef(view);
  shown.current = view;
  const [tab, setTab] = useState<Tab>(view.name === "tab" ? view.tab : "today");
  const [chosen, setChosen] = useState<ReadonlySet<"calendar" | "health">>(new Set());
  const [shared, setShared] = useState<Shared | null>(null);
  const [empty, setEmpty] = useState(false);
  const [fresh, setFresh] = useState(0);
  const question = useRef<SameNameQuestion | null>(null);
  const [asking, setAsking] = useState<((go: boolean) => void) | null>(null);
  const found = useRef(false);
  const quote = useRef<string | null>(null);
  const keepDraft = useRef("");
  const [nearlyFull, setNearlyFull] = useState(false);

  useEffect(() => {
    if (view.name === "tab") setTab(view.tab);
  }, [view]);

  const lookMind = useCallback(
    () =>
      Promise.all([mind.chosen(), mind.status()]).then(
        ([c, s]) => {
          setMindChosen(c);
          setMindStatus(s);
        },
        () => undefined,
      ),
    [mind],
  );
  useEffect(() => {
    void lookMind();
  }, [lookMind]);
  // Today and Ask say how the service is doing as they open.
  const onTab = view.name === "tab" ? view.tab : null;
  useEffect(() => {
    if (onTab === "today" || onTab === "ask") void lookMind();
  }, [onTab, lookMind]);

  const stats = useCallback(
    () =>
      home.stats().catch((e: unknown) => {
        if (e instanceof HomeRequestError && e.code === "notReady") onLost();
        throw e;
      }),
    [home, onLost],
  );
  // Shown once: once it is due and on screen, it does not come back.
  const due = useCallback(
    async (s: HomeStats) => {
      const show = await reminderDue(s, services.now(), home.notes);
      if (show) await reminderDone(home.notes);
      return show;
    },
    [services, home],
  );
  const calendarOn = useCallback(
    async () => (await channel.sources()).calendar === "on",
    [channel],
  );
  const refresh = useCallback(() => setFresh((n) => n + 1), []);

  // The mind reads and writes in the background: what it did shows as soon as it is done, a
  // burst of readings as one change.
  useEffect(() => {
    let timer: ReturnType<typeof setTimeout> | undefined;
    const soon = () => {
      clearTimeout(timer);
      timer = setTimeout(() => {
        void lookMind();
        refresh();
      }, 400);
    };
    const stops = ["item-changed", "page-ready"].map((name) =>
      listen(name, soon).catch(() => () => undefined),
    );
    return () => {
      clearTimeout(timer);
      for (const stop of stops) void stop.then((off) => off()).catch(() => undefined);
    };
  }, [lookMind, refresh]);

  // Once per phone: a plain line, then the phone's own prompt for what reads voice notes.
  // Resolves false when the person said "Not now": nothing then opens the phone's prompt.
  const askVoice = useCallback(
    () =>
      new Promise<boolean>((done) => {
        void (async () => {
          try {
            const access = await home.voiceAccess();
            const asked = await home.notes.get(VOICE_ASKED);
            if (asked || (access !== "prompt" && access !== "prompt-with-rationale"))
              return done(true);
            await home.notes.set(VOICE_ASKED, "1");
            setAsking(() => (go: boolean) => {
              setAsking(null);
              if (!go) return done(false);
              void home.voiceRequest().finally(() => done(true));
            });
          } catch {
            done(true);
          }
        })();
      }),
    [home],
  );
  const voice = useMemo(
    () => ({
      access: home.voiceAccess,
      turnOn: home.voiceRequest,
      asked: async () => Boolean(await home.notes.get(VOICE_ASKED)),
    }),
    [home],
  );
  const ios = services.platform.ios;
  const keepAlone = useMemo(
    () => ({
      beforeRecord: ios ? undefined : askVoice,
      onKept: (kind: string) => {
        if (ios && kind === "recording") void askVoice();
      },
      onSettings: () => void openAppSettings(),
      nearlyFull,
    }),
    [ios, askVoice, nearlyFull],
  );
  const itemAlone = useMemo(
    () => ({
      setPrivate: home.setPrivate,
      get found() {
        return found.current;
      },
      onForget: (id: string, withOriginal: boolean) => go({ name: "forget", id, withOriginal }),
      kept: home.itemKept,
    }),
    [home, go],
  );

  // What waited in the keep queue (the store was busy) goes into the memory now.
  useEffect(() => {
    void services.queue.flush(engine).then(
      (r) => r.sent > 0 && refresh(),
      () => undefined,
    );
  }, [engine, services.queue, refresh]);

  // How much room is left, on open and each time the app comes back to the front.
  useEffect(() => {
    let gone = false;
    const look = () =>
      void home.freeSpace().then((free) => {
        if (!gone) setNearlyFull(free !== null && free < NEARLY_FULL);
      });
    look();
    const front = () => {
      if (document.visibilityState === "visible") look();
    };
    document.addEventListener("visibilitychange", front);
    return () => {
      gone = true;
      document.removeEventListener("visibilitychange", front);
    };
  }, [home]);

  // Voice notes are read into words where the phone can: on open and each time it comes back.
  useEffect(() => {
    void home.hearVoiceNotes(language());
    const front = () => {
      if (document.visibilityState === "visible") void home.hearVoiceNotes(language());
    };
    document.addEventListener("visibilitychange", front);
    return () => document.removeEventListener("visibilitychange", front);
  }, [home]);

  useEffect(
    () =>
      watchLaunch(
        (route: LaunchRoute) => {
          if (route === "keep") go({ name: "keep" });
          else if (route === "record") go({ name: "keep", recording: true });
          else go({ name: "checkin" });
        },
        (next) => setShared(next),
      ),
    [go],
  );

  useEffect(() => {
    if (tab !== "ask") return;
    let gone = false;
    home.stats().then(
      (s) => !gone && setEmpty(s.counts.items === 0),
      () => undefined,
    );
    return () => {
      gone = true;
    };
  }, [tab, home]);

  const open = useCallback(
    (receipt: Receipt) => {
      if (receipt.kind === "reading")
        return go({ name: "journal", day: readingDay(receipt.keptAt), today: null });
      found.current = false;
      quote.current = receipt.span?.type === "quote" ? receipt.span.text : null;
      go({ name: "original", id: receipt.itemId });
    },
    [go],
  );
  const openFound = useCallback(
    (receipt: Receipt, words: string | null) => {
      if (receipt.kind === "reading") return open(receipt);
      found.current = words !== null;
      quote.current = words;
      go({ name: "original", id: receipt.itemId });
    },
    [go, open],
  );
  const openItem = useCallback(
    (id: string) => {
      found.current = false;
      quote.current = null;
      go({ name: "original", id });
    },
    [go],
  );

  // Journal and Ask sit one step above Today, so Back goes to Today and Back on Today leaves.
  const toTab = (next: Tab) => {
    if (view.name !== "tab") return go({ name: "tab", tab: next });
    if (view.above && next === "today") return history.back();
    if (view.above) return replace({ name: "tab", tab: next, above: true });
    if (view.tab === "today" && next !== "today")
      return go({ name: "tab", tab: next, above: true });
    replace({ name: "tab", tab: next });
  };
  const thisPhone = () => go({ name: "thisPhone" });
  const bar = (forTab: Tab, action?: ReactNode) => (
    <TabBar tab={forTab} action={action} onThisPhone={thisPhone} />
  );
  const toKeep = () => go({ name: "keep" });
  const toMind = () => go({ name: "mind" });
  // The details are asked again: a key refused, credit added or a new limit all start there.
  const fixMind = () => {
    draft.current = NEW_DRAFT;
    go({ name: "mindOnline", step: 2, depth: 1 });
  };
  const leaveMind = (depth: number) => {
    draft.current = NEW_DRAFT;
    leave(depth);
  };
  const turnOn = async (depth: number) => {
    const d = draft.current;
    await mind.turnOn(d.form);
    if (d.limit.kind !== "none") {
      // A limit that can't be kept turns the mind off again: nothing goes past it unkept.
      await mind.setLimit(d.limit).catch(async (e: unknown) => {
        await mind.turnOff().catch(() => undefined);
        throw e;
      });
    }
    draft.current = NEW_DRAFT;
    await lookMind();
    refresh();
    const first = await mind.reading().catch(() => null);
    // The person stepped away meanwhile: they are left where they went.
    if (shown.current.name !== "mindOnline") return;
    if (first && first.count > 0) replace({ name: "mindReading", depth, first });
    else leave(depth);
  };

  let screen: ReactNode = null;
  let tabbed = false;
  switch (view.name) {
    case "offer":
      screen = (
        <Offer
          channel={channel}
          connected
          alone
          chosen={chosen}
          onCalendar={() => {
            setChosen((s) => new Set(s).add("calendar"));
            go({ name: "calendar" });
          }}
          onHealth={() => {
            setChosen((s) => new Set(s).add("health"));
            go({ name: "health" });
          }}
          onDone={() => replace(TODAY)}
        />
      );
      break;
    case "calendar":
      screen = (
        <CalendarConnect
          channel={channel}
          onDone={back}
          alone
          onDeleteKept={() => go({ name: "deleteKept", source: "calendar", choose: false })}
        />
      );
      break;
    case "health":
      screen = (
        <HealthConnect
          channel={channel}
          onDone={back}
          alone
          onDeleteKept={() => go({ name: "deleteKept", source: "health", choose: false })}
        />
      );
      break;
    case "deleteKept":
      screen = (
        <DeleteKept
          key={view.source}
          first={view.source}
          choose={view.choose}
          preview={home.previewSourceDelete}
          remove={home.deleteSource}
          onClose={(deleted) => {
            if (deleted) refresh();
            back();
          }}
        />
      );
      break;
    case "putAside":
      screen = (
        <PutAside
          memory={view.memory}
          withTime={view.withTime}
          bringBack={home.bringBack}
          remove={home.deletePutAside}
          onBack={back}
          onDone={() => {
            refresh();
            back();
          }}
        />
      );
      break;
    case "permissions":
      screen = <Permissions ios={ios} onBack={back} />;
      break;
    case "forget":
      screen = (
        <ForgetItem
          id={view.id}
          withOriginal={view.withOriginal}
          preview={home.previewForgetItem}
          forget={home.forget}
          onBack={back}
          onDone={(gone) => {
            refresh();
            // The thing's own page went with it: back to where it was opened from.
            if (gone && history.length > 2) history.go(-2);
            else back();
          }}
        />
      );
      break;
    case "thisPhone":
      screen = (
        <HomeThisPhone
          channel={channel}
          stats={stats}
          version={services.version}
          voice={voice}
          onBack={back}
          onMind={toMind}
          onCalendar={() => go({ name: "calendar" })}
          onHealth={() => go({ name: "health" })}
          onBackup={() => go({ name: "backup" })}
          onRestore={() => go({ name: "restore" })}
          onDelete={() => go({ name: "delete" })}
          putAside={home.putAside}
          onPutAside={(memory, withTime) => go({ name: "putAside", memory, withTime })}
          onDeleteKept={() => go({ name: "deleteKept", source: "calendar", choose: true })}
          onPermissions={() => go({ name: "permissions" })}
          nearlyFull={nearlyFull}
          mind={{
            chosen: mindChosen,
            month: mind.month,
            reading: mind.reading,
            onMove: () => go({ name: "mindComputer" }),
            onSent: () => go({ name: "mindSent" }),
            onCheckKey: fixMind,
            fresh,
            onTurnOff: async () => {
              await mind.turnOff();
              setMindChosen({ state: "off" });
              setMindStatus({ state: "off" });
              refresh();
            },
          }}
        />
      );
      break;
    case "mind":
      screen = (
        <ChooseMind
          onBack={() => {
            draft.current = NEW_DRAFT;
            back();
          }}
          onChoose={(way) =>
            way === "online"
              ? go({ name: "mindOnline", step: 1, depth: 2 })
              : go({ name: "mindComputer" })
          }
        />
      );
      break;
    case "mindComputer":
      screen = <GiveMind backLabel={t("phone.back")} onBack={back} onScan={onScan} />;
      break;
    case "mindOnline": {
      const { depth } = view;
      // Opened again with the form gone (the app was closed): its details are asked again.
      const step = view.step > 2 && !draft.current.works ? 2 : view.step;
      const next = () =>
        go({ name: "mindOnline", step: (step + 1) as 2 | 3 | 4, depth: depth + 1 });
      const setDraft = (d: MindDraft) => {
        draft.current = d;
      };
      if (step === 1)
        screen = <MindWhat onBack={back} onNext={next} onNotNow={() => leaveMind(depth)} />;
      else if (step === 2)
        screen = (
          <MindDetails
            draft={draft.current}
            check={mind.check}
            onDraft={setDraft}
            onBack={back}
            onNext={next}
          />
        );
      else if (step === 3)
        screen = (
          <MindLimitStep draft={draft.current} onDraft={setDraft} onBack={back} onNext={next} />
        );
      else
        screen = (
          <MindAgree
            draft={draft.current}
            onBack={back}
            onNotNow={() => leaveMind(depth)}
            onTurnOn={() => turnOn(depth)}
          />
        );
      break;
    }
    case "mindReading":
      screen = (
        <MindReadingScreen
          first={view.first}
          reading={mind.reading}
          onDone={() => leave(view.depth)}
        />
      );
      break;
    case "mindSent":
      screen = <MindSentList host={host ?? ""} sent={mind.sent} onBack={back} />;
      break;
    case "backup":
      screen = (
        <SaveBackup
          save={home.saveBackupFile}
          stop={home.stopBackup}
          now={services.now}
          onClose={back}
        />
      );
      break;
    case "restore":
      screen = (
        <Restore
          holds
          pickFile={home.chooseBackupFile}
          check={home.checkBackup}
          restore={home.restoreBackup}
          stop={home.stopBackup}
          letGo={home.letGoBackupFile}
          onBack={back}
          onDone={() => {
            refresh();
            unwind();
          }}
        />
      );
      break;
    case "delete":
      screen = (
        <DeleteEverything
          onKeep={back}
          onDelete={async (phrase) => {
            await home.deleteEverything(phrase);
            await services.queue.clear().catch(() => undefined);
            await forgetReminder(home.notes);
            setMindChosen({ state: "off" });
            setMindStatus({ state: "off" });
            // Welcome is a new start: the back gesture does not walk into the screens before.
            unwind();
            onDeleted();
          }}
        />
      );
      break;
    case "keep":
      screen = (
        <Keep
          engine={engine}
          queue={services.queue}
          connection={conn}
          deviceName={name}
          now={services.now}
          onHome={back}
          onQueued={refresh}
          onMoreWords={(part) => go({ name: "checkin", part })}
          draft={keepDraft}
          recording={view.recording}
          alone={keepAlone}
        />
      );
      break;
    case "checkin":
      screen = (
        <CheckIn
          engine={engine}
          queue={services.queue}
          connection={conn}
          deviceName={name}
          now={services.now}
          part={view.part}
          onBack={back}
          onQueued={refresh}
          lastNight
          alone
        />
      );
      break;
    case "original":
      screen = (
        <Original
          engine={engine}
          itemId={view.id}
          quote={quote.current}
          onBack={back}
          alone={itemAlone}
        />
      );
      break;
    case "which":
      screen = (
        <Which
          engine={engine}
          question={question.current?.id === view.id ? question.current : null}
          questionId={view.id}
          day={view.day}
          today={view.day === view.today}
          onBack={back}
          onDone={back}
        />
      );
      break;
    case "journal":
    case "tab": {
      const journalProps = {
        engine,
        queue: services.queue,
        connection: conn,
        computerName: name,
        onDay: (day: string, today: string | null) => go({ name: "journal", day, today }),
        onOpen: openItem,
        onWhich: (q: SameNameQuestion, day: string, today: string | null) => {
          question.current = q;
          go({ name: "which", id: q.id, day, today });
        },
        onReflect: () => undefined,
        onReflection: () => undefined,
        onKeep: toKeep,
        onCheckIn: () => go({ name: "checkin" }),
        onBack: back,
        phone: true,
        noMind: true,
      };
      if (view.name === "journal") {
        screen = <Journal {...journalProps} day={view.day} today={view.today} />;
        break;
      }
      tabbed = true;
      if (view.tab === "today") {
        screen = (
          <HomeToday
            key={fresh}
            engine={engine}
            bar={bar("today")}
            now={services.now}
            calendarOn={calendarOn}
            stats={stats}
            reminderDue={due}
            onReminder={(save) => {
              if (save) go({ name: "backup" });
            }}
            onMind={toMind}
            onCheckIn={() => go({ name: "checkin" })}
            onItem={openItem}
            onCalendarItem={openItem}
            onNight={(to) => go({ name: "journal", day: readingDay(to), today: null })}
            onJournal={() => toTab("journal")}
            nearlyFull={nearlyFull}
            mind={host}
            status={mindStatus}
            onFixMind={fixMind}
            queue={services.queue}
            onQueued={refresh}
            onOpen={open}
            onJournalDay={(day) =>
              day ? go({ name: "journal", day, today: null }) : toTab("journal")
            }
          />
        );
      } else if (view.tab === "journal") {
        screen = (
          <Journal
            key={fresh}
            {...journalProps}
            day={null}
            today={null}
            tabBar={(action) => bar("journal", action)}
          />
        );
      } else if (mindStatus?.state === "fine") {
        screen = (
          <Ask
            model={ask.model}
            connected={conn.kind === "connected"}
            onAsk={(q) => void ask.ask(q)}
            onStop={() => void ask.stop()}
            onOpen={open}
            onBack={back}
            onSeeChange={() => void ask.seeChange()}
            onAskHere={() => void ask.askHere()}
            tabBar={bar("ask")}
            alone
          />
        );
      } else {
        screen = (
          <HomeFind
            status={mindStatus}
            onFixMind={fixMind}
            bar={bar("ask")}
            empty={empty}
            today={journalDay(new Date(services.now()))}
            find={home.find}
            onOpen={openFound}
            onMind={toMind}
            mind={host !== null}
          />
        );
      }
      break;
    }
  }

  return (
    <>
      {screen}
      {tabbed ? (
        <>
          {tab !== "ask" ? <KeepButton onKeep={toKeep} /> : null}
          <NavBar tab={tab} onTab={toTab} />
        </>
      ) : null}
      {asking ? (
        <VoiceAsk ios={ios} onContinue={() => asking(true)} onNotNow={() => asking(false)} />
      ) : null}
      {shared ? (
        <ShareSheet
          shared={shared}
          engine={engine}
          queue={services.queue}
          connection={conn}
          now={services.now}
          onQueued={refresh}
          alone
          onClose={() => {
            setShared(null);
            refresh();
          }}
        />
      ) : null}
    </>
  );
}
