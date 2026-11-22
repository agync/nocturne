import EventKit
import UIKit

struct Calendars: Encodable {
  let calendars: [PhoneCalendar]
}

struct Instances: Encodable {
  let windowFrom: String
  let windowTo: String
  let entries: [Entry]
}

/// The phone's calendar through EventKit, read only. Nothing here writes to it, though full
/// access would allow it.
enum CalendarReader {
  static func allowed() -> Bool {
    EKEventStore.authorizationStatus(for: .event) == .fullAccess
  }

  static func access() -> String {
    switch EKEventStore.authorizationStatus(for: .event) {
    case .fullAccess: return "granted"
    case .notDetermined: return "prompt"
    default: return "denied"
    }
  }

  static func row(_ cal: EKCalendar) -> CalendarRow {
    var rgb: (Double, Double, Double)? = nil
    if let colour = cal.cgColor {
      var r: CGFloat = 0
      var g: CGFloat = 0
      var b: CGFloat = 0
      var a: CGFloat = 0
      if UIColor(cgColor: colour).getRed(&r, green: &g, blue: &b, alpha: &a) {
        rgb = (Double(r), Double(g), Double(b))
      }
    }
    return CalendarRow(id: cal.calendarIdentifier, name: cal.title, rgb: rgb)
  }

  static func calendars(_ store: EKEventStore) -> [PhoneCalendar] {
    store.calendars(for: .event)
      .sorted { ($0.source.title, $0.title) < ($1.source.title, $1.title) }
      .map { CalendarRows.calendar(row($0)) }
  }

  private static func frequency(_ f: EKRecurrenceFrequency) -> String {
    switch f {
    case .daily: return "DAILY"
    case .weekly: return "WEEKLY"
    case .monthly: return "MONTHLY"
    case .yearly: return "YEARLY"
    @unknown default: return "DAILY"
    }
  }

  static func rule(_ r: EKRecurrenceRule) -> RuleRow {
    RuleRow(
      frequency: frequency(r.frequency),
      interval: r.interval,
      days: (r.daysOfTheWeek ?? []).map { ($0.dayOfTheWeek.rawValue, $0.weekNumber) },
      monthDays: (r.daysOfTheMonth ?? []).map(\.intValue),
      months: (r.monthsOfTheYear ?? []).map(\.intValue),
      positions: (r.setPositions ?? []).map(\.intValue),
      count: r.recurrenceEnd.flatMap { $0.occurrenceCount > 0 ? $0.occurrenceCount : nil },
      until: r.recurrenceEnd?.endDate)
  }

  static func row(_ e: EKEvent, withNotes: Bool) -> InstanceRow {
    let me = e.attendees?.first(where: \.isCurrentUser)
    return InstanceRow(
      eventId: e.eventIdentifier ?? e.calendarItemIdentifier,
      calendarId: e.calendar.calendarIdentifier,
      start: e.startDate,
      end: e.endDate,
      occurrence: e.occurrenceDate,
      allDay: e.isAllDay,
      title: e.title,
      location: e.location,
      notes: withNotes ? e.notes : nil,
      timeZone: e.timeZone,
      rule: e.recurrenceRules?.first.map(rule),
      detached: e.isDetached,
      cancelled: e.status == .canceled,
      unanswered: me?.participantStatus == .pending,
      declined: me?.participantStatus == .declined)
  }

  /// From `back` days before today to `ahead` days after it, both included, in the calendars
  /// named. EventKit gives each day of a repeat as its own event.
  static func instances(
    _ store: EKEventStore, back: Int, ahead: Int, calendarIds: [String], withNotes: Bool,
    now: Date = Date()
  ) -> Instances {
    let zone = TimeZone.current
    let window = CalendarRows.window(today: now, back: back, ahead: ahead, zone: zone)
    let wanted = Set(calendarIds)
    let cals = store.calendars(for: .event).filter { wanted.contains($0.calendarIdentifier) }
    var entries: [Entry] = []
    if !cals.isEmpty {
      let predicate = store.predicateForEvents(
        withStart: window.begin, end: window.end, calendars: cals)
      let events = store.events(matching: predicate).sorted { $0.startDate < $1.startDate }
      for e in events {
        let names = (e.attendees ?? []).filter { !$0.isCurrentUser }.map(\.name)
        if let entry = CalendarRows.entry(
          row(e, withNotes: withNotes), people: CalendarRows.people(names),
          withNotes: withNotes, phoneZone: zone)
        {
          entries.append(entry)
        }
      }
    }
    return Instances(windowFrom: window.from, windowTo: window.to, entries: entries)
  }
}
