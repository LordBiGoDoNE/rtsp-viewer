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

    @Test
    fun fontesNaoUsamPacotesProibidos() {
        assertTrue(sources.isNotEmpty(), "fontes do core não encontradas em $moduleDir")
        val violations = sources.flatMap { f ->
            f.readLines().mapIndexedNotNull { i, line ->
                val code = line.substringBefore("//")
                forbidden.firstOrNull { code.contains(it) }?.let { "${f.name}:${i + 1}: $it" }
            }
        }
        assertEquals(emptyList(), violations)
    }

    @Test
    fun semDependenciasDeProducao() {
        val build = File(moduleDir, "build.gradle.kts").readText()
        val deps = Regex("""^\s*(implementation|api|compileOnly|runtimeOnly)\(""", RegexOption.MULTILINE).findAll(build).toList()
        assertEquals(emptyList(), deps.map { it.value.trim() }, "o core só depende da biblioteca padrão do Kotlin")
    }
}
