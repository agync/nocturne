package app.nocturne.health

import android.app.Activity
import android.content.Intent
import android.webkit.WebView
import androidx.activity.ComponentActivity
import androidx.activity.result.ActivityResultLauncher
import androidx.core.net.toUri
import androidx.health.connect.client.HealthConnectClient
import androidx.health.connect.client.HealthConnectFeatures
import androidx.health.connect.client.PermissionController
import androidx.health.connect.client.changes.DeletionChange
import androidx.health.connect.client.changes.UpsertionChange
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
import androidx.health.connect.client.request.AggregateGroupByDurationRequest
import androidx.health.connect.client.request.AggregateGroupByPeriodRequest
import androidx.health.connect.client.request.AggregateRequest
import androidx.health.connect.client.request.ChangesTokenRequest
import androidx.health.connect.client.request.ReadRecordsRequest
import androidx.health.connect.client.time.TimeRangeFilter
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSArray
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import java.time.Duration
import java.time.Instant
import java.time.LocalDate
import java.time.Period
import java.time.ZoneId
import java.util.concurrent.atomic.AtomicBoolean
import kotlin.reflect.KClass
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch

@InvokeArg
class AskArgs {
    var kinds: Array<String> = emptyArray()
    var history: Boolean = false
}

@InvokeArg
class ReadArgs {
    var kinds: Array<String> = emptyArray()
    var days: Int = 30
}

@InvokeArg
class ChangesArgs {
    var kinds: Array<String> = emptyArray()
    var token: String = ""
    var refreshDays: Int = 2
}

@InvokeArg
class SettingsArgs {
    var target: String = "settings"
}

/**
 * Health Connect, read only: Nocturne asks only to read and never writes. Each read runs off the
 * main thread, because the app's Rust side waits for the answer.
 */
@TauriPlugin
class HealthPlugin(private val activity: Activity) : Plugin(activity) {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private var route: String? = null
    private var webView: WebView? = null
    private var asking: ActivityResultLauncher<Set<String>>? = null

    companion object {
        const val RATIONALE = "androidx.health.ACTION_SHOW_PERMISSIONS_RATIONALE"
        const val PERMISSION_USAGE = "android.intent.action.VIEW_PERMISSION_USAGE"
        const val HISTORY = HealthPermission.PERMISSION_READ_HEALTH_DATA_HISTORY
        const val ASK_TIMEOUT_MS = 5 * 60 * 1000L
    }

    private fun rationale(intent: Intent?) = intent?.action == RATIONALE || intent?.action == PERMISSION_USAGE

    /** Opened from Health Connect: the screens ask for the page once they are up. */
    override fun load(webView: WebView) {
        super.load(webView)
        this.webView = webView
        if (rationale(activity.intent)) route = "rationale"
    }

    /** Already open: the page is shown at once. */
    override fun onNewIntent(intent: Intent) {
        if (rationale(intent)) webView?.post { webView?.evaluateJavascript("location.hash='#/rationale'", null) }
    }

    private fun client(): HealthConnectClient? =
        if (HealthConnectClient.getSdkStatus(activity) == HealthConnectClient.SDK_AVAILABLE) {
            HealthConnectClient.getOrCreate(activity)
        } else {
            null
        }

    private fun kinds(wire: Array<String>): List<Kind> = wire.mapNotNull(Kind::of).distinct()

    private fun work(invoke: Invoke, body: suspend (HealthConnectClient) -> Unit) {
        val client = client() ?: return invoke.reject("unavailable")
        scope.launch {
            try {
                body(client)
            } catch (e: SecurityException) {
                invoke.reject("notAllowed")
            } catch (e: Exception) {
                invoke.reject(e.message ?: "failed")
            }
        }
    }

    private fun grantedOf(granted: Set<String>): JSObject {
        val kinds = JSArray()
        for (k in HealthTypes.allowed(Kind.values().toList(), granted)) kinds.put(k.wire)
        return JSObject()
            .put("kinds", kinds)
            .put("history", HISTORY in granted)
            .put("distance", HealthTypes.DISTANCE in granted)
    }

    @Command
    fun availability(invoke: Invoke) {
        val status = when (HealthConnectClient.getSdkStatus(activity)) {
            HealthConnectClient.SDK_AVAILABLE -> "available"
            HealthConnectClient.SDK_UNAVAILABLE_PROVIDER_UPDATE_REQUIRED -> "needsUpdate"
            else -> "missing"
        }
        // Older readings need a Health Connect that offers them.
        val older = client()?.features?.getFeatureStatus(HealthConnectFeatures.FEATURE_READ_HEALTH_DATA_HISTORY) ==
            HealthConnectFeatures.FEATURE_STATUS_AVAILABLE
        invoke.resolve(JSObject().put("status", status).put("olderReadings", older))
    }

    @Command
    fun granted(invoke: Invoke) = work(invoke) { client ->
        invoke.resolve(grantedOf(client.permissionController.getGrantedPermissions()))
    }

    /** Health Connect's own screen, listing only the kinds chosen; it only confirms them. */
    @Command
    fun request(invoke: Invoke) {
        val args = invoke.parseArgs(AskArgs::class.java)
        val owner = activity as? ComponentActivity ?: return invoke.reject("unavailable")
        if (client() == null) return invoke.reject("unavailable")
        val wanted = HealthTypes.toAsk(kinds(args.kinds)).toMutableSet()
        if (args.history) wanted += HISTORY
        val answered = AtomicBoolean(false)
        val answer: (Set<String>) -> Unit = { allowed ->
            if (answered.compareAndSet(false, true)) {
                activity.runOnUiThread {
                    asking?.unregister()
                    asking = null
                }
                work(invoke) { client ->
                    invoke.resolve(grantedOf(allowed + client.permissionController.getGrantedPermissions()))
                }
            }
        }
        activity.runOnUiThread {
            asking?.unregister()
            val launcher = owner.activityResultRegistry.register(
                "nocturne-health",
                PermissionController.createRequestPermissionResultContract(),
            ) { allowed -> answer(allowed) }
            asking = launcher
            launcher.launch(wanted)
            // The app's Rust side waits for this answer: never longer than a few minutes.
            webView?.postDelayed({ answer(emptySet()) }, ASK_TIMEOUT_MS)
        }
    }

    @Command
    fun read(invoke: Invoke) {
        val args = invoke.parseArgs(ReadArgs::class.java)
        work(invoke) { client ->
            val granted = client.permissionController.getGrantedPermissions()
            val chosen = HealthTypes.allowed(kinds(args.kinds), granted)
            val zone = ZoneId.systemDefault()
            val today = LocalDate.now(zone)
            val from = today.minusDays(maxOf(args.days - 1, 0).toLong())
            // The token comes first, so nothing that changes while reading is missed.
            val token = client.getChangesToken(ChangesTokenRequest(HealthTypes.tokenTypes(chosen, granted)))
            val readings = mutableListOf<Reading>()
            val start = from.atStartOfDay(zone).toInstant()
            val end = Instant.now()
            for (k in chosen) {
                if (k.daily) {
                    readings += daily(client, k, (0..today.toEpochDay() - from.toEpochDay()).map { from.plusDays(it) }.toSet(), zone)
                } else {
                    readings += records(client, k, start, end, zone, HealthTypes.DISTANCE in granted)
                }
            }
            invoke.resolve(
                JSObject()
                    .put("readings", json(readings))
                    .put("token", token)
                    .put("fromDay", from.toString()),
            )
        }
    }

    @Command
    fun changes(invoke: Invoke) {
        val args = invoke.parseArgs(ChangesArgs::class.java)
        work(invoke) { client ->
            val granted = client.permissionController.getGrantedPermissions()
            val chosen = HealthTypes.allowed(kinds(args.kinds), granted)
            val distance = HealthTypes.DISTANCE in granted
            val zone = ZoneId.systemDefault()
            var token = args.token
            val upserted = mutableListOf<Record>()
            val removed = mutableListOf<String>()
            val expired = JSObject().put("readings", JSArray()).put("removed", JSArray()).put("expired", true)
            do {
                // A permission taken back since the token was made fails the read: read whole.
                val page = try {
                    client.getChanges(token)
                } catch (e: SecurityException) {
                    return@work invoke.resolve(expired)
                }
                if (page.changesTokenExpired) return@work invoke.resolve(expired)
                for (change in page.changes) {
                    when (change) {
                        is UpsertionChange -> upserted += change.record
                        is DeletionChange -> removed += change.recordId
                    }
                }
                token = page.nextChangesToken
            } while (page.hasMore)
            val readings = mutableListOf<Reading>()
            val refreshed = JSObject()
            val today = LocalDate.now(zone)
            for (k in chosen) {
                val type = HealthTypes.record(k)
                val mine = upserted.filter { type.isInstance(it) }
                if (k.daily) {
                    val days = HealthRows.daysToRefresh(mine.mapNotNull(::instantOf), today, args.refreshDays, zone)
                    readings += daily(client, k, days, zone)
                    refreshed.put(k.wire, JSArray(days.sorted().map { it.toString() }))
                } else {
                    readings += mine.mapNotNull { mapRecord(client, it, zone, distance) }
                }
            }
            // A changed distance changes the workouts it falls in.
            if (Kind.WORKOUT in chosen && distance) {
                val seen = readings.map { it.phoneId }.toMutableSet()
                for (d in upserted.filterIsInstance<DistanceRecord>()) {
                    for (w in readAll(client, ExerciseSessionRecord::class, d.startTime.minus(Duration.ofHours(12)), d.endTime)) {
                        if (w.endTime > d.startTime && w.startTime < d.endTime && seen.add(w.metadata.id)) {
                            mapRecord(client, w, zone, true)?.let { readings += it }
                        }
                    }
                }
            }
            invoke.resolve(
                JSObject()
                    .put("readings", json(readings))
                    .put("removed", JSArray(removed))
                    .put("refreshed", refreshed)
                    .put("token", token)
                    .put("expired", false),
            )
        }
    }

    @Command
    fun openSettings(invoke: Invoke) {
        val args = invoke.parseArgs(SettingsArgs::class.java)
        val intent = if (args.target == "store") {
            Intent(Intent.ACTION_VIEW, "market://details?id=com.google.android.apps.healthdata&url=healthconnect%3A%2F%2Fonboarding".toUri())
        } else {
            HealthConnectClient.getHealthConnectManageDataIntent(activity)
        }
        try {
            activity.startActivity(intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
            invoke.resolve()
        } catch (e: Exception) {
            invoke.reject(e.message ?: "failed")
        }
    }

    /** The page Health Connect opened the app on, once. */
    @Command
    fun takeRoute(invoke: Invoke) {
        val out = JSObject().put("route", route)
        route = null
        invoke.resolve(out)
    }

    private fun instantOf(r: Record): Instant? = when (r) {
        is HeartRateRecord -> r.startTime
        is RestingHeartRateRecord -> r.time
        is StepsRecord -> r.startTime
        else -> null
    }

    private suspend fun daily(client: HealthConnectClient, kind: Kind, days: Set<LocalDate>, zone: ZoneId): List<Reading> {
        if (days.isEmpty()) return emptyList()
        val sorted = days.sorted()
        val out = mutableListOf<Reading>()
        // Contiguous runs of days, each read in one go, a week at most at a time.
        val runs = mutableListOf<MutableList<LocalDate>>()
        for (d in sorted) {
            val last = runs.lastOrNull()
            if (last != null && last.last().plusDays(1) == d && last.size < 7) last += d else runs += mutableListOf(d)
        }
        for (run in runs) {
            val first = run.first()
            val last = run.last()
            val byDay = TimeRangeFilter.between(first.atStartOfDay(), last.plusDays(1).atStartOfDay())
            when (kind) {
                Kind.HEART_RATE -> {
                    val counts = client.aggregateGroupByPeriod(
                        AggregateGroupByPeriodRequest(setOf(HeartRateRecord.MEASUREMENTS_COUNT), byDay, Period.ofDays(1)),
                    ).associate { it.startTime.toLocalDate() to (it.result[HeartRateRecord.MEASUREMENTS_COUNT] ?: 0L) }
                    val slices = client.aggregateGroupByDuration(
                        AggregateGroupByDurationRequest(
                            setOf(HeartRateRecord.BPM_MIN, HeartRateRecord.BPM_MAX, HeartRateRecord.BPM_AVG),
                            TimeRangeFilter.between(first.atStartOfDay(zone).toInstant(), last.plusDays(1).atStartOfDay(zone).toInstant()),
                            Duration.ofHours(1),
                        ),
                    ).map {
                        HourSlice(it.startTime, it.result[HeartRateRecord.BPM_MIN], it.result[HeartRateRecord.BPM_MAX], it.result[HeartRateRecord.BPM_AVG])
                    }
                    out += HealthRows.heartRate(slices, counts, zone)
                }
                Kind.RESTING_HEART_RATE -> client.aggregateGroupByPeriod(
                    AggregateGroupByPeriodRequest(setOf(RestingHeartRateRecord.BPM_AVG), byDay, Period.ofDays(1)),
                ).forEach { g ->
                    g.result[RestingHeartRateRecord.BPM_AVG]?.let {
                        out += HealthRows.daily(g.startTime.toLocalDate(), zone, Value.RestingHeartRate(it.toDouble()))
                    }
                }
                Kind.STEPS -> client.aggregateGroupByPeriod(
                    AggregateGroupByPeriodRequest(setOf(StepsRecord.COUNT_TOTAL), byDay, Period.ofDays(1)),
                ).forEach { g ->
                    g.result[StepsRecord.COUNT_TOTAL]?.takeIf { it > 0 }?.let {
                        out += HealthRows.daily(g.startTime.toLocalDate(), zone, Value.Steps(it))
                    }
                }
                else -> Unit
            }
        }
        return out
    }

    private suspend fun <T : Record> readAll(client: HealthConnectClient, type: KClass<T>, start: Instant, end: Instant): List<T> {
        val out = mutableListOf<T>()
        var page: String? = null
        do {
            val res = client.readRecords(ReadRecordsRequest(type, TimeRangeFilter.between(start, end), pageToken = page))
            out += res.records
            page = res.pageToken
        } while (page != null)
        return out
    }

    private suspend fun records(
        client: HealthConnectClient,
        kind: Kind,
        start: Instant,
        end: Instant,
        zone: ZoneId,
        distance: Boolean,
    ): List<Reading> {
        val found: List<Record> = when (kind) {
            Kind.SLEEP -> readAll(client, SleepSessionRecord::class, start, end)
            Kind.BLOOD_PRESSURE -> readAll(client, BloodPressureRecord::class, start, end)
            Kind.WORKOUT -> readAll(client, ExerciseSessionRecord::class, start, end)
            Kind.WEIGHT -> readAll(client, WeightRecord::class, start, end)
            else -> emptyList()
        }
        return found.mapNotNull { mapRecord(client, it, zone, distance) }
    }

    private suspend fun mapRecord(client: HealthConnectClient, r: Record, zone: ZoneId, distance: Boolean): Reading? {
        val id = r.metadata.id
        val device = r.metadata.device?.type
        return when (r) {
            is SleepSessionRecord -> HealthRows.sleep(
                id, r.startTime, r.endTime, r.stages.map { Stage(it.startTime, it.endTime, it.stage) }, device, zone,
            )
            is BloodPressureRecord -> HealthRows.moment(
                id, r.time, device, zone,
                Value.BloodPressure(r.systolic.inMillimetersOfMercury, r.diastolic.inMillimetersOfMercury),
            )
            is WeightRecord -> HealthRows.moment(id, r.time, device, zone, Value.Weight(r.weight.inKilograms, "kg"))
            is ExerciseSessionRecord -> {
                val metres = if (!distance) null else try {
                    client.aggregate(
                        AggregateRequest(setOf(DistanceRecord.DISTANCE_TOTAL), TimeRangeFilter.between(r.startTime, r.endTime)),
                    )[DistanceRecord.DISTANCE_TOTAL]?.inMeters
                } catch (e: SecurityException) {
                    null
                }
                HealthRows.workout(id, r.startTime, r.endTime, r.exerciseType, metres, device, zone)
            }
            else -> null
        }
    }

    private fun json(readings: List<Reading>): JSArray {
        val out = JSArray()
        for (r in readings) {
            out.put(
                JSObject()
                    .put("phoneId", r.phoneId)
                    .put("day", r.day)
                    .put("start", r.start)
                    .put("end", r.end)
                    .put("sourceDevice", r.sourceDevice)
                    .put("value", value(r.value)),
            )
        }
        return out
    }

    private fun value(v: Value): JSObject = when (v) {
        is Value.Sleep -> JSObject().put("type", "sleep").put(
            "asleep",
            JSArray().also { a -> v.asleep.forEach { a.put(JSObject().put("from", it.first).put("to", it.second)) } },
        ).put("inBedOnly", v.inBedOnly)
        is Value.HeartRate -> JSObject().put("type", "heartRate").put(
            "hours",
            JSArray().also { a ->
                v.hours.forEach {
                    a.put(JSObject().put("start", it.start).put("low", it.low).put("high", it.high).put("middle", it.middle))
                }
            },
        ).put("count", v.count)
        is Value.RestingHeartRate -> JSObject().put("type", "restingHeartRate").put("bpm", v.bpm)
        is Value.BloodPressure -> JSObject().put("type", "bloodPressure").put("top", v.top).put("bottom", v.bottom)
        is Value.Steps -> JSObject().put("type", "steps").put("total", v.total)
        is Value.Workout -> JSObject().put("type", "workout").put("kind", v.kind).put("lengthS", v.lengthS)
            .put("distanceM", v.distanceM)
        is Value.Weight -> JSObject().put("type", "weight").put("value", v.value).put("unit", v.unit)
    }
}
