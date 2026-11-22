package app.nocturne.system

import java.io.ByteArrayInputStream
import java.io.ByteArrayOutputStream
import java.io.IOException
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class FilesTest {
    @Test
    fun aCopyCarriesEveryByte() {
        val bytes = ByteArray(700_000) { (it % 251).toByte() }
        val out = ByteArrayOutputStream()
        assertEquals(700_000L, Files.copy(ByteArrayInputStream(bytes), out))
        assertArrayEquals(bytes, out.toByteArray())
    }

    @Test
    fun aFileFitsOnlyWithRoomToSpare() {
        assertTrue(Files.fits(100, 100 + Files.SPARE))
        assertFalse(Files.fits(101, 100 + Files.SPARE))
        assertTrue(Files.fits(-1, 0))
    }

    @Test
    fun noRoomIsToldApartFromOtherFailures() {
        assertTrue(Files.isFull(IOException("write failed: ENOSPC (No space left on device)")))
        assertTrue(Files.isFull(RuntimeException("wrapped", IOException("No space left on device"))))
        assertFalse(Files.isFull(IOException("Permission denied")))
        assertFalse(Files.isFull(null))
    }

    @Test
    fun aMissingDateReadsAsNone() {
        assertNull(Files.modified(0))
        assertNull(Files.modified(null))
        assertEquals(1_763_800_000_000L, Files.modified(1_763_800_000_000L))
    }

    @Test
    fun outcomesSpeakTheWordsTheAppReads() {
        assertEquals(
            listOf("saved", "chosen", "cancelled", "full", "failed"),
            FileOutcome.entries.map { it.wire },
        )
    }
}
