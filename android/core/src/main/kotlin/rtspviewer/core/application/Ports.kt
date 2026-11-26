package rtspviewer.core.application

import rtspviewer.core.domain.Camera
import rtspviewer.core.domain.CameraId
import rtspviewer.core.domain.ConnectionUrl
import rtspviewer.core.domain.DeviceAddress
import rtspviewer.core.domain.Password
import rtspviewer.core.domain.RtspUrl

// ------------------------------------------------------------------ persistência

class RepositoryException(message: String) : Exception(message)

/** Aviso não fatal da leitura da lista. */
sealed interface RepositoryWarning {
    /** O arquivo não pôde ser lido; foi preservado em [backup] e a lista começou vazia. */
    data class CorruptedFileRecovered(val backup: String) : RepositoryWarning
}

data class LoadedCameras(val cameras: List<Camera>, val warning: RepositoryWarning? = null)

interface CameraRepository {
    /** @throws RepositoryException */
    fun load(): LoadedCameras

    /** Grava a lista inteira de forma atômica. @throws RepositoryException */
    fun save(cameras: List<Camera>)
}

// ------------------------------------------------------------------ credenciais

class CredentialException(message: String) : Exception(message)

interface CredentialStore {
    /** @throws CredentialException */
    fun get(id: CameraId): Password?

    /** @throws CredentialException */
    fun set(id: CameraId, password: Password)

    /** Remove a senha; não é erro se ela não existir. @throws CredentialException */
    fun delete(id: CameraId)

    /** `false` quando as senhas ficam só em memória até o app fechar. */
    val isPersistent: Boolean
}

// ------------------------------------------------------------------ reprodução

/** Pedido de reprodução. A URL com credenciais existe só aqui, em memória. */
data class StreamRequest(
    val cameraId: CameraId,
    val url: ConnectionUrl,
    /** URL sem credenciais, para logs e mensagens. */
    val displayUrl: String,
)

/** Falhas do stream, com as mesmas mensagens do desktop. */
sealed class StreamError(message: String) : Exception(message) {
    class AuthenticationFailed : StreamError("Usuário ou senha inválidos.")
    class Unreachable(detail: String) : StreamError("Câmera inacessível: $detail")
    class UnsupportedCodec(codec: String) : StreamError("Formato de vídeo não suportado: $codec")
    class NoVideo : StreamError("A câmera não envia vídeo.")
    class Other(detail: String) : StreamError("Falha no stream: $detail")

    /** Erros que não se resolvem tentando de novo exigem ação do usuário. */
    val isRetryable: Boolean get() = this is Unreachable || this is Other
}

// ------------------------------------------------------------------ descoberta

/** Câmera encontrada na rede (WS-Discovery). */
data class DiscoveredCamera(
    /** Host da câmera (IP), usado para cadastrar e para "Já cadastrada". */
    val host: String,
    /** URL do serviço de dispositivo ONVIF anunciada pela câmera. */
    val deviceUrl: String,
    val suggestedName: String?,
)

class DiscoveryFailure(message: String) : Exception(message)

/** Busca câmeras na rede local. Bloqueia por alguns segundos: chamar fora da thread da UI. */
interface CameraDiscovery {
    /** @throws DiscoveryFailure */
    fun discover(): List<DiscoveredCamera>
}

/** Onde consultar a câmera. */
sealed interface StreamTarget {
    /** Endereço digitado (as portas ONVIF usuais são tentadas se não houver). */
    data class Address(val address: DeviceAddress) : StreamTarget

    /** URL de serviço ONVIF vinda da busca. */
    data class ServiceUrl(val url: String) : StreamTarget
}

data class ResolvedStreams(
    val main: RtspUrl,
    val sub: RtspUrl?,
    /** Fabricante e modelo, para sugerir o nome. */
    val suggestedName: String?,
)

sealed class ResolveError(message: String) : Exception(message) {
    class NotOnvif : ResolveError("A câmera não respondeu ao ONVIF.")
    class AuthenticationFailed : ResolveError("Usuário ou senha inválidos.")
    class NoVideoProfiles : ResolveError("A câmera não informou nenhum perfil de vídeo.")
    class Unreachable(detail: String) : ResolveError("A câmera não respondeu: $detail")
    class Protocol(detail: String) : ResolveError("Resposta inesperada da câmera: $detail")
}

/** Obtém da câmera as URLs RTSP (ONVIF). Bloqueia: chamar fora da thread da UI. */
interface StreamResolver {
    /** @throws ResolveError */
    fun resolve(target: StreamTarget, username: String?, password: Password?): ResolvedStreams
}
