import Foundation

/// The kinds Nocturne reads, by their names on the wire (`ReadingKind`).
enum Kind: String, CaseIterable, Codable {
  case sleep
  case heartRate
  case restingHeartRate
  case bloodPressure
  case steps
  case workout
  case weight

  /// Kept as one reading a day, worked out from many: read again whole when a day changes.
  var daily: Bool { self == .heartRate || self == .restingHeartRate || self == .steps }
}

struct Span: Equatable {
  let from: Date
  let to: Date
}

/// One sleep sample from Health: in bed, asleep (in any stage) or awake.
struct SleepSample {
  enum State { case inBed, asleep, awake }
  let from: Date
  let to: Date
  let state: State
  let device: String?
}

struct Night: Equatable {
  let start: Date
  let end: Date
  let asleep: [Span]
  let inBedOnly: Bool
  let device: String?
}

struct TimeSpan: Encodable, Equatable {
  let from: String
  let to: String
}

struct Hour: Encodable, Equatable {
  let start: String
  let low: Double
  let high: Double
  let middle: Double
}

/// One hour of heart rate as Health sums it up.
struct HourSlice {
  let start: Date
  let low: Double?
  let high: Double?
  let middle: Double?
}

/// What a reading holds, by kind (`ReadingValue`).
enum Value: Equatable {
  case sleep(asleep: [TimeSpan], inBedOnly: Bool)
  case heartRate(hours: [Hour], count: Int)
  case restingHeartRate(bpm: Double)
  case bloodPressure(top: Double, bottom: Double)
  case steps(total: Int)
  case workout(kind: String, lengthS: Int, distanceM: Double?)
  case weight(value: Double, unit: String)
}

extension Value: Encodable {
  private enum Keys: String, CodingKey {
    case type, asleep, inBedOnly, hours, count, bpm, top, bottom, total, kind, lengthS, distanceM,
      value, unit
  }

  func encode(to encoder: Encoder) throws {
    var c = encoder.container(keyedBy: Keys.self)
    switch self {
    case .sleep(let asleep, let inBedOnly):
      try c.encode("sleep", forKey: .type)
      try c.encode(asleep, forKey: .asleep)
      try c.encode(inBedOnly, forKey: .inBedOnly)
    case .heartRate(let hours, let count):
      try c.encode("heartRate", forKey: .type)
      try c.encode(hours, forKey: .hours)
      try c.encode(count, forKey: .count)
    case .restingHeartRate(let bpm):
      try c.encode("restingHeartRate", forKey: .type)
      try c.encode(bpm, forKey: .bpm)
    case .bloodPressure(let top, let bottom):
      try c.encode("bloodPressure", forKey: .type)
      try c.encode(top, forKey: .top)
      try c.encode(bottom, forKey: .bottom)
    case .steps(let total):
      try c.encode("steps", forKey: .type)
      try c.encode(total, forKey: .total)
    case .workout(let kind, let lengthS, let distanceM):
      try c.encode("workout", forKey: .type)
      try c.encode(kind, forKey: .kind)
      try c.encode(lengthS, forKey: .lengthS)
      try c.encode(distanceM, forKey: .distanceM)
    case .weight(let value, let unit):
      try c.encode("weight", forKey: .type)
      try c.encode(value, forKey: .value)
      try c.encode(unit, forKey: .unit)
    }
  }
}

/// A reading as the computer takes it (`Reading`).
struct Reading: Encodable, Equatable {
  let phoneId: String
  let day: String
  let start: String
  let end: String?
  let sourceDevice: String?
  let value: Value
}

enum HealthRows {
  static let maxSpans = 100
  static let maxHours = 48
  static let minSpan: TimeInterval = 60
  /// Sleep samples further apart than this are separate nights (or a nap).
  static let nightGap: TimeInterval = 60 * 60

  static func gregorian(_ zone: TimeZone) -> Calendar {
    var cal = Calendar(identifier: .gregorian)
    cal.timeZone = zone
    return cal
  }

  static func time(_ at: Date, _ zone: TimeZone) -> String {
    let f = ISO8601DateFormatter()
    f.formatOptions = [.withInternetDateTime]
    f.timeZone = zone
    return f.string(from: Date(timeIntervalSince1970: floor(at.timeIntervalSince1970)))
  }

  static func day(_ at: Date, _ zone: TimeZone) -> String {
    let c = gregorian(zone).dateComponents([.year, .month, .day], from: at)
    return String(format: "%04d-%02d-%02d", c.year!, c.month!, c.day!)
  }

  /// Joins overlapping spans.
  static func union(_ spans: [Span]) -> [Span] {
    var out: [Span] = []
    for s in spans.sorted(by: { $0.from < $1.from }) {
      if let last = out.last, s.from <= last.to {
        out[out.count - 1] = Span(from: last.from, to: max(last.to, s.to))
      } else {
        out.append(s)
      }
    }
    return out
  }

  static func subtract(_ spans: [Span], _ cut: [Span]) -> [Span] {
    var out = spans
    for c in cut.sorted(by: { $0.from < $1.from }) {
      out = out.flatMap { s -> [Span] in
        if c.to <= s.from || c.from >= s.to { return [s] }
        var parts: [Span] = []
        if c.from > s.from { parts.append(Span(from: s.from, to: c.from)) }
        if c.to < s.to { parts.append(Span(from: c.to, to: s.to)) }
        return parts
      }
    }
    return out
  }

  /// Health keeps sleep as samples (in bed, each stage asleep, awake), from every source. A
  /// night is a run of them with no long gap: asleep is every asleep sample less the awake
  /// ones, the gaps being the times awake. With nothing asleep, the night is time in bed.
  static func nights(_ samples: [SleepSample]) -> [Night] {
    let sorted = samples.filter { $0.to > $0.from }.sorted { $0.from < $1.from }
    var groups: [[SleepSample]] = []
    var groupEnd = Date.distantPast
    for s in sorted {
      if groups.isEmpty || s.from.timeIntervalSince(groupEnd) > nightGap {
        groups.append([s])
        groupEnd = s.to
      } else {
        groups[groups.count - 1].append(s)
        groupEnd = max(groupEnd, s.to)
      }
    }
    return groups.map(night)
  }

  static func night(_ group: [SleepSample]) -> Night {
    let start = group.map(\.from).min()!
    let end = group.map(\.to).max()!
    let device =
      group.compactMap(\.device).first(where: { $0 == "Watch" }) ?? group.compactMap(\.device).first
    let awake = group.filter { $0.state == .awake }.map { Span(from: $0.from, to: $0.to) }
    var asleep = subtract(
      union(group.filter { $0.state == .asleep }.map { Span(from: $0.from, to: $0.to) }), awake
    )
    .filter { $0.to.timeIntervalSince($0.from) >= minSpan }
    if asleep.isEmpty {
      return Night(
        start: start, end: end, asleep: [Span(from: start, to: end)], inBedOnly: true,
        device: device)
    }
    while asleep.count > maxSpans {
      // Join the two periods with the shortest time awake between them.
      let i = (0..<(asleep.count - 1)).min {
        asleep[$0 + 1].from.timeIntervalSince(asleep[$0].to)
          < asleep[$1 + 1].from.timeIntervalSince(asleep[$1].to)
      }!
      asleep.replaceSubrange(i...(i + 1), with: [Span(from: asleep[i].from, to: asleep[i + 1].to)])
    }
    return Night(start: start, end: end, asleep: asleep, inBedOnly: false, device: device)
  }

  /// Nights as readings. A night belongs to the day it ends on and is named by it; a second
  /// night ending the same day (a nap) is named by the day and its place in it. The names never
  /// look like a day, which names a daily kind's readings.
  static func sleep(_ nights: [Night], zone: TimeZone) -> [Reading] {
    var seen: [String: Int] = [:]
    return nights.sorted { $0.end < $1.end }.map { n in
      let d = day(n.end, zone)
      let k = (seen[d] ?? 0) + 1
      seen[d] = k
      return Reading(
        phoneId: nightId(d, k),
        day: d,
        start: time(n.start, zone),
        end: time(n.end, zone),
        sourceDevice: n.device,
        value: .sleep(
          asleep: n.asleep.map { TimeSpan(from: time($0.from, zone), to: time($0.to, zone)) },
          inBedOnly: n.inBedOnly))
    }
  }

  static func nightId(_ day: String, _ place: Int) -> String {
    place == 1 ? "night-\(day)" : "night-\(day)~\(place)"
  }

  /// Hourly slices grouped into one heart rate reading per day; days with no slices have none.
  static func heartRate(_ slices: [HourSlice], counts: [String: Int], zone: TimeZone) -> [Reading]
  {
    let full = slices.filter { $0.low != nil && $0.high != nil && $0.middle != nil }
    let byDay = Dictionary(grouping: full) { day($0.start, zone) }
    return byDay.keys.sorted().map { d in
      let hours = byDay[d]!.sorted { $0.start < $1.start }.prefix(maxHours).map { s -> Hour in
        let low = s.low!
        let high = max(s.high!, low)
        return Hour(start: time(s.start, zone), low: low, high: high, middle: min(max(s.middle!, low), high))
      }
      return daily(d, zone: zone, value: .heartRate(hours: Array(hours), count: counts[d] ?? 0))
    }
  }

  /// One reading for a day: its id is the day, as one day has one.
  static func daily(_ d: String, zone: TimeZone, value: Value) -> Reading {
    Reading(
      phoneId: d, day: d, start: time(startOfDay(d, zone), zone), end: nil, sourceDevice: nil,
      value: value)
  }

  static func startOfDay(_ d: String, _ zone: TimeZone) -> Date {
    let p = d.split(separator: "-").compactMap { Int($0) }
    return gregorian(zone).date(from: DateComponents(year: p[0], month: p[1], day: p[2]))!
  }

  static func workout(
    id: String, start: Date, end: Date, kind: String, distanceM: Double?, device: String?,
    zone: TimeZone
  ) -> Reading {
    Reading(
      phoneId: id,
      day: day(start, zone),
      start: time(start, zone),
      end: time(end, zone),
      sourceDevice: device,
      value: .workout(
        kind: kind, lengthS: max(0, Int(end.timeIntervalSince(start))),
        distanceM: distanceM.flatMap { $0 > 0 && $0.isFinite ? $0 : nil }))
  }

  /// One reading at a moment: blood pressure or weight.
  static func moment(id: String, at: Date, device: String?, zone: TimeZone, value: Value)
    -> Reading
  {
    Reading(
      phoneId: id, day: day(at, zone), start: time(at, zone), end: nil, sourceDevice: device,
      value: value)
  }

  /// The device as Health names it, in the words the computer shows.
  static func deviceName(productType: String?, model: String?) -> String? {
    for name in [productType, model].compactMap({ $0 }) {
      if name.hasPrefix("Watch") { return "Watch" }
      if name.hasPrefix("iPhone") { return "This phone" }
    }
    return nil
  }

  /// The days from `from` to `to`, both included.
  static func days(from: String, to: String, zone: TimeZone) -> [String] {
    let cal = gregorian(zone)
    var out: [String] = []
    var at = startOfDay(from, zone)
    let last = startOfDay(to, zone)
    while at <= last {
      out.append(day(at, zone))
      at = cal.date(byAdding: .day, value: 1, to: at)!
    }
    return out
  }

  static func daysBack(_ n: Int, from today: Date, zone: TimeZone) -> String {
    day(gregorian(zone).date(byAdding: .day, value: -n, to: today)!, zone)
  }

  /// The days to read again for a kind: those a changed sample touched, and the last few, none
  /// before `from` (the first day read) or after today.
  static func daysToRefresh(
    touched: [Date], today: Date, refreshDays: Int, from: String, zone: TimeZone
  ) -> [String] {
    let last = day(today, zone)
    var out = Set(touched.map { day($0, zone) })
    for i in 0..<max(refreshDays, 1) { out.insert(daysBack(i, from: today, zone: zone)) }
    return out.filter { $0 >= from && $0 <= last }.sorted()
  }
}
