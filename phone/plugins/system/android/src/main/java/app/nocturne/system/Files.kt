package app.nocturne.system

import java.io.IOException
import java.io.InputStream
import java.io.OutputStream

/** What came of a file the person saved or chose through the phone's own sheet. */
enum class FileOutcome(val wire: String) {
    Saved("saved"),
    Chosen("chosen"),
    Cancelled("cancelled"),
    Full("full"),
    Failed("failed"),
}

/** The parts of saving and choosing files that need no phone. */
object Files {
    /** Room left over after a copy, so the phone is not filled to the last byte. */
    const val SPARE = 16L * 1024 * 1024

    /** The name a chosen file is copied to inside the folder the app names. */
    const val CHOSEN = "chosen"

    fun copy(input: InputStream, output: OutputStream): Long {
        val buffer = ByteArray(256 * 1024)
        var total = 0L
        while (true) {
            val n = input.read(buffer)
            if (n < 0) return total
            output.write(buffer, 0, n)
            total += n
        }
    }

    /** Whether a file of `size` bytes fits in `available`; an unknown size is tried. */
    fun fits(size: Long, available: Long): Boolean = size < 0 || size + SPARE <= available

    /** Whether a failed write failed because the place it went to has no room. */
    fun isFull(e: Throwable?): Boolean {
        var cause = e
        while (cause != null) {
            val message = cause.message ?: ""
            if (cause is IOException && ("ENOSPC" in message || "No space left" in message)) {
                return true
            }
            cause = cause.cause
        }
        return false
    }

    /** A provider's last-modified column: milliseconds, or null when it gives none. */
    fun modified(ms: Long?): Long? = ms?.takeIf { it > 0 }
}
