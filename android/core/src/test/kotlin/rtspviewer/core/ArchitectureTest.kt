package rtspviewer.core

import java.io.File
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue

/**
 * Limites do `core` (spec `android-distribution`, "Separação do núcleo no
 * Android"): sem SDK do Android, rede, arquivos, serialização nem o módulo
 * `onvif`. O compilador já impede o SDK do Android (módulo JVM); este teste
 * cobre o resto, olhando as fontes e as dependências declaradas.
 */
class ArchitectureTest {
    private val moduleDir = File(".").absoluteFile
    private val sources = File(moduleDir, "src/main/kotlin").walkTopDown().filter { it.extension == "kt" }.toList()

    private val forbidden = listOf(
        "android.",
        "androidx.",
        "java.net.",
        "javax.net.",
        "java.io.File",
        "java.nio.file.",
        "java.nio.channels.",
        "kotlinx.serialization",
        "rtspviewer.onvif",
        "rtspviewer.app",
    )

    /** `import java.io.*`, `import java.nio.*`, `import java.nio.file.*`, `import java.net.*`... */
    private val wildcardImport = Regex("""^\s*import\s+java\.(io|nio|net)(\.\w+)*\.\*""")

    private fun violations(name: String, text: String): List<String> =
        codeOnly(text).lines().flatMapIndexed { i, code ->
            val hits = forbidden.filter { code.contains(it) } +
                listOfNotNull(wildcardImport.find(code)?.value?.trim())
            hits.map { "$name:${i + 1}: $it" }
        }

    @Test
    fun fontesNaoUsamPacotesProibidos() {
        assertTrue(sources.isNotEmpty(), "fontes do core não encontradas em $moduleDir")
        val found = sources.flatMap { violations(it.name, it.readText()) }
        assertEquals(emptyList(), found)
    }

    @Test
    fun verificacaoIgnoraTextoEComentariosMasNaoCodigo() {
        val ok = """
            val a = "rtsp://h/s android.x" // java.io.File
            /* java.net.Socket
               androidx.y */
            val b = ""${'"'}java.nio.file.Path""${'"'}
        """.trimIndent()
        assertEquals(emptyList(), violations("ok.kt", ok))

        val bad = """
            import java.io.*
            import java.nio.*
            val u = "rtsp://h/s"; val f = java.io.File("x") // comentário
        """.trimIndent()
        assertEquals(
            listOf("bad.kt:1: import java.io.*", "bad.kt:2: import java.nio.*", "bad.kt:3: java.io.File"),
            violations("bad.kt", bad),
        )
    }

    @Test
    fun semDependenciasDeProducao() {
        val build = File(moduleDir, "build.gradle.kts").readText()
        val deps = Regex("""^\s*(implementation|api|compileOnly|runtimeOnly)\(""", RegexOption.MULTILINE).findAll(build).toList()
        assertEquals(emptyList(), deps.map { it.value.trim() }, "o core só depende da biblioteca padrão do Kotlin")
    }
}

/**
 * Troca comentários e literais (texto, texto bruto e caractere) por espaços,
 * mantendo as quebras de linha. Versão simples: não acompanha literais dentro
 * de templates `${...}`, o que basta para as fontes do `core`.
 */
internal fun codeOnly(src: String): String {
    val out = StringBuilder(src.length)
    var i = 0
    fun blank(c: Char) = out.append(if (c == '\n') '\n' else ' ')
    while (i < src.length) {
        when {
            src.startsWith("//", i) -> {
                while (i < src.length && src[i] != '\n') blank(src[i++])
            }
            src.startsWith("/*", i) -> {
                var depth = 0
                while (i < src.length) {
                    if (src.startsWith("/*", i)) {
                        depth++
                        blank(src[i++]); blank(src[i++])
                    } else if (src.startsWith("*/", i)) {
                        depth--
                        blank(src[i++]); blank(src[i++])
                        if (depth == 0) break
                    } else {
                        blank(src[i++])
                    }
                }
            }
            src.startsWith("\"\"\"", i) -> {
                repeat(3) { blank(src[i++]) }
                while (i < src.length && !src.startsWith("\"\"\"", i)) blank(src[i++])
                while (i < src.length && src[i] == '"') blank(src[i++])
            }
            src[i] == '"' || src[i] == '\'' -> {
                val quote = src[i]
                blank(src[i++])
                while (i < src.length && src[i] != quote && src[i] != '\n') {
                    if (src[i] == '\\' && i + 1 < src.length) blank(src[i++])
                    blank(src[i++])
                }
                if (i < src.length && src[i] == quote) blank(src[i++])
            }
            else -> out.append(src[i++])
        }
    }
    return out.toString()
}
