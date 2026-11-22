package app.nocturne.speech

import kotlin.math.max
import kotlin.math.roundToInt

/** Turning decoded sound into the reader's: one channel, then 16 kHz. No phone needed. */
object Pcm {
    /** Interleaved samples of `channels` channels, as one channel: the mean of each frame. */
    fun downmix(samples: ShortArray, channels: Int): ShortArray {
        if (channels <= 1) return samples
        val frames = samples.size / channels
        return ShortArray(frames) { f ->
            var sum = 0
            for (c in 0 until channels) sum += samples[f * channels + c]
            (sum / channels).toShort()
        }
    }

    /** Float samples (-1 to 1) as 16-bit ones, clipped. */
    fun fromFloat(samples: FloatArray): ShortArray = ShortArray(samples.size) { i ->
        (samples[i].coerceIn(-1f, 1f) * Short.MAX_VALUE).roundToInt().toShort()
    }

    /** 16-bit samples as little-endian bytes, the order the reader's file takes. */
    fun bytes(samples: ShortArray): ByteArray {
        val out = ByteArray(samples.size * 2)
        for (i in samples.indices) {
            val s = samples[i].toInt()
            out[2 * i] = (s and 0xff).toByte()
            out[2 * i + 1] = ((s shr 8) and 0xff).toByte()
        }
        return out
    }
}

/**
 * One channel of sound from `from` Hz to `to` Hz, fed in pieces as a decoder gives them. Going
 * down, each new sample is the mean of the old ones it spans, which keeps out the high sounds
 * that would fold back as noise; going up, it lies on the line between its two neighbours.
 */
class Resampler(private val from: Int, private val to: Int = PcmFormat.SAMPLE_RATE) {
    private var carry = ShortArray(0)

    /**
     * Where the next new sample starts, counted from the start of [carry] in steps of `1 / to`
     * of an old sample, so pieces of any size give the same sound as the whole.
     */
    private var next = 0L

    init {
        require(from > 0 && to > 0)
    }

    fun push(input: ShortArray): ShortArray {
        if (from == to) return input
        val buf = carry + input
        val out = ShortArray(((buf.size.toLong() * to - next) / from).toInt() + 1)
        var n = 0
        if (from > to) {
            while (true) {
                val a = (next / to).toInt()
                val b = max(a + 1, ((next + from) / to).toInt())
                if (b > buf.size) break
                var sum = 0L
                for (i in a until b) sum += buf[i]
                out[n++] = (sum / (b - a)).toShort()
                next += from
            }
        } else {
            while ((next / to).toInt() + 1 < buf.size) {
                val i = (next / to).toInt()
                val f = (next % to).toDouble() / to
                out[n++] = (buf[i] * (1 - f) + buf[i + 1] * f).roundToInt().toShort()
                next += from
            }
        }
        val used = (next / to).toInt().coerceAtMost(buf.size)
        carry = buf.copyOfRange(used, buf.size)
        next -= used.toLong() * to
        return out.copyOf(n)
    }

    /** What is left at the end of the sound. */
    fun finish(): ShortArray {
        if (from == to || carry.isEmpty()) return ShortArray(0)
        val rest = if (from > to) {
            shortArrayOf((carry.sumOf { it.toLong() } / carry.size).toShort())
        } else {
            push(shortArrayOf(carry.last()))
        }
        carry = ShortArray(0)
        next = 0L
        return rest
    }
}
