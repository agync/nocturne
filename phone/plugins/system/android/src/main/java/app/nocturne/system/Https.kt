package app.nocturne.system

import java.io.IOException
import java.net.HttpURLConnection
import java.net.URL

/** A web service's answer: its status, its `Location` header if it gave one, and its body. */
class HttpReply(val code: Int, val body: ByteArray, val location: String? = null)

/** One request at a time to a web service. A network that is not there is an [IOException]. */
interface Http {
    @Throws(IOException::class)
    fun send(method: String, url: String, headers: Map<String, String>, body: ByteArray?): HttpReply
}

/** HTTPS through the phone's own stack, with timeouts so a dead network does not hold a night. */
class UrlHttp : Http {
    companion object {
        private const val CONNECT_MS = 20_000
        private const val READ_MS = 120_000
    }

    override fun send(
        method: String,
        url: String,
        headers: Map<String, String>,
        body: ByteArray?,
    ): HttpReply {
        val target = URL(url)
        if (target.protocol != "https") throw IOException("not https")
        val connection = target.openConnection() as HttpURLConnection
        try {
            connection.requestMethod = method
            connection.connectTimeout = CONNECT_MS
            connection.readTimeout = READ_MS
            connection.instanceFollowRedirects = false
            connection.useCaches = false
            for ((k, v) in headers) connection.setRequestProperty(k, v)
            if (body != null) {
                connection.doOutput = true
                connection.setFixedLengthStreamingMode(body.size.toLong())
                connection.outputStream.use { it.write(body) }
            }
            val code = connection.responseCode
            val stream = if (code >= 400) connection.errorStream else connection.inputStream
            val bytes = stream?.use { it.readBytes() } ?: ByteArray(0)
            return HttpReply(code, bytes, connection.getHeaderField("Location"))
        } finally {
            connection.disconnect()
        }
    }
}
