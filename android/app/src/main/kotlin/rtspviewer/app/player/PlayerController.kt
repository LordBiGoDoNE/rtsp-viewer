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
 *
 * Primeiro quadro: com o decoder já pronto ([VideoEngine.Listener.onInfo]) e
 * nenhum quadro em [FIRST_FRAME_TIMEOUT_MS], a reprodução para com
 * [StreamError.UnsupportedCodec]. É o caso de um stream cujos quadros-chave o
 * Media3 1.11 não reconhece (H.265 só com CRA, sem IDR; design D3): os dados
 * chegam, mas são todos descartados à espera de um quadro-chave, e reconectar
 * não muda isso. Sem esse prazo a tela ficaria em "Conectando" para sempre.
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
    private var firstFrameJob: Job? = null
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
        cancelFirstFrameWait()
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
                    cancelFirstFrameWait()
                    if (playingSince == null) playingSince = clock()
                    _state.value = PlayerState.Playing
                }

                override fun onInfo(info: StreamInfo) {
                    if (mySession != session) return
                    _info.value = info
                    if (playingSince == null && firstFrameJob == null) {
                        firstFrameJob = scope.launch {
                            delay(FIRST_FRAME_TIMEOUT_MS)
                            firstFrameJob = null
                            if (mySession == session) handleFailure(StreamError.UnsupportedCodec(info.codec))
                        }
                    }
                }

                override fun onFailure(error: StreamError) {
                    if (mySession == session) handleFailure(error)
                }
            },
        )
    }

    private fun cancelFirstFrameWait() {
        firstFrameJob?.cancel()
        firstFrameJob = null
    }

    private fun handleFailure(error: StreamError) {
        cancelFirstFrameWait()
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

/** Espera máxima pelo primeiro quadro com o decoder pronto; folga sobre intervalos de quadro-chave usuais. */
const val FIRST_FRAME_TIMEOUT_MS = 15_000L
