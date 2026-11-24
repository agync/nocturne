package app.nocturne.system

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.os.PersistableBundle

/**
 * Text the person copies to paste into a password manager: a backup's phrase or recovery key.
 * It is marked sensitive, so the phone keeps it out of its paste previews, and it leaves the
 * clipboard after a while unless something else was copied since. While another app is in front
 * the phone may not let Nocturne look, and then it leaves the clipboard alone: what the person
 * copied there is never wiped, and the phone clears a sensitive copy itself in time.
 */
object Clip {
    /** The label Nocturne's copies carry, so a later look knows the copy is still its own. */
    const val LABEL = "nocturne-secret"

    /** Android 13's name for the mark, which older phones' keyboards read too. */
    private const val SENSITIVE = "android.content.extra.IS_SENSITIVE"

    /** Whether the clipboard still holds Nocturne's copy, as far as the app may see. */
    fun stillOurs(label: CharSequence?, readable: Boolean): Boolean =
        readable && label?.toString() == LABEL

    fun copy(context: Context, text: String, clearAfterMs: Long) {
        val clipboard = context.getSystemService(ClipboardManager::class.java) ?: return
        val clip = ClipData.newPlainText(LABEL, text)
        clip.description.extras = PersistableBundle().apply {
            putBoolean(SENSITIVE, true)
        }
        clipboard.setPrimaryClip(clip)
        Handler(Looper.getMainLooper()).postDelayed({ clear(clipboard) }, clearAfterMs)
    }

    private fun clear(clipboard: ClipboardManager) {
        // Away from the front, the phone may not let the app read what is there.
        val description = runCatching { clipboard.primaryClipDescription }.getOrNull()
        if (!stillOurs(description?.label, description != null)) return
        if (Build.VERSION.SDK_INT >= 28) {
            clipboard.clearPrimaryClip()
        } else {
            clipboard.setPrimaryClip(ClipData.newPlainText("", ""))
        }
    }
}
