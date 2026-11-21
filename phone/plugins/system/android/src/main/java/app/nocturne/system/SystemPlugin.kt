package app.nocturne.system

import android.app.Activity
import android.content.Intent
import android.net.Uri
import android.provider.Settings
import android.webkit.WebView
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import app.tauri.annotation.Command
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin

/**
 * The app draws edge to edge, and the web view reports no safe area for the system bars. This
 * puts the bars and cut-outs on the page as CSS variables each time they change.
 */
@TauriPlugin
class SystemPlugin(private val activity: Activity) : Plugin(activity) {
    private var insets = CssInsets(0.0, 0.0, 0.0, 0.0)

    override fun load(webView: WebView) {
        super.load(webView)
        ViewCompat.setOnApplyWindowInsetsListener(webView) { view, all ->
            val bars = all.getInsets(
                WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.displayCutout()
            )
            val ime = all.getInsets(WindowInsetsCompat.Type.ime())
            val density = view.resources.displayMetrics.density
            insets = CssInsets.fromDevice(
                bars.top, bars.right, maxOf(bars.bottom, ime.bottom), bars.left, density
            )
            webView.evaluateJavascript(insets.script(), null)
            all
        }
        ViewCompat.requestApplyInsets(webView)
    }

    @Command
    fun insets(invoke: Invoke) {
        val out = JSObject()
        out.put("top", insets.top)
        out.put("right", insets.right)
        out.put("bottom", insets.bottom)
        out.put("left", insets.left)
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

    /** This app's page in the phone's settings, where the camera can be turned back on. */
    @Command
    fun openAppSettings(invoke: Invoke) {
        val intent = Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS)
            .setData(Uri.fromParts("package", activity.packageName, null))
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        activity.startActivity(intent)
        invoke.resolve()
    }
}
