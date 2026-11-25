package rtspviewer.onvif

import java.io.ByteArrayOutputStream
import java.io.IOException
import java.io.InputStream
import java.net.HttpURLConnection
import java.net.Inet6Address
import java.net.InetAddress
import java.net.Proxy
import java.net.URI
import java.net.URISyntaxException
import java.util.concurrent.ScheduledThreadPoolExecutor
import java.util.concurrent.TimeUnit
import rtspviewer.core.domain.redactCredentials

internal class HttpReply(val status: Int, val wwwAuthenticate: String?, val body: String)

/**
 * POST SOAP por `HttpURLConnection` (sem TLS, sem proxy, sem redirecionamento).
 *
 * A conexão vai para [address], o IP já resolvido e conferido do host da
 * câmera; de [url] só valem porta e caminho. O prazo total da consulta
 * ([deadlineNanos]) vale também durante a leitura: um vigia desconecta no
 * prazo, além do `readTimeout` com o tempo restante.
 */
internal object Http {
    /** Vigia do prazo; tarefas canceladas saem da fila na hora (não seguram a conexão até o prazo). */
    private val watchdog: ScheduledThreadPoolExecutor = ScheduledThreadPoolExecutor(1) { r ->
        Thread(r, "onvif-prazo").apply { isDaemon = true }
    }.apply { removeOnCancelPolicy = true }

    fun post(
        url: String,
        address: InetAddress,
        envelope: String,
        authorization: String?,
        deadlineNanos: Long,
        connectTimeoutMs: Long,
    ): HttpReply {
        val remainingMs = TimeUnit.NANOSECONDS.toMillis(deadlineNanos - System.nanoTime())
        if (remainingMs <= 0) throw OnvifException.Unreachable("tempo esgotado")
        val target = connectUrl(url, address) ?: throw OnvifException.Protocol("endereço de serviço inválido")
        val conn = try {
            URI(target).toURL().openConnection(Proxy.NO_PROXY) as HttpURLConnection
        } catch (e: IOException) {
            throw OnvifException.Unreachable(describe(e))
        } catch (_: URISyntaxException) {
            throw OnvifException.Protocol("endereço de serviço inválido")
        } catch (_: IllegalArgumentException) {
            throw OnvifException.Protocol("endereço de serviço inválido")
        }
        val timer = watchdog.schedule({ conn.disconnect() }, remainingMs, TimeUnit.MILLISECONDS)
        try {
            conn.connectTimeout = minOf(connectTimeoutMs, remainingMs).toInt().coerceAtLeast(1)
            conn.readTimeout = remainingMs.toInt().coerceAtLeast(1)
            conn.instanceFollowRedirects = false
            conn.useCaches = false
            conn.requestMethod = "POST"
            conn.doOutput = true
            conn.setRequestProperty("Content-Type", "application/soap+xml; charset=utf-8")
            if (authorization != null) conn.setRequestProperty("Authorization", authorization)
            val bytes = envelope.toByteArray(Charsets.UTF_8)
            conn.outputStream.use { it.write(bytes) }
            val status = conn.responseCode
            if (status < 0) throw OnvifException.NotOnvif()
            val wwwAuthenticate = conn.headerFields.entries
                .filter { it.key != null && it.key.equals("WWW-Authenticate", ignoreCase = true) }
                .flatMap { it.value }
                .firstOrNull { it.trimStart().startsWith("digest", ignoreCase = true) }
            if (conn.contentLengthLong > Soap.MAX_RESPONSE_BYTES) throw tooLarge()
            val stream = if (status >= 400) conn.errorStream else conn.inputStream
            val body = stream?.use { readLimited(it, deadlineNanos) } ?: ByteArray(0)
            return HttpReply(status, wwwAuthenticate, String(body, Charsets.UTF_8))
        } catch (e: OnvifException) {
            throw e
        } catch (e: IOException) {
            if (System.nanoTime() - deadlineNanos >= 0) throw OnvifException.Unreachable("tempo esgotado")
            throw OnvifException.Unreachable(describe(e))
        } finally {
            timer.cancel(false)
            conn.disconnect()
        }
    }

    private fun tooLarge() = OnvifException.Protocol("resposta grande demais")

    /** Lê até [Soap.MAX_RESPONSE_BYTES]; um byte a mais recusa a resposta. */
    private fun readLimited(input: InputStream, deadlineNanos: Long): ByteArray {
        val out = ByteArrayOutputStream()
        val buf = ByteArray(8192)
        while (true) {
            val n = input.read(buf)
            if (n < 0) break
            out.write(buf, 0, n)
            if (out.size() > Soap.MAX_RESPONSE_BYTES) throw tooLarge()
            if (System.nanoTime() - deadlineNanos >= 0) throw OnvifException.Unreachable("tempo esgotado")
        }
        return out.toByteArray()
    }

    private fun describe(e: IOException): String = redactCredentials(e.message ?: e.javaClass.simpleName)

    /**
     * `http://<IP conferido>[:porta]<caminho>` a partir de `http://<host>[:porta]<caminho>`.
     * `null` se a URL não for `http://` legível.
     */
    fun connectUrl(url: String, address: InetAddress): String? {
        val rest = url.removePrefix("http://").takeIf { url.startsWith("http://") } ?: return null
        val end = rest.indexOfFirst { it == '/' || it == '?' || it == '#' }.let { if (it < 0) rest.length else it }
        val authority = rest.substring(0, end)
        val path = rest.substring(end).ifEmpty { "/" }
        if (authority.contains('@')) return null
        val port = if (authority.startsWith('[')) {
            authority.substringAfter(']', "").removePrefix(":").ifEmpty { null }
        } else {
            authority.substringAfter(':', "").ifEmpty { null }
        }
        val host = if (address is Inet6Address) "[${address.hostAddress}]" else address.hostAddress
        return "http://$host" + (port?.let { ":$it" } ?: "") + path
    }
}
