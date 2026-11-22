package app.nocturne.speech

import android.annotation.SuppressLint
import android.app.Activity
import android.content.Intent
import android.os.Bundle
import android.os.ParcelFileDescriptor
import android.os.SystemClock
import android.speech.RecognitionSupport
import android.speech.RecognitionSupportCallback
import android.speech.RecognizerIntent
import android.speech.SpeechRecognizer
import android.util.Log
import java.io.File
import java.io.FileInputStream
import java.util.concurrent.Executors

/**
 * Debug only. `am start -n is.nocturne.phone/app.nocturne.speech.SpeechProbeActivity` with extras
 * runs the reader on a file in the app's own files directory and logs under PROBE.
 *
 * Extras: file (name in filesDir; absent means no audio source, the microphone), ondevice
 * (default true), lang (default en-GB), rate, enc, ch (default the PcmFormat), pipe (default true:
 * feed through a pipe, else hand over the file itself), check (only report support), timeout
 * (seconds, default 25).
 *
 * With engine=true it runs the plugin's own reader (the check, then the read, or the check alone
 * with no file) and logs every step with a time: runs (how many asks in a row, default 1), tamper
 * ("nosource" takes the audio source out of what the reader is asked, which makes it listen to the
 * microphone, to see the check refuse it).
 */
@SuppressLint("NewApi", "InlinedApi")
class SpeechProbeActivity : Activity() {
    private val started = SystemClock.elapsedRealtime()
    private var recognizer: SpeechRecognizer? = null
    private var rms = 0

    private fun log(msg: String) {
        Log.i(PROBE, "+${SystemClock.elapsedRealtime() - started}ms $msg")
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val x = intent
        val onDevice = x.getBooleanExtra("ondevice", true)
        val lang = x.getStringExtra("lang") ?: "en-GB"
        val name = x.getStringExtra("file")?.takeIf { '/' !in it && it != ".." }
        if (x.hasExtra("file") && name == null) return finish()
        val timeout = x.getIntExtra("timeout", 25)
        if (x.getBooleanExtra("engine", false)) return runEngine(lang, name, x.getIntExtra("runs", 1), x.getStringExtra("tamper"), timeout)
        log("start ondevice=$onDevice lang=$lang file=$name sdk=${android.os.Build.VERSION.SDK_INT}")
        log("isRecognitionAvailable=${SpeechRecognizer.isRecognitionAvailable(this)} isOnDeviceRecognitionAvailable=${SpeechRecognizer.isOnDeviceRecognitionAvailable(this)}")

        val r = if (onDevice) SpeechRecognizer.createOnDeviceSpeechRecognizer(this) else SpeechRecognizer.createSpeechRecognizer(this)
        recognizer = r

        val rIntent = Intent(RecognizerIntent.ACTION_RECOGNIZE_SPEECH).apply {
            putExtra(RecognizerIntent.EXTRA_LANGUAGE_MODEL, RecognizerIntent.LANGUAGE_MODEL_FREE_FORM)
            putExtra(RecognizerIntent.EXTRA_LANGUAGE, lang)
            putExtra(RecognizerIntent.EXTRA_PARTIAL_RESULTS, x.getBooleanExtra("partials", true))
            putExtra(RecognizerIntent.EXTRA_MAX_RESULTS, 3)
            if (x.getBooleanExtra("offline", false)) putExtra(RecognizerIntent.EXTRA_PREFER_OFFLINE, true)
        }

        if (x.getBooleanExtra("check", false)) {
            r.checkRecognitionSupport(rIntent, Executors.newSingleThreadExecutor(), object : RecognitionSupportCallback {
                override fun onSupportResult(s: RecognitionSupport) {
                    log("support installed=${s.installedOnDeviceLanguages} pending=${s.pendingOnDeviceLanguages} supported=${s.supportedOnDeviceLanguages} online=${s.onlineLanguages}")
                    finishSoon()
                }
                override fun onError(error: Int) {
                    log("support error=$error")
                    finishSoon()
                }
            })
            return
        }

        if (name != null) {
            val file = File(filesDir, name)
            log("file ${file.name} bytes=${file.length()}")
            val pfd: ParcelFileDescriptor = if (x.getBooleanExtra("pipe", true)) {
                val pipe = ParcelFileDescriptor.createPipe()
                Thread {
                    ParcelFileDescriptor.AutoCloseOutputStream(pipe[1]).use { out ->
                        val pace = x.getIntExtra("pace", 0)
                        FileInputStream(file).use { input ->
                            val buf = ByteArray(3200)
                            while (true) {
                                val n = input.read(buf)
                                if (n < 0) break
                                out.write(buf, 0, n)
                                if (pace > 0) Thread.sleep(pace.toLong())
                            }
                        }
                    }
                    log("pipe fed")
                }.start()
                pipe[0]
            } else {
                ParcelFileDescriptor.open(file, ParcelFileDescriptor.MODE_READ_ONLY)
            }
            rIntent.putExtra(RecognizerIntent.EXTRA_AUDIO_SOURCE, pfd)
            rIntent.putExtra(RecognizerIntent.EXTRA_AUDIO_SOURCE_ENCODING, x.getIntExtra("enc", PcmFormat.ENCODING_PCM_16BIT))
            rIntent.putExtra(RecognizerIntent.EXTRA_AUDIO_SOURCE_SAMPLING_RATE, x.getIntExtra("rate", PcmFormat.SAMPLE_RATE))
            rIntent.putExtra(RecognizerIntent.EXTRA_AUDIO_SOURCE_CHANNEL_COUNT, x.getIntExtra("ch", PcmFormat.CHANNELS))
        } else {
            log("no audio source: the microphone is the baseline")
        }

        r.setRecognitionListener(object : android.speech.RecognitionListener {
            override fun onReadyForSpeech(params: Bundle?) = log("onReadyForSpeech")
            override fun onBeginningOfSpeech() = log("onBeginningOfSpeech")
            override fun onRmsChanged(rmsdB: Float) { rms++ }
            override fun onBufferReceived(buffer: ByteArray?) = log("onBufferReceived ${buffer?.size}")
            override fun onEndOfSpeech() = log("onEndOfSpeech rmsCallbacks=$rms")
            override fun onError(error: Int) {
                log("onError code=$error outcome=${SpeechErrors.outcome(error)}")
                finishSoon()
            }
            override fun onResults(results: Bundle?) {
                val t = results?.getStringArrayList(SpeechRecognizer.RESULTS_RECOGNITION)
                log("onResults $t keys=${results?.keySet()} confidence=${results?.getFloatArray(SpeechRecognizer.CONFIDENCE_SCORES)?.toList()}")
                finishSoon()
            }
            override fun onPartialResults(partialResults: Bundle?) {
                log("onPartialResults ${partialResults?.getStringArrayList(SpeechRecognizer.RESULTS_RECOGNITION)}")
            }
            override fun onEvent(eventType: Int, params: Bundle?) = log("onEvent $eventType")
        })
        r.startListening(rIntent)
        log("startListening called")
        window.decorView.postDelayed({
            log("timeout after ${timeout}s, rmsCallbacks=$rms")
            finishSoon()
        }, timeout * 1000L)
    }

    private fun runEngine(lang: String, name: String?, runs: Int, tamper: String?, timeout: Int) {
        val reader = SpeechReader(this)
        reader.trace = { log("  $it") }
        if (tamper == "nosource") {
            reader.tamper = { it.removeExtra(RecognizerIntent.EXTRA_AUDIO_SOURCE) }
        }
        val file = name?.let { File(filesDir, it) }
        log("engine lang=$lang file=$name runs=$runs tamper=$tamper")
        var left = runs
        for (n in 1..runs) {
            reader.submit(lang, file) { outcome, text ->
                log("ENGINE run=$n outcome=$outcome text=$text")
                if (--left == 0) finishSoon()
            }
        }
        window.decorView.postDelayed({ log("engine timeout after ${timeout}s"); finishSoon() }, timeout * 1000L)
    }

    private fun finishSoon() {
        window.decorView.post {
            recognizer?.destroy()
            recognizer = null
            log("done")
            finish()
        }
    }

    companion object {
        const val PROBE = "NocturneSpeechProbe"
    }
}
