package rtspviewer.onvif

import java.security.MessageDigest
import java.security.SecureRandom
import java.util.Base64
import rtspviewer.core.domain.Password

/**
 * Autenticação ONVIF (`desktop/crates/onvif/src/auth.rs`, design D2 da
 * `descoberta-onvif`): WS-UsernameToken PasswordDigest com o relógio da câmera
 * e HTTP Digest (MD5) quando a câmera exige. A senha nunca é enviada em texto
 * puro.
 */

/** Credenciais de uma consulta. O `toString` não mostra a senha (vem de [Password]). */
data class Credentials(val username: String, val password: Password)

/** Diferença entre o relógio da câmera e o local, em segundos. */
@JvmInline
value class ClockOffset(val seconds: Long) {
    fun cameraTime(localNow: Long): Long = localNow + seconds

    companion object {
        val ZERO = ClockOffset(0)

        fun between(cameraNow: Long, localNow: Long) = ClockOffset(cameraNow - localNow)
    }
}

object Auth {
    private const val WSSE_NS =
        "http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-wssecurity-secext-1.0.xsd"
    private const val WSU_NS =
        "http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-wssecurity-utility-1.0.xsd"
    private const val DIGEST_TYPE =
        "http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-username-token-profile-1.0#PasswordDigest"
    private const val BASE64_TYPE =
        "http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-soap-message-security-1.0#Base64Binary"

    private val random = SecureRandom()

    /** Cabeçalho WS-Security com UsernameToken PasswordDigest. */
    fun wsSecurityHeader(creds: Credentials, nonce: ByteArray, createdUtc: Long): String {
        val created = formatUtc(createdUtc)
        val digest = passwordDigest(nonce, created, creds.password.expose())
        return """<wsse:Security s:mustUnderstand="1" xmlns:wsse="$WSSE_NS" xmlns:wsu="$WSU_NS">""" +
            "<wsse:UsernameToken><wsse:Username>${Xml.escape(creds.username)}</wsse:Username>" +
            """<wsse:Password Type="$DIGEST_TYPE">$digest</wsse:Password>""" +
            """<wsse:Nonce EncodingType="$BASE64_TYPE">${base64(nonce)}</wsse:Nonce>""" +
            "<wsu:Created>$created</wsu:Created></wsse:UsernameToken></wsse:Security>"
    }

    /** `Base64(SHA1(nonce + created + senha))`. */
    fun passwordDigest(nonce: ByteArray, created: String, password: String): String {
        val h = MessageDigest.getInstance("SHA-1")
        h.update(nonce)
        h.update(created.toByteArray(Charsets.UTF_8))
        h.update(password.toByteArray(Charsets.UTF_8))
        return base64(h.digest())
    }

    /** Bytes aleatórios para nonce (WS-Security e cnonce do HTTP Digest). */
    fun randomBytes(n: Int): ByteArray = ByteArray(n).also { random.nextBytes(it) }

    fun md5Hex(s: String): String = hex(MessageDigest.getInstance("MD5").digest(s.toByteArray(Charsets.UTF_8)))

    internal fun hex(bytes: ByteArray): String = buildString(bytes.size * 2) {
        for (b in bytes) {
            val v = b.toInt() and 0xFF
            append(HEX[v shr 4]).append(HEX[v and 0x0F])
        }
    }

    private const val HEX = "0123456789abcdef"

    internal fun base64(bytes: ByteArray): String = Base64.getEncoder().encodeToString(bytes)

    /** `AAAA-MM-DDTHH:MM:SSZ`. */
    fun formatUtc(t: Long): String {
        val days = Math.floorDiv(t, 86_400L)
        val rem = Math.floorMod(t, 86_400L)
        val (y, m, d) = civilFromDays(days)
        return "%04d-%02d-%02dT%02d:%02d:%02dZ".format(
            java.util.Locale.ROOT, y, m, d, rem / 3600, (rem % 3600) / 60, rem % 60,
        )
    }

    /** Data e hora UTC → segundos Unix; `null` se os campos forem inválidos. */
    fun utcToUnix(y: Long, m: Int, d: Int, hh: Int, mm: Int, ss: Int): Long? {
        if (m !in 1..12 || d !in 1..31 || hh !in 0..23 || mm !in 0..59 || ss !in 0..60) return null
        return daysFromCivil(y, m, d) * 86_400L + hh * 3600L + mm * 60L + ss
    }

    // Algoritmos de Howard Hinnant (calendário gregoriano proléptico).
    private fun civilFromDays(z0: Long): Triple<Long, Int, Int> {
        val z = z0 + 719_468
        val era = Math.floorDiv(z, 146_097L)
        val doe = Math.floorMod(z, 146_097L)
        val yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365
        val doy = doe - (365 * yoe + yoe / 4 - yoe / 100)
        val mp = (5 * doy + 2) / 153
        val d = (doy - (153 * mp + 2) / 5 + 1).toInt()
        val m = (if (mp < 10) mp + 3 else mp - 9).toInt()
        return Triple(yoe + era * 400 + if (m <= 2) 1 else 0, m, d)
    }

    private fun daysFromCivil(y0: Long, m: Int, d: Int): Long {
        val y = if (m <= 2) y0 - 1 else y0
        val era = Math.floorDiv(y, 400L)
        val yoe = Math.floorMod(y, 400L)
        val mp = (if (m > 2) m - 3 else m + 9).toLong()
        val doy = (153 * mp + 2) / 5 + d - 1
        val doe = yoe * 365 + yoe / 4 - yoe / 100 + doy
        return era * 146_097 + doe - 719_468
    }
}

/** Desafio `WWW-Authenticate: Digest ...` de uma resposta 401. */
data class DigestChallenge(
    val realm: String,
    val nonce: String,
    val opaque: String?,
    val qopAuth: Boolean,
) {
    class ChallengeException(message: String) : Exception(message)

    /** Valor do cabeçalho `Authorization` para um pedido em `uri` (caminho). */
    fun authorization(creds: Credentials, method: String, uri: String, cnonce: String): String {
        val ha1 = Auth.md5Hex("${creds.username}:$realm:${creds.password.expose()}")
        val ha2 = Auth.md5Hex("$method:$uri")
        val nc = "00000001"
        val response = if (qopAuth) {
            Auth.md5Hex("$ha1:$nonce:$nc:$cnonce:auth:$ha2")
        } else {
            Auth.md5Hex("$ha1:$nonce:$ha2")
        }
        val h = StringBuilder(
            "Digest username=\"${quoteSafe(creds.username)}\", realm=\"${quoteSafe(realm)}\", " +
                "nonce=\"${quoteSafe(nonce)}\", uri=\"${quoteSafe(uri)}\", response=\"$response\", algorithm=MD5",
        )
        if (qopAuth) h.append(", qop=auth, nc=$nc, cnonce=\"$cnonce\"")
        opaque?.let { h.append(", opaque=\"${quoteSafe(it)}\"") }
        return h.toString()
    }

    override fun toString(): String = "DigestChallenge(realm=$realm, qopAuth=$qopAuth)"

    companion object {
        const val NOT_DIGEST = "a câmera não pediu HTTP Digest"
        const val INCOMPLETE = "desafio HTTP Digest incompleto"

        /** @throws ChallengeException */
        fun parse(header: String): DigestChallenge {
            val h = header.trim()
            if (h.length < 6 || !h.substring(0, 6).equals("digest", ignoreCase = true)) {
                throw ChallengeException(NOT_DIGEST)
            }
            val params = parseParams(h.substring(6))
            fun get(k: String) = params.firstOrNull { it.first.equals(k, ignoreCase = true) }?.second
            get("algorithm")?.let {
                if (!it.equals("MD5", ignoreCase = true)) {
                    throw ChallengeException("a câmera pede HTTP Digest com algoritmo não suportado: $it")
                }
            }
            val qopAuth = get("qop")?.split(',')?.any { it.trim().equals("auth", ignoreCase = true) } ?: false
            return DigestChallenge(
                realm = get("realm") ?: throw ChallengeException(INCOMPLETE),
                nonce = get("nonce") ?: throw ChallengeException(INCOMPLETE),
                opaque = get("opaque"),
                qopAuth = qopAuth,
            )
        }

        private fun quoteSafe(s: String): String = s.replace("\"", "").replace("\\", "")

        /** `k1="v1", k2=v2` → pares, respeitando vírgulas dentro de aspas. */
        private fun parseParams(s: String): List<Pair<String, String>> {
            val out = mutableListOf<Pair<String, String>>()
            var i = 0
            while (true) {
                while (i < s.length && (s[i].isWhitespace() || s[i] == ',')) i++
                val keyStart = i
                while (i < s.length && s[i] != '=') i++
                val key = s.substring(keyStart, i).trim()
                if (i < s.length) i++ // consome o '='
                if (key.isEmpty()) break
                val value: String
                if (i < s.length && s[i] == '"') {
                    i++
                    val v = StringBuilder()
                    while (i < s.length) {
                        val c = s[i++]
                        when (c) {
                            '\\' -> if (i < s.length) v.append(s[i++])
                            '"' -> break
                            else -> v.append(c)
                        }
                    }
                    value = v.toString()
                } else {
                    val start = i
                    while (i < s.length && s[i] != ',') i++
                    value = s.substring(start, i).trim()
                    if (i < s.length) i++ // consome a ','
                }
                out.add(key to value)
            }
            return out
        }
    }
}
