package app.nocturne.speech

import android.Manifest
import android.app.Activity
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.os.ParcelFileDescriptor
import android.os.SystemClock
import android.provider.Settings
import android.speech.RecognitionSupport
import android.speech.RecognitionSupportCallback
import android.speech.RecognizerIntent
import android.speech.SpeechRecognizer
import android.system.ErrnoException
import android.system.Os
import android.system.OsConstants
import androidx.annotation.RequiresApi
import androidx.core.content.ContextCompat
import java.io.File
import java.io.FileInputStream
import java.util.concurrent.Executor
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicLong

internal typealias Respond = (Outcome, String?) -> Unit

internal sealed interface Sound {
    /** Nothing, held open, for the check. */
    object Empty : Sound

    class Kept(val file: File) : Sound
}

/**
 * The phone's own speech reader, given a kept file of raw sound (see [PcmFormat]) instead of the
 * microphone. A reader that does not take the file listens to the room, so every read is
 * preceded by a check on a stream with nothing in it (see [Probe]), and an answer that comes
 * before the first bytes of the file is never taken as words (see [Reading]). A reader that fails
 * the check is not asked again until the app restarts or the phone's reader changes. A reader is
 * used only for a language whose pack is already on the phone, and always with the offline
 * preference. One ask runs at a time; the others wait their turn. Everything here runs on the
 * main thread, so a caller must never wait for an answer from there.
 */
internal class SpeechReader(private val activity: Activity) {
    private val main = Handler(Looper.getMainLooper())
    private val refused = mutableSetOf<String>()
    private val waiting = ArrayDeque<(() -> Unit) -> Unit>()
    private var busy = false
    private var destroyedAt = 0L

    private fun allowed(): Boolean =
        ContextCompat.checkSelfPermission(activity, Manifest.permission.RECORD_AUDIO) ==
            PackageManager.PERMISSION_GRANTED

    /** Set only by the debug probe: sees what the reader does, as it happens. */
    var trace: ((String) -> Unit)? = null

    /** Set only by the debug probe: changes what is asked of the reader, to test the guard. */
    var tamper: ((Intent) -> Unit)? = null

    /**
     * Whether a file of the language can be read now (a null [file]), or its words. [respond] is
     * called once, on the main thread. A check runs first, so on a reader that fails it the
     * microphone may be open for a second.
     */
    fun submit(language: String, file: File?, respond: Respond) {
        main.post {
            waiting.addLast { next ->
                var answered = false
                val once: Respond = { outcome, text ->
                    if (!answered) {
                        answered = true
                        respond(outcome, text)
                        next()
                    }
                }
                try {
                    begin(language, file, once)
                } catch (_: Exception) {
                    once(Outcome.Failed, null)
                }
            }
            pump()
        }
    }

    private fun pump() {
        if (busy) return
        val job = waiting.removeFirstOrNull() ?: return
        busy = true
        job {
            busy = false
            pump()
        }
    }

    private fun begin(language: String, file: File?, respond: Respond) {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU) return respond(Outcome.NotSupported, null)
        beginOnTiramisu(language, file, respond)
    }

    private fun guarded(respond: Respond, block: () -> Unit) {
        try {
            block()
        } catch (_: Exception) {
            respond(Outcome.Failed, null)
        }
    }

    @RequiresApi(Build.VERSION_CODES.TIRAMISU)
    private fun beginOnTiramisu(language: String, file: File?, respond: Respond) {
        if (!allowed()) return respond(Outcome.NoPermission, null)
        if (file != null) {
            if (!file.isFile || !PcmFormat.isWhole(file.length())) return respond(Outcome.Failed, null)
            if (file.length() == 0L) return respond(Outcome.NothingHeard, null)
        }
        if (!SpeechRecognizer.isRecognitionAvailable(activity)) return respond(Outcome.NotSupported, null)

        val onDeviceUsable = SpeechRecognizer.isOnDeviceRecognitionAvailable(activity)
        installed(onDeviceUsable, true, language, respond) { onDevice ->
            installed(true, false, language, respond) { default ->
                guarded(respond) {
                    val reader = Readers.choose(onDevice, default, language)
                        ?: return@guarded respond(Outcome.LanguageUnavailable, null)
                    val key = "$reader|${Settings.Secure.getString(activity.contentResolver, "voice_recognition_service")}|$language"
                    if (key in refused) return@guarded respond(Outcome.ListensToMic, null)
                    listen(reader, language, Sound.Empty, respond) { check, _ ->
                        when (check) {
                            Outcome.NothingHeard ->
                                if (file == null) {
                                    respond(Outcome.Ready, null)
                                } else {
                                    listen(reader, language, Sound.Kept(file), respond, respond)
                                }
                            Outcome.ListensToMic -> {
                                refused += key
                                respond(Outcome.ListensToMic, null)
                            }
                            else -> respond(check, null)
                        }
                    }
                }
            }
        }
    }

    @RequiresApi(Build.VERSION_CODES.TIRAMISU)
    private fun recognizer(reader: Reader): SpeechRecognizer =
        if (reader == Reader.OnDevice) {
            SpeechRecognizer.createOnDeviceSpeechRecognizer(activity)
        } else {
            SpeechRecognizer.createSpeechRecognizer(activity)
        }

    @RequiresApi(Build.VERSION_CODES.TIRAMISU)
    private fun intent(language: String): Intent =
        Intent(RecognizerIntent.ACTION_RECOGNIZE_SPEECH).apply {
            putExtra(RecognizerIntent.EXTRA_LANGUAGE_MODEL, RecognizerIntent.LANGUAGE_MODEL_FREE_FORM)
            putExtra(RecognizerIntent.EXTRA_LANGUAGE, language)
            putExtra(RecognizerIntent.EXTRA_PARTIAL_RESULTS, true)
            putExtra(RecognizerIntent.EXTRA_PREFER_OFFLINE, true)
        }

    /**
     * The languages a reader holds a pack for, or null when it cannot say or does not in time. The
     * default reader answers with an error and a list both, the error first, so the list gets a
     * moment to come.
     */
    @RequiresApi(Build.VERSION_CODES.TIRAMISU)
    private fun installed(
        usable: Boolean,
        onDevice: Boolean,
        language: String,
        respond: Respond,
        then: (List<String>?) -> Unit,
    ) {
        if (!usable) return then(null)
        afterGap { guarded(respond) { installedNow(onDevice, language, respond, then) } }
    }

    @RequiresApi(Build.VERSION_CODES.TIRAMISU)
    private fun installedNow(onDevice: Boolean, language: String, respond: Respond, then: (List<String>?) -> Unit) {
        val r = if (onDevice) {
            SpeechRecognizer.createOnDeviceSpeechRecognizer(activity)
        } else {
            SpeechRecognizer.createSpeechRecognizer(activity)
        }
        var delivered = false
        fun deliver(list: List<String>?) {
            if (delivered) return
            delivered = true
            main.removeCallbacksAndMessages(r)
            r.destroy()
            destroyedAt = SystemClock.uptimeMillis()
            guarded(respond) { then(list) }
        }
        main.postAtTime({ deliver(null) }, r, SystemClock.uptimeMillis() + SUPPORT_MS)
        r.checkRecognitionSupport(
            intent(language),
            Executor { main.post(it) },
            object : RecognitionSupportCallback {
                override fun onSupportResult(recognitionSupport: RecognitionSupport) {
                    deliver(recognitionSupport.installedOnDeviceLanguages)
                }

                override fun onError(error: Int) {
                    main.postAtTime({ deliver(null) }, r, SystemClock.uptimeMillis() + AFTER_ERROR_MS)
                }
            },
        )
    }

    /**
     * Asks the reader to read [sound]. With nothing, this is the check and [then] gets how the
     * reader answered it. With a file, [then] gets the words.
     */
    @RequiresApi(Build.VERSION_CODES.TIRAMISU)
    private fun listen(reader: Reader, language: String, sound: Sound, respond: Respond, then: Respond) {
        afterGap { guarded(respond) { listenNow(reader, language, sound, respond, then) } }
    }

    /**
     * The phone's reader is bound and unbound as they come and go, and one asked for while the one
     * before is still unbinding answers "disconnected" (11) at once. So a new one waits.
     */
    private fun afterGap(block: () -> Unit) {
        val wait = RECOGNISER_GAP_MS - (SystemClock.uptimeMillis() - destroyedAt)
        if (wait <= 0) block() else main.postDelayed(block, wait)
    }

    @RequiresApi(Build.VERSION_CODES.TIRAMISU)
    private fun listenNow(reader: Reader, language: String, sound: Sound, respond: Respond, then: Respond) {
        val r = recognizer(reader)
        val transcript = Transcript()
        val pipe = ParcelFileDescriptor.createPipe()
        val stop = AtomicBoolean(false)
        val fed = AtomicBoolean(false)
        val closedAt = AtomicLong(0)
        var finished = false

        fun finish(outcome: Outcome, text: String? = null) {
            if (finished) return
            finished = true
            trace?.invoke("finish ${if (sound is Sound.Empty) "check" else "read"} outcome=$outcome")
            stop.set(true)
            main.removeCallbacksAndMessages(r)
            r.destroy()
            destroyedAt = SystemClock.uptimeMillis()
            closeQuietly(pipe[0])
            closeQuietly(pipe[1])
            then(outcome, text)
        }

        fun sinceClose(): Long? = closedAt.get().takeIf { it != 0L }?.let { SystemClock.elapsedRealtime() - it }

        fun verdict(v: Verdict) = finish(v.outcome, v.text)

        trace?.invoke("listen ${if (sound is Sound.Empty) "check" else "read"} reader=$reader")
        Thread { feed(sound, pipe[1], stop, fed, closedAt) }.start()

        val i = intent(language).apply {
            putExtra(RecognizerIntent.EXTRA_AUDIO_SOURCE, pipe[0])
            putExtra(RecognizerIntent.EXTRA_AUDIO_SOURCE_ENCODING, PcmFormat.ENCODING_PCM_16BIT)
            putExtra(RecognizerIntent.EXTRA_AUDIO_SOURCE_SAMPLING_RATE, PcmFormat.SAMPLE_RATE)
            putExtra(RecognizerIntent.EXTRA_AUDIO_SOURCE_CHANNEL_COUNT, PcmFormat.CHANNELS)
        }
        val checking = sound is Sound.Empty
        r.setRecognitionListener(object : android.speech.RecognitionListener {
            override fun onReadyForSpeech(params: Bundle?) {
                trace?.invoke("onReadyForSpeech")
            }
            override fun onBeginningOfSpeech() {}
            override fun onRmsChanged(rmsdB: Float) {}
            override fun onBufferReceived(buffer: ByteArray?) {}
            override fun onEndOfSpeech() {}
            override fun onEvent(eventType: Int, params: Bundle?) {}

            override fun onPartialResults(partialResults: Bundle?) = guarded(respond) {
                val best = partialResults?.getStringArrayList(SpeechRecognizer.RESULTS_RECOGNITION)?.firstOrNull()
                    ?: return@guarded
                if (best.isBlank()) return@guarded
                trace?.invoke("partial fed=${fed.get()} sinceClose=${sinceClose()}")
                if (checking || !fed.get()) finish(Probe.onWords()) else transcript.partial(best)
            }

            override fun onResults(results: Bundle?) = guarded(respond) {
                val best = results?.getStringArrayList(SpeechRecognizer.RESULTS_RECOGNITION)?.firstOrNull()
                trace?.invoke("onResults fed=${fed.get()} sinceClose=${sinceClose()} empty=${best.isNullOrBlank()}")
                if (checking) {
                    if (!best.isNullOrBlank()) {
                        finish(Probe.onWords())
                    } else {
                        finish(Probe.onError(SpeechErrors.NO_MATCH, sinceClose()))
                    }
                    return@guarded
                }
                if (fed.get() && best != null) transcript.result(best)
                verdict(Reading.onResults(fed.get(), transcript.text()))
            }

            override fun onError(error: Int) = guarded(respond) {
                trace?.invoke("onError code=$error fed=${fed.get()} sinceClose=${sinceClose()}")
                if (checking) {
                    finish(Probe.onError(error, sinceClose()))
                } else {
                    verdict(Reading.onError(error, fed.get(), transcript.text()))
                }
            }
        })
        val deadline = if (sound is Sound.Kept) {
            Reading.deadlineMs(PcmFormat.durationMs(sound.file.length()))
        } else {
            Probe.DEADLINE_MS
        }
        main.postAtTime(
            { finish(if (checking) Outcome.ListensToMic else Outcome.Failed) },
            r,
            SystemClock.uptimeMillis() + deadline,
        )
        tamper?.invoke(i)
        r.startListening(i)
    }

    /**
     * Writes the sound into the pipe without ever blocking on it, so a reader that stops reading
     * cannot hold the thread. The pipe's write end is closed when the sound is all in, or when
     * [stop] is set.
     */
    @RequiresApi(Build.VERSION_CODES.TIRAMISU)
    private fun feed(
        sound: Sound,
        out: ParcelFileDescriptor,
        stop: AtomicBoolean,
        fed: AtomicBoolean,
        closedAt: AtomicLong,
    ) {
        try {
            val fd = out.fileDescriptor
            Os.fcntlInt(fd, OsConstants.F_SETFL, Os.fcntlInt(fd, OsConstants.F_GETFL, 0) or OsConstants.O_NONBLOCK)
            when (sound) {
                is Sound.Empty -> pause(Probe.HOLD_MS, stop)
                is Sound.Kept -> {
                    pause(Reading.FIRST_BYTE_DELAY_MS, stop)
                    val buffer = ByteArray(8_192)
                    FileInputStream(sound.file).use { input ->
                        while (!stop.get()) {
                            val n = input.read(buffer)
                            if (n < 0) break
                            if (fed.compareAndSet(false, true)) trace?.invoke("first bytes offered")
                            var at = 0
                            while (at < n && !stop.get()) {
                                try {
                                    at += Os.write(fd, buffer, at, n - at)
                                } catch (e: ErrnoException) {
                                    if (e.errno != OsConstants.EAGAIN) throw e
                                    Thread.sleep(20)
                                }
                            }
                        }
                    }
                }
            }
        } catch (_: Exception) {
        } finally {
            closeQuietly(out)
            closedAt.compareAndSet(0, SystemClock.elapsedRealtime())
            trace?.invoke("stream closed")
        }
    }

    private fun pause(ms: Long, stop: AtomicBoolean) {
        val until = SystemClock.elapsedRealtime() + ms
        while (!stop.get() && SystemClock.elapsedRealtime() < until) Thread.sleep(20)
    }

    private fun closeQuietly(fd: ParcelFileDescriptor) {
        try {
            fd.close()
        } catch (_: java.io.IOException) {
        }
    }

    private companion object {
        const val SUPPORT_MS = 2_000L
        const val AFTER_ERROR_MS = 500L
        const val RECOGNISER_GAP_MS = 400L
    }
}
