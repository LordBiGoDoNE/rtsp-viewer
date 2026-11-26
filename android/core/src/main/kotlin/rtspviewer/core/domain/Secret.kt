package rtspviewer.core.domain

/**
 * Senha de câmera. O `toString` é mascarado, para que a senha não vaze em logs
 * ou mensagens por acidente.
 */
class Password(private val value: String) {
    /** Acesso explícito ao valor, só para o cofre ou a URL de conexão. */
    fun expose(): String = value

    fun isEmpty(): Boolean = value.isEmpty()

    override fun toString(): String = "Password(***)"

    override fun equals(other: Any?): Boolean = other is Password && other.value == value

    override fun hashCode(): Int = value.hashCode()
}

/**
 * URL de conexão com credenciais embutidas. Existe só em memória, no momento
 * de abrir o stream. O `toString` não mostra as credenciais.
 */
class ConnectionUrl internal constructor(private val value: String) {
    fun expose(): String = value

    override fun toString(): String = "ConnectionUrl(${redactCredentials(value)})"

    override fun equals(other: Any?): Boolean = other is ConnectionUrl && other.value == value

    override fun hashCode(): Int = value.hashCode()
}

/**
 * Remove credenciais de URLs `rtsp://`/`rtsps://` presentes num texto qualquer
 * (mensagens do player, erros), trocando `usuario:senha@` por `***@`.
 *
 * Tudo entre o esquema e o **último** `@` da palavra (até o próximo espaço) é
 * mascarado, e não só a autoridade aparente: uma senha com `/`, `?` ou `#` sem
 * codificação deixaria parte dela antes do corte da autoridade. Na dúvida,
 * mascarar a mais é o lado seguro.
 */
fun redactCredentials(text: String): String {
    val out = StringBuilder(text.length)
    var rest = text
    while (true) {
        val pos = findScheme(rest) ?: break
        out.append(rest, 0, pos)
        val fromScheme = rest.substring(pos)
        val sep = fromScheme.indexOf("://").let { if (it < 0) 0 else it + 3 }
        out.append(fromScheme, 0, sep)
        val afterScheme = fromScheme.substring(sep)
        val wordEnd = afterScheme.indexOfFirst { it.isWhitespace() }.let { if (it < 0) afterScheme.length else it }
        val at = afterScheme.substring(0, wordEnd).lastIndexOf('@')
        rest = if (at >= 0) {
            out.append("***")
            afterScheme.substring(at)
        } else {
            afterScheme
        }
    }
    out.append(rest)
    return out.toString()
}

private fun findScheme(text: String): Int? {
    // Minúsculas só em ASCII, para os índices continuarem alinhados ao texto.
    val lower = asciiLowercase(text)
    return listOf(lower.indexOf("rtsp://"), lower.indexOf("rtsps://")).filter { it >= 0 }.minOrNull()
}

internal fun asciiLowercase(s: String): String =
    buildString(s.length) { for (c in s) append(if (c in 'A'..'Z') c + 32 else c) }

internal fun isAsciiAlnum(c: Char): Boolean = c in 'a'..'z' || c in 'A'..'Z' || c in '0'..'9'

internal fun isAsciiHex(c: Char): Boolean = c in '0'..'9' || c in 'a'..'f' || c in 'A'..'F'

internal fun isAsciiDigit(c: Char): Boolean = c in '0'..'9'

/** Porta 1..65535 só com dígitos ASCII; `null` se inválida. */
internal fun parsePort(p: String): Int? {
    if (p.isEmpty() || !p.all(::isAsciiDigit)) return null
    val n = p.trimStart('0').ifEmpty { "0" }.let { if (it.length > 5) return null else it.toInt() }
    return n.takeIf { it in 1..65535 }
}
