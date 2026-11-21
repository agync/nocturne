import XCTest

@testable import tauri_plugin_nocturne_health

final class HealthRowsTests: XCTestCase {
  let london = TimeZone(identifier: "Europe/London")!
  // 2026-11-20 23:00 UTC
  let eleven = Date(timeIntervalSince1970: 1_795_215_600)

  func at(_ minutes: Double) -> Date { eleven.addingTimeInterval(minutes * 60) }

  func s(_ from: Double, _ to: Double, _ state: SleepSample.State, _ device: String? = nil)
    -> SleepSample
  {
    SleepSample(from: at(from), to: at(to), state: state, device: device)
  }

  func testANightIsItsAsleepSamplesLessTheAwakeOnes() {
    let n = HealthRows.nights([
      s(0, 480, .inBed),
      s(10, 200, .asleep, "Watch"),
      s(200, 470, .asleep, "Watch"),
      s(120, 135, .awake, "Watch"),
    ])
    XCTAssertEqual(n.count, 1)
    XCTAssertEqual(
      n[0].asleep, [Span(from: at(10), to: at(120)), Span(from: at(135), to: at(470))])
    XCTAssertFalse(n[0].inBedOnly)
    XCTAssertEqual(n[0].start, at(0))
    XCTAssertEqual(n[0].end, at(480))
    XCTAssertEqual(n[0].device, "Watch")
  }

  func testTwoSourcesOverlappingMakeOneNight() {
    let n = HealthRows.nights([
      s(0, 400, .asleep, "This phone"), s(30, 420, .asleep, "Watch"),
    ])
    XCTAssertEqual(n.count, 1)
    XCTAssertEqual(n[0].asleep, [Span(from: at(0), to: at(420))])
    XCTAssertEqual(n[0].device, "Watch")
  }

  func testOnlyTimeInBedIsTimeInBed() {
    let n = HealthRows.nights([s(0, 480, .inBed)])
    XCTAssertTrue(n[0].inBedOnly)
    XCTAssertEqual(n[0].asleep, [Span(from: at(0), to: at(480))])
  }

  func testALongGapStartsAnotherNightAndANapIsNamedByItsPlace() {
    let nights = HealthRows.nights([s(0, 420, .asleep), s(900, 960, .asleep)])
    XCTAssertEqual(nights.count, 2)
    let r = HealthRows.sleep(nights, zone: london)
    XCTAssertEqual(r.map(\.phoneId), ["night-2026-11-21", "night-2026-11-21~2"])
    XCTAssertEqual(r[0].day, "2026-11-21")
    XCTAssertEqual(r[0].start, "2026-11-20T23:00:00Z")
    XCTAssertEqual(r[0].end, "2026-11-21T06:00:00Z")
    XCTAssertEqual(
      r[0].value, .sleep(asleep: [TimeSpan(from: r[0].start, to: r[0].end!)], inBedOnly: false))
  }

  func testShortAsleepBitsAreDroppedAndTooManyAreJoined() {
    var samples = [s(0, 0.5, .asleep)]
    for i in 0..<120 { samples.append(s(10 + Double(i) * 3, 12 + Double(i) * 3, .asleep)) }
    let n = HealthRows.nights(samples)
    XCTAssertEqual(n.count, 1)
    XCTAssertEqual(n[0].asleep.count, HealthRows.maxSpans)
    XCTAssertEqual(n[0].asleep.first?.from, at(10))
  }

  func testHeartRateIsOneReadingADayByTheHour() {
    let slices = [
      HourSlice(start: at(60), low: 50, high: 70, middle: 60),
      HourSlice(start: at(120), low: 55, high: 50, middle: 90),
      HourSlice(start: at(180), low: nil, high: nil, middle: nil),
    ]
    let r = HealthRows.heartRate(slices, counts: ["2026-11-21": 12], zone: london)
    XCTAssertEqual(r.count, 1)
    XCTAssertEqual(r[0].phoneId, "2026-11-21")
    XCTAssertEqual(r[0].start, "2026-11-21T00:00:00Z")
    guard case .heartRate(let hours, let count) = r[0].value else { return XCTFail() }
    XCTAssertEqual(count, 12)
    XCTAssertEqual(hours[1], Hour(start: "2026-11-21T01:00:00Z", low: 55, high: 55, middle: 55))
  }

  func testAWorkoutHasItsLengthAndDistanceWhenThereIsOne() {
    let r = HealthRows.workout(
      id: "W", start: at(0), end: at(42), kind: "Walk", distanceM: 0, device: "Watch",
      zone: london)
    XCTAssertEqual(r.day, "2026-11-20")
    XCTAssertEqual(r.value, .workout(kind: "Walk", lengthS: 2520, distanceM: nil))
  }

  func testTheDeviceIsNamedInPlainWords() {
    XCTAssertEqual(HealthRows.deviceName(productType: "Watch7,1", model: nil), "Watch")
    XCTAssertEqual(HealthRows.deviceName(productType: "iPhone18,1", model: nil), "This phone")
    XCTAssertEqual(HealthRows.deviceName(productType: nil, model: "Watch"), "Watch")
    XCTAssertNil(HealthRows.deviceName(productType: "SomeScale", model: nil))
  }

  func testDaysToReadAgainAreTheTouchedOnesAndTheLastFew() {
    let now = at(600)  // 2026-11-21 09:00
    let d = HealthRows.daysToRefresh(
      touched: [at(-3000), at(-6000), at(3000)], today: now, refreshDays: 2, from: "2026-11-17",
      zone: london)
    XCTAssertEqual(d, ["2026-11-18", "2026-11-20", "2026-11-21"])
  }

  func testReadingsGoAcrossAsTheComputerTakesThem() throws {
    let r = HealthRows.daily("2026-11-20", zone: london, value: .steps(total: 8412))
    let json = try JSONSerialization.jsonObject(with: try JSONEncoder().encode(r)) as! [String: Any]
    XCTAssertEqual(json["phoneId"] as? String, "2026-11-20")
    XCTAssertEqual(json["start"] as? String, "2026-11-20T00:00:00Z")
    let value = json["value"] as! [String: Any]
    XCTAssertEqual(value["type"] as? String, "steps")
    XCTAssertEqual(value["total"] as? Int, 8412)
  }

  func testATokenRoundTrips() {
    let t = Token(fromDay: "2026-11-01", anchors: ["steps": ""])
    XCTAssertEqual(Token.decoded(t.encoded())?.anchors, ["steps": ""])
    XCTAssertNil(Token.decoded("not a token"))
  }
}
