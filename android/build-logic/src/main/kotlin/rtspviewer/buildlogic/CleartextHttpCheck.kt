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
    // Só clientes HTTP. RTSP do Media3 (`RtspMediaSource`, `androidx.media3.*`
    // em geral) continua permitido; as fontes HTTP do Media3 são recusadas.
    private val FORBIDDEN = listOf(
        Regex("""\b\w*URLConnection\b"""),
        Regex("""\bopenConnection\s*\("""),
        Regex("""\bopenStream\s*\("""),
        Regex("""\bjava\.net\.URL\b"""),
        Regex("""\btoURL\s*\("""),
        Regex("""\bokhttp3\b"""),
        Regex("""\bio\.ktor\b"""),
        Regex("""\bjava\.net\.http\b"""),
        Regex("""\bretrofit2\b"""),
        Regex("""\bcoil3?\.\w"""),
        Regex("""\bcom\.bumptech\.glide\b"""),
        Regex("""\bcom\.android\.volley\b"""),
        Regex("""\b(Default|OkHttp|Cronet)?HttpDataSource\b"""),
        Regex("""\b(OkHttp|Cronet)DataSource\b"""),
        Regex("""\bandroidx\.media3\.datasource\.(okhttp|cronet)\b"""),
        Regex("""\bWebView\b"""),
        Regex("""\bDownloadManager\b"""),
    )

    fun violations(fileName: String, source: String): List<String> {
        val out = mutableListOf<String>()
        val original = source.lines()
        KotlinSource.codeLines(source).forEachIndexed { i, code ->
            if (FORBIDDEN.any { it.containsMatchIn(code) }) {
                out += "$fileName:${i + 1}: ${original[i].trim()}"
            }
        }
        return out
    }
}
