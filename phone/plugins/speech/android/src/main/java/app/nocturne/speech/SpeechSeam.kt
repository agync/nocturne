package app.nocturne.speech

/** The raw sound the phone's reader takes from a file: 16-bit, mono, 16 kHz, no header. */
object PcmFormat {
    const val ENCODING_PCM_16BIT = 2
    const val SAMPLE_RATE = 16_000
    const val CHANNELS = 1
    const val BYTES_PER_SAMPLE = 2

    const val BYTES_PER_SECOND = SAMPLE_RATE * CHANNELS * BYTES_PER_SAMPLE

    fun durationMs(bytes: Long): Long = bytes * 1000 / BYTES_PER_SECOND

    /** Whole samples only: a half sample at the end is sound the reader cannot take. */
    fun isWhole(bytes: Long): Boolean = bytes % (CHANNELS * BYTES_PER_SAMPLE) == 0L
}

/** What one question to the phone's reader came to, as the app's Rust side takes it. */
enum class Outcome(val wire: String) {
    Ready("ready"),
    Text("text"),
    LanguageUnavailable("languageUnavailable"),
    NotSupported("notSupported"),
    NoPermission("noPermission"),
    NothingHeard("nothingHeard"),

    /** The reader did not stop at the end of the file, so it may be listening to the microphone. */
    ListensToMic("listensToMic"),
    Failed("failed"),
}

object SpeechErrors {
    const val NETWORK_TIMEOUT = 1
    const val NETWORK = 2
    const val AUDIO = 3
    const val SERVER = 4
    const val CLIENT = 5
    const val SPEECH_TIMEOUT = 6
    const val NO_MATCH = 7
    const val RECOGNIZER_BUSY = 8
    const val INSUFFICIENT_PERMISSIONS = 9
    const val LANGUAGE_NOT_SUPPORTED = 12
    const val LANGUAGE_UNAVAILABLE = 13
    const val CANNOT_CHECK_SUPPORT = 14

    fun outcome(code: Int): Outcome = when (code) {
        INSUFFICIENT_PERMISSIONS -> Outcome.NoPermission
        LANGUAGE_NOT_SUPPORTED -> Outcome.NotSupported
        LANGUAGE_UNAVAILABLE -> Outcome.LanguageUnavailable
        NO_MATCH, SPEECH_TIMEOUT -> Outcome.NothingHeard
        else -> Outcome.Failed
    }
}

/** Which of the phone's readers can read a language with the sound pack already on the phone. */
enum class Reader { OnDevice, DefaultOffline }

object Readers {
    fun hasLanguage(installed: List<String>, tag: String): Boolean =
        installed.any { it.equals(tag, ignoreCase = true) }

    /**
     * The reader built for the phone itself comes first. The default reader is used only with its
     * offline preference, and only for a language it holds a pack for: asked for a language it
     * lacks, it answers "unavailable" and does not go online.
     */
    fun choose(onDevice: List<String>?, default: List<String>?, tag: String): Reader? = when {
        onDevice != null && hasLanguage(onDevice, tag) -> Reader.OnDevice
        default != null && hasLanguage(default, tag) -> Reader.DefaultOffline
        else -> null
    }
}

/** What a question to the reader came to, with the words when there are any. */
data class Verdict(val outcome: Outcome, val text: String? = null)

/**
 * What the reader's answers mean, so that no answer from a reader that is not taking our file is
 * ever read as words. The check feeds the reader a stream with nothing in it, keeps it open for
 * [Probe.HOLD_MS], then closes it. A reader taking the stream waits for the end and answers "no
 * match" within [Probe.AFTER_CLOSE_MS] of it (185 ms measured). A reader on the microphone answers
 * early or late, or with words, and a reader that gives up on a quiet room gives up with "speech
 * timeout" or "no match" before the stream is closed.
 */
object Probe {
    const val HOLD_MS = 1_000L
    const val AFTER_CLOSE_MS = 750L

    /** Nothing at all by then means the reader is not waiting on the stream. */
    const val DEADLINE_MS = HOLD_MS + AFTER_CLOSE_MS + 500

    /** [sinceCloseMs] is null while the stream is still open. */
    fun onError(code: Int, sinceCloseMs: Long?): Outcome = when (code) {
        SpeechErrors.INSUFFICIENT_PERMISSIONS -> Outcome.NoPermission
        SpeechErrors.LANGUAGE_NOT_SUPPORTED -> Outcome.NotSupported
        SpeechErrors.LANGUAGE_UNAVAILABLE -> Outcome.LanguageUnavailable
        SpeechErrors.NO_MATCH ->
            if (sinceCloseMs != null && sinceCloseMs in 0..AFTER_CLOSE_MS) Outcome.NothingHeard else Outcome.ListensToMic
        SpeechErrors.SPEECH_TIMEOUT -> Outcome.ListensToMic
        else -> Outcome.Failed
    }

    /** Any words from nothing came from the room. */
    fun onWords(): Outcome = Outcome.ListensToMic
}

/** What the reader's answers mean while a real file is being read. */
object Reading {
    /** The first bytes go in this long after the reader starts, so an answer before them is its own. */
    const val FIRST_BYTE_DELAY_MS = 300L

    /** The deadline for a read, from the sound's length. */
    fun deadlineMs(soundMs: Long): Long = 15_000L + soundMs * 3

    fun onError(code: Int, fed: Boolean, text: String): Verdict = when (code) {
        SpeechErrors.INSUFFICIENT_PERMISSIONS -> Verdict(Outcome.NoPermission)
        SpeechErrors.LANGUAGE_NOT_SUPPORTED -> Verdict(Outcome.NotSupported)
        SpeechErrors.LANGUAGE_UNAVAILABLE -> Verdict(Outcome.LanguageUnavailable)
        SpeechErrors.NO_MATCH, SpeechErrors.SPEECH_TIMEOUT -> when {
            !fed -> Verdict(Outcome.ListensToMic)
            text.isNotEmpty() -> Verdict(Outcome.Text, text)
            else -> Verdict(Outcome.NothingHeard)
        }
        else -> Verdict(Outcome.Failed)
    }

    fun onResults(fed: Boolean, text: String): Verdict = when {
        !fed -> Verdict(Outcome.ListensToMic)
        text.isEmpty() -> Verdict(Outcome.NothingHeard)
        else -> Verdict(Outcome.Text, text)
    }
}

/**
 * Joins what the reader says into one text. The reader's final answer is usually empty: the words
 * come as partial results, each the whole of the stretch being read, revised as it goes. A result
 * carries on from the one before when its first word is the same, or a longer or shorter form of
 * it, and it has not lost more than one word, or when it holds the earlier one whole. Otherwise the earlier stretch is closed and a new one
 * begins. A final answer that holds the whole text replaces what was gathered.
 */
class Transcript {
    private val closed = mutableListOf<String>()
    private var open = ""

    fun partial(best: String) {
        val text = best.trim()
        if (text.isEmpty()) return
        if (open.isNotEmpty() && !sameStretch(open, text)) closed += open
        open = text
    }

    fun result(best: String) {
        val text = best.trim()
        if (text.isEmpty()) return
        if (closed.isNotEmpty() && text.startsWith(closed[0], ignoreCase = true)) {
            closed.clear()
            open = text
        } else {
            partial(text)
        }
    }

    fun text(): String = (closed + open).filter { it.isNotEmpty() }.joinToString(" ")

    private fun sameStretch(a: String, b: String): Boolean {
        val before = a.split(' ')
        val after = b.split(' ')
        if (a.length < b.length && b.contains(a, ignoreCase = true)) return true
        if (after.size < before.size - 1) return false
        val x = before[0].lowercase()
        val y = after[0].lowercase()
        if (x == y) return true
        val fragment = before.size == 1 || minOf(x.length, y.length) >= 3
        return fragment && (x.startsWith(y) || y.startsWith(x))
    }
}
