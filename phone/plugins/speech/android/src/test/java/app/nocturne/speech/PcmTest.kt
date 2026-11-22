package app.nocturne.speech

import kotlin.math.PI
import kotlin.math.abs
import kotlin.math.sin
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class PcmTest {
    private fun steady(n: Int, value: Short) = ShortArray(n) { value }

    private fun all(r: Resampler, pieces: List<ShortArray>): ShortArray {
        var out = ShortArray(0)
        for (p in pieces) out += r.push(p)
        return out + r.finish()
    }

    @Test
    fun stereoBecomesTheMeanOfEachFrame() {
        val stereo = shortArrayOf(100, 300, -200, 200, Short.MAX_VALUE, Short.MAX_VALUE)
        assertArrayEquals(shortArrayOf(200, 0, Short.MAX_VALUE), Pcm.downmix(stereo, 2))
    }

    @Test
    fun oneChannelIsLeftAsItIs() {
        val mono = shortArrayOf(1, 2, 3)
        assertArrayEquals(mono, Pcm.downmix(mono, 1))
    }

    @Test
    fun aHalfFrameAtTheEndIsDropped() {
        assertArrayEquals(shortArrayOf(2), Pcm.downmix(shortArrayOf(1, 3, 5), 2))
    }

    @Test
    fun floatSoundIsClippedTo16Bits() {
        assertArrayEquals(
            shortArrayOf(0, Short.MAX_VALUE, (-Short.MAX_VALUE).toShort(), Short.MAX_VALUE),
            Pcm.fromFloat(floatArrayOf(0f, 1f, -1f, 2.5f)),
        )
    }

    @Test
    fun samplesAreWrittenLittleEndian() {
        assertArrayEquals(
            byteArrayOf(0x34, 0x12, 0xff.toByte(), 0xff.toByte()),
            Pcm.bytes(shortArrayOf(0x1234, -1)),
        )
    }

    @Test
    fun fortyEightKilohertzBecomesAThirdAsMany() {
        val out = all(Resampler(48_000), listOf(steady(48_000, 1000)))
        assertEquals(16_000, out.size)
        assertTrue(out.all { it == 1000.toShort() })
    }

    @Test
    fun eightKilohertzBecomesTwiceAsMany() {
        val out = all(Resampler(8_000), listOf(steady(8_000, -500)))
        assertTrue("got ${out.size}", abs(out.size - 16_000) <= 2)
        assertTrue(out.all { it == (-500).toShort() })
    }

    @Test
    fun anUnevenRateKeepsItsLength() {
        val out = all(Resampler(44_100), listOf(steady(44_100 * 3, 7)))
        assertTrue("got ${out.size}", abs(out.size - 48_000) <= 2)
    }

    @Test
    fun piecesGiveWhatTheWholeGives() {
        val sound = ShortArray(9_600) { (sin(it * 2 * PI * 440 / 48_000) * 8000).toInt().toShort() }
        val whole = all(Resampler(48_000), listOf(sound))
        val pieces = all(
            Resampler(48_000),
            listOf(sound.copyOfRange(0, 1_001), sound.copyOfRange(1_001, 5_000), sound.copyOfRange(5_000, 9_600)),
        )
        assertArrayEquals(whole, pieces)
        val up = all(Resampler(11_025), listOf(sound))
        val upPieces = all(Resampler(11_025), listOf(sound.copyOfRange(0, 333), sound.copyOfRange(333, 9_600)))
        assertArrayEquals(up, upPieces)
    }

    @Test
    fun goingDownKeepsOutWhatWouldFoldBack() {
        // A tone at 24 kHz, too high for 16 kHz sound, averages away instead of turning to noise.
        val high = ShortArray(48_000) { if (it % 2 == 0) 10_000 else -10_000 }
        val out = all(Resampler(48_000), listOf(high))
        assertTrue(out.all { abs(it.toInt()) <= 10_000 / 3 + 1 })
    }

    @Test
    fun theSameRatePassesThrough() {
        val sound = shortArrayOf(1, 2, 3)
        val r = Resampler(16_000)
        assertArrayEquals(sound, r.push(sound))
        assertEquals(0, r.finish().size)
    }
}
