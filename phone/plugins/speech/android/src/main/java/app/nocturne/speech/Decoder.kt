package app.nocturne.speech

import android.media.AudioFormat
import android.media.MediaCodec
import android.media.MediaExtractor
import android.media.MediaFormat
import java.io.File
import java.nio.ByteOrder

/**
 * A recording as the phone keeps it (webm with Opus from the web view's recorder; m4a, aac, mp3
 * and the like from another app) turned into the raw sound the reader takes, with the phone's own
 * decoders. Blocks: never on the main thread.
 */
internal object Decoder {
    private const val TIMEOUT_US = 10_000L

    /** Waits in a row for a decoder that gives nothing back, before giving up on it. */
    private const val MAX_IDLE = 500

    /** Writes `src` to `dest` as 16-bit, mono, 16 kHz; answers how long it plays, or null. */
    fun toPcm(src: File, dest: File): Long? {
        val extractor = MediaExtractor()
        var codec: MediaCodec? = null
        try {
            extractor.setDataSource(src.path)
            val track = (0 until extractor.trackCount).firstOrNull {
                extractor.getTrackFormat(it).getString(MediaFormat.KEY_MIME)
                    ?.startsWith("audio/") == true
            } ?: return null
            val format = extractor.getTrackFormat(track)
            extractor.selectTrack(track)
            val decoder = MediaCodec.createDecoderByType(format.getString(MediaFormat.KEY_MIME)!!)
            codec = decoder
            decoder.configure(format, null, null, 0)
            decoder.start()
            var written = 0L
            dest.outputStream().buffered().use { out ->
                written = drain(extractor, decoder, format) { out.write(Pcm.bytes(it)) }
            }
            if (written == 0L) {
                dest.delete()
                return null
            }
            return PcmFormat.durationMs(written)
        } catch (_: Exception) {
            dest.delete()
            return null
        } finally {
            try {
                codec?.stop()
            } catch (_: Exception) {
            }
            codec?.release()
            extractor.release()
        }
    }

    private class Shape(format: MediaFormat) {
        val rate = format.getInteger(MediaFormat.KEY_SAMPLE_RATE)
        val channels = format.getInteger(MediaFormat.KEY_CHANNEL_COUNT)
        val float = format.containsKey(MediaFormat.KEY_PCM_ENCODING) &&
            format.getInteger(MediaFormat.KEY_PCM_ENCODING) == AudioFormat.ENCODING_PCM_FLOAT
        val resampler = Resampler(rate)
    }

    /** Feeds the decoder and hands each piece of finished sound to `write`; answers its bytes. */
    private fun drain(
        extractor: MediaExtractor,
        codec: MediaCodec,
        input: MediaFormat,
        write: (ShortArray) -> Unit,
    ): Long {
        val info = MediaCodec.BufferInfo()
        var shape = Shape(input)
        var fed = false
        var idle = 0
        var written = 0L
        val emit = { samples: ShortArray ->
            if (samples.isNotEmpty()) {
                write(samples)
                written += samples.size * 2L
            }
        }
        while (true) {
            if (!fed) {
                val index = codec.dequeueInputBuffer(TIMEOUT_US)
                if (index >= 0) {
                    val buffer = codec.getInputBuffer(index)!!
                    val size = extractor.readSampleData(buffer, 0)
                    if (size < 0) {
                        codec.queueInputBuffer(index, 0, 0, 0, MediaCodec.BUFFER_FLAG_END_OF_STREAM)
                        fed = true
                    } else {
                        codec.queueInputBuffer(index, 0, size, extractor.sampleTime, 0)
                        extractor.advance()
                    }
                }
            }
            val index = codec.dequeueOutputBuffer(info, TIMEOUT_US)
            idle = if (index == MediaCodec.INFO_TRY_AGAIN_LATER) idle + 1 else 0
            check(idle < MAX_IDLE) { "the decoder stopped" }
            when {
                index == MediaCodec.INFO_OUTPUT_FORMAT_CHANGED -> {
                    emit(shape.resampler.finish())
                    shape = Shape(codec.outputFormat)
                }
                index >= 0 -> {
                    val buffer = codec.getOutputBuffer(index)!!
                    buffer.position(info.offset)
                    buffer.limit(info.offset + info.size)
                    val ordered = buffer.order(ByteOrder.nativeOrder())
                    val samples = if (shape.float) {
                        val floats = FloatArray(info.size / 4)
                        ordered.asFloatBuffer().get(floats)
                        Pcm.fromFloat(floats)
                    } else {
                        val shorts = ShortArray(info.size / 2)
                        ordered.asShortBuffer().get(shorts)
                        shorts
                    }
                    codec.releaseOutputBuffer(index, false)
                    emit(shape.resampler.push(Pcm.downmix(samples, shape.channels)))
                    if (info.flags and MediaCodec.BUFFER_FLAG_END_OF_STREAM != 0) {
                        emit(shape.resampler.finish())
                        return written
                    }
                }
            }
        }
    }
}
