package app.nocturne.system

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class SharesTest {
    @Test
    fun onlyNocturneShortcutsOpenARoute() {
        assertEquals("keep", Shares.route("nocturne", "keep"))
        assertEquals("checkin", Shares.route("nocturne", "checkin"))
        assertNull(Shares.route("nocturne", "settings"))
        assertNull(Shares.route("https", "keep"))
        assertNull(Shares.route(null, null))
    }

    @Test
    fun aSharedNameStaysInsideTheFolder() {
        assertEquals("0-passwd", Shares.fileName(0, "../../etc/passwd"))
        assertEquals("1-hidden", Shares.fileName(1, ".hidden"))
        assertEquals("2-School letter (1).jpg", Shares.fileName(2, "School letter (1).jpg"))
        assertEquals("3-shared", Shares.fileName(3, null))
        assertEquals("4-shared", Shares.fileName(4, "///"))
    }

    @Test
    fun theShownNameDropsTheNumber() {
        assertEquals("trip-form.pdf", Shares.shownName("12-trip-form.pdf"))
    }
}
