package app.nocturne.system

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class ClipTest {
    @Test
    fun onlyNocturnesOwnCopyIsCleared() {
        assertTrue(Clip.stillOurs(Clip.LABEL, readable = true))
        assertFalse(Clip.stillOurs("from another app", readable = true))
        assertFalse(Clip.stillOurs(null, readable = true))
    }

    @Test
    fun aClipboardTheAppCannotReadIsLeftAlone() {
        assertFalse(Clip.stillOurs(Clip.LABEL, readable = false))
        assertFalse(Clip.stillOurs(null, readable = false))
    }
}
