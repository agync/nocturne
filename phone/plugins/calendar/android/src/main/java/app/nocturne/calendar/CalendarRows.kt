package app.nocturne.calendar

import java.security.MessageDigest
import java.time.Instant
import java.time.LocalDate
import java.time.ZoneId
import java.time.ZoneOffset
import java.time.format.DateTimeFormatter
import java.time.temporal.ChronoUnit
import java.util.Base64

/** Holiday and birthday calendars start off; every other calendar is ordinary. */
enum class CalendarKind(val wire: String) { ORDINARY("ordinary"), HOLIDAYS("holidays"), BIRTHDAYS("birthdays") }

/** One row of the phone's Calendars table, as read. */
data class CalendarRow(
    val id: Long,
    val name: String?,
    val colour: Int?,
    val accountType: String?,
    val ownerAccount: String?,
)

data class Calendar(val id: String, val name: String, val colour: String?, val kind: CalendarKind)

/** One row of the Instances table: one appointment on one day. */
data class InstanceRow(
    val eventId: Long,
    val calendarId: Long,
    val begin: Long,
    val end: Long,
    val allDay: Boolean,
    val title: String?,
    val location: String?,
    val description: String?,
    val timeZone: String?,
    val rrule: String?,
    val rdate: String?,
    val originalId: Long?,
    /** For an edited instance of a repeat: when the instance it replaces began. */
    val originalInstanceTime: Long?,
    val selfStatus: Int,
    val status: Int,
)

data class Attendee(val name: String?, val email: String?)

/** An appointment as the computer takes it (`CalendarEntry`). */
data class Entry(
    val phoneId: String,
    val calendarId: String,
    val title: String,
    val start: String,
    val end: String,
    val timeZone: String?,
    val allDay: Boolean,
    val place: String?,
    val people: List<String>,
    val repeats: Boolean,
    val notes: String?,
    val repeatRule: String?,
)

/** The days read, both included, and their bounds in milliseconds for the Instances query. */
data class Window(val from: LocalDate, val to: LocalDate, val beginMs: Long, val endMs: Long)

object CalendarRows {
    /** The computer refuses a field longer than this. */
    const val MAX_FIELD = 2000
    const val MAX_PEOPLE = 100
    /** The computer keeps a calendar's name to this many characters. */
    const val MAX_NAME = 200

    // CalendarContract values, kept here so the mapping runs without the phone.
    const val STATUS_CANCELED = 2
    const val ATTENDEE_DECLINED = 2
    const val ATTENDEE_INVITED = 3

    private val HOLIDAY_OWNER = Regex("#holiday@group\\.v\\.calendar\\.google\\.com$", RegexOption.IGNORE_CASE)
    private val BIRTHDAY_OWNER = Regex("#contacts@group\\.v\\.calendar\\.google\\.com$", RegexOption.IGNORE_CASE)
    private val HOLIDAY_NAME = Regex(
        "holiday|feiertag|vacances|jours fériés|festività|festivos|feestdagen|helgdagar|helligdage",
        RegexOption.IGNORE_CASE,
    )
    private val BIRTHDAY_NAME = Regex(
        "birthday|geburtstag|anniversaire|compleanno|cumpleaños|verjaardag|födelsedag|fødselsdag",
        RegexOption.IGNORE_CASE,
    )
    private val BIRTHDAY_ACCOUNTS = setOf("com.android.contacts", "com.samsung.android.contacts")

    fun kind(row: CalendarRow): CalendarKind {
        val owner = row.ownerAccount.orEmpty()
        val name = row.name.orEmpty()
        return when {
            BIRTHDAY_OWNER.containsMatchIn(owner) -> CalendarKind.BIRTHDAYS
            HOLIDAY_OWNER.containsMatchIn(owner) -> CalendarKind.HOLIDAYS
            row.accountType in BIRTHDAY_ACCOUNTS -> CalendarKind.BIRTHDAYS
            BIRTHDAY_NAME.containsMatchIn(name) -> CalendarKind.BIRTHDAYS
            HOLIDAY_NAME.containsMatchIn(name) -> CalendarKind.HOLIDAYS
            else -> CalendarKind.ORDINARY
        }
    }

    fun colour(argb: Int?): String? = argb?.let { String.format("#%06X", it and 0xFFFFFF) }

    fun calendar(row: CalendarRow): Calendar = Calendar(
        id = row.id.toString(),
        name = cut(row.name?.trim()?.takeIf { it.isNotEmpty() } ?: row.id.toString(), MAX_NAME),
        colour = colour(row.colour),
        kind = kind(row),
    )

    /** From `back` days before today to `ahead` days after, both included, in the phone's zone. */
    fun window(today: LocalDate, back: Int, ahead: Int, zone: ZoneId): Window {
        val from = today.minusDays(back.toLong())
        val to = today.plusDays(ahead.toLong())
        return Window(
            from,
            to,
            from.atStartOfDay(zone).toInstant().toEpochMilli(),
            to.plusDays(1).atStartOfDay(zone).toInstant().toEpochMilli(),
        )
    }

    /** A stable, short id from what names an appointment on the phone. */
    fun idOf(seed: String): String {
        val digest = MessageDigest.getInstance("SHA-256").digest(seed.toByteArray())
        return Base64.getUrlEncoder().withoutPadding().encodeToString(digest).substring(0, 22)
    }

    /**
     * The phone's own id for an appointment, the same however often it moves: a one-off by its
     * event, an edited day of a repeat by the day it replaced (so the edit is a change, not a new
     * entry), and every other day of a repeat by its event and the day it falls on.
     */
    fun phoneId(row: InstanceRow): String = when {
        row.originalId != null -> idOf("${row.originalId}:${row.originalInstanceTime ?: row.begin}")
        row.rrule.isNullOrBlank() && row.rdate.isNullOrBlank() -> idOf("${row.eventId}")
        else -> idOf("${row.eventId}:${row.begin}")
    }

    private val TIMED = DateTimeFormatter.ISO_OFFSET_DATE_TIME

    private fun zoneOf(id: String?, fallback: ZoneId): ZoneId =
        id?.let { runCatching { ZoneId.of(it) }.getOrNull() } ?: fallback

    /** An all-day entry's dates are in UTC on the phone; a timed one's instant in its zone. */
    fun time(ms: Long, allDay: Boolean, zone: ZoneId): String =
        if (allDay) {
            Instant.ofEpochMilli(ms).atZone(ZoneOffset.UTC).toLocalDate().toString()
        } else {
            Instant.ofEpochMilli(ms).truncatedTo(ChronoUnit.SECONDS).atZone(zone).format(TIMED)
        }

    /** The names of the people invited, without the person and without bare addresses. */
    fun people(attendees: List<Attendee>, owner: String?): List<String> =
        attendees
            .filter { a -> owner == null || !a.email.equals(owner, ignoreCase = true) }
            .mapNotNull { it.name?.trim()?.takeIf { n -> n.isNotEmpty() && !n.contains('@') } }
            .map(::cut)
            .distinct()
            .take(MAX_PEOPLE)

    fun cut(s: String, max: Int = MAX_FIELD): String =
        if (s.codePointCount(0, s.length) <= max) s
        else s.substring(0, s.offsetByCodePoints(0, max))

    private fun text(s: String?): String? = s?.trim()?.takeIf { it.isNotEmpty() }?.let(::cut)

    /**
     * One appointment on one day, or null for what is left out: an invitation not answered or
     * declined, a cancelled entry, or one without a title. Notes only when asked for.
     */
    fun entry(
        row: InstanceRow,
        people: List<String>,
        withNotes: Boolean,
        phoneZone: ZoneId,
    ): Entry? {
        if (row.status == STATUS_CANCELED) return null
        if (row.selfStatus == ATTENDEE_DECLINED || row.selfStatus == ATTENDEE_INVITED) return null
        val title = text(row.title) ?: return null
        val zone = zoneOf(row.timeZone, phoneZone)
        val rule = row.rrule?.trim()?.takeIf { it.isNotEmpty() }
        val repeats = rule != null || !row.rdate.isNullOrBlank() || row.originalId != null
        return Entry(
            phoneId = phoneId(row),
            calendarId = row.calendarId.toString(),
            title = title,
            start = time(row.begin, row.allDay, zone),
            end = time(maxOf(row.end, row.begin), row.allDay, zone),
            timeZone = if (row.allDay) null else zone.id,
            allDay = row.allDay,
            place = text(row.location),
            people = people,
            repeats = repeats,
            notes = if (withNotes) text(row.description) else null,
            repeatRule = rule?.let(::cut),
        )
    }
}
