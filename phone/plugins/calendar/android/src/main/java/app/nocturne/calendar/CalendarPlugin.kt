package app.nocturne.calendar

import android.Manifest
import android.app.Activity
import android.content.ContentUris
import android.content.Context
import android.content.pm.PackageManager
import android.database.Cursor
import android.os.Build
import android.os.UserManager
import android.provider.CalendarContract
import androidx.core.content.ContextCompat
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.Permission
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSArray
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import java.time.LocalDate
import java.time.ZoneId

@InvokeArg
class ReadArgs {
    var back: Int = 2
    var ahead: Int = 14
    var calendarIds: Array<String> = emptyArray()
    var withNotes: Boolean = false
}

/**
 * The phone's calendar, read only. Nothing here writes to it. Each read runs off the main thread,
 * because the app's Rust side waits for the answer.
 */
@TauriPlugin(
    permissions = [Permission(strings = [Manifest.permission.READ_CALENDAR], alias = "calendar")],
)
class CalendarPlugin(private val activity: Activity) : Plugin(activity) {
    private fun allowed(): Boolean =
        ContextCompat.checkSelfPermission(activity, Manifest.permission.READ_CALENDAR) ==
            PackageManager.PERMISSION_GRANTED

    private fun background(invoke: Invoke, work: () -> Unit) {
        if (!allowed()) return invoke.reject("notAllowed")
        Thread {
            try {
                work()
            } catch (e: SecurityException) {
                invoke.reject("notAllowed")
            } catch (e: Exception) {
                invoke.reject(e.message ?: "failed")
            }
        }.start()
    }

    private fun Cursor.str(col: String): String? = getColumnIndex(col).takeIf { it >= 0 && !isNull(it) }?.let(::getString)
    private fun Cursor.long(col: String): Long? = getColumnIndex(col).takeIf { it >= 0 && !isNull(it) }?.let(::getLong)
    private fun Cursor.int(col: String): Int? = getColumnIndex(col).takeIf { it >= 0 && !isNull(it) }?.let(::getInt)

    private fun queryCalendars(): List<CalendarRow> {
        val rows = mutableListOf<CalendarRow>()
        activity.contentResolver.query(
            CalendarContract.Calendars.CONTENT_URI,
            arrayOf(
                CalendarContract.Calendars._ID,
                CalendarContract.Calendars.CALENDAR_DISPLAY_NAME,
                CalendarContract.Calendars.CALENDAR_COLOR,
                CalendarContract.Calendars.OWNER_ACCOUNT,
                CalendarContract.Calendars.ACCOUNT_NAME,
            ),
            "${CalendarContract.Calendars.VISIBLE} = 1",
            null,
            "${CalendarContract.Calendars._ID} ASC",
        )?.use { cur ->
            while (cur.moveToNext()) {
                rows += CalendarRow(
                    id = cur.long(CalendarContract.Calendars._ID) ?: continue,
                    name = cur.str(CalendarContract.Calendars.CALENDAR_DISPLAY_NAME),
                    colour = cur.int(CalendarContract.Calendars.CALENDAR_COLOR),
                    ownerAccount = cur.str(CalendarContract.Calendars.OWNER_ACCOUNT),
                    account = cur.str(CalendarContract.Calendars.ACCOUNT_NAME),
                )
            }
        }
        return rows
    }

    /**
     * Whether the phone has a work profile Android could show this app. From Android 10 on, the
     * work profile's calendars come through the enterprise URIs, which allow only a few columns
     * and give no name, owner or guest list, and only where the device's administrator allows it.
     */
    private fun hasWorkProfile(): Boolean {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.Q) return false
        val users = activity.getSystemService(Context.USER_SERVICE) as? UserManager ?: return false
        return users.userProfiles.size > 1
    }

    private fun workCalendars(): List<CalendarRow> {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.Q || !hasWorkProfile()) return emptyList()
        val rows = mutableListOf<CalendarRow>()
        try {
            activity.contentResolver.query(
                CalendarContract.Calendars.ENTERPRISE_CONTENT_URI,
                arrayOf(CalendarContract.Calendars._ID, CalendarContract.Calendars.CALENDAR_COLOR),
                "${CalendarContract.Calendars.VISIBLE} = 1",
                null,
                null,
            )?.use { cur ->
                while (cur.moveToNext()) {
                    rows += CalendarRow(
                        id = cur.long(CalendarContract.Calendars._ID) ?: continue,
                        name = null,
                        colour = cur.int(CalendarContract.Calendars.CALENDAR_COLOR),
                        ownerAccount = null,
                        enterprise = true,
                    )
                }
            }
        } catch (e: SecurityException) {
            return emptyList()
        } catch (e: UnsupportedOperationException) {
            // Android's answer when the work profile is not shared with this app or is not running.
            return emptyList()
        }
        return rows.sortedBy { it.id }
    }

    private fun calendarRows(): List<CalendarRow> = queryCalendars() + workCalendars()

    @Command
    fun listCalendars(invoke: Invoke) = background(invoke) {
        val list = JSArray()
        for (cal in CalendarRows.list(calendarRows())) {
            list.put(
                JSObject()
                    .put("id", cal.id)
                    .put("name", cal.name)
                    .put("colour", cal.colour)
                    .put("account", cal.account)
                    .put("workIndex", cal.workIndex),
            )
        }
        invoke.resolve(JSObject().put("calendars", list))
    }

    private fun attendees(eventIds: Collection<Long>): Map<Long, List<Attendee>> {
        if (eventIds.isEmpty()) return emptyMap()
        val out = mutableMapOf<Long, MutableList<Attendee>>()
        for (chunk in eventIds.chunked(500)) {
            activity.contentResolver.query(
                CalendarContract.Attendees.CONTENT_URI,
                arrayOf(
                    CalendarContract.Attendees.EVENT_ID,
                    CalendarContract.Attendees.ATTENDEE_NAME,
                    CalendarContract.Attendees.ATTENDEE_EMAIL,
                ),
                "${CalendarContract.Attendees.EVENT_ID} IN (${chunk.joinToString(",")})",
                null,
                null,
            )?.use { cur ->
                while (cur.moveToNext()) {
                    val id = cur.long(CalendarContract.Attendees.EVENT_ID) ?: continue
                    out.getOrPut(id) { mutableListOf() } += Attendee(
                        cur.str(CalendarContract.Attendees.ATTENDEE_NAME),
                        cur.str(CalendarContract.Attendees.ATTENDEE_EMAIL),
                    )
                }
            }
        }
        return out
    }

    private fun instances(
        enterprise: Boolean,
        calendarIds: Set<Long>,
        window: Window,
        withNotes: Boolean,
    ): List<InstanceRow> {
        if (calendarIds.isEmpty()) return emptyList()
        if (enterprise && Build.VERSION.SDK_INT < Build.VERSION_CODES.Q) return emptyList()
        val rows = mutableListOf<InstanceRow>()
        val base = if (enterprise) {
            CalendarContract.Instances.ENTERPRISE_CONTENT_URI
        } else {
            CalendarContract.Instances.CONTENT_URI
        }
        val uri = base.buildUpon().also {
            ContentUris.appendId(it, window.beginMs)
            ContentUris.appendId(it, window.endMs)
        }.build()
        // The work profile allows only these columns: no notes, and no link from an edited day of
        // a repeat to the day it replaced. It also takes no selection on the calendar, so the
        // calendars are picked after the read.
        val projection = if (enterprise) {
            arrayOf(
                CalendarContract.Instances.EVENT_ID,
                CalendarContract.Instances.CALENDAR_ID,
                CalendarContract.Instances.BEGIN,
                CalendarContract.Instances.END,
                CalendarContract.Instances.ALL_DAY,
                CalendarContract.Instances.TITLE,
                CalendarContract.Instances.EVENT_LOCATION,
                CalendarContract.Instances.EVENT_TIMEZONE,
                CalendarContract.Instances.RRULE,
                CalendarContract.Instances.RDATE,
                CalendarContract.Instances.SELF_ATTENDEE_STATUS,
                CalendarContract.Instances.STATUS,
            )
        } else {
            arrayOf(
                CalendarContract.Instances.EVENT_ID,
                CalendarContract.Instances.CALENDAR_ID,
                CalendarContract.Instances.BEGIN,
                CalendarContract.Instances.END,
                CalendarContract.Instances.ALL_DAY,
                CalendarContract.Instances.TITLE,
                CalendarContract.Instances.EVENT_LOCATION,
                CalendarContract.Instances.DESCRIPTION,
                CalendarContract.Instances.EVENT_TIMEZONE,
                CalendarContract.Instances.RRULE,
                CalendarContract.Instances.RDATE,
                CalendarContract.Instances.ORIGINAL_ID,
                CalendarContract.Instances.ORIGINAL_INSTANCE_TIME,
                CalendarContract.Instances.SELF_ATTENDEE_STATUS,
                CalendarContract.Instances.STATUS,
            )
        }
        activity.contentResolver.query(
            uri,
            projection,
            if (enterprise) null else "${CalendarContract.Instances.CALENDAR_ID} IN (${calendarIds.joinToString(",")})",
            null,
            if (enterprise) null else "${CalendarContract.Instances.BEGIN} ASC",
        )?.use { cur ->
            while (cur.moveToNext()) {
                val calendarId = cur.long(CalendarContract.Instances.CALENDAR_ID) ?: continue
                if (calendarId !in calendarIds) continue
                rows += InstanceRow(
                    eventId = cur.long(CalendarContract.Instances.EVENT_ID) ?: continue,
                    calendarId = calendarId,
                    begin = cur.long(CalendarContract.Instances.BEGIN) ?: continue,
                    end = cur.long(CalendarContract.Instances.END) ?: 0L,
                    allDay = cur.int(CalendarContract.Instances.ALL_DAY) == 1,
                    title = cur.str(CalendarContract.Instances.TITLE),
                    location = cur.str(CalendarContract.Instances.EVENT_LOCATION),
                    description = if (withNotes) cur.str(CalendarContract.Instances.DESCRIPTION) else null,
                    timeZone = cur.str(CalendarContract.Instances.EVENT_TIMEZONE),
                    rrule = cur.str(CalendarContract.Instances.RRULE),
                    rdate = cur.str(CalendarContract.Instances.RDATE),
                    originalId = cur.long(CalendarContract.Instances.ORIGINAL_ID),
                    originalInstanceTime = cur.long(CalendarContract.Instances.ORIGINAL_INSTANCE_TIME),
                    selfStatus = cur.int(CalendarContract.Instances.SELF_ATTENDEE_STATUS) ?: 0,
                    status = cur.int(CalendarContract.Instances.STATUS) ?: 0,
                    enterprise = enterprise,
                )
            }
        }
        return rows
    }

    @Command
    fun listInstances(invoke: Invoke) {
        val args = invoke.parseArgs(ReadArgs::class.java)
        background(invoke) {
            val zone = ZoneId.systemDefault()
            val window = CalendarRows.window(LocalDate.now(zone), args.back, args.ahead, zone)
            val (personal, work) = CalendarRows.split(args.calendarIds.toList())
            val owners = queryCalendars().associate { it.id to it.ownerAccount }
            val rows = mutableListOf<InstanceRow>()
            rows += instances(false, personal, window, args.withNotes)
            if (work.isNotEmpty() && hasWorkProfile()) {
                rows += try {
                    instances(true, work, window, args.withNotes)
                } catch (e: SecurityException) {
                    emptyList()
                } catch (e: UnsupportedOperationException) {
                    emptyList()
                }
            }
            val people = attendees(rows.filter { !it.enterprise }.map { it.eventId }.toSet())
            val entries = JSArray()
            for (row in rows) {
                val names = if (row.enterprise) {
                    emptyList()
                } else {
                    CalendarRows.people(people[row.eventId].orEmpty(), owners[row.calendarId])
                }
                val e = CalendarRows.entry(row, names, args.withNotes, zone) ?: continue
                entries.put(
                    JSObject()
                        .put("phoneId", e.phoneId)
                        .put("calendarId", e.calendarId)
                        .put("title", e.title)
                        .put("start", e.start)
                        .put("end", e.end)
                        .put("timeZone", e.timeZone)
                        .put("allDay", e.allDay)
                        .put("place", e.place)
                        .put("people", JSArray(e.people))
                        .put("repeats", e.repeats)
                        .put("notes", e.notes)
                        .also { o -> e.repeatRule?.let { o.put("repeatRule", it) } },
                )
            }
            invoke.resolve(
                JSObject()
                    .put("windowFrom", window.from.toString())
                    .put("windowTo", window.to.toString())
                    .put("entries", entries),
            )
        }
    }
}
