package app.nocturne.system

import android.content.Context
import androidx.work.Constraints
import androidx.work.ExistingPeriodicWorkPolicy
import androidx.work.NetworkType
import androidx.work.PeriodicWorkRequest
import androidx.work.WorkManager
import androidx.work.Worker
import androidx.work.WorkerParameters
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

/**
 * A time the system lets the app work in the background: opened by [MindWorker] or
 * [BackupWorker] while the phone charges and is online, ended when the core says it is done,
 * when the system stops the work, or at [Window.LONGEST_MINUTES]. The core looks at it, through
 * the plugin, between its passes.
 */
object Window {
    /** The system stops background work at ten minutes; the window ends before. */
    const val LONGEST_MINUTES = 9L

    /** One latch for each worker holding a window open. */
    private val open = mutableSetOf<CountDownLatch>()

    /** The core is running in this process, so a window can be used. */
    @Volatile var coreRunning = false

    @Synchronized
    fun begin(): CountDownLatch = CountDownLatch(1).also { open.add(it) }

    val isOpen: Boolean
        @Synchronized get() = open.isNotEmpty()

    /** The core has nothing left to do: every open window ends. */
    @Synchronized
    fun done() {
        open.forEach { it.countDown() }
        open.clear()
    }

    /** One worker's window ends. */
    @Synchronized
    fun end(latch: CountDownLatch) {
        latch.countDown()
        open.remove(latch)
    }
}

/** A worker that holds one background window open until the core is done with it. */
abstract class WindowWorker(context: Context, params: WorkerParameters) :
    Worker(context, params) {
    @Volatile private var latch: CountDownLatch? = null

    override fun doWork(): Result {
        // The core starts with the app's screen; a process started for this work alone has
        // nothing to run it.
        if (!Window.coreRunning) return Result.success()
        val held = Window.begin().also { latch = it }
        held.await(Window.LONGEST_MINUTES, TimeUnit.MINUTES)
        Window.end(held)
        return Result.success()
    }

    override fun onStopped() {
        latch?.let(Window::end)
    }
}

/** One background window for the mind, while the phone charges and is online. */
class MindWorker(context: Context, params: WorkerParameters) : WindowWorker(context, params)

/** One background window for the nightly backup, while the phone charges, on Wi-Fi if asked. */
class BackupWorker(context: Context, params: WorkerParameters) : WindowWorker(context, params)

/**
 * Asks the system for background windows: the mind's while it is on, and the backup's while it
 * is on, each only while the phone charges. The backup's wait for an unmetered network while
 * "Wi-Fi only" is on.
 */
object Background {
    private const val MIND = "nocturne-mind"
    private const val BACKUP = "nocturne-backup"
    private const val EVERY_MINUTES = 60L

    /** What the backup's windows wait for. */
    fun backupNetwork(wifiOnly: Boolean): NetworkType =
        if (wifiOnly) NetworkType.UNMETERED else NetworkType.CONNECTED

    private fun constraints(network: NetworkType) = Constraints.Builder()
        .setRequiresCharging(true)
        .setRequiredNetworkType(network)
        .build()

    fun schedule(context: Context, mind: Boolean, backup: Boolean, wifiOnly: Boolean) {
        val work = WorkManager.getInstance(context)
        if (mind) {
            val request = PeriodicWorkRequest.Builder(
                MindWorker::class.java, EVERY_MINUTES, TimeUnit.MINUTES
            ).setConstraints(constraints(NetworkType.CONNECTED)).build()
            work.enqueueUniquePeriodicWork(MIND, ExistingPeriodicWorkPolicy.KEEP, request)
        } else {
            work.cancelUniqueWork(MIND)
        }
        if (backup) {
            val request = PeriodicWorkRequest.Builder(
                BackupWorker::class.java, EVERY_MINUTES, TimeUnit.MINUTES
            ).setConstraints(constraints(backupNetwork(wifiOnly))).build()
            // The same work with the network it now needs, when "Wi-Fi only" changed.
            work.enqueueUniquePeriodicWork(BACKUP, ExistingPeriodicWorkPolicy.UPDATE, request)
        } else {
            work.cancelUniqueWork(BACKUP)
        }
        if (!mind && !backup) Window.done()
    }
}
