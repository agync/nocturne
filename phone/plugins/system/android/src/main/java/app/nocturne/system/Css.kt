package app.nocturne.system

import kotlin.math.roundToInt

/** System bars and cut-outs, in CSS pixels. */
data class CssInsets(val top: Double, val right: Double, val bottom: Double, val left: Double) {
    /** The script that puts the insets on the page as `--ph-inset-*` variables. */
    fun script(): String {
        val vars = listOf("top" to top, "right" to right, "bottom" to bottom, "left" to left)
            .joinToString(";") { (side, px) ->
                "s.setProperty('--ph-inset-$side','${round(px)}px')"
            }
        return "(function(){var s=document.documentElement.style;$vars})()"
    }

    companion object {
        /** Device pixels to CSS pixels at this screen density. */
        fun fromDevice(top: Int, right: Int, bottom: Int, left: Int, density: Float): CssInsets {
            val d = if (density > 0f) density.toDouble() else 1.0
            return CssInsets(top / d, right / d, bottom / d, left / d)
        }

        private fun round(px: Double): String {
            val tenths = (px * 10).roundToInt()
            return if (tenths % 10 == 0) "${tenths / 10}" else "${tenths / 10.0}"
        }
    }
}
