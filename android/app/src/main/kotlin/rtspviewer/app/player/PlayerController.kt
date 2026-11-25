package rtspviewer.app.player

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import rtspviewer.core.application.StreamError
import rtspviewer.core.application.StreamRequest
import rtspviewer.core.domain.ConnectionUrl
import rtspviewer.core.domain.ReconnectPolicy

/** Estados visíveis da reprodução (spec `live-view`, "Estados e reconexão"). */
sealed interface PlayerState {
    data object Idle : PlayerState
    data object Connecting : PlayerState
    data object Playing : PlayerState
    data class Reconnecting(val attempt: Int, val delayMs: Long) : PlayerState
    data class Failed(val error: StreamError) : PlayerState
}

/** Informações do painel "Informações". */
data class StreamInfo(
    val codec: String,
    val width: Int,
    val height: Int,
    /** Nome do decoder (`MediaCodec`). */
    val decoder: String,
    val hardware: Boolean,
) {
    /** "hardware (c2.qti.avc.decoder)" ou "software (c2.android.avc.decoder)". */
    val decodeLabel: String get() = "${if (hardware) "hardware" else "software"} ($decoder)"
}

/**
 * Motor de vídeo (Media3 no app, falso nos testes). Todos os métodos e
 * callbacks acontecem na thread principal.
 */
interface VideoEngine {
    fun open(url: ConnectionUrl, listener: Listener)

    /** Encerra a conexão e libera o decoder. Idempotente. */
    fun close()

    interface Listener {
        fun onPlaying()

        fun onInfo(info: StreamInfo)

        fun onFailure(error: StreamError)
    }
}

/**
 * Máquina de estados da reprodução, independente do Media3: conectando,
 * reproduzindo, reconectando com espera crescente (1–30 s, reinício após 10 s
 * estáveis) e erro. Falhas que exigem ação do usuário (senha, codec) param sem
 * reconectar. Chamar só na thread principal; [scope] usa o dispatcher dela.
 */
class PlayerController(
    private val engine: VideoEngine,
    private val scope: CoroutineScope,
    private val clock: () -> Long = System::currentTimeMillis,
) {
    private val _state = MutableStateFlow<PlayerState>(PlayerState.Idle)
    val state: StateFlow<PlayerState> = _state.asStateFlow()

    private val _info = MutableStateFlow<StreamInfo?>(null)
    val info: StateFlow<StreamInfo?> = _info.asStateFlow()

    private var request: StreamRequest? = null
    private var policy = ReconnectPolicy()
    private var session = 0L
    private var retryJob: Job? = null
    private var playingSince: Long? = null

    /** Abre a câmera; substitui qualquer reprodução anterior. */
    fun start(request: StreamRequest) {
        stop()
        this.request = request
        policy = ReconnectPolicy()
        _state.value = PlayerState.Connecting
        open()
    }

    /** "Tentar de novo" depois de um erro, com a sequência de esperas zerada. */
    fun retry() {
        request?.let(::start)
    }

    /** Encerra a conexão (sair da tela, app em segundo plano). */
    fun stop() {
        session++
        retryJob?.cancel()
        retryJob = null
        playingSince = null
        engine.close()
        request = null
        _info.value = null
        _state.value = PlayerState.Idle
    }

    private fun open() {
        val req = request ?: return
        val mySession = ++session
        playingSince = null
        engine.open(
            req.url,
            object : VideoEngine.Listener {
                override fun onPlaying() {
                    if (mySession != session) return
                    if (playingSince == null) playingSince = clock()
                    _state.value = PlayerState.Playing
                }

                override fun onInfo(info: StreamInfo) {
                    if (mySession == session) _info.value = info
                }

                override fun onFailure(error: StreamError) {
                    if (mySession == session) handleFailure(error)
                }
            },
        )
    }

    private fun handleFailure(error: StreamError) {
        engine.close()
        session++
        _info.value = null // o decoder foi liberado; o painel não mostra dados velhos
        playingSince?.let { policy.recordPlayback(clock() - it) }
        playingSince = null
        if (!error.isRetryable) {
            _state.value = PlayerState.Failed(error)
            return
        }
        val (attempt, delayMs) = policy.nextDelay()
        _state.value = PlayerState.Reconnecting(attempt, delayMs)
        retryJob = scope.launch {
            delay(delayMs)
            retryJob = null
            open()
        }
    }
}
