package app.nocturne.system

import java.io.IOException
import java.net.URLEncoder
import java.security.MessageDigest
import java.time.Instant
import java.util.UUID
import org.json.JSONArray
import org.json.JSONException
import org.json.JSONObject

/** The access token for the person's Drive. Each call throws [PlaceFailure] when there is none. */
interface Tokens {
    fun token(): String

    /** A new token in place of `stale`, which the account no longer takes. */
    fun renew(stale: String): String
}

/**
 * The "Nocturne backup" folder in the person's Google Drive, through the Drive REST API with the
 * scope for files the app makes (`drive.file`): it sees only that folder and what it put there.
 * A file is written as a new Drive file, then every older file of that name is deleted, so a
 * name shows only once whole and no earlier version of it stays. A new file has one revision, so
 * no older revision is left behind either.
 */
class Drive(
    private val http: Http,
    private val tokens: Tokens,
    private val sleep: (Long) -> Unit = { Thread.sleep(it) },
) {
    companion object {
        const val FOLDER = "Nocturne backup"
        const val API = "https://www.googleapis.com/drive/v3"
        const val UPLOAD = "https://www.googleapis.com/upload/drive/v3"
        const val FOLDER_TYPE = "application/vnd.google-apps.folder"
        private const val BYTES_TYPE = "application/octet-stream"
        private const val JSON_TYPE = "application/json; charset=UTF-8"
        /** Larger files go up in a resumable upload, as Google asks. */
        const val MULTIPART_MAX = 5 * 1024 * 1024
        private const val TRIES = 4
        private const val FILE_FIELDS = "id,name,size,modifiedTime,createdTime,md5Checksum"

        private val RATE = setOf("rateLimitExceeded", "userRateLimitExceeded")
        private val NO_ROOM = setOf("storageQuotaExceeded")
        private val NO_ACCESS = setOf(
            "authError",
            "insufficientPermissions",
            "insufficientFilePermissions",
            "appNotAuthorizedToFile",
            "forbidden",
            "domainPolicy",
        )

        /** The reasons Drive gave in an error body; empty when the body is not one. */
        fun reasons(body: ByteArray): Set<String> = try {
            val errors = JSONObject(String(body, Charsets.UTF_8))
                .getJSONObject("error").optJSONArray("errors") ?: JSONArray()
            (0 until errors.length()).mapNotNull { errors.optJSONObject(it)?.optString("reason") }
                .toSet()
        } catch (e: JSONException) {
            emptySet()
        }

        /** Whether the same request may go again after a pause. */
        fun retryable(code: Int, body: ByteArray): Boolean =
            code == 429 || code in 500..599 || (code == 403 && reasons(body).any { it in RATE })

        /** What a failed answer means for the backup. */
        fun outcome(code: Int, body: ByteArray): PlaceOutcome {
            val why = reasons(body)
            return when {
                code == 401 -> PlaceOutcome.NoAccess
                why.any { it in NO_ROOM } -> PlaceOutcome.NoRoom
                retryable(code, body) -> PlaceOutcome.Offline
                code == 403 && (why.isEmpty() || why.any { it in NO_ACCESS }) ->
                    PlaceOutcome.NoAccess
                else -> PlaceOutcome.Failed
            }
        }

        fun md5(bytes: ByteArray): String =
            MessageDigest.getInstance("MD5").digest(bytes).joinToString("") { "%02x".format(it) }

        private fun query(q: String) = URLEncoder.encode(q, "UTF-8")

        fun millis(time: String?): Long? = try {
            time?.let { Instant.parse(it).toEpochMilli() }
        } catch (e: java.time.format.DateTimeParseException) {
            null
        }
    }

    private class DriveFile(val id: String, val name: String, val size: Long, val modified: Long?)

    @Volatile private var folderId: String? = null
    /** The folder last used, so what was written to another folder is not listed from this one. */
    private var lastFolder: String? = null
    @Volatile private var token: String? = null

    /** Files this run wrote, by name, until a listing shows them: Drive's search lags. */
    private val written = mutableMapOf<String, DriveFile>()

    /** Another account was chosen: nothing known of the last one is used. */
    @Synchronized
    fun forget() {
        folderId = null
        token = null
        written.clear()
    }

    private fun failure(reply: HttpReply, what: String): PlaceFailure =
        PlaceFailure(outcome(reply.code, reply.body), "$what: ${reply.code}")

    /**
     * One request with the person's token. An expired token is renewed once; a busy or failing
     * service is asked again after a pause. Any other answer is the caller's to read.
     */
    private fun call(
        method: String,
        url: String,
        body: ByteArray? = null,
        type: String? = null,
        extra: Map<String, String> = emptyMap(),
    ): HttpReply {
        var current = token ?: tokens.token().also { token = it }
        var renewed = false
        var attempt = 0
        while (true) {
            val headers = HashMap(extra)
            headers["Authorization"] = "Bearer $current"
            if (type != null) headers["Content-Type"] = type
            val reply = try {
                http.send(method, url, headers, body)
            } catch (e: IOException) {
                throw PlaceFailure(PlaceOutcome.Offline, e.message ?: "offline")
            }
            when {
                reply.code == 401 && !renewed -> {
                    token = null
                    current = tokens.renew(current).also { token = it }
                    renewed = true
                }
                retryable(reply.code, reply.body) && attempt < TRIES - 1 -> {
                    sleep(1000L shl attempt)
                    attempt++
                }
                else -> return reply
            }
        }
    }

    private fun json(reply: HttpReply): JSONObject = try {
        JSONObject(String(reply.body, Charsets.UTF_8))
    } catch (e: JSONException) {
        throw PlaceFailure(PlaceOutcome.Failed, "not json")
    }

    /** The folder's id: the oldest "Nocturne backup" folder the app made, or a new one. */
    @Synchronized
    private fun folder(): String {
        folderId?.let { return it }
        val q = query("name = '$FOLDER' and mimeType = '$FOLDER_TYPE' and trashed = false")
        val found = call("GET", "$API/files?q=$q&spaces=drive&orderBy=createdTime&fields=files(id)")
        if (found.code != 200) throw failure(found, "folder")
        val existing = json(found).optJSONArray("files")?.optJSONObject(0)?.optString("id")
        val id = if (!existing.isNullOrEmpty()) {
            existing
        } else {
            val meta = JSONObject().put("name", FOLDER).put("mimeType", FOLDER_TYPE)
            val made = call(
                "POST", "$API/files?fields=id", meta.toString().toByteArray(), JSON_TYPE
            )
            if (made.code !in 200..299) throw failure(made, "new folder")
            json(made).optString("id").ifEmpty { throw PlaceFailure(PlaceOutcome.Failed, "no id") }
        }
        folderId = id
        if (lastFolder != null && lastFolder != id) written.clear()
        lastFolder = id
        return id
    }

    /** Every file in the folder, or those named `name`, newest first. */
    private fun files(name: String? = null): List<DriveFile> {
        val folder = folder()
        val named = if (name != null) " and name = '$name'" else ""
        val q = query("'$folder' in parents and trashed = false$named")
        val out = mutableListOf<DriveFile>()
        var page: String? = null
        do {
            val next = page?.let { "&pageToken=${query(it)}" } ?: ""
            val reply = call(
                "GET",
                "$API/files?q=$q&spaces=drive&orderBy=createdTime%20desc&pageSize=1000" +
                    "&fields=nextPageToken,files($FILE_FIELDS)$next",
            )
            if (reply.code == 404) {
                // The folder was deleted in the Drive: the next call makes a new one.
                forget()
                return emptyList()
            }
            if (reply.code != 200) throw failure(reply, "list")
            val body = json(reply)
            val files = body.optJSONArray("files") ?: JSONArray()
            for (i in 0 until files.length()) {
                val f = files.getJSONObject(i)
                out += DriveFile(
                    f.getString("id"),
                    f.getString("name"),
                    f.optString("size").toLongOrNull() ?: 0L,
                    millis(f.optString("modifiedTime", "").ifEmpty { null }),
                )
            }
            page = body.optString("nextPageToken", "").ifEmpty { null }
        } while (page != null)
        return out
    }

    /** The newest file of each name starting with `prefix`, with what this run wrote. */
    fun list(prefix: String): List<PlaceEntry> {
        // The person may have moved the folder to the bin since it was found: look again.
        folderId = null
        val listed = files()
        val newest = LinkedHashMap<String, DriveFile>()
        for (f in listed) newest.putIfAbsent(f.name, f)
        val ids = listed.mapTo(HashSet()) { it.id }
        synchronized(this) {
            written.values.removeAll { it.id in ids }
            for ((name, f) in written) newest[name] = f
        }
        return newest.values.filter { it.name.startsWith(prefix) }
            .map { PlaceEntry(it.name, it.size, it.modified) }
    }

    /** The newest file named `name`, or null when there is none. */
    fun get(name: String): ByteArray? {
        val known = synchronized(this) { written[name] }
        val candidates = listOfNotNull(known) + files(name).filter { it.id != known?.id }
        for (f in candidates) {
            val reply = call("GET", "$API/files/${f.id}?alt=media")
            if (reply.code == 200) return reply.body
            if (reply.code != 404) throw failure(reply, "get")
        }
        return null
    }

    /** Writes `bytes` as a new file named `name`, then deletes every older file of that name. */
    fun put(name: String, bytes: ByteArray) {
        val made = upload(name, bytes)
        val prior = synchronized(this) { written.put(name, made) }
        if (prior != null && prior.id != made.id) remove(prior.id)
        for (older in files(name)) {
            if (older.id != made.id) remove(older.id)
        }
    }

    /** Deletes every file named `name`. None is fine. */
    fun delete(name: String) {
        val known = synchronized(this) { written.remove(name) }
        known?.let { remove(it.id) }
        for (f in files(name)) if (f.id != known?.id) remove(f.id)
    }

    private fun remove(id: String) {
        val reply = call("DELETE", "$API/files/$id")
        if (reply.code !in 200..299 && reply.code != 404) throw failure(reply, "delete")
    }

    /** The room left in the Drive, in bytes, or null when it has no limit. */
    fun free(): Long? {
        val reply = call("GET", "$API/about?fields=storageQuota(limit,usage)")
        if (reply.code != 200) throw failure(reply, "room")
        val quota = json(reply).optJSONObject("storageQuota") ?: return null
        val limit = quota.optString("limit").toLongOrNull() ?: return null
        val usage = quota.optString("usage").toLongOrNull() ?: 0L
        return maxOf(0L, limit - usage)
    }

    private fun upload(name: String, bytes: ByteArray): DriveFile {
        val reply = send(name, bytes)
        val made = if (reply.code == 404) {
            // The folder went since it was found: once more into a new one.
            forget()
            send(name, bytes)
        } else {
            reply
        }
        if (made.code !in 200..299) throw failure(made, "put")
        val file = json(made)
        val id = file.optString("id").ifEmpty { throw PlaceFailure(PlaceOutcome.Failed, "no id") }
        if (file.optString("md5Checksum") != md5(bytes)) {
            remove(id)
            throw PlaceFailure(PlaceOutcome.Failed, "the file arrived damaged")
        }
        return DriveFile(id, name, bytes.size.toLong(), millis(file.optString("modifiedTime")))
    }

    private fun send(name: String, bytes: ByteArray): HttpReply {
        val meta = JSONObject()
            .put("name", name)
            .put("mimeType", BYTES_TYPE)
            .put("parents", JSONArray().put(folder()))
            .toString()
        val fields = "fields=${query("id,md5Checksum,modifiedTime")}"
        if (bytes.size <= MULTIPART_MAX) {
            val boundary = "nocturne-" + UUID.randomUUID().toString()
            val head = "--$boundary\r\nContent-Type: $JSON_TYPE\r\n\r\n$meta\r\n" +
                "--$boundary\r\nContent-Type: $BYTES_TYPE\r\n\r\n"
            val body = head.toByteArray() + bytes + "\r\n--$boundary--\r\n".toByteArray()
            return call(
                "POST",
                "$UPLOAD/files?uploadType=multipart&$fields",
                body,
                "multipart/related; boundary=$boundary",
            )
        }
        val start = call(
            "POST",
            "$UPLOAD/files?uploadType=resumable&$fields",
            meta.toByteArray(),
            JSON_TYPE,
            mapOf(
                "X-Upload-Content-Type" to BYTES_TYPE,
                "X-Upload-Content-Length" to bytes.size.toString(),
            ),
        )
        if (start.code != 200) return start
        val session = start.location?.takeIf { it.startsWith("$UPLOAD/") }
            ?: throw PlaceFailure(PlaceOutcome.Failed, "no upload session")
        return call("PUT", session, bytes, BYTES_TYPE)
    }
}
