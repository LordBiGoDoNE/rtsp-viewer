package rtspviewer.buildlogic

/**
 * Verificação de uso de rede HTTP fora do módulo `onvif` (ADR 0006).
 *
 * A `network_security_config` libera texto puro para qualquer host, porque o
 * Android não aceita faixas de IP. O único cliente HTTP do app é o do módulo
 * `onvif`, chamado pelo adaptador do app que só aceita endereços da rede local.
 * Esta verificação impede que `app` ou `core` abram conexões por outro caminho
 * (que herdaria o texto puro liberado). Ela não prova o guarda de endereço do
 * adaptador ONVIF, que tem testes próprios (tarefa 3.5).
 * Devolve as linhas que violam a regra, no formato `arquivo:linha: trecho`.
 */
object CleartextHttpCheck {
    private val FORBIDDEN = listOf(
        Regex("""\bHttpURLConnection\b"""),
        Regex("""\bopenConnection\s*\("""),
        Regex("""\bopenStream\s*\("""),
        Regex("""\bokhttp3\b"""),
        Regex("""\bio\.ktor\b"""),
        Regex("""\bjava\.net\.http\b"""),
        Regex("""\bWebView\b"""),
        Regex("""\bDownloadManager\b"""),
    )

    fun violations(fileName: String, source: String): List<String> {
        val out = mutableListOf<String>()
        source.lines().forEachIndexed { i, line ->
            val trimmed = line.trim()
            if (trimmed.startsWith("*") || trimmed.startsWith("/*")) return@forEachIndexed
            val code = line.substringBefore("//")
            if (FORBIDDEN.any { it.containsMatchIn(code) }) {
                out += "$fileName:${i + 1}: $trimmed"
            }
        }
        return out
    }
}
