package app.nocturne.system

/**
 * Read now while the person is away: what the core sends the phone, what it gets back, and the
 * few decisions that do not need Android to make.
 */
object Reading {
    /** The first Android that asks before an app may post a notification. */
    const val ASKS_FROM_SDK = 33

    /** The person pressed Stop in the notification since the core last asked. */
    @Volatile var stopAsked = false

    /** Android's own daily time for data sync ran out since the core last asked. */
    @Volatile var timedOut = false

    /** A Nocturne screen is in front. */
    @Volatile var inFront = false

    /** The notification's service is up. */
    @Volatile var running = false

    /** What the core gets back after each call. */
    data class Answer(val stop: Boolean, val timeout: Boolean, val notes: Boolean)

    /** The calls the core makes. */
    enum class State { Reading, Ended }

    fun state(wire: String?): State? = when (wire) {
        "reading" -> State.Reading
        "ended" -> State.Ended
        else -> null
    }

    /** A stop is told once: the next answer starts clean. */
    @Synchronized
    fun answer(notes: Boolean): Answer {
        val answer = Answer(stopAsked, timedOut, notes)
        stopAsked = false
        timedOut = false
        return answer
    }

    /** After a Stop or the time limit, the reading is not shown again until the core says. */
    fun keepShowing(answer: Answer): Boolean = !answer.stop && !answer.timeout

    /** One quiet line stays after the end only for a person who is not looking at Nocturne. */
    fun leaveLine(title: String?, inFront: Boolean): Boolean = title != null && !inFront

    /** Asks for the notification permission once, and only where Android asks. */
    fun shouldAsk(sdk: Int, granted: Boolean, askedBefore: Boolean): Boolean =
        sdk >= ASKS_FROM_SDK && !granted && !askedBefore
}
