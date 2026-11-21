package app.nocturne.health

import androidx.health.connect.client.permission.HealthPermission
import androidx.health.connect.client.records.BloodPressureRecord
import androidx.health.connect.client.records.DistanceRecord
import androidx.health.connect.client.records.ExerciseSessionRecord
import androidx.health.connect.client.records.HeartRateRecord
import androidx.health.connect.client.records.Record
import androidx.health.connect.client.records.RestingHeartRateRecord
import androidx.health.connect.client.records.SleepSessionRecord
import androidx.health.connect.client.records.StepsRecord
import androidx.health.connect.client.records.WeightRecord
import androidx.health.connect.client.records.metadata.Device
import java.time.Instant
import java.time.LocalDate
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.time.temporal.ChronoUnit
import kotlin.reflect.KClass

/** The kinds Nocturne reads, by their names on the wire (`ReadingKind`). */
enum class Kind(val wire: String) {
    SLEEP("sleep"),
    HEART_RATE("heartRate"),
    RESTING_HEART_RATE("restingHeartRate"),
    BLOOD_PRESSURE("bloodPressure"),
    STEPS("steps"),
    WORKOUT("workout"),
    WEIGHT("weight");

    /** Kept as one reading a day, worked out from many: read again whole when a day changes. */
    val daily: Boolean get() = this == HEART_RATE || this == RESTING_HEART_RATE || this == STEPS

    companion object {
        fun of(wire: String): Kind? = values().firstOrNull { it.wire == wire }
    }
}

/** What each kind is read from, and the permissions that allow it. */
object HealthTypes {
    val DISTANCE: String = HealthPermission.getReadPermission(DistanceRecord::class)

    fun record(kind: Kind): KClass<out Record> = when (kind) {
        Kind.SLEEP -> SleepSessionRecord::class
        Kind.HEART_RATE -> HeartRateRecord::class
        Kind.RESTING_HEART_RATE -> RestingHeartRateRecord::class
        Kind.BLOOD_PRESSURE -> BloodPressureRecord::class
        Kind.STEPS -> StepsRecord::class
        Kind.WORKOUT -> ExerciseSessionRecord::class
        Kind.WEIGHT -> WeightRecord::class
    }

    /** The permission that decides whether a kind is allowed. */
    fun permission(kind: Kind): String = HealthPermission.getReadPermission(record(kind))

    /** What to ask for: distance goes with workouts, to give them their length on the road. */
    fun toAsk(kinds: List<Kind>): Set<String> =
        kinds.map(::permission).toSet() + (if (Kind.WORKOUT in kinds) setOf(DISTANCE) else emptySet())

    /** The kinds chosen that are allowed. */
    fun allowed(kinds: List<Kind>, granted: Set<String>): List<Kind> = kinds.filter { permission(it) in granted }

    /**
     * The record types a changes token follows: only what is allowed, since a token over a type
     * that is not allowed fails every read. Distance only when it is allowed too.
     */
    fun tokenTypes(kinds: List<Kind>, granted: Set<String>): Set<KClass<out Record>> {
        val allowed = allowed(kinds, granted)
        val distance = Kind.WORKOUT in allowed && DISTANCE in granted
        return allowed.map(::record).toSet() + (if (distance) setOf(DistanceRecord::class) else emptySet())
    }
}

data class Span(val from: Instant, val to: Instant)

data class Stage(val from: Instant, val to: Instant, val type: Int)

data class Night(val asleep: List<Span>, val inBedOnly: Boolean)

data class HourSlice(val start: Instant, val low: Long?, val high: Long?, val middle: Long?)

data class Hour(val start: String, val low: Double, val high: Double, val middle: Double)

sealed class Value {
    data class Sleep(val asleep: List<Pair<String, String>>, val inBedOnly: Boolean) : Value()
    data class HeartRate(val hours: List<Hour>, val count: Long) : Value()
    data class RestingHeartRate(val bpm: Double) : Value()
    data class BloodPressure(val top: Double, val bottom: Double) : Value()
    data class Steps(val total: Long) : Value()
    data class Workout(val kind: String, val lengthS: Long, val distanceM: Double?) : Value()
    data class Weight(val value: Double, val unit: String) : Value()
}

/** A reading as the computer takes it (`Reading`). */
data class Reading(
    val phoneId: String,
    val day: String,
    val start: String,
    val end: String?,
    val sourceDevice: String?,
    val value: Value,
)

object HealthRows {
    const val MAX_SPANS = 100
    private const val MAX_HOURS = 48
    private val MIN_SPAN = java.time.Duration.ofMinutes(1)

    private val AWAKE = setOf(
        SleepSessionRecord.STAGE_TYPE_AWAKE,
        SleepSessionRecord.STAGE_TYPE_OUT_OF_BED,
        SleepSessionRecord.STAGE_TYPE_AWAKE_IN_BED,
    )

    fun time(at: Instant, zone: ZoneId): String =
        at.truncatedTo(ChronoUnit.SECONDS).atZone(zone).format(DateTimeFormatter.ISO_OFFSET_DATE_TIME)

    fun day(at: Instant, zone: ZoneId): LocalDate = at.atZone(zone).toLocalDate()

    /**
     * A night's asleep periods: the session less its awake stages, the gaps between them being the
     * times awake. With no stages the whole session counts as asleep; when only awake-in-bed is
     * left, the session is time in bed.
     */
    fun night(start: Instant, end: Instant, stages: List<Stage>): Night {
        var asleep = listOf(Span(start, end))
        for (s in stages.filter { it.type in AWAKE }.sortedBy { it.from }) {
            asleep = asleep.flatMap { span ->
                if (s.to <= span.from || s.from >= span.to) {
                    listOf(span)
                } else {
                    listOfNotNull(
                        Span(span.from, s.from).takeIf { s.from > span.from },
                        Span(s.to, span.to).takeIf { s.to < span.to },
                    )
                }
            }
        }
        asleep = asleep.filter { java.time.Duration.between(it.from, it.to) >= MIN_SPAN }
        if (asleep.isEmpty()) return Night(listOf(Span(start, end)), inBedOnly = true)
        while (asleep.size > MAX_SPANS) {
            // Join the two periods with the shortest time awake between them.
            val i = (0 until asleep.size - 1).minByOrNull {
                java.time.Duration.between(asleep[it].to, asleep[it + 1].from)
            }!!
            asleep = asleep.take(i) + Span(asleep[i].from, asleep[i + 1].to) + asleep.drop(i + 2)
        }
        return Night(asleep, inBedOnly = false)
    }

    fun sleep(id: String, start: Instant, end: Instant, stages: List<Stage>, device: Int?, zone: ZoneId): Reading {
        val n = night(start, end, stages)
        return Reading(
            phoneId = id,
            // A night belongs to the day it ends on.
            day = day(end, zone).toString(),
            start = time(start, zone),
            end = time(end, zone),
            sourceDevice = deviceName(device),
            value = Value.Sleep(n.asleep.map { time(it.from, zone) to time(it.to, zone) }, n.inBedOnly),
        )
    }

    /** Hourly slices grouped into one heart rate reading per day; days with no slices have none. */
    fun heartRate(slices: List<HourSlice>, counts: Map<LocalDate, Long>, zone: ZoneId): List<Reading> =
        slices
            .filter { it.low != null && it.high != null && it.middle != null }
            .groupBy { day(it.start, zone) }
            .toSortedMap()
            .map { (d, hours) ->
                val list = hours.sortedBy { it.start }.take(MAX_HOURS).map {
                    val low = it.low!!.toDouble()
                    val high = maxOf(it.high!!.toDouble(), low)
                    Hour(time(it.start, zone), low, high, it.middle!!.toDouble().coerceIn(low, high))
                }
                daily(d, zone, Value.HeartRate(list, counts[d] ?: 0L))
            }

    /** One reading for a day: its id is the day, as one day has one. */
    fun daily(d: LocalDate, zone: ZoneId, value: Value): Reading = Reading(
        phoneId = d.toString(),
        day = d.toString(),
        start = time(d.atStartOfDay(zone).toInstant(), zone),
        end = null,
        sourceDevice = null,
        value = value,
    )

    fun workout(
        id: String,
        start: Instant,
        end: Instant,
        type: Int,
        distanceM: Double?,
        device: Int?,
        zone: ZoneId,
    ): Reading = Reading(
        phoneId = id,
        day = day(start, zone).toString(),
        start = time(start, zone),
        end = time(end, zone),
        sourceDevice = deviceName(device),
        value = Value.Workout(
            kind = workoutKind(type),
            lengthS = maxOf(0L, java.time.Duration.between(start, end).seconds),
            distanceM = distanceM?.takeIf { it > 0.0 && it.isFinite() },
        ),
    )

    /** One reading at a moment: blood pressure or weight. */
    fun moment(id: String, at: Instant, device: Int?, zone: ZoneId, value: Value): Reading = Reading(
        phoneId = id,
        day = day(at, zone).toString(),
        start = time(at, zone),
        end = null,
        sourceDevice = deviceName(device),
        value = value,
    )

    /** The word the computer shows ("Walk · 42 min"). */
    fun workoutKind(type: Int): String = when (type) {
        ExerciseSessionRecord.EXERCISE_TYPE_WALKING -> "Walk"
        ExerciseSessionRecord.EXERCISE_TYPE_RUNNING,
        ExerciseSessionRecord.EXERCISE_TYPE_RUNNING_TREADMILL -> "Run"
        ExerciseSessionRecord.EXERCISE_TYPE_SWIMMING_POOL,
        ExerciseSessionRecord.EXERCISE_TYPE_SWIMMING_OPEN_WATER -> "Swim"
        ExerciseSessionRecord.EXERCISE_TYPE_BIKING,
        ExerciseSessionRecord.EXERCISE_TYPE_BIKING_STATIONARY -> "Cycle"
        ExerciseSessionRecord.EXERCISE_TYPE_HIKING -> "Hike"
        ExerciseSessionRecord.EXERCISE_TYPE_YOGA -> "Yoga"
        ExerciseSessionRecord.EXERCISE_TYPE_PILATES -> "Pilates"
        ExerciseSessionRecord.EXERCISE_TYPE_DANCING -> "Dance"
        ExerciseSessionRecord.EXERCISE_TYPE_ROWING,
        ExerciseSessionRecord.EXERCISE_TYPE_ROWING_MACHINE -> "Row"
        ExerciseSessionRecord.EXERCISE_TYPE_STRENGTH_TRAINING,
        ExerciseSessionRecord.EXERCISE_TYPE_WEIGHTLIFTING -> "Strength"
        ExerciseSessionRecord.EXERCISE_TYPE_TENNIS -> "Tennis"
        else -> "Workout"
    }

    /** The device as Health names it, in the words the computer shows. */
    fun deviceName(type: Int?): String? = when (type) {
        Device.TYPE_WATCH -> "Watch"
        Device.TYPE_PHONE -> "This phone"
        Device.TYPE_RING -> "Ring"
        Device.TYPE_SCALE -> "Scale"
        Device.TYPE_FITNESS_BAND -> "Fitness band"
        Device.TYPE_CHEST_STRAP -> "Chest strap"
        else -> null
    }

    /** The days to read again for a daily kind: those a changed record touched, and the last few. */
    fun daysToRefresh(touched: Collection<Instant>, today: LocalDate, refreshDays: Int, zone: ZoneId): Set<LocalDate> =
        touched.map { day(it, zone) }.filter { !it.isAfter(today) }.toSet() +
            (0 until maxOf(refreshDays, 1)).map { today.minusDays(it.toLong()) }
}
