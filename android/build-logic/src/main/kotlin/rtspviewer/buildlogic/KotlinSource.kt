package rtspviewer.buildlogic

/**
 * Leitura mínima de código Kotlin para as verificações do build.
 *
 * [codeLines] devolve as linhas do arquivo com os comentários (`//`, `/* */`,
 * inclusive aninhados e KDoc) trocados por espaços. Literais de string e de
 * caractere são preservados e reconhecidos, para que `//` dentro de
 * `"http://..."` não seja tomado por comentário. Templates `${...}` dentro de
 * strings voltam a ser código. Não é um parser completo: serve para não
 * esconder código das verificações por causa de comentários e URLs.
 */
object KotlinSource {
    private enum class Mode { CODE, LINE_COMMENT, BLOCK_COMMENT, STRING, RAW_STRING, CHAR }

    fun codeLines(source: String): List<String> {
        val out = StringBuilder(source.length)
        // Pilha de contextos: string que abriu um template `${` e a profundidade de chaves.
        val templates = ArrayDeque<Pair<Mode, Int>>()
        var mode = Mode.CODE
        var braces = 0
        var blockDepth = 0
        var i = 0
        val s = source
        fun at(k: Int) = if (k < s.length) s[k] else '\u0000'
        fun blank(c: Char) { out.append(if (c == '\n' || c == '\r') c else ' ') }

        while (i < s.length) {
            val c = s[i]
            when (mode) {
                Mode.CODE -> when {
                    c == '/' && at(i + 1) == '/' -> { mode = Mode.LINE_COMMENT; out.append("  "); i += 2; continue }
                    c == '/' && at(i + 1) == '*' -> { mode = Mode.BLOCK_COMMENT; blockDepth = 1; out.append("  "); i += 2; continue }
                    c == '"' && at(i + 1) == '"' && at(i + 2) == '"' -> { mode = Mode.RAW_STRING; out.append("\"\"\""); i += 3; continue }
                    c == '"' -> mode = Mode.STRING
                    c == '\'' -> mode = Mode.CHAR
                    c == '{' -> braces++
                    c == '}' -> {
                        if (braces == 0 && templates.isNotEmpty()) {
                            val (back, outer) = templates.removeLast()
                            mode = back
                            braces = outer
                        } else if (braces > 0) {
                            braces--
                        }
                    }
                }
                Mode.LINE_COMMENT -> {
                    if (c == '\n') mode = Mode.CODE
                    blank(c); i++; continue
                }
                Mode.BLOCK_COMMENT -> {
                    when {
                        c == '/' && at(i + 1) == '*' -> { blockDepth++; out.append("  "); i += 2; continue }
                        c == '*' && at(i + 1) == '/' -> {
                            blockDepth--
                            if (blockDepth == 0) mode = Mode.CODE
                            out.append("  "); i += 2; continue
                        }
                    }
                    blank(c); i++; continue
                }
                Mode.STRING, Mode.RAW_STRING -> {
                    val raw = mode == Mode.RAW_STRING
                    when {
                        !raw && c == '\\' -> { out.append(s, i, minOf(i + 2, s.length)); i += 2; continue }
                        c == '$' && at(i + 1) == '{' -> {
                            templates.addLast(mode to braces)
                            mode = Mode.CODE
                            braces = 0
                            out.append("\${"); i += 2; continue
                        }
                        raw && c == '"' && at(i + 1) == '"' && at(i + 2) == '"' -> {
                            // Aspas extras antes do fechamento pertencem à string.
                            var end = i + 3
                            while (at(end) == '"') end++
                            out.append(s, i, end); i = end; mode = Mode.CODE; continue
                        }
                        !raw && (c == '"' || c == '\n') -> mode = Mode.CODE
                    }
                }
                Mode.CHAR -> when {
                    c == '\\' -> { out.append(s, i, minOf(i + 2, s.length)); i += 2; continue }
                    c == '\'' || c == '\n' -> mode = Mode.CODE
                }
            }
            out.append(c)
            i++
        }
        return out.toString().lines()
    }
}
