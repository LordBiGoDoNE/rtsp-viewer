package rtspviewer.app.ui

import rtspviewer.app.player.PlayerState
import rtspviewer.app.player.StreamInfo
import rtspviewer.core.application.CatalogError
import rtspviewer.core.application.Field
import rtspviewer.core.application.RepositoryWarning
import rtspviewer.core.application.ResolveError
import rtspviewer.core.domain.CameraName
import rtspviewer.core.domain.DeviceAddress
import rtspviewer.core.domain.redactCredentials

/**
 * Textos exibidos, derivados do domínio (design/telas.md). Funções puras, sem
 * Compose, iguais às de `presentation.rs` do desktop.
 */
object Texts {
    /** Edição com usuário e sem senha digitada: a consulta ONVIF precisa dela. */
    const val LOOKUP_NEEDS_PASSWORD = "Informe a senha para consultar a câmera."
    const val CONNECTING = "Conectando…"

    /** Nome de uma câmera encontrada na busca. */
    fun foundName(suggested: String?): String = suggested?.trim()?.ifEmpty { null } ?: "Câmera ONVIF"

    /**
     * Nome de uma câmera cadastrada só com o endereço, quando o usuário deixou
     * o nome vazio e a câmera não informou um.
     */
    fun fallbackName(address: DeviceAddress): String = takeCodePoints("Câmera $address", CameraName.MAX)

    /** Nome sugerido pela câmera, cortado no limite do nome. */
    fun suggestedName(name: String?): String? = name?.trim()?.ifEmpty { null }?.let { takeCodePoints(it, CameraName.MAX) }

    /**
     * Motivo exibido na escolha de fabricante; vazio quando o motivo é a
     * própria falta de ONVIF, que o texto da tela já diz.
     */
    fun vendorReason(reason: ResolveError): String =
        if (reason is ResolveError.NotOnvif) "" else "Motivo: ${reason.message}"

    fun discoveryError(message: String): String = "Não foi possível procurar câmeras: $message"

    fun banner(warning: RepositoryWarning): String = when (warning) {
        is RepositoryWarning.CorruptedFileRecovered ->
            "A lista de câmeras não pôde ser lida e foi guardada em ${warning.backup}. Começando com a lista vazia."
    }

    fun removeTitle(name: String): String = "Remover $name?"

    fun passwordTitle(username: String): String = "Senha de $username"

    fun rowActions(name: String): String = "Ações de $name"

    /** Linhas do painel "Informações". */
    fun info(info: StreamInfo?, displayUrl: String): List<String> = buildList {
        if (info != null) {
            add("Codec: ${info.codec}")
            add("Resolução: ${info.width}x${info.height}")
            add("Decodificação: ${info.decodeLabel}")
        }
        add("Endereço: ${redactCredentials(displayUrl)}")
    }

    /** Estado exibido sobre o vídeo. */
    fun status(viewer: ViewerState, player: PlayerState): ViewStatus {
        viewer.error?.let { return ViewStatus.Failed(redactCredentials(it)) }
        if (viewer.passwordUser != null) return ViewStatus.None
        return when (player) {
            PlayerState.Idle, PlayerState.Connecting -> ViewStatus.Message(CONNECTING)
            PlayerState.Playing -> ViewStatus.None
            is PlayerState.Reconnecting -> ViewStatus.Message("Reconectando (tentativa ${player.attempt})…")
            is PlayerState.Failed -> ViewStatus.Failed(redactCredentials(player.error.message.orEmpty()))
        }
    }

    /** Erros do formulário distribuídos por campo: (nome, endereço, geral). */
    fun formErrors(error: CatalogError): Triple<String?, String?, String?> {
        if (error !is CatalogError.Invalid) return Triple(null, null, error.message)
        var name: String? = null
        var url: String? = null
        for (f in error.errors) {
            when (f.field) {
                Field.Name -> name = f.message
                Field.Url -> url = f.message
            }
        }
        return Triple(name, url, null)
    }

    private fun takeCodePoints(s: String, n: Int): String =
        if (s.codePointCount(0, s.length) <= n) s else s.substring(0, s.offsetByCodePoints(0, n))
}

/** Mensagem sobre o vídeo. */
sealed interface ViewStatus {
    data object None : ViewStatus

    data class Message(val text: String) : ViewStatus

    /** Erro com o botão "Tentar de novo". */
    data class Failed(val text: String) : ViewStatus
}
