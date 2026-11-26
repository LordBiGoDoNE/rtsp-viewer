package rtspviewer.core.domain

enum class AddressError(val message: String) {
    Empty("Informe o endereço da câmera."),
    Invalid("Use um IP, um nome de host ou uma URL rtsp:// completa."),
    InvalidPort("Porta inválida: use um número entre 1 e 65535."),
}

class InvalidAddressException(val error: AddressError) : IllegalArgumentException(error.message)

/**
 * Endereço de uma câmera sem esquema: IP, nome de host ou `host:porta` (IPv6
 * entre colchetes). É o que o usuário digita para o app consultar a câmera
 * pelo ONVIF.
 */
class DeviceAddress private constructor(val host: String, val port: Int?) {
    /** Host como aparece numa URL (IPv6 entre colchetes). */
    fun hostForUrl(): String = if (host.contains(':')) "[$host]" else host

    override fun toString(): String = if (port != null) "${hostForUrl()}:$port" else hostForUrl()

    override fun equals(other: Any?): Boolean = other is DeviceAddress && other.host == host && other.port == port

    override fun hashCode(): Int = toString().hashCode()

    companion object {
        fun parse(input: String): DeviceAddress {
            val t = input.trim()
            if (t.isEmpty()) fail(AddressError.Empty)
            if (t.contains("://") || t.any { it in "/?#@" } || t.any { it.isWhitespace() }) fail(AddressError.Invalid)
            val host: String
            val portText: String?
            if (t.startsWith('[')) {
                val rest = t.substring(1)
                val close = rest.indexOf(']')
                if (close < 0) fail(AddressError.Invalid)
                val h = rest.substring(0, close)
                val after = rest.substring(close + 1)
                if (!(h.contains(':') && h.all { isAsciiHex(it) || it == ':' || it == '.' })) fail(AddressError.Invalid)
                portText = when {
                    after.isEmpty() -> null
                    after.startsWith(':') -> after.substring(1)
                    else -> fail(AddressError.InvalidPort)
                }
                host = asciiLowercase(h)
            } else {
                val colon = t.indexOf(':')
                val h = if (colon >= 0) t.substring(0, colon) else t
                portText = if (colon >= 0) t.substring(colon + 1) else null
                val valid = h.isNotEmpty() &&
                    h.all { isAsciiAlnum(it) || it in "-._" } &&
                    !h.startsWith('.') && !h.startsWith('-') &&
                    !h.endsWith('.') && !h.endsWith('-')
                if (!valid) fail(AddressError.Invalid)
                host = asciiLowercase(h)
            }
            val port = portText?.let { parsePort(it) ?: fail(AddressError.InvalidPort) }
            return DeviceAddress(host, port)
        }

        private fun fail(error: AddressError): Nothing = throw InvalidAddressException(error)
    }
}

/**
 * Host da câmera apontado por uma URL de serviço ONVIF (`http://h:p/...`).
 * `null` para URLs com credenciais ou fora de `http://`.
 */
fun hostOfServiceUrl(url: String): String? {
    if (!url.startsWith("http://")) return null
    val rest = url.removePrefix("http://")
    val authority = rest.split('/', '?', '#').first()
    if (authority.contains('@')) return null
    val host = if (authority.startsWith('[')) {
        authority.substring(1).substringBefore(']')
    } else {
        authority.substringBefore(':')
    }
    return host.ifEmpty { null }?.let(::asciiLowercase)
}
