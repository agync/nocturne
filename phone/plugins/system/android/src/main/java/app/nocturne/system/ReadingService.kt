package app.nocturne.system

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.IBinder
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.app.ServiceCompat

/** What the notification says, as the core sends it. */
data class ReadingNote(
    val channel: String,
    val channelAbout: String,
    val title: String,
    val text: String,
    val stop: String,
    val done: Int,
    val count: Int,
)

/**
 * Keeps the app running, with a notification the person can see and stop, while Read now reads on
 * battery. It starts only from the person's tap, while a Nocturne screen is in front.
 */
class ReadingService : Service() {
    companion object {
        const val ACTION_STOP = "app.nocturne.system.READING_STOP"
        const val CHANNEL = "nocturne-reading-now"
        const val ID = 41
        const val LEFT_ID = 42

        fun note(context: Context, n: ReadingNote): Notification {
            channel(context, n.channel, n.channelAbout)
            val stop = PendingIntent.getService(
                context, 0,
                Intent(context, ReadingService::class.java).setAction(ACTION_STOP),
                PendingIntent.FLAG_IMMUTABLE
            )
            return NotificationCompat.Builder(context, CHANNEL)
                .setSmallIcon(android.R.drawable.stat_notify_sync)
                .setContentTitle(n.title)
                .setContentText(n.text)
                .setProgress(n.count, n.done, false)
                .setOngoing(true)
                .setSilent(true)
                .setOnlyAlertOnce(true)
                .setContentIntent(open(context))
                .addAction(0, n.stop, stop)
                .build()
        }

        /** The one line left after a stop, in the same quiet channel. */
        fun leftover(context: Context, n: ReadingNote, title: String, text: String) {
            channel(context, n.channel, n.channelAbout)
            val note = NotificationCompat.Builder(context, CHANNEL)
                .setSmallIcon(android.R.drawable.stat_notify_sync)
                .setContentTitle(title)
                .setContentText(text)
                .setSilent(true)
                .setAutoCancel(true)
                .setContentIntent(open(context))
                .build()
            show(context, LEFT_ID, note)
        }

        fun show(context: Context, id: Int, note: Notification) {
            val manager = NotificationManagerCompat.from(context)
            if (!manager.areNotificationsEnabled()) return
            try {
                manager.notify(id, note)
            } catch (_: SecurityException) {
            }
        }

        fun clearLeftover(context: Context) {
            NotificationManagerCompat.from(context).cancel(LEFT_ID)
        }

        fun extras(intent: Intent, n: ReadingNote): Intent = intent
            .putExtra("channel", n.channel)
            .putExtra("channelAbout", n.channelAbout)
            .putExtra("title", n.title)
            .putExtra("text", n.text)
            .putExtra("stop", n.stop)
            .putExtra("done", n.done)
            .putExtra("count", n.count)

        private fun from(intent: Intent) = ReadingNote(
            intent.getStringExtra("channel") ?: "",
            intent.getStringExtra("channelAbout") ?: "",
            intent.getStringExtra("title") ?: "",
            intent.getStringExtra("text") ?: "",
            intent.getStringExtra("stop") ?: "",
            intent.getIntExtra("done", 0),
            intent.getIntExtra("count", 0),
        )

        private fun channel(context: Context, name: String, about: String) {
            val channel = NotificationChannel(CHANNEL, name, NotificationManager.IMPORTANCE_LOW)
            channel.description = about
            channel.setSound(null, null)
            channel.enableVibration(false)
            context.getSystemService(NotificationManager::class.java)
                .createNotificationChannel(channel)
        }

        private fun open(context: Context): PendingIntent? {
            val launch = context.packageManager.getLaunchIntentForPackage(context.packageName)
                ?: return null
            return PendingIntent.getActivity(context, 0, launch, PendingIntent.FLAG_IMMUTABLE)
        }
    }

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        if (intent == null || intent.action == ACTION_STOP) {
            stopReading()
            return START_NOT_STICKY
        }
        Reading.running = true
        ServiceCompat.startForeground(
            this, ID, note(this, from(intent)), ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC
        )
        return START_NOT_STICKY
    }

    /** Android 15 ends a data sync service after its allowed time: told as its own stop. */
    override fun onTimeout(startId: Int, fgsType: Int) {
        Reading.timedOut = true
        endService()
    }

    override fun onDestroy() {
        Reading.running = false
        super.onDestroy()
    }

    private fun stopReading() {
        Reading.stopAsked = true
        endService()
    }

    private fun endService() {
        Reading.running = false
        ServiceCompat.stopForeground(this, ServiceCompat.STOP_FOREGROUND_REMOVE)
        stopSelf()
    }
}
