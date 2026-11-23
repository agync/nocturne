package app.nocturne.system

import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class SafeTest {
    @Test
    fun aSealedSecretUnpacksAsItWasPacked() {
        val iv = ByteArray(12) { it.toByte() }
        val sealed = byteArrayOf(9, 8, 7, 6)
        val (back, bytes) = Safe.unpack(Safe.pack(iv, sealed))!!
        assertArrayEquals(iv, back)
        assertArrayEquals(sealed, bytes)
    }

    @Test
    fun aFileThisDidNotWriteIsNotOpened() {
        assertNull(Safe.unpack(byteArrayOf()))
        assertNull(Safe.unpack(byteArrayOf(0, 1, 2)))
        assertNull(Safe.unpack(byteArrayOf(12, 1, 2)))
    }

    @Test
    fun onlyPlainNamesAreSecrets() {
        assertTrue(Safe.validName("mind"))
        assertTrue(Safe.validName("mind-key"))
        assertFalse(Safe.validName(""))
        assertFalse(Safe.validName("../mind"))
        assertFalse(Safe.validName("Mind"))
        assertFalse(Safe.validName("a".repeat(33)))
    }
}
