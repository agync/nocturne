package app.nocturne.system

import kotlin.math.roundToInt

/** System bars and cut-outs, and the keyboard while it is up, in CSS pixels. */
data class CssInsets(
    val top: Double,
    val right: Double,
    val bottom: Double,
    val left: Double,
    val keyboard: Double = 0.0,
) {
    /**
     * The script that puts the insets on the page as `--ph-inset-*` variables, marks the page
     * while the keyboard is up, and tells the page they changed.
     */
    fun script(): String {
        val vars = listOf(
            "top" to top, "right" to right, "bottom" to bottom, "left" to left, "keyboard" to keyboard,
        ).joinToString(";") { (side, px) ->
            "s.setProperty('--ph-inset-$side','${round(px)}px')"
        }
        val up = if (keyboard > 0.0) "true" else "false"
        return "(function(){var d=document.documentElement,s=d.style;$vars;" +
            "d.classList.toggle('ph-keyboard',$up);window.dispatchEvent(new Event('ph-insets'))})()"
    }

    companion object {
        /** Device pixels to CSS pixels at this screen density. */
        fun fromDevice(
            top: Int,
            right: Int,
            bottom: Int,
            left: Int,
            density: Float,
            keyboard: Int = 0,
        ): CssInsets {
            val d = if (density > 0f) density.toDouble() else 1.0
            return CssInsets(top / d, right / d, bottom / d, left / d, keyboard / d)
        }

        private fun round(px: Double): String {
            val tenths = (px * 10).roundToInt()
            return if (tenths % 10 == 0) "${tenths / 10}" else "${tenths / 10.0}"
        }
    }
}
