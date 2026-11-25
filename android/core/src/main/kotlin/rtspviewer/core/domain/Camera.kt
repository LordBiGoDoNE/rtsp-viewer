package rtspviewer.core.domain

import java.util.UUID

/** Identidade estável de uma câmera (UUID v4 em texto). */
@JvmInline
value class CameraId private constructor(val value: String) {
    override fun toString(): String = value

    companion object {
        fun generate(): CameraId = CameraId(UUID.randomUUID().toString())

        /** Reconstrói um id lido da persistência. Aceita só UUIDs. */
        fun parse(value: String): CameraId? {
            if (!UUID_TEXT.matches(value)) return null
            return runCatching { CameraId(UUID.fromString(value).toString()) }.getOrNull()
        }

        private val UUID_TEXT = Regex("^[0-9A-Fa-f]{8}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{12}$")
    }
}

enum class NameError(val message: String) {
    Empty("Informe um nome."),
    TooLong("Use no máximo ${CameraName.MAX} caracteres."),
}

class InvalidNameException(val error: NameError) : IllegalArgumentException(error.message)

/** Nome exibido da câmera: 1 a 60 caracteres, não composto só de espaços. */
@JvmInline
value class CameraName private constructor(val value: String) {
    override fun toString(): String = value

    companion object {
        const val MAX = 60

        fun parse(value: String): CameraName {
            val trimmed = value.trim()
            if (trimmed.isEmpty()) throw InvalidNameException(NameError.Empty)
            if (trimmed.codePointCount(0, trimmed.length) > MAX) throw InvalidNameException(NameError.TooLong)
            return CameraName(trimmed)
        }
    }
}

/** Stream secundário de outro dispositivo: o host difere do principal. */
class SubstreamHostMismatchException :
    IllegalArgumentException("O stream secundário precisa usar o mesmo endereço do principal.")

/**
 * Câmera cadastrada. A senha nunca faz parte da entidade: fica no cofre,
 * indexada pelo [CameraId].
 */
class Camera private constructor(
    val id: CameraId,
    name: CameraName,
    url: RtspUrl,
    username: String?,
    substream: RtspUrl?,
) {
    var name: CameraName = name
        private set
    var url: RtspUrl = url
        private set
    var username: String? = normalizeUsername(username)
        private set

    /**
     * Stream secundário (menor resolução), com as mesmas credenciais.
     * Invariante: tem o mesmo host do principal. Trocar a URL principal o
     * descarta (ver [changeUrl]).
     */
    var substream: RtspUrl? = substream
        private set

    /**
     * Define (ou remove, com `null`) o stream secundário. Recusa um
     * secundário com host diferente do principal: as credenciais do principal
     * não devem ir para outro dispositivo.
     */
    fun changeSubstream(substream: RtspUrl?) {
        if (substream != null && !sameHost(substream, url)) throw SubstreamHostMismatchException()
        this.substream = substream
    }

    fun rename(name: CameraName) {
        this.name = name
    }

    /**
     * Troca a URL principal. Se ela mudar, o secundário é descartado: ele foi
     * derivado da URL antiga e não há como saber se continua válido.
     */
    fun changeUrl(url: RtspUrl) {
        if (url != this.url) substream = null
        this.url = url
    }

    fun changeUsername(username: String?) {
        this.username = normalizeUsername(username)
    }

    /** Uma câmera com usuário precisa de senha para conectar. */
    fun requiresCredentials(): Boolean = username != null

    /** Cópia independente, para editar sem afetar a lista atual. */
    fun copy(): Camera = Camera(id, name, url, username, substream)

    override fun equals(other: Any?): Boolean =
        other is Camera && other.id == id && other.name == name && other.url == url &&
            other.username == username && other.substream == substream

    override fun hashCode(): Int = id.hashCode()

    override fun toString(): String = "Camera(id=$id, name=$name, url=$url, username=$username, substream=$substream)"

    companion object {
        fun create(name: CameraName, url: RtspUrl, username: String?): Camera =
            Camera(CameraId.generate(), name, url, username, null)

        /** Reconstrói uma câmera já persistida. */
        fun restore(id: CameraId, name: CameraName, url: RtspUrl, username: String?): Camera =
            Camera(id, name, url, username, null)

        private fun sameHost(a: RtspUrl, b: RtspUrl) = asciiLowercase(a.host) == asciiLowercase(b.host)

        private fun normalizeUsername(username: String?): String? = username?.trim()?.ifEmpty { null }
    }
}
