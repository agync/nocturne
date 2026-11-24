package app.nocturne.system

/** Why a call to the person's account did not do what was asked, as the core names it. */
enum class PlaceOutcome(val wire: String) {
    NoRoom("noRoom"),
    NoAccess("noAccess"),
    Offline("offline"),
    Failed("failed"),
}

class PlaceFailure(val outcome: PlaceOutcome, message: String) : Exception(message)

/** One file in the backup folder; `modified` in milliseconds since 1970, when the account says. */
data class PlaceEntry(val name: String, val size: Long, val modified: Long?)

object PlaceNames {
    private fun plain(c: Char) = c in 'a'..'z' || c in '0'..'9' || c == '-'

    /** A file's name in the backup folder: short plain ASCII, so it needs no quoting anywhere. */
    fun valid(name: String): Boolean = name.length in 1..128 && name.all(::plain)

    fun validPrefix(prefix: String): Boolean = prefix.length <= 128 && prefix.all(::plain)
}
