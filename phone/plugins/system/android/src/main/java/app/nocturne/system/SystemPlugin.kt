package app.nocturne.system

import android.app.Activity
import android.content.Intent
import android.content.IntentFilter
import android.os.BatteryManager
import android.net.Uri
import android.os.StatFs
import android.provider.DocumentsContract
import android.provider.OpenableColumns
import android.provider.Settings
import android.text.format.DateFormat
import android.webkit.WebView
import androidx.activity.result.ActivityResult
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
import java.io.File
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
class BackgroundArgs {
    var on: Boolean = false
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
    }

    private val safe by lazy { Safe(File(activity.noBackupFilesDir, "safe")) }

    override fun load(webView: WebView) {
        super.load(webView)
        Window.coreRunning = true
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
        invoke.resolve(JSObject().put("charging", charging).put("window", Window.isOpen))
    }

    /** Background windows while the mind is on; none while it is off. */
    @Command
    fun background(invoke: Invoke) {
        val on = invoke.parseArgs(BackgroundArgs::class.java).on
        try {
            Background.schedule(activity.applicationContext, on)
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
