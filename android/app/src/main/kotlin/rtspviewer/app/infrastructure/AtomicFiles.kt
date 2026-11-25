package rtspviewer.app.infrastructure

import java.io.File
import java.io.FileOutputStream
import java.nio.file.FileAlreadyExistsException
import java.nio.file.Files
import java.nio.file.StandardCopyOption
import java.time.Instant
import java.time.ZoneOffset
import java.time.format.DateTimeFormatter

/**
 * Grava num temporário no mesmo diretório, sincroniza com o disco e renomeia
 * por cima do destino: o arquivo anterior só é substituído depois da escrita
 * completa (mesma estratégia do desktop).
 */
internal fun writeAtomic(target: File, data: ByteArray) {
    val dir = target.absoluteFile.parentFile
    dir.mkdirs()
    val tmp = File(dir, "${target.name}.tmp")
    FileOutputStream(tmp).use { out ->
        out.write(data)
        out.fd.sync()
    }
    Files.move(tmp.toPath(), target.toPath(), StandardCopyOption.ATOMIC_MOVE, StandardCopyOption.REPLACE_EXISTING)
}

private val STAMP = DateTimeFormatter.ofPattern("yyyyMMdd-HHmmss").withZone(ZoneOffset.UTC)

/** `AAAAMMDD-HHMMSS` em UTC, para o sufixo `.corrompido-<data e hora>`. */
internal fun timestampUtc(epochMillis: Long): String = STAMP.format(Instant.ofEpochMilli(epochMillis))

/**
 * Renomeia o arquivo ilegível para `<nome>.corrompido-<data e hora>` e devolve o
 * novo caminho. Se já houver uma cópia com esse nome (duas no mesmo segundo),
 * usa `-2`, `-3`... em vez de sobrescrevê-la.
 */
internal fun preserveCorrupted(file: File, nowMillis: Long): File {
    val dir = file.absoluteFile.parentFile
    val base = "${file.name}.corrompido-${timestampUtc(nowMillis)}"
    var n = 1
    while (true) {
        val backup = File(dir, if (n == 1) base else "$base-$n")
        try {
            // Sem REPLACE_EXISTING: nunca sobrescreve uma cópia anterior.
            Files.move(file.toPath(), backup.toPath())
            return backup
        } catch (_: FileAlreadyExistsException) {
            n++
        }
    }
}
