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
 * A time the system lets the app work in the background: opened by [MindWorker] while the phone
 * charges and is online, ended when the core says it is done, when the system stops the work,
 * or at [Window.LONGEST_MINUTES]. The core looks at it, through the plugin, between its passes.
 */
object Window {
    /** The system stops background work at ten minutes; the window ends before. */
    const val LONGEST_MINUTES = 9L

    @Volatile private var open: CountDownLatch? = null

    /** The core is running in this process, so a window can be used. */
    @Volatile var coreRunning = false

    @Synchronized
    fun begin(): CountDownLatch = CountDownLatch(1).also { open = it }

    val isOpen: Boolean get() = open?.count == 1L

    /** The core has nothing left to do in this window. */
    @Synchronized
    fun done() {
        open?.countDown()
        open = null
    }
}

/** One background window, while the phone charges and is online. */
class MindWorker(context: Context, params: WorkerParameters) : Worker(context, params) {
    override fun doWork(): Result {
        // The core starts with the app's screen; a process started for this work alone has
        // nothing to run it.
        if (!Window.coreRunning) return Result.success()
        Window.begin().await(Window.LONGEST_MINUTES, TimeUnit.MINUTES)
        Window.done()
        return Result.success()
    }

    override fun onStopped() {
        Window.done()
    }
}

/** Asks the system for background windows while the mind is on, and for none while it is off. */
object Background {
    private const val NAME = "nocturne-mind"
    private const val EVERY_MINUTES = 60L

    fun schedule(context: Context, on: Boolean) {
        val work = WorkManager.getInstance(context)
        if (!on) {
            work.cancelUniqueWork(NAME)
            Window.done()
            return
        }
        val constraints = Constraints.Builder()
            .setRequiresCharging(true)
            .setRequiredNetworkType(NetworkType.CONNECTED)
            .build()
        val request = PeriodicWorkRequest.Builder(
            MindWorker::class.java, EVERY_MINUTES, TimeUnit.MINUTES
        ).setConstraints(constraints).build()
        work.enqueueUniquePeriodicWork(NAME, ExistingPeriodicWorkPolicy.KEEP, request)
    }
}
