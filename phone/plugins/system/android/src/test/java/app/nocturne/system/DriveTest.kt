package app.nocturne.system

import java.io.IOException
import java.net.URLDecoder
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test

/** Google Drive as far as the backup uses it, in memory, with answers that can be made to fail. */
class FakeDrive : Http {
    class File(
        val id: String,
        val name: String,
        val parent: String?,
        var bytes: ByteArray,
        val created: Int,
        val folder: Boolean = false,
        var trashed: Boolean = false,
    )

    val files = mutableListOf<File>()
    val requests = mutableListOf<String>()
    /** Answers given in place of the real one, first to last. */
    val failNext = ArrayDeque<HttpReply>()
    var offline = false
    /** New files stay out of searches, as Drive's search catches up later. */
    var searchLags = false
    var damage = false
    var validToken = "good"
    /** Files in one page of a search. */
    var pageSize = 1000
    /** Where a resumable upload says to send the bytes, in place of its own address. */
    var sessionAt: String? = null
    private var lagging = mutableSetOf<String>()
    private var nextId = 1
    private val sessions = mutableMapOf<String, JSONObject>()

    fun named(name: String) = files.filter { it.name == name && !it.folder }

    fun folders() = files.filter { it.folder }

    fun catchUp() = lagging.clear()

    private fun reply(code: Int, json: Any = JSONObject(), location: String? = null) =
        HttpReply(code, json.toString().toByteArray(), location)

    private fun make(name: String, parent: String?, bytes: ByteArray, folder: Boolean): File {
        val f = File("id${nextId}", name, parent, bytes, nextId, folder)
        nextId++
        files += f
        if (searchLags && !folder) lagging += f.id
        return f
    }

    private fun meta(f: File): JSONObject = JSONObject()
        .put("id", f.id)
        .put("name", f.name)
        .put("size", f.bytes.size.toString())
        .put("modifiedTime", "2026-11-24T03:00:0${f.created % 10}.000Z")
        .put("md5Checksum", if (damage) "0" else Drive.md5(f.bytes))

    private fun params(url: String): Map<String, String> =
        url.substringAfter('?', "").split('&').filter { it.isNotEmpty() }.associate {
            val (k, v) = (it.split('=', limit = 2) + "").take(2)
            k to URLDecoder.decode(v, "UTF-8")
        }

    override fun send(
        method: String,
        url: String,
        headers: Map<String, String>,
        body: ByteArray?,
    ): HttpReply {
        requests += "$method ${url.substringBefore('?')}"
        if (offline) throw IOException("no network")
        failNext.removeFirstOrNull()?.let { return it }
        if (headers["Authorization"] != "Bearer $validToken") return reply(401)
        val path = url.substringBefore('?')
        val p = params(url)
        return when {
            method == "GET" && path == "${Drive.API}/files" -> search(p["q"]!!, p["pageToken"])
            method == "POST" && path == "${Drive.API}/files" -> {
                val m = JSONObject(String(body!!))
                reply(200, JSONObject().put("id", make(m.getString("name"), null, ByteArray(0), true).id))
            }
            method == "POST" && path == "${Drive.UPLOAD}/files" && p["uploadType"] == "multipart" ->
                multipart(headers["Content-Type"]!!, body!!)
            method == "POST" && path == "${Drive.UPLOAD}/files" && p["uploadType"] == "resumable" -> {
                val session = "s${sessions.size}"
                sessions[session] = JSONObject(String(body!!))
                val at = sessionAt ?: "${Drive.UPLOAD}/files?uploadType=resumable&upload_id=$session"
                reply(200, location = at)
            }
            method == "PUT" && path == "${Drive.UPLOAD}/files" -> {
                val m = sessions.remove(p["upload_id"]) ?: return reply(404)
                created(m, body!!)
            }
            method == "GET" && path.startsWith("${Drive.API}/files/") -> {
                val f = files.find { it.id == path.substringAfterLast('/') } ?: return reply(404)
                HttpReply(200, f.bytes)
            }
            method == "DELETE" && path.startsWith("${Drive.API}/files/") ->
                if (files.removeAll { it.id == path.substringAfterLast('/') }) reply(204) else reply(404)
            method == "GET" && path == "${Drive.API}/about" -> reply(
                200,
                JSONObject().put(
                    "storageQuota",
                    JSONObject().put("limit", "1000").put("usage", "400")
                )
            )
            else -> reply(400)
        }
    }

    private fun created(m: JSONObject, bytes: ByteArray): HttpReply {
        val parent = m.getJSONArray("parents").getString(0)
        if (folders().none { it.id == parent }) return reply(404)
        return reply(200, meta(make(m.getString("name"), parent, bytes, false)))
    }

    private fun multipart(type: String, body: ByteArray): HttpReply {
        val boundary = type.substringAfter("boundary=")
        val text = String(body, Charsets.ISO_8859_1)
        val parts = text.split("--$boundary")
        val json = parts[1].substringAfter("\r\n\r\n").removeSuffix("\r\n")
        val bytes = parts[2].substringAfter("\r\n\r\n").removeSuffix("\r\n")
        return created(JSONObject(json), bytes.toByteArray(Charsets.ISO_8859_1))
    }

    private fun search(q: String, token: String?): HttpReply {
        val found = if (q.contains("mimeType = '${Drive.FOLDER_TYPE}'")) {
            folders().filter { q.contains("name = '${it.name}'") && !it.trashed }
                .sortedBy { it.created }
        } else {
            val parent = Regex("'([^']+)' in parents").find(q)!!.groupValues[1]
            val name = Regex("and name = '([^']+)'").find(q)?.groupValues?.get(1)
            files.filter {
                it.parent == parent && !it.folder && it.id !in lagging &&
                    (name == null || it.name == name)
            }.sortedByDescending { it.created }
        }
        val from = token?.toInt() ?: 0
        val out = JSONArray()
        found.drop(from).take(pageSize).forEach { out.put(meta(it)) }
        val answer = JSONObject().put("files", out)
        if (from + pageSize < found.size) answer.put("nextPageToken", "${from + pageSize}")
        return reply(200, answer)
    }
}

class FakeTokens(private val drive: FakeDrive) : Tokens {
    var current = "good"
    var renewals = 0
    var refuse: PlaceOutcome? = null

    override fun token(): String {
        refuse?.let { throw PlaceFailure(it, "refused") }
        return current
    }

    override fun renew(stale: String): String {
        renewals++
        current = drive.validToken
        return current
    }
}

class DriveTest {
    private val fake = FakeDrive()
    private val tokens = FakeTokens(fake)
    private val drive = Drive(fake, tokens) {}

    private fun failure(block: () -> Unit): PlaceOutcome {
        try {
            block()
        } catch (e: PlaceFailure) {
            return e.outcome
        }
        fail("no failure")
        throw AssertionError()
    }

    @Test
    fun aFileComesBackAsItWasWrittenInOneFolder() {
        drive.put("piece-a1", byteArrayOf(1, 2, 3))
        drive.put("night-1-ab", byteArrayOf(4))
        assertArrayEquals(byteArrayOf(1, 2, 3), drive.get("piece-a1"))
        assertNull(drive.get("piece-none"))
        assertEquals(1, fake.folders().size)
        assertEquals(Drive.FOLDER, fake.folders()[0].name)
        assertTrue(fake.files.filter { !it.folder }.all { it.parent == fake.folders()[0].id })
    }

    @Test
    fun anExistingFolderIsUsedAndNotMadeAgain() {
        drive.put("key-phrase", byteArrayOf(1))
        val again = Drive(fake, tokens) {}
        assertArrayEquals(byteArrayOf(1), again.get("key-phrase"))
        again.put("key-recovery", byteArrayOf(2))
        assertEquals(1, fake.folders().size)
    }

    @Test
    fun writingAgainLeavesOnlyTheNewFile() {
        drive.put("key-phrase", byteArrayOf(1))
        drive.put("key-phrase", byteArrayOf(2))
        assertEquals(1, fake.named("key-phrase").size)
        assertArrayEquals(byteArrayOf(2), drive.get("key-phrase"))
    }

    @Test
    fun aListingHasTheNewestOfEachNameWithItsSize() {
        drive.put("night-1-ab", ByteArray(10))
        drive.put("night-2-ab", ByteArray(20))
        drive.put("piece-ff", ByteArray(5))
        val nights = drive.list("night-").sortedBy { it.name }
        assertEquals(listOf("night-1-ab", "night-2-ab"), nights.map { it.name })
        assertEquals(listOf(10L, 20L), nights.map { it.size })
        assertTrue(nights.all { it.modified != null })
        assertEquals(3, drive.list("").size)
    }

    @Test
    fun deletingRemovesEveryFileOfTheNameAndAMissingOneIsFine() {
        drive.put("piece-aa", byteArrayOf(1))
        drive.delete("piece-aa")
        drive.delete("piece-aa")
        drive.delete("piece-never")
        assertTrue(fake.named("piece-aa").isEmpty())
        assertNull(drive.get("piece-aa"))
    }

    @Test
    fun aLargeFileGoesUpInAResumableUpload() {
        val big = ByteArray(Drive.MULTIPART_MAX + 1) { (it % 251).toByte() }
        drive.put("night-3-ab", big)
        assertArrayEquals(big, drive.get("night-3-ab"))
        assertTrue(fake.requests.any { it == "PUT ${Drive.UPLOAD}/files" })
    }

    @Test
    fun whatThisRunWroteIsSeenBeforeDrivesSearchCatchesUp() {
        fake.searchLags = true
        drive.put("night-4-ab", byteArrayOf(7))
        assertArrayEquals(byteArrayOf(7), drive.get("night-4-ab"))
        assertEquals(listOf("night-4-ab"), drive.list("night-").map { it.name })
        drive.delete("night-4-ab")
        assertTrue(fake.named("night-4-ab").isEmpty())
        fake.catchUp()
        assertTrue(drive.list("night-").isEmpty())
    }

    @Test
    fun writingANameTwiceBeforeSearchCatchesUpLeavesOnlyTheSecond() {
        fake.searchLags = true
        drive.put("key-phrase", byteArrayOf(1))
        drive.put("key-phrase", byteArrayOf(2))
        assertEquals(1, fake.named("key-phrase").size)
        fake.catchUp()
        assertArrayEquals(byteArrayOf(2), drive.get("key-phrase"))
    }

    @Test
    fun aListingReadsEveryPage() {
        fake.pageSize = 2
        for (i in 1..5) drive.put("piece-$i", byteArrayOf(i.toByte()))
        assertEquals(5, drive.list("piece-").size)
    }

    @Test
    fun theTokenNeverGoesToAnUploadAddressThatIsNotGoogles() {
        fake.sessionAt = "https://elsewhere.example/upload"
        val big = ByteArray(Drive.MULTIPART_MAX + 1)
        assertEquals(PlaceOutcome.Failed, failure { drive.put("night-5-ab", big) })
        assertFalse(fake.requests.any { it.contains("elsewhere") })
    }

    @Test
    fun aFolderMovedToTheBinIsLeftThereAndANewOneMade() {
        drive.put("piece-aa", byteArrayOf(1))
        fake.folders().single().trashed = true
        assertTrue(drive.list("").isEmpty())
        drive.put("piece-bb", byteArrayOf(2))
        val live = fake.folders().filter { !it.trashed }
        assertEquals(1, live.size)
        assertEquals(live[0].id, fake.named("piece-bb").single().parent)
    }

    @Test
    fun theTokenIsAskedForOnceAndKeptUntilRefused() {
        var asked = 0
        val counting = object : Tokens {
            override fun token(): String {
                asked++
                return fake.validToken
            }

            override fun renew(stale: String) = token()
        }
        val kept = Drive(fake, counting) {}
        kept.put("piece-aa", byteArrayOf(1))
        kept.get("piece-aa")
        assertEquals(1, asked)
    }

    @Test
    fun aFullDriveIsNoRoom() {
        drive.put("piece-aa", byteArrayOf(1))
        val full = """{"error":{"code":403,"errors":[{"reason":"storageQuotaExceeded"}]}}"""
        fake.failNext += HttpReply(403, full.toByteArray())
        assertEquals(PlaceOutcome.NoRoom, failure { drive.put("piece-bb", byteArrayOf(2)) })
    }

    @Test
    fun anExpiredTokenIsRenewedOnce() {
        fake.validToken = "fresh"
        drive.put("piece-aa", byteArrayOf(1))
        assertEquals(1, tokens.renewals)
        assertEquals("fresh", tokens.current)
    }

    @Test
    fun aTokenTheAccountStillRefusesIsNoPermission() {
        fake.failNext += HttpReply(401, ByteArray(0))
        fake.failNext += HttpReply(401, ByteArray(0))
        assertEquals(PlaceOutcome.NoPermission, failure { drive.list("") })
    }

    @Test
    fun noAccountOrNoPermissionIsSaidApart() {
        tokens.refuse = PlaceOutcome.SignedOut
        assertEquals(PlaceOutcome.SignedOut, failure { drive.get("key-phrase") })
        tokens.refuse = PlaceOutcome.NoPermission
        assertEquals(PlaceOutcome.NoPermission, failure { drive.get("key-phrase") })
    }

    @Test
    fun noNetworkIsOffline() {
        fake.offline = true
        assertEquals(PlaceOutcome.Offline, failure { drive.put("piece-aa", byteArrayOf(1)) })
    }

    @Test
    fun aBusyServiceIsAskedAgainThenCountsAsOffline() {
        fake.failNext += HttpReply(503, ByteArray(0))
        drive.put("piece-aa", byteArrayOf(1))
        assertArrayEquals(byteArrayOf(1), drive.get("piece-aa"))
        repeat(4) { fake.failNext += HttpReply(503, ByteArray(0)) }
        assertEquals(PlaceOutcome.Offline, failure { drive.get("piece-aa") })
    }

    @Test
    fun aDamagedUploadIsRemovedAndFails() {
        drive.put("piece-aa", byteArrayOf(1))
        fake.damage = true
        assertEquals(PlaceOutcome.Failed, failure { drive.put("piece-bb", byteArrayOf(2)) })
        assertTrue(fake.named("piece-bb").isEmpty())
    }

    @Test
    fun aFolderDeletedInTheDriveIsMadeAgain() {
        drive.put("piece-aa", byteArrayOf(1))
        fake.files.clear()
        drive.put("piece-bb", byteArrayOf(2))
        assertEquals(1, fake.folders().size)
        assertArrayEquals(byteArrayOf(2), drive.get("piece-bb"))
    }

    @Test
    fun theRoomLeftComesFromTheAccount() {
        assertEquals(600L, drive.free())
    }

    @Test
    fun failedAnswersMeanWhatTheBackupShows() {
        fun body(reason: String) =
            """{"error":{"errors":[{"reason":"$reason"}]}}""".toByteArray()
        assertEquals(PlaceOutcome.NoPermission, Drive.outcome(401, ByteArray(0)))
        assertEquals(PlaceOutcome.NoRoom, Drive.outcome(403, body("storageQuotaExceeded")))
        assertEquals(
            PlaceOutcome.NoPermission, Drive.outcome(403, body("insufficientPermissions"))
        )
        assertEquals(
            PlaceOutcome.NoPermission, Drive.outcome(403, body("appNotAuthorizedToFile"))
        )
        assertEquals(PlaceOutcome.NoPermission, Drive.outcome(403, "not json".toByteArray()))
        assertEquals(PlaceOutcome.Offline, Drive.outcome(403, body("userRateLimitExceeded")))
        assertEquals(PlaceOutcome.Offline, Drive.outcome(429, ByteArray(0)))
        assertEquals(PlaceOutcome.Offline, Drive.outcome(502, ByteArray(0)))
        assertEquals(PlaceOutcome.Failed, Drive.outcome(400, ByteArray(0)))
        assertEquals(PlaceOutcome.Failed, Drive.outcome(403, body("somethingElse")))
        assertTrue(Drive.retryable(403, body("rateLimitExceeded")))
        assertFalse(Drive.retryable(403, body("storageQuotaExceeded")))
    }

    @Test
    fun onlyPlainNamesReachTheDrive() {
        assertTrue(PlaceNames.valid("piece-0af3"))
        assertTrue(PlaceNames.valid("a".repeat(128)))
        assertFalse(PlaceNames.valid(""))
        assertFalse(PlaceNames.valid("a".repeat(129)))
        assertFalse(PlaceNames.valid("Piece"))
        assertFalse(PlaceNames.valid("a'b"))
        assertFalse(PlaceNames.valid("../a"))
        assertTrue(PlaceNames.validPrefix(""))
        assertFalse(PlaceNames.validPrefix("night_"))
    }
}
