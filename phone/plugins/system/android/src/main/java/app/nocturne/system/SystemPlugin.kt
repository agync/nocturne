package app.nocturne.system

import android.accounts.AccountManager
import android.Manifest
import android.app.Activity
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.net.ConnectivityManager
import android.net.NetworkCapabilities
import android.content.pm.PackageManager
import android.os.BatteryManager
import android.os.Build
import android.net.Uri
import android.os.PowerManager
import android.os.StatFs
import android.provider.DocumentsContract
import android.provider.OpenableColumns
import android.provider.Settings
import android.text.format.DateFormat
import android.webkit.WebView
import androidx.activity.result.ActivityResult
import androidx.activity.result.IntentSenderRequest
import androidx.core.app.ActivityCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.content.ContextCompat
import androidx.core.content.IntentCompat
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import app.tauri.annotation.ActivityCallback
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSArray
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import com.google.android.gms.common.api.ApiException
import com.google.android.gms.common.api.CommonStatusCodes
import java.io.File
import java.util.concurrent.Executors
import java.util.concurrent.atomic.AtomicBoolean

@InvokeArg
class SaveArgs {
    var path: String = ""
    var name: String = ""
}

@InvokeArg
class FolderArgs {
    var path: String = ""
}

@InvokeArg
class SecretArgs {
    var name: String = ""
    var value: String? = null
}

@InvokeArg
class CopyArgs {
    var text: String = ""
    var clearAfterMs: Long = 60_000
}

@InvokeArg
class BackgroundArgs {
    var mind: Boolean = false
    var backup: Boolean = false
    var wifiOnly: Boolean = false
}

@InvokeArg
class PlaceArgs {
    var name: String = ""
    var path: String = ""
    var prefix: String = ""
}

@InvokeArg
class ConnectArgs {
    var choose: Boolean = false
}

@InvokeArg
class ReadingArgs {
    var state: String? = null
    var channel: String = ""
    var channelAbout: String = ""
    var title: String? = null
    var text: String? = null
    var stop: String = ""
    var done: Int = 0
    var count: Int = 0
    var pausedTitle: String = ""
    var pausedText: String = ""
    var why: String? = null
}

/**
 * The app draws edge to edge, and the web view reports no safe area for the system bars. This
 * puts the bars, cut-outs and keyboard on the page as CSS variables each time they change. It
 * also holds what the app was opened for from outside: a shortcut on its icon, or something
 * another app shared, until the screens take it.
 */
@TauriPlugin
class SystemPlugin(private val activity: Activity) : Plugin(activity) {
    private var insets = CssInsets(0.0, 0.0, 0.0, 0.0)
    @Volatile private var route: String? = null
    @Volatile private var share: Intent? = null

    /** A save or open sheet is up: the phone keeps one answer waiting at a time. */
    private val sheetUp = AtomicBoolean(false)

    companion object {
        /** The largest file kept from a share, as the computer takes at most. */
        const val MAX_FILE = 52_428_800L
        /** The most files and bytes kept from one share. */
        const val MAX_FILES = 10
        const val MAX_TOTAL = 4 * MAX_FILE
        private const val NOTES_REQUEST = 4101
    }

    @Volatile private var lastNote: ReadingNote? = null
    private var askedForNotes = false

    private val safe by lazy { Safe(File(activity.noBackupFilesDir, "safe")) }

    /** The backup's calls to the person's Drive, one at a time and never on the main thread. */
    private val placeWork = Executors.newSingleThreadExecutor()
    private val driveAccount by lazy { DriveAccount(activity.applicationContext) }
    private val drive by lazy { Drive(UrlHttp(), driveAccount) }

    override fun load(webView: WebView) {
        super.load(webView)
        Window.coreRunning = true
        Reading.inFront = true
        ViewCompat.setOnApplyWindowInsetsListener(webView) { view, all ->
            val bars = all.getInsets(
                WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.displayCutout()
            )
            val ime = all.getInsets(WindowInsetsCompat.Type.ime())
            val density = view.resources.displayMetrics.density
            insets = CssInsets.fromDevice(
                bars.top, bars.right, bars.bottom, bars.left, density, ime.bottom
            )
            webView.evaluateJavascript(insets.script(), null)
            all
        }
        ViewCompat.requestApplyInsets(webView)
        // Shares from an earlier run that were never kept.
        File(activity.cacheDir, "shared").deleteRecursively()
        take(activity.intent)
    }

    override fun onResume() {
        Reading.inFront = true
        ReadingService.clearLeftover(activity.applicationContext)
    }

    override fun onPause() {
        Reading.inFront = false
    }

    override fun onNewIntent(intent: Intent) {
        take(intent)
    }

    /**
     * Takes what an intent opened the app for, once: an app opened again from Recents carries
     * its first intent, which must not keep twice or start the recorder again.
     */
    private fun take(intent: Intent?) {
        if (intent == null) return
        if (intent.flags and Intent.FLAG_ACTIVITY_LAUNCHED_FROM_HISTORY != 0) return
        val taken = when (intent.action) {
            Intent.ACTION_SEND, Intent.ACTION_SEND_MULTIPLE -> {
                share = intent
                true
            }
            Intent.ACTION_VIEW -> Shares.route(intent.data?.scheme, intent.data?.host)
                ?.also { route = it } != null
            else -> false
        }
        if (taken) activity.intent = Intent(Intent.ACTION_MAIN)
    }

    @Command
    fun insets(invoke: Invoke) {
        val out = JSObject()
        out.put("top", insets.top)
        out.put("right", insets.right)
        out.put("bottom", insets.bottom)
        out.put("left", insets.left)
        out.put("keyboard", insets.keyboard)
        invoke.resolve(out)
    }

    /** The name the person gave this phone in its settings, when there is one. */
    @Command
    fun deviceName(invoke: Invoke) {
        val name = Settings.Global.getString(activity.contentResolver, Settings.Global.DEVICE_NAME)
        val out = JSObject()
        out.put("name", name)
        invoke.resolve(out)
    }

    /** Whether the phone is set to 24-hour time, as its settings say. */
    @Command
    fun hourClock(invoke: Invoke) {
        val out = JSObject()
        out.put("is24", DateFormat.is24HourFormat(activity))
        invoke.resolve(out)
    }

    /** A phrase or recovery key to paste elsewhere: kept out of paste previews, cleared later. */
    @Command
    fun copySecret(invoke: Invoke) {
        val args = invoke.parseArgs(CopyArgs::class.java)
        Clip.copy(activity, args.text, args.clearAfterMs)
        invoke.resolve()
    }

    /** This app's page in the phone's settings, where the camera can be turned back on. */
    @Command
    fun openAppSettings(invoke: Invoke) {
        val intent = Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS)
            .setData(Uri.fromParts("package", activity.packageName, null))
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        activity.startActivity(intent)
        invoke.resolve()
    }

    /** The shortcut the app was opened from, once. */
    @Command
    fun takeLaunch(invoke: Invoke) {
        val out = JSObject().put("route", route)
        route = null
        invoke.resolve(out)
    }

    /**
     * What another app shared, once: its text, and each file copied into this app's own space,
     * because the other app's permission to read it lasts only while this screen is open.
     */
    @Command
    fun takeShared(invoke: Invoke) {
        val intent = share
        share = null
        if (intent == null) return invoke.resolve(JSObject().put("shared", null))
        Thread {
            try {
                invoke.resolve(JSObject().put("shared", copy(intent)))
            } catch (e: Exception) {
                invoke.reject(e.message ?: "failed")
            }
        }.start()
    }

    private fun uris(intent: Intent): List<Uri> {
        val found = mutableListOf<Uri>()
        if (intent.action == Intent.ACTION_SEND_MULTIPLE) {
            IntentCompat.getParcelableArrayListExtra(intent, Intent.EXTRA_STREAM, Uri::class.java)
                ?.let { found.addAll(it) }
        } else {
            IntentCompat.getParcelableExtra(intent, Intent.EXTRA_STREAM, Uri::class.java)
                ?.let { found.add(it) }
        }
        val clip = intent.clipData
        if (found.isEmpty() && clip != null) {
            for (i in 0 until clip.itemCount) clip.getItemAt(i).uri?.let { found.add(it) }
        }
        return found.filter { it.scheme == "content" }
    }

    private fun copy(intent: Intent): JSObject {
        // A folder of its own, so a second share never touches the files of the first.
        val dir = File(File(activity.cacheDir, "shared"), System.nanoTime().toString())
        dir.mkdirs()
        val files = JSArray()
        var total = 0L
        for ((index, uri) in uris(intent).take(MAX_FILES).withIndex()) {
            val resolver = activity.contentResolver
            var name: String? = null
            var size = -1L
            resolver.query(uri, null, null, null, null)?.use { c ->
                if (c.moveToFirst()) {
                    val n = c.getColumnIndex(OpenableColumns.DISPLAY_NAME)
                    val s = c.getColumnIndex(OpenableColumns.SIZE)
                    if (n >= 0) name = c.getString(n)
                    if (s >= 0 && !c.isNull(s)) size = c.getLong(s)
                }
            }
            if (size > MAX_FILE || total + maxOf(size, 0L) > MAX_TOTAL) continue
            val kept = Shares.fileName(index, name)
            val target = File(dir, kept)
            val written = resolver.openInputStream(uri)?.use { input ->
                target.outputStream().use { output -> copyCapped(input, output) }
            } ?: continue
            if (written < 0 || total + written > MAX_TOTAL) {
                target.delete()
                continue
            }
            total += written
            files.put(
                JSObject()
                    .put("name", kept)
                    .put("shown", Shares.shownName(kept))
                    .put("mime", resolver.getType(uri) ?: intent.type ?: "application/octet-stream")
                    .put("size", written)
            )
        }
        return JSObject()
            .put("text", intent.getCharSequenceExtra(Intent.EXTRA_TEXT)?.toString())
            .put("subject", intent.getStringExtra(Intent.EXTRA_SUBJECT))
            .put("dir", dir.absolutePath)
            .put("files", files)
    }

    /** Bytes copied, or -1 when the file is larger than the computer takes. */
    private fun copyCapped(input: java.io.InputStream, output: java.io.OutputStream): Long {
        val buffer = ByteArray(64 * 1024)
        var total = 0L
        while (true) {
            val n = input.read(buffer)
            if (n < 0) return total
            total += n
            if (total > MAX_FILE) return -1
            output.write(buffer, 0, n)
        }
    }

    /** Keeps a secret in the phone's safe place, in place of one of the same name. */
    @Command
    fun secretKeep(invoke: Invoke) {
        val args = invoke.parseArgs(SecretArgs::class.java)
        val value = args.value
        if (!Safe.validName(args.name) || value == null) return invoke.reject("invalid")
        try {
            safe.keep(args.name, value)
            invoke.resolve()
        } catch (e: Exception) {
            invoke.reject(e.message ?: "failed")
        }
    }

    /** A kept secret, or null. */
    @Command
    fun secretRead(invoke: Invoke) {
        val args = invoke.parseArgs(SecretArgs::class.java)
        if (!Safe.validName(args.name)) return invoke.reject("invalid")
        try {
            invoke.resolve(JSObject().put("value", safe.read(args.name)))
        } catch (e: Exception) {
            invoke.reject(e.message ?: "failed")
        }
    }

    @Command
    fun secretForget(invoke: Invoke) {
        val args = invoke.parseArgs(SecretArgs::class.java)
        if (!Safe.validName(args.name)) return invoke.reject("invalid")
        try {
            safe.forget(args.name)
            invoke.resolve()
        } catch (e: Exception) {
            invoke.reject(e.message ?: "failed")
        }
    }

    /** Whether the phone charges, and whether a background window is open. */
    @Command
    fun power(invoke: Invoke) {
        val battery = activity.applicationContext
            .registerReceiver(null, IntentFilter(Intent.ACTION_BATTERY_CHANGED))
        val charging = battery?.let {
            Power.charging(
                it.getIntExtra(BatteryManager.EXTRA_PLUGGED, -1),
                it.getIntExtra(BatteryManager.EXTRA_STATUS, -1)
            )
        }
        val percent = battery?.let {
            Power.percent(
                it.getIntExtra(BatteryManager.EXTRA_LEVEL, -1),
                it.getIntExtra(BatteryManager.EXTRA_SCALE, -1)
            )
        }
        val saver = activity.getSystemService(PowerManager::class.java)?.isPowerSaveMode
        invoke.resolve(
            JSObject()
                .put("charging", charging)
                .put("window", Window.isOpen)
                .put("unmetered", unmetered())
                .put("percent", percent)
                .put("saver", saver)
        )
    }

    /**
     * Read now while the person is away: the notification that keeps the app running and shows
     * the progress, and the one line left after a stop. The core calls it from the person's tap
     * and again between passes.
     */
    @Command
    fun readingNow(invoke: Invoke) {
        val args = invoke.parseArgs(ReadingArgs::class.java)
        val context = activity.applicationContext
        when (Reading.state(args.state)) {
            Reading.State.Reading -> reading(context, args)
            Reading.State.Ended -> ended(context, args)
            null -> return invoke.reject("invalid")
        }
        val notes = NotificationManagerCompat.from(context).areNotificationsEnabled()
        val answer = Reading.answer(notes)
        invoke.resolve(
            JSObject().put("stop", answer.stop).put("paused", false).put("notes", answer.notes)
        )
    }

    private fun reading(context: Context, args: ReadingArgs) {
        val note = ReadingNote(
            args.channel, args.channelAbout, args.title ?: "", args.text ?: "", args.stop,
            args.done, args.count
        )
        lastNote = note
        if (Reading.running) {
            ReadingService.show(context, ReadingService.ID, ReadingService.note(context, note))
            return
        }
        if (!Reading.inFront) return
        askForNotes()
        try {
            ContextCompat.startForegroundService(
                context, ReadingService.extras(Intent(context, ReadingService::class.java), note)
            )
        } catch (_: Exception) {
        }
    }

    private fun ended(context: Context, args: ReadingArgs) {
        context.stopService(Intent(context, ReadingService::class.java))
        val title = args.title
        val note = lastNote
        if (title != null && note != null && Reading.leaveLine(title, Reading.inFront)) {
            ReadingService.leftover(context, note, title, args.text ?: "")
        }
    }

    private fun askForNotes() {
        val granted = ContextCompat.checkSelfPermission(
            activity, Manifest.permission.POST_NOTIFICATIONS
        ) == PackageManager.PERMISSION_GRANTED
        if (!Reading.shouldAsk(Build.VERSION.SDK_INT, granted, askedForNotes)) return
        askedForNotes = true
        ActivityCompat.requestPermissions(
            activity, arrayOf(Manifest.permission.POST_NOTIFICATIONS), NOTES_REQUEST
        )
    }

    /** Whether the network in use costs nothing by the byte, such as Wi-Fi; null when offline. */
    private fun unmetered(): Boolean? {
        val connectivity = activity.getSystemService(ConnectivityManager::class.java) ?: return null
        val network = connectivity.activeNetwork ?: return null
        val caps = connectivity.getNetworkCapabilities(network) ?: return null
        return caps.hasCapability(NetworkCapabilities.NET_CAPABILITY_NOT_METERED) ||
            (Build.VERSION.SDK_INT >= 30 &&
                caps.hasCapability(NetworkCapabilities.NET_CAPABILITY_TEMPORARILY_NOT_METERED))
    }

    /**
     * The phone's account picker, then Google's one-time permission for files the app makes in
     * the Drive. The picker shows when no account was chosen yet, when the one chosen left the
     * phone, or with `choose`. Nothing of Nocturne's shows between these screens.
     */
    @Command
    fun placeConnect(invoke: Invoke) {
        val choose = invoke.parseArgs(ConnectArgs::class.java).choose
        val chosen = if (choose) null else driveAccount.chosen()?.takeIf(driveAccount::onPhone)
        if (chosen != null) return authorizeFor(invoke, chosen)
        try {
            val picker = AccountManager.newChooseAccountIntent(
                null, null, arrayOf(DriveAccount.GOOGLE), null, null, null, null
            )
            startActivityForResult(invoke, picker, "placeAccountChosen")
        } catch (e: Exception) {
            connectOutcome(invoke, "failed")
        }
    }

    @ActivityCallback
    fun placeAccountChosen(invoke: Invoke, result: ActivityResult) {
        val email = result.data?.getStringExtra(AccountManager.KEY_ACCOUNT_NAME)
        if (result.resultCode != Activity.RESULT_OK || email.isNullOrEmpty()) {
            return connectOutcome(invoke, "noAccount")
        }
        authorizeFor(invoke, email)
    }

    /** Google's permission for `email`, shown only when it is not given yet. */
    private fun authorizeFor(invoke: Invoke, email: String) {
        placeWork.execute {
            try {
                driveAccount.authorize(email)
                    .addOnSuccessListener { result ->
                        try {
                            if (!result.hasResolution()) {
                                return@addOnSuccessListener connected(invoke, email)
                            }
                            val intent = result.pendingIntent
                                ?: return@addOnSuccessListener connectOutcome(invoke, "failed")
                            pendingEmail = email
                            startIntentSenderForResult(
                                invoke,
                                IntentSenderRequest.Builder(intent.intentSender).build(),
                                "placeAuthorized"
                            )
                        } catch (e: Exception) {
                            connectOutcome(invoke, "failed")
                        }
                    }
                    .addOnFailureListener { e -> connectFailed(invoke, e) }
            } catch (e: Exception) {
                connectFailed(invoke, e)
            }
        }
    }

    /** The account Google's permission screen is up for. */
    @Volatile private var pendingEmail: String? = null

    @ActivityCallback
    fun placeAuthorized(invoke: Invoke, result: ActivityResult) {
        val email = pendingEmail
        pendingEmail = null
        if (result.resultCode != Activity.RESULT_OK || email == null) {
            return connectOutcome(invoke, "refused")
        }
        try {
            val authorized = driveAccount.resultFrom(result.data)
            if (authorized.accessToken == null) return connectOutcome(invoke, "refused")
            connected(invoke, email)
        } catch (e: Exception) {
            connectFailed(invoke, e)
        }
    }

    private fun connectOutcome(invoke: Invoke, outcome: String) =
        invoke.resolve(JSObject().put("outcome", outcome))

    private fun connectFailed(invoke: Invoke, e: Exception) {
        val outcome = when ((e as? ApiException)?.statusCode) {
            CommonStatusCodes.CANCELED -> "refused"
            CommonStatusCodes.SIGN_IN_REQUIRED -> "noAccount"
            CommonStatusCodes.NETWORK_ERROR, CommonStatusCodes.TIMEOUT -> "offline"
            else -> "failed"
        }
        connectOutcome(invoke, outcome)
    }

    /** The permission is there: the account is kept for the nights, with the Drive's room left. */
    private fun connected(invoke: Invoke, email: String) {
        placeWork.execute {
            try {
                val previous = driveAccount.chosen()
                driveAccount.keep(email)
                val changed = previous != null && !previous.equals(email, ignoreCase = true)
                if (changed) drive.forget()
                val free = try {
                    drive.free()
                } catch (e: PlaceFailure) {
                    null
                }
                invoke.resolve(
                    JSObject()
                        .put("outcome", "connected")
                        .put("account", email)
                        .put("previous", if (changed) previous else null)
                        .put("free", free)
                )
            } catch (e: Exception) {
                connectOutcome(invoke, "failed")
            }
        }
    }

    /** One call to the Drive on the backup's own thread; its outcome is always an answer. */
    private fun place(invoke: Invoke, work: () -> JSObject) {
        placeWork.execute {
            val out = try {
                work().put("outcome", "ok")
            } catch (e: PlaceFailure) {
                JSObject().put("outcome", e.outcome.wire).put("message", e.message)
            } catch (e: Exception) {
                JSObject().put("outcome", PlaceOutcome.Failed.wire)
                    .put("message", e.message ?: e.javaClass.simpleName)
            }
            invoke.resolve(out)
        }
    }

    private fun named(invoke: Invoke): PlaceArgs? {
        val args = invoke.parseArgs(PlaceArgs::class.java)
        if (PlaceNames.valid(args.name)) return args
        invoke.reject("invalid")
        return null
    }

    /** Writes the file at `path` to the backup folder as `name`. */
    @Command
    fun placePut(invoke: Invoke) {
        val args = named(invoke) ?: return
        place(invoke) {
            drive.put(args.name, File(args.path).readBytes())
            JSObject()
        }
    }

    /** Reads `name` from the backup folder into the file at `path`, when it is there. */
    @Command
    fun placeGet(invoke: Invoke) {
        val args = named(invoke) ?: return
        place(invoke) {
            val bytes = drive.get(args.name)
            if (bytes != null) {
                val target = File(args.path)
                val staged = File(target.parentFile, target.name + ".part")
                staged.writeBytes(bytes)
                if (!staged.renameTo(target)) {
                    staged.delete()
                    throw java.io.IOException("not written")
                }
            }
            JSObject().put("found", bytes != null)
        }
    }

    @Command
    fun placeList(invoke: Invoke) {
        val args = invoke.parseArgs(PlaceArgs::class.java)
        if (!PlaceNames.validPrefix(args.prefix)) return invoke.reject("invalid")
        place(invoke) {
            val entries = JSArray()
            for (e in drive.list(args.prefix)) {
                entries.put(
                    JSObject().put("name", e.name).put("size", e.size).put("modified", e.modified)
                )
            }
            JSObject().put("entries", entries)
        }
    }

    @Command
    fun placeDelete(invoke: Invoke) {
        val args = named(invoke) ?: return
        place(invoke) {
            drive.delete(args.name)
            JSObject()
        }
    }

    /** Background windows for the mind and for the backup while each is on. */
    @Command
    fun background(invoke: Invoke) {
        val args = invoke.parseArgs(BackgroundArgs::class.java)
        try {
            Background.schedule(activity.applicationContext, args.mind, args.backup, args.wifiOnly)
            invoke.resolve()
        } catch (e: Exception) {
            invoke.reject(e.message ?: "failed")
        }
    }

    /** The core has nothing left to do in this background window. */
    @Command
    fun backgroundDone(invoke: Invoke) {
        Window.done()
        invoke.resolve()
    }

    /** Bytes free on the volume that holds the folder, as an app may use them. */
    @Command
    fun freeSpace(invoke: Invoke) {
        val path = invoke.parseArgs(FolderArgs::class.java).path
        try {
            invoke.resolve(JSObject().put("bytes", StatFs(path).availableBytes))
        } catch (e: IllegalArgumentException) {
            invoke.resolve(JSObject().put("bytes", null))
        }
    }

    private fun outcome(invoke: Invoke, outcome: FileOutcome) =
        invoke.resolve(JSObject().put("outcome", outcome.wire))

    /** The phone's save sheet, so the person chooses where a file of the app's goes. */
    @Command
    fun saveFile(invoke: Invoke) {
        val args = invoke.parseArgs(SaveArgs::class.java)
        if (!File(args.path).isFile) return outcome(invoke, FileOutcome.Failed)
        val intent = Intent(Intent.ACTION_CREATE_DOCUMENT)
            .addCategory(Intent.CATEGORY_OPENABLE)
            .setType("application/octet-stream")
            .putExtra(Intent.EXTRA_TITLE, args.name)
        showSheet(invoke, intent, "saveChosen")
    }

    private fun showSheet(invoke: Invoke, intent: Intent, callback: String) {
        if (!sheetUp.compareAndSet(false, true)) return outcome(invoke, FileOutcome.Failed)
        try {
            startActivityForResult(invoke, intent, callback)
        } catch (e: Exception) {
            sheetUp.set(false)
            outcome(invoke, FileOutcome.Failed)
        }
    }

    @ActivityCallback
    fun saveChosen(invoke: Invoke, result: ActivityResult) {
        sheetUp.set(false)
        val uri = result.data?.data
        if (result.resultCode != Activity.RESULT_OK || uri == null) {
            return outcome(invoke, FileOutcome.Cancelled)
        }
        val source = File(invoke.parseArgs(SaveArgs::class.java).path)
        Thread {
            val resolver = activity.contentResolver
            try {
                val written = resolver.openOutputStream(uri, "wt")?.use { output ->
                    source.inputStream().use { input -> Files.copy(input, output) }
                }
                if (written == source.length()) return@Thread outcome(invoke, FileOutcome.Saved)
                throw java.io.IOException("short write")
            } catch (e: Exception) {
                // The empty or half file the sheet made is no backup: it goes.
                try {
                    DocumentsContract.deleteDocument(resolver, uri)
                } catch (_: Exception) {
                }
                outcome(invoke, if (Files.isFull(e)) FileOutcome.Full else FileOutcome.Failed)
            }
        }.start()
    }

    /** The phone's open sheet; the chosen file is copied into the folder the app names. */
    @Command
    fun chooseFile(invoke: Invoke) {
        val intent = Intent(Intent.ACTION_OPEN_DOCUMENT)
            .addCategory(Intent.CATEGORY_OPENABLE)
            .setType("*/*")
        showSheet(invoke, intent, "fileChosen")
    }

    @ActivityCallback
    fun fileChosen(invoke: Invoke, result: ActivityResult) {
        sheetUp.set(false)
        val uri = result.data?.data
        if (result.resultCode != Activity.RESULT_OK || uri == null) {
            return outcome(invoke, FileOutcome.Cancelled)
        }
        val dir = File(invoke.parseArgs(FolderArgs::class.java).path)
        Thread {
            val target = File(dir, Files.CHOSEN)
            try {
                val resolver = activity.contentResolver
                var name: String? = null
                var size = -1L
                var modified: Long? = null
                resolver.query(uri, null, null, null, null)?.use { c ->
                    if (c.moveToFirst()) {
                        val n = c.getColumnIndex(OpenableColumns.DISPLAY_NAME)
                        val s = c.getColumnIndex(OpenableColumns.SIZE)
                        val m = c.getColumnIndex(DocumentsContract.Document.COLUMN_LAST_MODIFIED)
                        if (n >= 0) name = c.getString(n)
                        if (s >= 0 && !c.isNull(s)) size = c.getLong(s)
                        if (m >= 0 && !c.isNull(m)) modified = Files.modified(c.getLong(m))
                    }
                }
                dir.mkdirs()
                if (!Files.fits(size, StatFs(dir.path).availableBytes)) {
                    return@Thread outcome(invoke, FileOutcome.Full)
                }
                val copied = resolver.openInputStream(uri)?.use { input ->
                    target.outputStream().use { output -> Files.copy(input, output) }
                } ?: throw java.io.IOException("unreadable")
                invoke.resolve(
                    JSObject()
                        .put("outcome", FileOutcome.Chosen.wire)
                        .put("path", target.absolutePath)
                        .put("name", name ?: "")
                        .put("size", copied)
                        .put("modified", modified)
                )
            } catch (e: Exception) {
                target.delete()
                outcome(invoke, if (Files.isFull(e)) FileOutcome.Full else FileOutcome.Failed)
            }
        }.start()
    }
}
