package rtspviewer.buildlogic

import org.gradle.api.DefaultTask
import org.gradle.api.GradleException
import org.gradle.api.Plugin
import org.gradle.api.Project
import org.gradle.api.file.ConfigurableFileCollection
import org.gradle.api.file.DirectoryProperty
import org.gradle.api.file.RegularFileProperty
import org.gradle.api.provider.Property
import org.gradle.api.tasks.CacheableTask
import org.gradle.api.tasks.Input
import org.gradle.api.tasks.InputFile
import org.gradle.api.tasks.InputFiles
import org.gradle.api.tasks.OutputDirectory
import org.gradle.api.tasks.OutputFile
import org.gradle.api.tasks.PathSensitive
import org.gradle.api.tasks.PathSensitivity
import org.gradle.api.tasks.SkipWhenEmpty
import org.gradle.api.tasks.TaskAction

/** Gera `Tokens.kt` a partir de `design/tokens.json`. */
@CacheableTask
abstract class GenerateDesignTokens : DefaultTask() {
    @get:InputFile
    @get:PathSensitive(PathSensitivity.NONE)
    abstract val tokensFile: RegularFileProperty

    @get:Input
    abstract val packageName: Property<String>

    @get:OutputDirectory
    abstract val outputDir: DirectoryProperty

    @TaskAction
    fun generate() {
        val source = try {
            DesignTokens.generate(tokensFile.get().asFile.readText(), packageName.get())
        } catch (e: TokenError) {
            throw GradleException(e.message ?: "token inválido", e)
        }
        val dir = outputDir.get().asFile
        dir.deleteRecursively()
        val file = dir.resolve(packageName.get().replace('.', '/')).resolve("Tokens.kt")
        file.parentFile.mkdirs()
        file.writeText(source)
    }
}

/** Falha se o código da UI usar cor literal em vez de `Tokens`. */
abstract class CheckUiColors : DefaultTask() {
    @get:InputFiles
    @get:SkipWhenEmpty
    @get:PathSensitive(PathSensitivity.RELATIVE)
    abstract val sources: ConfigurableFileCollection

    @get:OutputFile
    abstract val report: RegularFileProperty

    @TaskAction
    fun check() {
        val found = sources.files.sortedBy { it.path }.flatMap { f ->
            UiColorCheck.violations(f.name, f.readText())
        }
        report.get().asFile.apply { parentFile.mkdirs(); writeText(found.joinToString("\n")) }
        if (found.isNotEmpty()) {
            throw GradleException(
                "Cor literal no código da UI; use Tokens.Colors (ADR 0004):\n" + found.joinToString("\n"),
            )
        }
    }
}

/**
 * Falha se `app` ou `core` usarem cliente HTTP: o texto puro liberado no
 * manifesto só pode ser usado pelo módulo `onvif`, atrás do guarda de rede local.
 */
abstract class CheckCleartextHttp : DefaultTask() {
    @get:InputFiles
    @get:SkipWhenEmpty
    @get:PathSensitive(PathSensitivity.RELATIVE)
    abstract val sources: ConfigurableFileCollection

    @get:OutputFile
    abstract val report: RegularFileProperty

    @TaskAction
    fun check() {
        val found = sources.files.sortedBy { it.path }.flatMap { f ->
            CleartextHttpCheck.violations(f.name, f.readText())
        }
        report.get().asFile.apply { parentFile.mkdirs(); writeText(found.joinToString("\n")) }
        if (found.isNotEmpty()) {
            throw GradleException(
                "Cliente HTTP fora do módulo onvif; o texto puro só vale para o ONVIF na rede local (ADR 0006):\n" +
                    found.joinToString("\n"),
            )
        }
    }
}

/** Registra as tasks; o módulo `app` liga a saída às fontes de cada variante. */
class DesignTokensPlugin : Plugin<Project> {
    override fun apply(project: Project) {
        project.tasks.register("generateDesignTokens", GenerateDesignTokens::class.java) {
            tokensFile.set(project.rootProject.layout.projectDirectory.file("../design/tokens.json"))
            packageName.set("rtspviewer.app.ui.theme")
            outputDir.set(project.layout.buildDirectory.dir("generated/designTokens"))
        }
        project.tasks.register("checkUiColors", CheckUiColors::class.java) {
            sources.from(
                project.fileTree("src/main/kotlin") {
                    include("**/ui/**/*.kt")
                },
            )
            report.set(project.layout.buildDirectory.file("reports/checkUiColors.txt"))
        }
        project.tasks.register("checkCleartextHttp", CheckCleartextHttp::class.java) {
            sources.from(
                project.fileTree("src/main") { include("**/*.kt", "**/*.java") },
                project.rootProject.fileTree("core/src/main") { include("**/*.kt", "**/*.java") },
            )
            report.set(project.layout.buildDirectory.file("reports/checkCleartextHttp.txt"))
        }
    }
}
