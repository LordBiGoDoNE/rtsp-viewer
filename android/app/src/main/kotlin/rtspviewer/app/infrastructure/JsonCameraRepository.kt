package rtspviewer.app.infrastructure

import java.io.File
import java.io.FileNotFoundException
import java.io.IOException
import kotlinx.serialization.Serializable
import kotlinx.serialization.SerialName
import kotlinx.serialization.json.Json
import rtspviewer.core.application.CameraRepository
import rtspviewer.core.application.LoadedCameras
import rtspviewer.core.application.RepositoryException
import rtspviewer.core.application.RepositoryWarning
import rtspviewer.core.domain.Camera
import rtspviewer.core.domain.CameraId
import rtspviewer.core.domain.CameraName
import rtspviewer.core.domain.RtspUrl
import rtspviewer.core.domain.SubstreamHostMismatchException

/**
 * Lista de câmeras em `cameras.json` (formato versão 1, igual ao do desktop),
 * com escrita atômica e recuperação de arquivo corrompido. Senhas nunca entram
 * neste arquivo.
 */
class JsonCameraRepository(
    private val dir: File,
    private val clock: () -> Long = System::currentTimeMillis,
) : CameraRepository {
    val file: File get() = File(dir, FILE_NAME)

    @Synchronized
    override fun load(): LoadedCameras {
        val bytes = try {
            file.readBytes()
        } catch (_: FileNotFoundException) {
            return LoadedCameras(emptyList())
        } catch (e: IOException) {
            throw RepositoryException("falha ao ler ${file.path}: ${e.message}")
        }
        val cameras = parse(bytes) ?: run {
            val backup = try {
                preserveCorrupted(file, clock())
            } catch (e: IOException) {
                throw RepositoryException("não foi possível preservar o arquivo corrompido: ${e.message}")
            }
            return LoadedCameras(emptyList(), RepositoryWarning.CorruptedFileRecovered(backup.path))
        }
        return LoadedCameras(cameras)
    }

    @Synchronized
    override fun save(cameras: List<Camera>) {
        val dto = FileDto(
            CURRENT_VERSION,
            cameras.map { CameraDto(it.id.value, it.name.value, it.url.toString(), it.username, it.substream?.toString()) },
        )
        try {
            writeAtomic(file, JSON.encodeToString(FileDto.serializer(), dto).toByteArray(Charsets.UTF_8))
        } catch (e: IOException) {
            throw RepositoryException("falha ao gravar ${file.path}: ${e.message}")
        }
    }

    @Serializable
    private data class FileDto(val version: Int, val cameras: List<CameraDto>)

    @Serializable
    private data class CameraDto(
        val id: String,
        val name: String,
        val url: String,
        val username: String? = null,
        /** Stream secundário (opcional; arquivos antigos não têm o campo). */
        @SerialName("substream_url") val substreamUrl: String? = null,
    )

    private companion object {
        const val FILE_NAME = "cameras.json"
        const val CURRENT_VERSION = 1

        val JSON = Json {
            prettyPrint = true
            prettyPrintIndent = "  "
            explicitNulls = false
            ignoreUnknownKeys = true
        }

        /** `null` quando o arquivo não é uma lista válida (vira "corrompido"). */
        fun parse(bytes: ByteArray): List<Camera>? = try {
            val dto = JSON.decodeFromString(FileDto.serializer(), bytes.toString(Charsets.UTF_8))
            if (dto.version != CURRENT_VERSION) {
                null
            } else {
                dto.cameras.map { c ->
                    val id = CameraId.parse(c.id) ?: error("id inválido")
                    val name = CameraName.parse(c.name)
                    val parsed = RtspUrl.parse(c.url)
                    check(parsed.password == null) { "URL com senha no arquivo" }
                    val substream = c.substreamUrl?.let {
                        val p = RtspUrl.parse(it)
                        check(p.password == null) { "URL secundária com senha no arquivo" }
                        p.url
                    }
                    Camera.restore(id, name, parsed.url, c.username).also { cam ->
                        // Secundário de outro host (arquivo editado à mão) é descartado
                        // sem tratar o arquivo como corrompido.
                        try {
                            cam.changeSubstream(substream)
                        } catch (_: SubstreamHostMismatchException) {
                        }
                    }
                }
            }
        } catch (_: Exception) {
            null
        }
    }
}
