import XCTest

@testable import tauri_plugin_nocturne_calendar

final class CalendarRowsTests: XCTestCase {
  let london = TimeZone(identifier: "Europe/London")!
  // 2026-11-23 09:00 UTC
  let nine = Date(timeIntervalSince1970: 1_795_424_400)

  func row(
    title: String? = "Dentist",
    allDay: Bool = false,
    start: Date? = nil,
    end: Date? = nil,
    occurrence: Date? = nil,
    zone: String? = "Europe/London",
    rule: RuleRow? = nil,
    detached: Bool = false,
    cancelled: Bool = false,
    unanswered: Bool = false,
    declined: Bool = false
  ) -> InstanceRow {
    let s = start ?? nine
    return InstanceRow(
      eventId: "E1", calendarId: "C3", start: s, end: end ?? s.addingTimeInterval(1800),
      occurrence: occurrence ?? s, allDay: allDay, title: title, location: " Smile Dental ",
      notes: "Call link: example", timeZone: zone.flatMap(TimeZone.init(identifier:)),
      rule: rule, detached: detached, cancelled: cancelled, unanswered: unanswered,
      declined: declined)
  }

  let weekly = RuleRow(
    frequency: "WEEKLY", interval: 1, days: [(day: 6, week: 0)], monthDays: [], months: [],
    positions: [], count: nil, until: nil)

  func testATimedEntryIsItsInstantInItsZone() {
    let e = CalendarRows.entry(row(), people: ["Sam"], withNotes: false, phoneZone: london)!
    XCTAssertEqual(e.start, "2026-11-23T09:00:00Z")
    XCTAssertEqual(e.end, "2026-11-23T09:30:00Z")
    XCTAssertEqual(e.timeZone, "Europe/London")
    XCTAssertEqual(e.place, "Smile Dental")
    XCTAssertEqual(e.people, ["Sam"])
    XCTAssertEqual(e.calendarId, "C3")
    XCTAssertFalse(e.allDay)
    XCTAssertFalse(e.repeats)
    XCTAssertNil(e.repeatRule)
  }

  func testAnEntryInAnotherZoneKeepsItsOwnOffset() {
    let e = CalendarRows.entry(
      row(zone: "America/New_York"), people: [], withNotes: false, phoneZone: london)!
    XCTAssertEqual(e.start, "2026-11-23T04:00:00-05:00")
    XCTAssertEqual(e.timeZone, "America/New_York")
  }

  func testAnEntryWithoutAZoneTakesThePhones() {
    let e = CalendarRows.entry(row(zone: nil), people: [], withNotes: false, phoneZone: london)!
    XCTAssertEqual(e.timeZone, "Europe/London")
  }

  func testAnAllDayEntryIsItsDatesWithTheEndAfterIt() {
    // 2026-11-22 00:00 to 23:59:59 in London, as EventKit gives a one-day entry.
    let begin = Date(timeIntervalSince1970: 1_795_305_600)
    let e = CalendarRows.entry(
      row(allDay: true, start: begin, end: begin.addingTimeInterval(86_399), zone: nil),
      people: [], withNotes: false, phoneZone: london)!
    XCTAssertEqual(e.start, "2026-11-22")
    XCTAssertEqual(e.end, "2026-11-23")
    XCTAssertNil(e.timeZone)
    XCTAssertTrue(e.allDay)
  }

  func testAnAllDayEntryEndingAtMidnightEndsTheDayAfter() {
    let begin = Date(timeIntervalSince1970: 1_795_305_600)
    let e = CalendarRows.entry(
      row(allDay: true, start: begin, end: begin.addingTimeInterval(2 * 86_400), zone: nil),
      people: [], withNotes: false, phoneZone: london)!
    XCTAssertEqual(e.start, "2026-11-22")
    XCTAssertEqual(e.end, "2026-11-24")
  }

  func testNotesComeOnlyWhenAsked() {
    XCTAssertEqual(
      CalendarRows.entry(row(), people: [], withNotes: true, phoneZone: london)!.notes,
      "Call link: example")
    XCTAssertNil(CalendarRows.entry(row(), people: [], withNotes: false, phoneZone: london)!.notes)
  }

  func testWhatIsLeftOut() {
    for r in [
      row(cancelled: true), row(unanswered: true), row(declined: true), row(title: "  "),
      row(title: nil),
    ] {
      XCTAssertNil(CalendarRows.entry(r, people: [], withNotes: false, phoneZone: london))
    }
  }

  func testAOneOffIsNamedByItsEvent() {
    XCTAssertEqual(CalendarRows.phoneId(row()), "IiSaph_SWU5QkQqldK-7l3")
  }

  func testADayOfARepeatIsNamedByItsEventAndDay() {
    XCTAssertEqual(CalendarRows.phoneId(row(rule: weekly)), "OsX4iMl-Pav4_8YYSj2reF")
    let next = row(start: nine.addingTimeInterval(7 * 86_400), rule: weekly)
    XCTAssertNotEqual(CalendarRows.phoneId(next), CalendarRows.phoneId(row(rule: weekly)))
  }

  func testAnEditedDayKeepsTheIdOfTheDayItReplaced() {
    let moved = row(
      start: nine.addingTimeInterval(3600), occurrence: nine, rule: weekly, detached: true)
    XCTAssertEqual(CalendarRows.phoneId(moved), CalendarRows.phoneId(row(rule: weekly)))
    let e = CalendarRows.entry(moved, people: [], withNotes: false, phoneZone: london)!
    XCTAssertTrue(e.repeats)
    XCTAssertNil(e.repeatRule)
  }

  func testARepeatCarriesItsRule() {
    let e = CalendarRows.entry(row(rule: weekly), people: [], withNotes: false, phoneZone: london)!
    XCTAssertTrue(e.repeats)
    XCTAssertEqual(e.repeatRule, "FREQ=WEEKLY;BYDAY=FR")
  }

  func testARuleReadsAsICalendar() {
    let until = Date(timeIntervalSince1970: 1_798_761_600)  // 2027-01-01 00:00 UTC
    let r = RuleRow(
      frequency: "MONTHLY", interval: 2, days: [(day: 2, week: -1), (day: 4, week: 2)],
      monthDays: [], months: [3, 9], positions: [1], count: nil, until: until)
    XCTAssertEqual(
      CalendarRows.rrule(r),
      "FREQ=MONTHLY;INTERVAL=2;UNTIL=20270101T000000Z;BYDAY=-1MO,2WE;BYMONTH=3,9;BYSETPOS=1")
    let counted = RuleRow(
      frequency: "DAILY", interval: 1, days: [], monthDays: [15], months: [], positions: [],
      count: 10, until: until)
    XCTAssertEqual(CalendarRows.rrule(counted), "FREQ=DAILY;COUNT=10;BYMONTHDAY=15")
  }

  func testPeopleAreNamesWithoutAddresses() {
    XCTAssertEqual(
      CalendarRows.people([" Sam ", nil, "", "sam@example.org", "Sam", "Alex"]), ["Sam", "Alex"])
    XCTAssertEqual(CalendarRows.people(Array(repeating: nil, count: 3)), [])
    XCTAssertEqual(CalendarRows.people((0..<150).map { "P\($0)" }).count, CalendarRows.maxPeople)
  }

  func testACalendarHasANameAndAColour() {
    let c = CalendarRows.calendar(CalendarRow(id: "ABC", name: "  ", rgb: (1, 0.5, 0)))
    XCTAssertEqual(c.name, "ABC")
    XCTAssertEqual(c.colour, "#FF8000")
    XCTAssertNil(CalendarRows.colour(nil))
  }

  func testTheWindowIsWholeDaysInThePhonesZone() {
    let w = CalendarRows.window(today: nine, back: 2, ahead: 14, zone: london)
    XCTAssertEqual(w.from, "2026-11-21")
    XCTAssertEqual(w.to, "2026-12-07")
    XCTAssertEqual(w.begin, Date(timeIntervalSince1970: 1_795_219_200))
    XCTAssertEqual(w.end.timeIntervalSince(w.begin), 17 * 86_400)
  }

  func testALongFieldIsCut() {
    XCTAssertEqual(CalendarRows.cut(String(repeating: "é", count: 2500)).unicodeScalars.count, 2000)
    XCTAssertEqual(CalendarRows.cut("short"), "short")
  }
}
