package app.nocturne.system

/** Where an intent opened the app: a shortcut on its icon, or another app's share menu. */
object Shares {
    /** The shortcuts on the app's icon, by the address their intents carry. */
    val ROUTES = setOf("keep", "record", "checkin")

    /** `nocturne://keep` and the like: the shortcut it names, or null. */
    fun route(scheme: String?, host: String?): String? =
        if (scheme == "nocturne" && host in ROUTES) host else null

    /**
     * A shared file's name as it is kept in the app's own space: no folders, nothing hidden,
     * and a number in front so two files of the same name stay apart.
     */
    fun fileName(index: Int, name: String?): String {
        val base = (name ?: "").substringAfterLast('/').substringAfterLast('\\')
            .filter { it.isLetterOrDigit() || it in " ._-()" }
            .trim()
            .trimStart('.')
            .take(80)
        return "$index-${base.ifEmpty { "shared" }}"
    }

    /** The name shown for a kept file: its own name without the number in front. */
    fun shownName(kept: String): String = kept.substringAfter('-')
}
