package app.nocturne.speech

import android.Manifest
import android.app.Activity
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.Permission
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import java.io.File

@InvokeArg
class ReadArgs {
    var path: String = ""
    var language: String = "en-GB"
}

/** The commands the app's Rust side calls; the reading is in [SpeechReader]. */
@TauriPlugin(
    permissions = [Permission(strings = [Manifest.permission.RECORD_AUDIO], alias = "microphone")],
)
class SpeechPlugin(activity: Activity) : Plugin(activity) {
    private val reader = SpeechReader(activity)

    private fun answer(invoke: Invoke, outcome: Outcome, text: String?) {
        val out = JSObject()
        out.put("outcome", outcome.wire)
        if (text != null) out.put("text", text)
        invoke.resolve(out)
    }

    /**
     * Whether a file of the language can be read now, without reading one. It runs the check, so
     * on a reader that fails it the microphone may be open for a second.
     */
    @Command
    fun checkSupport(invoke: Invoke) {
        val language = invoke.parseArgs(ReadArgs::class.java).language
        reader.submit(language, null) { outcome, text -> answer(invoke, outcome, text) }
    }

    @Command
    fun transcribe(invoke: Invoke) {
        val args = invoke.parseArgs(ReadArgs::class.java)
        reader.submit(args.language, File(args.path)) { outcome, text -> answer(invoke, outcome, text) }
    }
}
