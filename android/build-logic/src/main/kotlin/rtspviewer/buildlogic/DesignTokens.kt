package rtspviewer.buildlogic

import groovy.json.JsonSlurper

/** Erro de token: a mensagem sempre indica o token (ADR 0004). */
class TokenError(message: String) : Exception(message)

/**
 * Gera o tema do Compose a partir de `design/tokens.json` (ADR 0004 e 0006).
 * Função pura, testável sem Gradle.
 */
object DesignTokens {
    private val REQUIRED = mapOf(
        "color" to listOf(
            "background", "surface", "surface-raised", "surface-hover", "border",
            "text", "text-muted", "primary", "primary-hover", "on-primary",
            "danger", "on-danger", "warning", "success", "video-background",
            "overlay", "focus",
        ),
        "font" to listOf(
            "size-small", "size-body", "size-title", "size-display",
            "weight-regular", "weight-bold",
        ),
        "space" to listOf("xs", "s", "m", "l", "xl"),
        "radius" to listOf("s", "m"),
        "size" to listOf(
            "control-height", "list-item-height", "toolbar-height", "form-width",
            "dialog-width", "status-width", "info-width", "about-width",
        ),
    )

    /** Tokens que só fazem sentido no desktop (janela). */
    private fun desktopOnly(group: String, key: String) =
        group == "size" && key.startsWith("window-")

    private val COLOR = Regex("^#([0-9A-Fa-f]{6})([0-9A-Fa-f]{2})?$")
    private val KEY = Regex("^[a-z][a-z0-9]*(-[a-z0-9]+)*$")

    fun generate(json: String, packageName: String): String {
        val root = try {
            JsonSlurper().parseText(json)
        } catch (e: Exception) {
            throw TokenError("design/tokens.json inválido: ${e.message}")
        } as? Map<*, *> ?: throw TokenError("design/tokens.json: a raiz deve ser um objeto")

        val groups = linkedMapOf<String, Map<String, Any?>>()
        for ((group, required) in REQUIRED) {
            val raw = root[group] as? Map<*, *>
                ?: throw TokenError("token ausente: grupo \"$group\"")
            val entries = linkedMapOf<String, Any?>()
            for ((k, v) in raw) {
                val key = k.toString()
                if (!KEY.matches(key)) throw TokenError("token com nome inválido: $group.$key")
                if (!desktopOnly(group, key)) entries[key] = v
            }
            for (key in required) {
                if (key !in entries) throw TokenError("token ausente: $group.$key")
            }
            groups[group] = entries
        }

        val out = StringBuilder()
        out.appendLine("// Gerado de design/tokens.json pela task generateDesignTokens. Não editar.")
        out.appendLine("package $packageName")
        out.appendLine()
        out.appendLine("import androidx.compose.ui.graphics.Color")
        out.appendLine("import androidx.compose.ui.text.font.FontWeight")
        out.appendLine("import androidx.compose.ui.unit.dp")
        out.appendLine("import androidx.compose.ui.unit.sp")
        out.appendLine()
        out.appendLine("object Tokens {")

        out.appendLine("    object Colors {")
        for ((key, v) in groups.getValue("color")) {
            out.appendLine("        val ${camel(key)} = Color(${argb("color.$key", v)})")
        }
        out.appendLine("    }")

        out.appendLine("    object Font {")
        for ((key, v) in groups.getValue("font")) {
            val n = positive("font.$key", v)
            when {
                key.startsWith("size-") -> out.appendLine("        val ${camel(key)} = ${n}.sp")
                key.startsWith("weight-") -> {
                    if (n !in 1..1000) throw TokenError("token fora do intervalo 1..1000: font.$key")
                    out.appendLine("        val ${camel(key)} = FontWeight($n)")
                }
                else -> throw TokenError("token de fonte desconhecido: font.$key")
            }
        }
        out.appendLine("    }")

        for ((group, obj) in listOf("space" to "Space", "radius" to "Radius", "size" to "Size")) {
            out.appendLine("    object $obj {")
            for ((key, v) in groups.getValue(group)) {
                out.appendLine("        val ${camel(key)} = ${positive("$group.$key", v)}.dp")
            }
            out.appendLine("    }")
        }
        out.appendLine("}")
        return out.toString()
    }

    private fun argb(name: String, v: Any?): String {
        val s = v as? String ?: throw TokenError("cor inválida (use #RRGGBB ou #RRGGBBAA): $name")
        val m = COLOR.matchEntire(s) ?: throw TokenError("cor inválida (use #RRGGBB ou #RRGGBBAA): $name")
        val rgb = m.groupValues[1].uppercase()
        val alpha = m.groupValues[2].ifEmpty { "FF" }.uppercase()
        return "0x$alpha$rgb"
    }

    private fun positive(name: String, v: Any?): Int {
        val n = v as? Number ?: throw TokenError("número inválido: $name")
        val i = n.toInt()
        if (i.toDouble() != n.toDouble() || i <= 0) {
            throw TokenError("número inválido (inteiro positivo): $name")
        }
        return i
    }

    internal fun camel(key: String): String {
        val parts = key.split('-')
        return parts.first() + parts.drop(1).joinToString("") { p -> p.replaceFirstChar { it.uppercase() } }
    }
}

/**
 * Verificação de cor literal no código da UI (ADR 0004): toda cor vem de `Tokens`.
 * Devolve as linhas que violam a regra, no formato `arquivo:linha: trecho`.
 */
object UiColorCheck {
    private val ALLOWED = setOf("Transparent", "Unspecified")
    private val LITERAL_CALL = Regex("""\bColor\s*\(""")
    private val NAMED = Regex("""\bColor\.([A-Z][A-Za-z]*)\b""")

    fun violations(fileName: String, source: String): List<String> {
        val out = mutableListOf<String>()
        source.lines().forEachIndexed { i, line ->
            val code = line.substringBefore("//")
            val named = NAMED.findAll(code).map { it.groupValues[1] }.filter { it !in ALLOWED }.toList()
            if (LITERAL_CALL.containsMatchIn(code) || named.isNotEmpty()) {
                out += "$fileName:${i + 1}: ${line.trim()}"
            }
        }
        return out
    }
}
