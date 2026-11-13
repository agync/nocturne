import type { ItemKind, ItemView, Source } from "../../bindings";
import type { BaseContext } from "./common";

/** What the household kept, newest last. Capture and the recently kept list own this file. */
export function captureSeed({ at }: BaseContext): ItemView[] {
  const item = (
    id: string,
    kind: ItemKind,
    title: string,
    source: Source,
    keptAt: string,
    extra: Partial<ItemView> = {},
  ): ItemView => ({
    id,
    kind,
    title,
    source,
    trust: source === "pastedWeb" || source === "agent" ? "untrusted" : "trusted",
    keptAt,
    status: "understood",
    reason: null,
    durationMs: null,
    pages: null,
    fileName: null,
    hasOriginal: true,
    originalPath: `originals/${id}`,
    private: false,
    memoryCount: 1,
    understoodInMs: 4000,
    summary: null,
    text: null,
    language: "en",
    ...extra,
  });

  const items: ItemView[] = [
    item("item-letter", "image", "Letter", "drop", at(-2, 18, 12), {
      summary:
        "A letter from Willow Primary about the Year 4 trip: signature and £12 by the deadline.",
      memoryCount: 2,
    }),
    item("item-card", "image", "Appointment card", "drop", at(-10, 9, 30), {
      summary: "An appointment card from the dentist.",
    }),
    item("item-dentist-voice", "audio", "Dentist moved", "recording", at(-3, 12, 40), {
      durationMs: 8000,
      text: "They moved me to Thursday at ten.",
    }),
    item("item-swim", "text", "Swimming moved to Monday", "paste", at(-1, 17, 5), {
      text: "Swimming is now Mondays at 17:30, same pool.",
    }),
    item("item-lake", "audio", "Lake District", "recording", at(-1, 19, 22), {
      durationMs: 42000,
      text: "Let’s book the Lake District cottage for half term.",
    }),
    item("item-gran", "text", "Gran’s new number", "quickCapture", at(-1, 20, 1), {
      text: "Gran’s new number ends 4471.",
    }),
    item("item-coach", "text", "Swimming with Coach Sam", "quickCapture", at(-9, 10, 15), {
      text: "Leo swims with Coach Sam on Wednesdays.",
    }),
    item("item-gym", "text", "Gym door code", "quickCapture", at(-95, 8, 0), {
      text: "Gym door code is 4821.",
    }),
    item("item-pizza", "text", "Pizza place on Elm Street", "quickCapture", at(-120, 19, 0), {
      text: "Good pizza place on Elm Street.",
    }),
    item("item-gp", "image", "GP letter", "drop", at(-250, 11, 0), {
      summary: "A letter from the GP about a penicillin allergy.",
    }),
    item("item-prescription", "image", "Prescription", "drop", at(-3, 15, 0), {
      summary: "A prescription for Gran: amlodipine 2.5 mg, once a day.",
    }),
    item("item-nut", "web", "Nut-free lunchbox ideas", "pastedWeb", at(-4, 13, 0), {
      status: "unconfirmed",
      text: "Leo is allergic to nuts so I always...",
    }),
    item("item-dentist-chen", "text", "Dr Chen", "quickCapture", at(-30, 9, 0), {
      text: "Leo’s dentist is Dr Chen at Willow Dental.",
    }),
    item("item-counciltax", "document", "council-tax-2026.pdf", "drop", at(-1, 9, 9), {
      status: "couldntRead",
      reason: "locked",
      fileName: "council-tax-2026.pdf",
      pages: 4,
      memoryCount: 0,
      understoodInMs: null,
    }),
    item("item-gran-court", "text", "Where Gran lives", "quickCapture", at(-40, 11, 0), {
      text: "Gran lives at Rowan Court.",
    }),
    item("item-jo", "text", "Swimming teacher", "quickCapture", at(-66, 16, 0), {
      text: "Ava’s swimming teacher is called Jo.",
    }),
    item("item-bin", "text", "Bin day", "paste", at(-84, 8, 0), {
      text: "Bin day moved to Tuesdays over summer.",
    }),
    item("item-parking", "image", "Parking permit renewal", "quickCapture", at(0, 6, 41), {
      status: "reading",
      memoryCount: 0,
      understoodInMs: null,
    }),
    item("item-sam", "audio", "Ask Sam about Saturday", "recording", at(0, 6, 15), {
      durationMs: 6000,
      text: "Ask Sam about Saturday.",
    }),
    item("item-whatsapp", "text", "Class WhatsApp, 3 messages", "paste", at(0, 5, 58), {
      text: "Class WhatsApp: reminders about swimming kit and the school trip.",
    }),
    item("item-inbox-card", "image", "Dentist appointment card", "watchedFolder", at(-1, 8, 30), {
      status: "needsYou",
      memoryCount: 0,
      understoodInMs: null,
    }),
    item("item-lunchbox", "web", "Lunchbox recipe ideas", "pastedWeb", at(-1, 13, 5), {
      status: "unconfirmed",
      text: "Quick lunchbox recipes for school days.",
    }),
    item("item-timetable", "image", "Swimming timetable", "drop", at(-1, 10, 0), {
      summary: "The pool timetable for the winter term.",
    }),
  ];

  return items;
}
