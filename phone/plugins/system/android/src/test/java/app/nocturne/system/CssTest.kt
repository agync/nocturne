package app.nocturne.system

import org.junit.Assert.assertEquals
import org.junit.Test

class CssTest {
    @Test
    fun devicePixelsBecomeCssPixels() {
        val i = CssInsets.fromDevice(96, 0, 63, 0, 2.625f)
        assertEquals(36.571, i.top, 0.001)
        assertEquals(24.0, i.bottom, 0.001)
    }

    @Test
    fun theScriptSetsEveryVariable() {
        val s = CssInsets(24.0, 0.0, 16.5, 0.0).script()
        assertEquals(
            "(function(){var d=document.documentElement,s=d.style;" +
                "s.setProperty('--ph-inset-top','24px');" +
                "s.setProperty('--ph-inset-right','0px');" +
                "s.setProperty('--ph-inset-bottom','16.5px');" +
                "s.setProperty('--ph-inset-left','0px');" +
                "s.setProperty('--ph-inset-keyboard','0px');" +
                "d.classList.toggle('ph-keyboard',false);" +
                "window.dispatchEvent(new Event('ph-insets'))})()",
            s,
        )
    }

    @Test
    fun theKeyboardIsItsOwnInset() {
        val i = CssInsets.fromDevice(96, 0, 63, 0, 2.625f, keyboard = 840)
        assertEquals(24.0, i.bottom, 0.001)
        assertEquals(320.0, i.keyboard, 0.001)
        assert(i.script().contains("toggle('ph-keyboard',true)"))
    }

    @Test
    fun noDensityCountsAsOne() {
        assertEquals(10.0, CssInsets.fromDevice(10, 0, 0, 0, 0f).top, 0.0)
    }
}
