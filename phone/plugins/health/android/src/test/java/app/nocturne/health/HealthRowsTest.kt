package app.nocturne.health

import androidx.health.connect.client.records.DistanceRecord
import androidx.health.connect.client.records.ExerciseSessionRecord
import androidx.health.connect.client.records.StepsRecord
import androidx.health.connect.client.records.SleepSessionRecord
import androidx.health.connect.client.records.metadata.Device
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.time.Instant
import java.time.LocalDate
import java.time.ZoneId

class HealthRowsTest {
    private val london = ZoneId.of("Europe/London")
    private fun at(s: String) = Instant.parse(s)

    @Test
    fun awakeStagesBreakTheNight() {
        val n = HealthRows.night(
            at("2026-11-20T23:50:00Z"),
            at("2026-11-21T07:05:00Z"),
            listOf(
                Stage(at("2026-11-20T23:50:00Z"), at("2026-11-21T03:00:00Z"), SleepSessionRecord.STAGE_TYPE_DEEP),
                Stage(at("2026-11-21T03:00:00Z"), at("2026-11-21T03:20:00Z"), SleepSessionRecord.STAGE_TYPE_AWAKE),
                Stage(at("2026-11-21T03:20:00Z"), at("2026-11-21T07:05:00Z"), SleepSessionRecord.STAGE_TYPE_LIGHT),
            ),
        )
        assertFalse(n.inBedOnly)
        assertEquals(
            listOf(
                Span(at("2026-11-20T23:50:00Z"), at("2026-11-21T03:00:00Z")),
                Span(at("2026-11-21T03:20:00Z"), at("2026-11-21T07:05:00Z")),
            ),
            n.asleep,
        )
    }

    @Test
    fun noStagesIsOneStretchAsleep() {
        val n = HealthRows.night(at("2026-11-20T23:00:00Z"), at("2026-11-21T07:00:00Z"), emptyList())
        assertEquals(1, n.asleep.size)
        assertFalse(n.inBedOnly)
    }

    @Test
    fun onlyAwakeInBedIsTimeInBed() {
        val n = HealthRows.night(
            at("2026-11-20T23:00:00Z"),
            at("2026-11-21T07:00:00Z"),
            listOf(Stage(at("2026-11-20T23:00:00Z"), at("2026-11-21T07:00:00Z"), SleepSessionRecord.STAGE_TYPE_AWAKE_IN_BED)),
        )
        assertTrue(n.inBedOnly)
        assertEquals(listOf(Span(at("2026-11-20T23:00:00Z"), at("2026-11-21T07:00:00Z"))), n.asleep)
    }

    @Test
    fun aNightBelongsToTheDayItEndsOn() {
        val r = HealthRows.sleep("s-1", at("2026-11-20T23:50:00Z"), at("2026-11-21T07:05:00Z"), emptyList(), Device.TYPE_WATCH, london)
        assertEquals("2026-11-21", r.day)
        assertEquals("Watch", r.sourceDevice)
        assertEquals("2026-11-20T23:50:00Z", r.start)
        val v = r.value as Value.Sleep
        assertEquals(listOf("2026-11-20T23:50:00Z" to "2026-11-21T07:05:00Z"), v.asleep)
    }

    @Test
    fun tooManyPeriodsJoinAcrossTheShortestGaps() {
        val start = at("2026-11-20T22:00:00Z")
        val stages = (0 until 150).map { i ->
            val from = start.plusSeconds(i * 180L + 120)
            Stage(from, from.plusSeconds(if (i == 7) 50 else 60), SleepSessionRecord.STAGE_TYPE_AWAKE)
        }
        val n = HealthRows.night(start, start.plusSeconds(150 * 180L + 300), stages)
        assertEquals(HealthRows.MAX_SPANS, n.asleep.size)
    }

    @Test
    fun hoursGroupIntoDaysInThePhonesTime() {
        val zone = ZoneId.of("Europe/Berlin")
        val slices = listOf(
            HourSlice(at("2026-11-20T22:00:00Z"), 52, 60, 55), // 23:00 in Berlin, the 20th
            HourSlice(at("2026-11-20T23:00:00Z"), 50, 58, 54), // midnight, the 21st
            HourSlice(at("2026-11-21T09:00:00Z"), 70, 148, 90),
            HourSlice(at("2026-11-21T10:00:00Z"), null, null, null),
        )
        val got = HealthRows.heartRate(slices, mapOf(LocalDate.of(2026, 11, 21) to 412L), zone)
        assertEquals(listOf("2026-11-20", "2026-11-21"), got.map { it.day })
        assertEquals("2026-11-21", got[1].phoneId)
        val v = got[1].value as Value.HeartRate
        assertEquals(2, v.hours.size)
        assertEquals(412L, v.count)
        assertEquals("2026-11-21T00:00:00+01:00", v.hours[0].start)
        assertEquals(0L, (got[0].value as Value.HeartRate).count)
    }

    @Test
    fun aWorkoutHasItsWordAndLength() {
        val r = HealthRows.workout(
            "w-1",
            at("2026-11-21T08:00:00Z"),
            at("2026-11-21T08:42:00Z"),
            ExerciseSessionRecord.EXERCISE_TYPE_WALKING,
            3100.0,
            Device.TYPE_PHONE,
            london,
        )
        assertEquals(Value.Workout("Walk", 2520, 3100.0), r.value)
        assertEquals("This phone", r.sourceDevice)
        val swim = HealthRows.workout("w-2", at("2026-11-21T08:00:00Z"), at("2026-11-21T08:30:00Z"),
            ExerciseSessionRecord.EXERCISE_TYPE_SWIMMING_POOL, 0.0, null, london)
        assertEquals(Value.Workout("Swim", 1800, null), swim.value)
        assertNull(swim.sourceDevice)
        assertEquals("Workout", HealthRows.workoutKind(ExerciseSessionRecord.EXERCISE_TYPE_FENCING))
    }

    @Test
    fun aDayReadingIsKeyedByItsDay() {
        val r = HealthRows.daily(LocalDate.of(2026, 11, 21), london, Value.Steps(8412))
        assertEquals("2026-11-21", r.phoneId)
        assertEquals("2026-11-21T00:00:00Z", r.start)
        assertNull(r.end)
    }

    @Test
    fun touchedDaysAndTheLastFewAreReadAgain() {
        val today = LocalDate.of(2026, 11, 21)
        val days = HealthRows.daysToRefresh(listOf(at("2026-11-10T12:00:00Z"), at("2026-12-01T12:00:00Z")), today, 2, london)
        assertEquals(setOf(LocalDate.of(2026, 11, 10), today, today.minusDays(1)), days)
    }

    @Test
    fun kindsRoundTripByTheirWireNames() {
        for (k in Kind.values()) assertEquals(k, Kind.of(k.wire))
        assertNull(Kind.of("bloodOxygen"))
        assertTrue(Kind.STEPS.daily)
        assertFalse(Kind.SLEEP.daily)
    }

    @Test
    fun theTokenFollowsOnlyWhatIsAllowed() {
        val all = Kind.values().toList()
        val workout = HealthTypes.permission(Kind.WORKOUT)
        val steps = HealthTypes.permission(Kind.STEPS)
        // Distance unticked: workouts are read, distance is not followed.
        assertEquals(
            setOf(ExerciseSessionRecord::class, StepsRecord::class),
            HealthTypes.tokenTypes(all, setOf(workout, steps)),
        )
        assertEquals(
            setOf(ExerciseSessionRecord::class, DistanceRecord::class),
            HealthTypes.tokenTypes(listOf(Kind.WORKOUT), setOf(workout, HealthTypes.DISTANCE)),
        )
        // Distance alone does not bring workouts.
        assertEquals(emptySet<Any>(), HealthTypes.tokenTypes(all, setOf(HealthTypes.DISTANCE)))
        assertEquals(listOf(Kind.STEPS), HealthTypes.allowed(listOf(Kind.SLEEP, Kind.STEPS), setOf(steps)))
        assertTrue(HealthTypes.DISTANCE in HealthTypes.toAsk(listOf(Kind.WORKOUT)))
        assertFalse(HealthTypes.DISTANCE in HealthTypes.toAsk(listOf(Kind.STEPS)))
    }
}
