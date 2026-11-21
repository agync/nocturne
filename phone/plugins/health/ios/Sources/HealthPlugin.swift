import HealthKit
import Tauri
import UIKit

struct AskArgs: Decodable {
  var kinds: [String]?
  var history: Bool?
}

struct ReadArgs: Decodable {
  var kinds: [String]?
  var days: Int?
}

struct ChangesArgs: Decodable {
  var kinds: [String]?
  var token: String?
  var refreshDays: Int?
}

struct SettingsArgs: Decodable {
  var target: String?
}

struct Available: Encodable {
  let status: String
  let olderReadings: Bool
}

struct Granted: Encodable {
  let kinds: [Kind]
  let history: Bool
  let distance: Bool
  /// Health never says which kinds it refused, so a missing reading never reads as deleted.
  let refusalsHidden: Bool
}

struct FullRead: Encodable {
  let readings: [Reading]
  let token: String
  let fromDay: String
}

struct Changes: Encodable {
  let readings: [Reading]
  let removed: [String]
  let token: String?
  let expired: Bool
  let refreshed: [String: [String]]
}

struct Route: Encodable {
  let route: String?
}

/// Where each kind's changes were read up to, per kind, and the first day read.
struct Token: Codable {
  var fromDay: String
  /// An archived `HKQueryAnchor` per kind; empty when nothing was there to read yet.
  var anchors: [String: String]

  func encoded() -> String {
    (try? JSONEncoder().encode(self))?.base64EncodedString() ?? ""
  }

  static func decoded(_ s: String) -> Token? {
    Data(base64Encoded: s).flatMap { try? JSONDecoder().decode(Token.self, from: $0) }
  }

  static func archive(_ anchor: HKQueryAnchor?) -> String {
    guard let anchor,
      let data = try? NSKeyedArchiver.archivedData(
        withRootObject: anchor, requiringSecureCoding: true)
    else { return "" }
    return data.base64EncodedString()
  }

  static func unarchive(_ s: String) -> HKQueryAnchor? {
    Data(base64Encoded: s).flatMap {
      try? NSKeyedUnarchiver.unarchivedObject(ofClass: HKQueryAnchor.self, from: $0)
    }
  }
}

enum HealthError: Error {
  case expired
}

/// What each kind is read from, and what is asked to read it.
enum HealthTypes {
  static let distances: [HKQuantityType] = [
    HKQuantityType(.distanceWalkingRunning), HKQuantityType(.distanceCycling),
    HKQuantityType(.distanceSwimming),
  ]

  static func sample(_ k: Kind) -> HKSampleType {
    switch k {
    case .sleep: return HKCategoryType(.sleepAnalysis)
    case .heartRate: return HKQuantityType(.heartRate)
    case .restingHeartRate: return HKQuantityType(.restingHeartRate)
    case .bloodPressure: return HKCorrelationType(.bloodPressure)
    case .steps: return HKQuantityType(.stepCount)
    case .workout: return HKWorkoutType.workoutType()
    case .weight: return HKQuantityType(.bodyMass)
    }
  }

  /// Read only: distance goes with workouts, to give them their length on the road. Never the
  /// route.
  static func toRead(_ k: Kind) -> Set<HKObjectType> {
    switch k {
    case .bloodPressure:
      return [HKQuantityType(.bloodPressureSystolic), HKQuantityType(.bloodPressureDiastolic)]
    case .workout:
      return Set([HKWorkoutType.workoutType()] + distances)
    default:
      return [sample(k)]
    }
  }

  static func kinds(_ wire: [String]?) -> [Kind] {
    var out: [Kind] = []
    for w in wire ?? [] {
      if let k = Kind(rawValue: w), !out.contains(k) { out.append(k) }
    }
    return out
  }

  /// The word the computer shows ("Walk · 42 min").
  static func workoutKind(_ t: HKWorkoutActivityType) -> String {
    switch t {
    case .walking: return "Walk"
    case .running: return "Run"
    case .swimming: return "Swim"
    case .cycling: return "Cycle"
    case .hiking: return "Hike"
    case .yoga: return "Yoga"
    case .pilates: return "Pilates"
    case .socialDance, .cardioDance: return "Dance"
    case .rowing: return "Row"
    case .traditionalStrengthTraining, .functionalStrengthTraining: return "Strength"
    case .tennis: return "Tennis"
    default: return "Workout"
    }
  }

  static func distance(_ t: HKWorkoutActivityType) -> [HKQuantityType] {
    switch t {
    case .cycling: return [HKQuantityType(.distanceCycling)]
    case .swimming: return [HKQuantityType(.distanceSwimming)]
    default: return [HKQuantityType(.distanceWalkingRunning)]
    }
  }
}

/// Apple Health, read only, as plain calls that wait for HealthKit. Run off the main thread.
/// Health never says which kinds it refused: a refused kind reads as no readings.
final class HealthReader {
  let store = HKHealthStore()

  private func wait<T>(_ body: (@escaping (Result<T, Error>) -> Void) -> Void) throws -> T {
    let done = DispatchSemaphore(value: 0)
    var out: Result<T, Error>!
    body { r in
      out = r
      done.signal()
    }
    done.wait()
    return try out.get()
  }

  /// Health has not been asked about this kind, or never answers: nothing to read.
  private func quiet(_ error: Error) -> Bool {
    guard let e = error as? HKError else { return false }
    return e.code == .errorAuthorizationDenied || e.code == .errorAuthorizationNotDetermined
  }

  func asked(_ k: Kind) -> Bool {
    let status: HKAuthorizationRequestStatus? = try? wait { done in
      store.getRequestStatusForAuthorization(toShare: [], read: HealthTypes.toRead(k)) { s, e in
        done(e.map { .failure($0) } ?? .success(s))
      }
    }
    return status == .unnecessary
  }

  func granted() -> Granted {
    let kinds = Kind.allCases.filter(asked)
    return Granted(
      kinds: kinds, history: true, distance: kinds.contains(.workout), refusalsHidden: true)
  }

  func anchored(_ type: HKSampleType, predicate: NSPredicate?, anchor: HKQueryAnchor?) throws
    -> (added: [HKSample], deleted: [HKDeletedObject], anchor: HKQueryAnchor?)
  {
    do {
      return try wait { done in
        let q = HKAnchoredObjectQuery(
          type: type, predicate: predicate, anchor: anchor, limit: HKObjectQueryNoLimit
        ) { _, added, deleted, next, error in
          if let error { return done(.failure(error)) }
          done(.success((added ?? [], deleted ?? [], next)))
        }
        store.execute(q)
      }
    } catch {
      if quiet(error) { return ([], [], anchor) }
      if let e = error as? HKError, e.code == .errorInvalidArgument { throw HealthError.expired }
      throw error
    }
  }

  func samples(_ type: HKSampleType, from: Date, to: Date) throws -> [HKSample] {
    do {
      return try wait { done in
        let q = HKSampleQuery(
          sampleType: type,
          predicate: HKQuery.predicateForSamples(withStart: from, end: to),
          limit: HKObjectQueryNoLimit, sortDescriptors: nil
        ) { _, found, error in
          if let error { return done(.failure(error)) }
          done(.success(found ?? []))
        }
        store.execute(q)
      }
    } catch {
      if quiet(error) { return [] }
      throw error
    }
  }

  func statistics(
    _ type: HKQuantityType, _ options: HKStatisticsOptions, from: Date, to: Date,
    every interval: DateComponents
  ) throws -> [HKStatistics] {
    do {
      return try wait { done in
        let q = HKStatisticsCollectionQuery(
          quantityType: type,
          quantitySamplePredicate: HKQuery.predicateForSamples(withStart: from, end: to),
          options: options, anchorDate: from, intervalComponents: interval)
        q.initialResultsHandler = { _, results, error in
          if let error { return done(.failure(error)) }
          var out: [HKStatistics] = []
          results?.enumerateStatistics(from: from, to: to) { s, _ in out.append(s) }
          done(.success(out))
        }
        store.execute(q)
      }
    } catch {
      if quiet(error) { return [] }
      throw error
    }
  }

  // MARK: Mapping

  static func device(_ s: HKSample) -> String? {
    HealthRows.deviceName(productType: s.sourceRevision.productType, model: s.device?.model)
  }

  static func sleepSample(_ s: HKSample) -> SleepSample? {
    guard let c = s as? HKCategorySample,
      let v = HKCategoryValueSleepAnalysis(rawValue: c.value)
    else { return nil }
    let state: SleepSample.State
    switch v {
    case .inBed: state = .inBed
    case .awake: state = .awake
    default: state = .asleep
    }
    return SleepSample(from: c.startDate, to: c.endDate, state: state, device: device(c))
  }

  static func reading(_ s: HKSample, zone: TimeZone) -> Reading? {
    let id = s.uuid.uuidString
    switch s {
    case let w as HKWorkout:
      let metres = HealthTypes.distance(w.workoutActivityType).lazy
        .compactMap { w.statistics(for: $0)?.sumQuantity()?.doubleValue(for: .meter()) }
        .first
      return HealthRows.workout(
        id: id, start: w.startDate, end: w.endDate,
        kind: HealthTypes.workoutKind(w.workoutActivityType), distanceM: metres,
        device: device(w), zone: zone)
    case let c as HKCorrelation:
      let mmHg = HKUnit.millimeterOfMercury()
      guard
        let top = c.objects(for: HKQuantityType(.bloodPressureSystolic)).first
          as? HKQuantitySample,
        let bottom = c.objects(for: HKQuantityType(.bloodPressureDiastolic)).first
          as? HKQuantitySample
      else { return nil }
      return HealthRows.moment(
        id: id, at: c.startDate, device: device(c), zone: zone,
        value: .bloodPressure(
          top: top.quantity.doubleValue(for: mmHg), bottom: bottom.quantity.doubleValue(for: mmHg)))
    case let q as HKQuantitySample where q.quantityType == HKQuantityType(.bodyMass):
      return HealthRows.moment(
        id: id, at: q.startDate, device: device(q), zone: zone,
        value: .weight(value: q.quantity.doubleValue(for: .gramUnit(with: .kilo)), unit: "kg"))
    default:
      return nil
    }
  }

  // MARK: Reads

  /// Nights ending on the days given, from the samples around them.
  func nights(days: [String], zone: TimeZone) throws -> [Reading] {
    guard let first = days.min(), let last = days.max() else { return [] }
    let from = HealthRows.startOfDay(first, zone).addingTimeInterval(-86_400)
    let to = HealthRows.startOfDay(last, zone).addingTimeInterval(86_400)
    let found = try samples(HealthTypes.sample(.sleep), from: from, to: to)
    let wanted = Set(days)
    return HealthRows.sleep(HealthRows.nights(found.compactMap(Self.sleepSample)), zone: zone)
      .filter { wanted.contains($0.day) }
  }

  /// One reading a day for a daily kind, over the days given. Heart rate counts its samples:
  /// those already read when given, else read here.
  func daily(_ k: Kind, days: [String], zone: TimeZone, samples known: [HKSample]? = nil) throws
    -> [Reading]
  {
    guard let first = days.min(), let last = days.max() else { return [] }
    let from = HealthRows.startOfDay(first, zone)
    let to = HealthRows.gregorian(zone).date(
      byAdding: .day, value: 1, to: HealthRows.startOfDay(last, zone))!
    let wanted = Set(days)
    let perMinute = HKUnit.count().unitDivided(by: .minute())
    var out: [Reading] = []
    switch k {
    case .heartRate:
      let type = HKQuantityType(.heartRate)
      var counts: [String: Int] = [:]
      for s in try known ?? samples(type, from: from, to: to) {
        counts[HealthRows.day(s.startDate, zone), default: 0] += 1
      }
      let slices = try statistics(
        type, [.discreteMin, .discreteMax, .discreteAverage], from: from, to: to,
        every: DateComponents(hour: 1)
      ).map {
        HourSlice(
          start: $0.startDate, low: $0.minimumQuantity()?.doubleValue(for: perMinute),
          high: $0.maximumQuantity()?.doubleValue(for: perMinute),
          middle: $0.averageQuantity()?.doubleValue(for: perMinute))
      }
      out = HealthRows.heartRate(slices, counts: counts, zone: zone)
    case .restingHeartRate:
      for s in try statistics(
        HKQuantityType(.restingHeartRate), .discreteAverage, from: from, to: to,
        every: DateComponents(day: 1))
      {
        if let bpm = s.averageQuantity()?.doubleValue(for: perMinute) {
          out.append(
            HealthRows.daily(
              HealthRows.day(s.startDate, zone), zone: zone, value: .restingHeartRate(bpm: bpm)))
        }
      }
    case .steps:
      for s in try statistics(
        HKQuantityType(.stepCount), .cumulativeSum, from: from, to: to,
        every: DateComponents(day: 1))
      {
        if let total = s.sumQuantity()?.doubleValue(for: .count()), total > 0 {
          out.append(
            HealthRows.daily(
              HealthRows.day(s.startDate, zone), zone: zone,
              value: .steps(total: Int(total.rounded()))))
        }
      }
    default:
      break
    }
    return out.filter { wanted.contains($0.day) }
  }

  /// Everything in the kinds asked for since `days` ago, and where each kind was read up to.
  func read(_ kinds: [Kind], days: Int, now: Date = Date()) throws -> FullRead {
    let zone = TimeZone.current
    let fromDay = HealthRows.daysBack(max(days - 1, 0), from: now, zone: zone)
    let today = HealthRows.day(now, zone)
    let start = HealthRows.startOfDay(fromDay, zone)
    let all = HealthRows.days(from: fromDay, to: today, zone: zone)
    var token = Token(fromDay: fromDay, anchors: [:])
    var readings: [Reading] = []
    for k in kinds {
      // The anchor is taken with the read, so nothing that changes while reading is missed.
      let since = k == .sleep ? start.addingTimeInterval(-86_400) : start
      let got = try anchored(
        HealthTypes.sample(k), predicate: HKQuery.predicateForSamples(withStart: since, end: nil),
        anchor: nil)
      token.anchors[k.rawValue] = Token.archive(got.anchor)
      if k.daily {
        readings += try daily(k, days: all, zone: zone, samples: got.added)
      } else if k == .sleep {
        readings += try nights(days: all, zone: zone)
      } else {
        readings += got.added.compactMap { Self.reading($0, zone: zone) }
      }
    }
    return FullRead(readings: readings, token: token.encoded(), fromDay: fromDay)
  }

  /// What changed since the token. Each daily kind and sleep is read again over the days a
  /// change touched and the last `refreshDays`. Health never says which kinds it refused, so a
  /// reading is removed only when Health reports a deletion: for a deleted workout, blood
  /// pressure or weight by its id; for a daily kind or sleep, whose deletions say no day, by
  /// reading the last month again and naming those days in `refreshed`.
  func changes(_ kinds: [Kind], token raw: String, refreshDays: Int, now: Date = Date()) throws
    -> Changes
  {
    let expired = Changes(readings: [], removed: [], token: nil, expired: true, refreshed: [:])
    guard var token = Token.decoded(raw) else { return expired }
    let zone = TimeZone.current
    let start = HealthRows.startOfDay(token.fromDay, zone)
    let month = HealthRows.days(
      from: max(token.fromDay, HealthRows.daysBack(29, from: now, zone: zone)),
      to: HealthRows.day(now, zone), zone: zone)
    var readings: [Reading] = []
    var removed: [String] = []
    var refreshed: [String: [String]] = [:]
    for k in kinds {
      guard let saved = token.anchors[k.rawValue] else { return expired }
      let anchor = Token.unarchive(saved)
      if anchor == nil && !saved.isEmpty { return expired }
      let since = k == .sleep ? start.addingTimeInterval(-86_400) : start
      let got: (added: [HKSample], deleted: [HKDeletedObject], anchor: HKQueryAnchor?)
      do {
        got = try anchored(
          HealthTypes.sample(k), predicate: HKQuery.predicateForSamples(withStart: since, end: nil),
          anchor: anchor)
      } catch HealthError.expired {
        return expired
      }
      token.anchors[k.rawValue] = Token.archive(got.anchor ?? anchor)
      let deleted = !got.deleted.isEmpty
      if k.daily || k == .sleep {
        if got.added.isEmpty && !deleted && k == .sleep { continue }
        let touched = got.added.map { k == .sleep ? $0.endDate : $0.startDate }
        var days = Set(
          HealthRows.daysToRefresh(
            touched: touched, today: now, refreshDays: refreshDays, from: token.fromDay,
            zone: zone))
        if deleted {
          days.formUnion(month)
          refreshed[k.rawValue] = month
        }
        let sorted = days.sorted()
        readings += try k == .sleep ? nights(days: sorted, zone: zone) : daily(k, days: sorted, zone: zone)
      } else {
        removed += got.deleted.map { $0.uuid.uuidString }
        readings += got.added.compactMap { Self.reading($0, zone: zone) }
      }
    }
    return Changes(
      readings: readings, removed: removed, token: token.encoded(), expired: false,
      refreshed: refreshed)
  }
}

class HealthPlugin: Plugin {
  private let reader = HealthReader()
  private let queue = DispatchQueue(label: "app.nocturne.health", qos: .userInitiated)

  /// Each read runs off the main thread, because the app's Rust side waits for the answer.
  private func work(_ invoke: Invoke, _ body: @escaping (HealthReader) throws -> Void) {
    guard HKHealthStore.isHealthDataAvailable() else { return invoke.reject("unavailable") }
    let reader = self.reader
    queue.async {
      do {
        try body(reader)
      } catch {
        invoke.reject(error.localizedDescription)
      }
    }
  }

  @objc public func availability(_ invoke: Invoke) {
    invoke.resolve(
      Available(
        status: HKHealthStore.isHealthDataAvailable() ? "available" : "missing",
        olderReadings: true))
  }

  @objc public func granted(_ invoke: Invoke) {
    work(invoke) { invoke.resolve($0.granted()) }
  }

  /// Apple Health's own sheet for these kinds, to read only. It never says what was allowed.
  @objc public func request(_ invoke: Invoke) throws {
    let args = try invoke.parseArgs(AskArgs.self)
    let kinds = HealthTypes.kinds(args.kinds)
    let types = kinds.reduce(into: Set<HKObjectType>()) { $0.formUnion(HealthTypes.toRead($1)) }
    let queue = self.queue
    work(invoke) { reader in
      reader.store.requestAuthorization(toShare: [], read: types) { _, _ in
        queue.async { invoke.resolve(reader.granted()) }
      }
    }
  }

  @objc public func read(_ invoke: Invoke) throws {
    let args = try invoke.parseArgs(ReadArgs.self)
    work(invoke) { reader in
      invoke.resolve(try reader.read(HealthTypes.kinds(args.kinds), days: args.days ?? 30))
    }
  }

  @objc public func changes(_ invoke: Invoke) throws {
    let args = try invoke.parseArgs(ChangesArgs.self)
    work(invoke) { reader in
      invoke.resolve(
        try reader.changes(
          HealthTypes.kinds(args.kinds), token: args.token ?? "",
          refreshDays: args.refreshDays ?? 2))
    }
  }

  /// The Health app, where Nocturne's kinds are turned on and off; there is no store to open.
  @objc public func openSettings(_ invoke: Invoke) throws {
    let args = try invoke.parseArgs(SettingsArgs.self)
    let url =
      args.target == "store"
      ? URL(string: UIApplication.openSettingsURLString)!
      : URL(string: "x-apple-health://")!
    DispatchQueue.main.async {
      UIApplication.shared.open(url) { ok in
        ok ? invoke.resolve() : invoke.reject("failed")
      }
    }
  }

  /// Health never opens the app on a page of its own.
  @objc public func takeRoute(_ invoke: Invoke) {
    invoke.resolve(Route(route: nil))
  }
}

@_cdecl("init_plugin_nocturne_health")
func initPlugin() -> Plugin {
  return HealthPlugin()
}
