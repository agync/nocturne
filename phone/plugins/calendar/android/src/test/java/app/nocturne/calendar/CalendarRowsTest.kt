package app.nocturne.calendar

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.time.LocalDate
import java.time.ZoneId

class CalendarRowsTest {
    private val london = ZoneId.of("Europe/London")

    private fun row(
        title: String? = "Dentist",
        allDay: Boolean = false,
        begin: Long = 1_795_424_400_000, // 2026-11-23 09:00 UTC
        end: Long = 1_795_426_200_000,
        selfStatus: Int = 1,
        status: Int = 1,
        rrule: String? = null,
        description: String? = "Call link: example",
        timeZone: String? = "Europe/London",
        originalId: Long? = null,
        originalInstanceTime: Long? = null,
        eventId: Long = 41,
        enterprise: Boolean = false,
    ) = InstanceRow(
        eventId = eventId,
        calendarId = 3,
        begin = begin,
        end = end,
        allDay = allDay,
        title = title,
        location = " Smile Dental ",
        description = description,
        timeZone = timeZone,
        rrule = rrule,
        rdate = null,
        originalId = originalId,
        originalInstanceTime = originalInstanceTime,
        selfStatus = selfStatus,
        status = status,
        enterprise = enterprise,
    )

    @Test
    fun aTimedEntryCarriesItsZoneAndOffset() {
        val e = CalendarRows.entry(row(), listOf("Sam"), false, london)!!
        assertEquals("2026-11-23T09:00:00Z", e.start)
        assertEquals("2026-11-23T09:30:00Z", e.end)
        assertEquals("Europe/London", e.timeZone)
        assertEquals("Smile Dental", e.place)
        assertEquals(listOf("Sam"), e.people)
        assertEquals("3", e.calendarId)
        assertFalse(e.allDay)
    }

    @Test
    fun anEntryInAnotherZoneKeepsItsOwnOffset() {
        val e = CalendarRows.entry(row(timeZone = "America/New_York"), emptyList(), false, london)!!
        assertEquals("2026-11-23T04:00:00-05:00", e.start)
        assertEquals("America/New_York", e.timeZone)
    }

    @Test
    fun anAllDayEntryIsItsDatesWithTheEndAfterIt() {
        val begin = 1_795_305_600_000 // 2026-11-22 00:00 UTC
        val e = CalendarRows.entry(
            row(allDay = true, begin = begin, end = begin + 86_400_000, timeZone = "UTC"),
            emptyList(),
            false,
            london,
        )!!
        assertEquals("2026-11-22", e.start)
        assertEquals("2026-11-23", e.end)
        assertNull(e.timeZone)
        assertTrue(e.allDay)
    }

    @Test
    fun notesComeOnlyWhenAskedFor() {
        assertNull(CalendarRows.entry(row(), emptyList(), false, london)!!.notes)
        assertEquals("Call link: example", CalendarRows.entry(row(), emptyList(), true, london)!!.notes)
        assertNull(CalendarRows.entry(row(description = "  "), emptyList(), true, london)!!.notes)
    }

    @Test
    fun unansweredDeclinedAndCancelledAreLeftOut() {
        assertNull(CalendarRows.entry(row(selfStatus = CalendarRows.ATTENDEE_INVITED), emptyList(), false, london))
        assertNull(CalendarRows.entry(row(selfStatus = CalendarRows.ATTENDEE_DECLINED), emptyList(), false, london))
        assertNull(CalendarRows.entry(row(status = CalendarRows.STATUS_CANCELED), emptyList(), false, london))
        assertNull(CalendarRows.entry(row(title = "  "), emptyList(), false, london))
        // The person's own entries have no answer of their own.
        assertTrue(CalendarRows.entry(row(selfStatus = 0), emptyList(), false, london) != null)
    }

    @Test
    fun aRepeatGoesWithItsRule() {
        val e = CalendarRows.entry(row(rrule = "FREQ=WEEKLY;BYDAY=FR"), emptyList(), false, london)!!
        assertTrue(e.repeats)
        assertEquals("FREQ=WEEKLY;BYDAY=FR", e.repeatRule)
        val once = CalendarRows.entry(row(), emptyList(), false, london)!!
        assertFalse(once.repeats)
        assertNull(once.repeatRule)
    }

    @Test
    fun aOneOffKeepsItsIdWhenItMoves() {
        val before = CalendarRows.phoneId(row())
        val moved = CalendarRows.phoneId(row(begin = 1_795_510_800_000, end = 1_795_512_600_000))
        assertEquals(before, moved)
        assertEquals(22, before.length)
        assertTrue(before.all { it.isLetterOrDigit() || it == '-' || it == '_' })
        assertTrue(before != CalendarRows.phoneId(row(eventId = 42)))
    }

    @Test
    fun eachDayOfARepeatHasItsOwnId() {
        val friday = CalendarRows.phoneId(row(rrule = "FREQ=WEEKLY;BYDAY=FR"))
        val next = CalendarRows.phoneId(row(rrule = "FREQ=WEEKLY;BYDAY=FR", begin = 1_796_029_200_000))
        assertTrue(friday != next)
        assertEquals(CalendarRows.idOf("41:1795424400000"), friday)
    }

    @Test
    fun anEditedDayOfARepeatKeepsTheIdOfTheDayItReplaced() {
        val original = CalendarRows.phoneId(row(rrule = "FREQ=WEEKLY;BYDAY=FR"))
        val edited = CalendarRows.phoneId(
            row(eventId = 77, begin = 1_795_428_000_000, originalId = 41, originalInstanceTime = 1_795_424_400_000),
        )
        assertEquals(original, edited)
    }

    @Test
    fun aWorkCalendarHasAnIdThatNeverMeetsAPersonalOneAndNoName() {
        val personal = CalendarRows.calendar(CalendarRow(3, "Family", 0xFF3F51B5.toInt(), null))
        val work = CalendarRows.calendar(CalendarRow(3, null, 0xFF3F51B5.toInt(), null, enterprise = true), 1)
        assertEquals("3", personal.id)
        assertEquals("Family", personal.name)
        assertNull(personal.workIndex)
        assertEquals("enterprise:3", work.id)
        assertNull(work.name)
        assertEquals(1, work.workIndex)
        assertEquals(personal.colour, work.colour)
    }

    @Test
    fun aCalendarCarriesItsAccountExceptInTheWorkProfile() {
        val personal = CalendarRows.calendar(CalendarRow(3, "Holidays", 1, null, account = " maya.k@example.com "))
        val blank = CalendarRows.calendar(CalendarRow(4, "Holidays", 1, null, account = "  "))
        val work = CalendarRows.calendar(CalendarRow(5, null, 1, null, enterprise = true, account = "x"), 1)
        assertEquals("maya.k@example.com", personal.account)
        assertNull(blank.account)
        assertNull(work.account)
    }

    @Test
    fun workCalendarsAreListedAfterPersonalOnesAndNumberedFromOne() {
        val rows = listOf(
            CalendarRow(1, "Family", 1, "ana@example.com"),
            CalendarRow(1, null, 2, null, enterprise = true),
            CalendarRow(5, null, 3, null, enterprise = true),
        )
        val got = CalendarRows.list(rows)
        assertEquals(listOf("1", "enterprise:1", "enterprise:5"), got.map { it.id })
        assertEquals(listOf(null, 1, 2), got.map { it.workIndex })
        assertEquals(listOf("Family", null, null), got.map { it.name })
    }

    @Test
    fun aWorkEntryCarriesTheWorkCalendarIdAndItsOwnId() {
        val personal = CalendarRows.entry(row(), emptyList(), false, london)!!
        val work = CalendarRows.entry(row(enterprise = true), emptyList(), false, london)!!
        assertEquals("3", personal.calendarId)
        assertEquals("enterprise:3", work.calendarId)
        assertEquals(CalendarRows.idOf("41"), personal.phoneId)
        assertTrue(work.phoneId != personal.phoneId)
        assertEquals(work.phoneId, CalendarRows.phoneId(row(enterprise = true)))
        assertEquals("Dentist", work.title)
        assertEquals("2026-11-23T09:00:00Z", work.start)
    }

    @Test
    fun aWorkEntryWithoutNotesOrOriginalLinksHasIdsFromItsEventAndDay() {
        val oneOff = row(enterprise = true, description = null)
        assertEquals(CalendarRows.idOf("enterprise:41"), CalendarRows.phoneId(oneOff))
        assertNull(CalendarRows.entry(oneOff, emptyList(), true, london)!!.notes)
        val repeat = row(enterprise = true, description = null, rrule = "FREQ=WEEKLY;BYDAY=FR")
        assertEquals(CalendarRows.idOf("enterprise:41:1795424400000"), CalendarRows.phoneId(repeat))
    }

    @Test
    fun theIdsTheComputerAsksForAreSplitByProfile() {
        val (personal, work) = CalendarRows.split(listOf("3", "enterprise:3", "enterprise:9", "x", "enterprise:y"))
        assertEquals(setOf(3L), personal)
        assertEquals(setOf(3L, 9L), work)
    }

    @Test
    fun peopleAreNamesWithoutThePerson() {
        val got = CalendarRows.people(
            listOf(
                Attendee("Ana", "ana@example.com"),
                Attendee("Sam Lee", "sam@example.com"),
                Attendee(null, "kim@example.com"),
                Attendee("lee@example.com", "lee@example.com"),
                Attendee("Sam Lee", "sam2@example.com"),
            ),
            "ANA@example.com",
        )
        assertEquals(listOf("Sam Lee"), got)
    }

    @Test
    fun theWindowIsTwoDaysBackToTwoWeeksAhead() {
        val w = CalendarRows.window(LocalDate.of(2026, 11, 21), 2, 14, london)
        assertEquals(LocalDate.of(2026, 11, 19), w.from)
        assertEquals(LocalDate.of(2026, 12, 5), w.to)
        assertEquals(1_795_046_400_000, w.beginMs)
        assertEquals(1_796_515_200_000, w.endMs)
    }

    @Test
    fun longTextIsCutToWhatTheComputerTakes() {
        val long = "a".repeat(CalendarRows.MAX_FIELD + 10)
        assertEquals(CalendarRows.MAX_FIELD, CalendarRows.cut(long).length)
        val named = CalendarRows.calendar(CalendarRow(1, "n".repeat(300), null, null))
        assertEquals(CalendarRows.MAX_NAME, named.name!!.length)
        assertEquals("#3F51B5", CalendarRows.colour(0xFF3F51B5.toInt()))
    }
}
