import CryptoKit
import Foundation

/// One of the phone's calendars, as read.
struct CalendarRow {
  let id: String
  let name: String?
  /// Red, green and blue from 0 to 1, when the calendar has a colour.
  let rgb: (Double, Double, Double)?
}

struct PhoneCalendar: Encodable, Equatable {
  let id: String
  let name: String
  let colour: String?
}

/// How an appointment repeats, as EventKit holds it.
struct RuleRow {
  /// DAILY, WEEKLY, MONTHLY or YEARLY.
  let frequency: String
  let interval: Int
  /// Day of the week from 1 (Sunday) to 7, with the week in the month or year (0 for every).
  let days: [(day: Int, week: Int)]
  let monthDays: [Int]
  let months: [Int]
  let positions: [Int]
  let count: Int?
  let until: Date?
}

/// One appointment on one day, as EventKit gives it.
struct InstanceRow {
  let eventId: String
  let calendarId: String
  let start: Date
  let end: Date
  /// When this day of a repeat was due to begin, before any edit moved it.
  let occurrence: Date?
  let allDay: Bool
  let title: String?
  let location: String?
  let notes: String?
  let timeZone: TimeZone?
  let rule: RuleRow?
  /// An edited day of a repeat.
  let detached: Bool
  let cancelled: Bool
  /// The person's own answer to an invitation: unanswered and declined are left out.
  let unanswered: Bool
  let declined: Bool
}

/// An appointment as the computer takes it (`CalendarEntry`).
struct Entry: Encodable, Equatable {
  let phoneId: String
  let calendarId: String
  let title: String
  let start: String
  let end: String
  let timeZone: String?
  let allDay: Bool
  let place: String?
  let people: [String]
  let repeats: Bool
  let notes: String?
  let repeatRule: String?
}

/// The days read, both included, in the phone's zone, and their bounds for the query.
struct Window: Equatable {
  let from: String
  let to: String
  let begin: Date
  let end: Date
}

enum CalendarRows {
  /// The computer refuses a field longer than this.
  static let maxField = 2000
  static let maxPeople = 100
  /// The computer keeps a calendar's name to this many characters.
  static let maxName = 200

  static func colour(_ rgb: (Double, Double, Double)?) -> String? {
    guard let (r, g, b) = rgb else { return nil }
    func byte(_ v: Double) -> Int { Int((min(max(v, 0), 1) * 255).rounded()) }
    return String(format: "#%02X%02X%02X", byte(r), byte(g), byte(b))
  }

  static func calendar(_ row: CalendarRow) -> PhoneCalendar {
    let name = row.name?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
    return PhoneCalendar(
      id: row.id,
      name: cut(name.isEmpty ? row.id : name, maxName),
      colour: colour(row.rgb))
  }

  static func gregorian(_ zone: TimeZone) -> Calendar {
    var cal = Calendar(identifier: .gregorian)
    cal.timeZone = zone
    return cal
  }

  static func day(_ date: Date, _ zone: TimeZone) -> String {
    let c = gregorian(zone).dateComponents([.year, .month, .day], from: date)
    return String(format: "%04d-%02d-%02d", c.year!, c.month!, c.day!)
  }

  /// From `back` days before today to `ahead` days after, both included, in the phone's zone.
  static func window(today: Date, back: Int, ahead: Int, zone: TimeZone) -> Window {
    let cal = gregorian(zone)
    let midnight = cal.startOfDay(for: today)
    let from = cal.date(byAdding: .day, value: -back, to: midnight)!
    let to = cal.date(byAdding: .day, value: ahead, to: midnight)!
    return Window(
      from: day(from, zone), to: day(to, zone), begin: from,
      end: cal.date(byAdding: .day, value: 1, to: to)!)
  }

  /// A stable, short id from what names an appointment on the phone.
  static func idOf(_ seed: String) -> String {
    let digest = SHA256.hash(data: Data(seed.utf8))
    let b64 = Data(digest).base64EncodedString()
      .replacingOccurrences(of: "+", with: "-")
      .replacingOccurrences(of: "/", with: "_")
      .replacingOccurrences(of: "=", with: "")
    return String(b64.prefix(22))
  }

  static func millis(_ date: Date) -> Int64 {
    Int64((date.timeIntervalSince1970 * 1000).rounded())
  }

  /// The phone's own id for an appointment, the same however often it moves: a one-off by its
  /// event, and each day of a repeat by its event and the day it was due, so an edited day is a
  /// change, not a new entry.
  static func phoneId(_ row: InstanceRow) -> String {
    if row.rule == nil && !row.detached { return idOf(row.eventId) }
    return idOf("\(row.eventId):\(millis(row.occurrence ?? row.start))")
  }

  /// A timed entry's instant in its zone, to the second.
  static func time(_ date: Date, _ zone: TimeZone) -> String {
    let f = ISO8601DateFormatter()
    f.formatOptions = [.withInternetDateTime]
    f.timeZone = zone
    return f.string(from: Date(timeIntervalSince1970: floor(date.timeIntervalSince1970)))
  }

  /// The names of the people invited, without the person and without bare addresses.
  static func people(_ names: [String?]) -> [String] {
    var seen = Set<String>()
    var out: [String] = []
    for n in names {
      guard let name = n?.trimmingCharacters(in: .whitespacesAndNewlines), !name.isEmpty,
        !name.contains("@")
      else { continue }
      let c = cut(name)
      if seen.insert(c).inserted { out.append(c) }
      if out.count == maxPeople { break }
    }
    return out
  }

  /// At most `max` characters, counted as the computer counts them.
  static func cut(_ s: String, _ max: Int = maxField) -> String {
    let scalars = s.unicodeScalars
    if scalars.count <= max { return s }
    var v = String.UnicodeScalarView()
    v.append(contentsOf: scalars.prefix(max))
    return String(v)
  }

  private static func text(_ s: String?) -> String? {
    guard let t = s?.trimmingCharacters(in: .whitespacesAndNewlines), !t.isEmpty else {
      return nil
    }
    return cut(t)
  }

  private static let weekdays = ["SU", "MO", "TU", "WE", "TH", "FR", "SA"]

  /// The repeat as an iCalendar `RRULE`, the form the computer keeps.
  static func rrule(_ r: RuleRow) -> String {
    var parts = ["FREQ=\(r.frequency)"]
    if r.interval > 1 { parts.append("INTERVAL=\(r.interval)") }
    if let n = r.count, n > 0 {
      parts.append("COUNT=\(n)")
    } else if let until = r.until {
      let f = DateFormatter()
      f.locale = Locale(identifier: "en_US_POSIX")
      f.timeZone = TimeZone(identifier: "UTC")
      f.dateFormat = "yyyyMMdd'T'HHmmss'Z'"
      parts.append("UNTIL=\(f.string(from: until))")
    }
    let days = r.days.compactMap { d -> String? in
      guard (1...7).contains(d.day) else { return nil }
      return (d.week == 0 ? "" : "\(d.week)") + weekdays[d.day - 1]
    }
    if !days.isEmpty { parts.append("BYDAY=" + days.joined(separator: ",")) }
    if !r.monthDays.isEmpty {
      parts.append("BYMONTHDAY=" + r.monthDays.map(String.init).joined(separator: ","))
    }
    if !r.months.isEmpty {
      parts.append("BYMONTH=" + r.months.map(String.init).joined(separator: ","))
    }
    if !r.positions.isEmpty {
      parts.append("BYSETPOS=" + r.positions.map(String.init).joined(separator: ","))
    }
    return parts.joined(separator: ";")
  }

  /// One appointment on one day, or nil for what is left out: an invitation not answered or
  /// declined, a cancelled entry, or one without a title. Notes only when asked for.
  static func entry(_ row: InstanceRow, people: [String], withNotes: Bool, phoneZone: TimeZone)
    -> Entry?
  {
    if row.cancelled || row.unanswered || row.declined { return nil }
    guard let title = text(row.title) else { return nil }
    let zone = row.timeZone ?? phoneZone
    let end = max(row.end, row.start)
    let start: String
    let finish: String
    if row.allDay {
      // EventKit ends an all-day entry on the last second of its last day, or at the midnight
      // after it; the computer takes the day after it, as on Android.
      start = day(row.start, phoneZone)
      let cal = gregorian(phoneZone)
      let midnight = end > row.start && cal.startOfDay(for: end) == end
      let last = cal.startOfDay(for: midnight ? end.addingTimeInterval(-1) : end)
      finish = day(cal.date(byAdding: .day, value: 1, to: last)!, phoneZone)
    } else {
      start = time(row.start, zone)
      finish = time(end, zone)
    }
    let rule = row.detached ? nil : row.rule.map { cut(rrule($0)) }
    return Entry(
      phoneId: phoneId(row),
      calendarId: row.calendarId,
      title: title,
      start: start,
      end: finish,
      timeZone: row.allDay ? nil : zone.identifier,
      allDay: row.allDay,
      place: text(row.location),
      people: people,
      repeats: row.rule != nil || row.detached,
      notes: withNotes ? text(row.notes) : nil,
      repeatRule: rule)
  }
}
