package rtspviewer.core.domain

import java.io.ByteArrayOutputStream
import java.nio.ByteBuffer
import java.nio.charset.CharacterCodingException
import java.nio.charset.CodingErrorAction

/** Motivo da recusa de uma URL RTSP; a mensagem vai para o campo do formulário. */
enum class UrlError(val message: String) {
    Empty("Informe o endereço da câmera."),
    InvalidScheme("O endereço deve começar com rtsp:// ou rtsps://."),
    InvalidHost("Endereço sem host válido."),
    InvalidPort("Porta inválida: use um número entre 1 e 65535."),
    InvalidCharacters("O endereço contém espaços ou caracteres inválidos."),
    InvalidCredentials("Usuário ou senha na URL com codificação inválida."),
    UnencodedCredentials(
        "Usuário ou senha na URL com /, ? ou # sem codificação. Informe a senha no campo Senha " +
            "ou codifique esses caracteres (%2F, %3F, %23, %40).",
    ),
}

class InvalidUrlException(val error: UrlError) : IllegalArgumentException(error.message)

/**
 * Resultado da leitura de uma URL digitada: o endereço limpo e as credenciais
 * que vieram embutidas nela, se houver.
 */
data class ParsedUrl(val url: RtspUrl, val username: String?, val password: Password?)

/** Endereço RTSP validado, sempre **sem** credenciais (mesmas regras do desktop). */
class RtspUrl private constructor(
    private val secure: Boolean,
    val host: String,
    val port: Int?,
    /** Caminho e consulta, com a barra inicial; vazio quando não há caminho. */
    private val path: String,
) {
    /**
     * Monta a URL de conexão com as credenciais codificadas. O resultado só
     * deve existir em memória, ao abrir o stream. Senha sem usuário é ignorada.
     */
    fun withCredentials(username: String?, password: Password?): ConnectionUrl {
        val s = StringBuilder(scheme())
        if (username != null) {
            s.append(percentEncode(username))
            if (password != null) s.append(':').append(percentEncode(password.expose()))
            s.append('@')
        }
        appendHostPortPath(s)
        return ConnectionUrl(s.toString())
    }

    private fun scheme() = if (secure) "rtsps://" else "rtsp://"

    private fun appendHostPortPath(s: StringBuilder) {
        if (host.contains(':')) s.append('[').append(host).append(']') else s.append(host)
        if (port != null) s.append(':').append(port)
        s.append(path)
    }

    override fun toString(): String = StringBuilder(scheme()).also(::appendHostPortPath).toString()

    override fun equals(other: Any?): Boolean =
        other is RtspUrl && other.secure == secure && other.host == host && other.port == port && other.path == path

    override fun hashCode(): Int = toString().hashCode()

    companion object {
        /** Lê a URL digitada; lança [InvalidUrlException] com o motivo. */
        fun parse(input: String): ParsedUrl {
            val trimmed = input.trim()
            if (trimmed.isEmpty()) fail(UrlError.Empty)
            val sepIdx = trimmed.indexOf("://")
            if (sepIdx < 0) fail(UrlError.InvalidScheme)
            val secure = when (asciiLowercase(trimmed.substring(0, sepIdx))) {
                "rtsp" -> false
                "rtsps" -> true
                else -> fail(UrlError.InvalidScheme)
            }
            val rest = trimmed.substring(sepIdx + 3)
            if (rest.any { it.isWhitespace() || it.isISOControl() }) fail(UrlError.InvalidCharacters)

            val authorityEnd = rest.indexOfFirst { it == '/' || it == '?' || it == '#' }
                .let { if (it < 0) rest.length else it }
            val authority = rest.substring(0, authorityEnd)
            val pathPart = rest.substring(authorityEnd)
            // `@` depois do fim da autoridade quase sempre é senha com `/`, `?` ou
            // `#` sem codificação: aceitar faria parte da senha virar host, porta
            // ou caminho, que são gravados no arquivo de configuração.
            if (pathPart.contains('@')) fail(UrlError.UnencodedCredentials)
            val path = pathPart.substringBefore('#')

            val at = authority.lastIndexOf('@')
            val userinfo = if (at >= 0) authority.substring(0, at) else null
            val hostport = if (at >= 0) authority.substring(at + 1) else authority

            var username: String? = null
            var password: Password? = null
            if (userinfo != null) {
                val colon = userinfo.indexOf(':')
                val user = if (colon >= 0) userinfo.substring(0, colon) else userinfo
                val pass = if (colon >= 0) userinfo.substring(colon + 1) else null
                username = percentDecode(user).ifEmpty { null }
                password = pass?.let(::percentDecode)?.ifEmpty { null }?.let(::Password)
            }

            val (host, port) = splitHostPort(hostport)
            return ParsedUrl(RtspUrl(secure, host, port, path), username, password)
        }

        private const val HEX = "0123456789ABCDEF"

        private fun fail(error: UrlError): Nothing = throw InvalidUrlException(error)

        private fun splitHostPort(hostport: String): Pair<String, Int?> {
            val host: String
            val portText: String?
            if (hostport.startsWith('[')) {
                val rest = hostport.substring(1)
                val close = rest.indexOf(']')
                if (close < 0) fail(UrlError.InvalidHost)
                val h = rest.substring(0, close)
                val after = rest.substring(close + 1)
                // Endereço IPv6 (hex, ':' e '.') com zona opcional depois de '%'.
                val pct = h.indexOf('%')
                val addr = if (pct >= 0) h.substring(0, pct) else h
                val zone = if (pct >= 0) h.substring(pct + 1) else ""
                val addrOk = addr.contains(':') && addr.all { isAsciiHex(it) || it == ':' || it == '.' }
                val zoneOk = zone.all { isAsciiAlnum(it) || it in "-_." }
                if (!addrOk || !zoneOk) fail(UrlError.InvalidHost)
                portText = when {
                    after.isEmpty() -> null
                    after.startsWith(':') -> after.substring(1)
                    else -> fail(UrlError.InvalidPort)
                }
                host = h
            } else {
                val colon = hostport.indexOf(':')
                val h = if (colon >= 0) hostport.substring(0, colon) else hostport
                portText = if (colon >= 0) hostport.substring(colon + 1) else null
                val valid = h.isNotEmpty() &&
                    h.all { isAsciiAlnum(it) || it in "-._" } &&
                    !h.startsWith('.') && !h.startsWith('-') &&
                    !h.endsWith('-')
                if (!valid) fail(UrlError.InvalidHost)
                host = asciiLowercase(h)
            }
            val port = portText?.let { parsePort(it) ?: fail(UrlError.InvalidPort) }
            return host to port
        }

        private fun percentDecode(s: String): String {
            val bytes = s.toByteArray(Charsets.UTF_8)
            val out = ByteArrayOutputStream(bytes.size)
            var i = 0
            while (i < bytes.size) {
                val b = bytes[i]
                if (b == '%'.code.toByte()) {
                    if (i + 2 >= bytes.size) fail(UrlError.InvalidCredentials)
                    val h1 = bytes[i + 1].toInt().toChar()
                    val h2 = bytes[i + 2].toInt().toChar()
                    if (!isAsciiHex(h1) || !isAsciiHex(h2)) fail(UrlError.InvalidCredentials)
                    out.write("$h1$h2".toInt(16))
                    i += 3
                } else {
                    out.write(b.toInt())
                    i += 1
                }
            }
            val decoder = Charsets.UTF_8.newDecoder()
                .onMalformedInput(CodingErrorAction.REPORT)
                .onUnmappableCharacter(CodingErrorAction.REPORT)
            return try {
                decoder.decode(ByteBuffer.wrap(out.toByteArray())).toString()
            } catch (_: CharacterCodingException) {
                fail(UrlError.InvalidCredentials)
            }
        }

        private fun percentEncode(s: String): String {
            val out = StringBuilder(s.length)
            for (b in s.toByteArray(Charsets.UTF_8)) {
                val c = (b.toInt() and 0xFF).toChar()
                if (c.code < 128 && (isAsciiAlnum(c) || c in "-._~")) {
                    out.append(c)
                } else {
                    val v = b.toInt() and 0xFF
                    out.append('%').append(HEX[v shr 4]).append(HEX[v and 0x0F])
                }
            }
            return out.toString()
        }
    }
}
