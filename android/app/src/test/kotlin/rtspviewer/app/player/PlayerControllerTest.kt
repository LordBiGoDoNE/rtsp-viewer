package rtspviewer.app.player

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertIs
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import rtspviewer.core.application.StreamError
import rtspviewer.core.application.StreamRequest
import rtspviewer.core.domain.CameraId
import rtspviewer.core.domain.RtspUrl

/** Motor falso: registra aberturas e fechamentos e deixa o teste disparar eventos. */
private class FakeEngine : VideoEngine {
    var opens = 0
    var closes = 0
    var listener: VideoEngine.Listener? = null
    val isOpen get() = listener != null

    override fun open(url: rtspviewer.core.domain.ConnectionUrl, listener: VideoEngine.Listener) {
        opens++
        this.listener = listener
    }

    override fun close() {
        if (listener != null) closes++
        listener = null
    }
}

@OptIn(ExperimentalCoroutinesApi::class)
class PlayerControllerTest {
    private val request = StreamRequest(
        CameraId.generate(),
        RtspUrl.parse("rtsp://h/s").url.withCredentials("admin", null),
        "rtsp://h/s",
    )

    private fun TestScope.controller(engine: FakeEngine) =
        PlayerController(engine, backgroundScope) { testScheduler.currentTime }

    @Test
    fun conectaEReproduz() = runTest {
        val e = FakeEngine()
        val c = controller(e)
        c.start(request)
        assertEquals(PlayerState.Connecting, c.state.value)
        e.listener!!.onPlaying()
        assertEquals(PlayerState.Playing, c.state.value)
        e.listener!!.onInfo(StreamInfo("h264", 1280, 720, "c2.qti.avc.decoder", true))
        assertEquals("hardware (c2.qti.avc.decoder)", c.info.value!!.decodeLabel)
    }

    @Test
    fun falhaLimpaAsInformacoes() = runTest {
        val e = FakeEngine()
        val c = controller(e)
        c.start(request)
        e.listener!!.onPlaying()
        e.listener!!.onInfo(StreamInfo("h264", 1280, 720, "c2.qti.avc.decoder", true))
        e.listener!!.onFailure(StreamError.Unreachable("x"))
        assertIs<PlayerState.Reconnecting>(c.state.value)
        assertEquals(null, c.info.value)
    }

    @Test
    fun quedaReconectaComEsperaCrescente() = runTest {
        val e = FakeEngine()
        val c = controller(e)
        c.start(request)
        val esperas = mutableListOf<Long>()
        repeat(7) {
            e.listener!!.onFailure(StreamError.Unreachable("tempo"))
            val s = assertIs<PlayerState.Reconnecting>(c.state.value)
            assertEquals(it + 1, s.attempt)
            esperas += s.delayMs
            assertEquals(false, e.isOpen, "a conexão antiga fecha antes de esperar")
            advanceTimeBy(s.delayMs - 1)
            runCurrent()
            assertEquals(false, e.isOpen)
            advanceTimeBy(1)
            runCurrent()
            assertEquals(true, e.isOpen, "reabre depois da espera")
        }
        assertEquals(listOf(1_000L, 2_000, 4_000, 8_000, 16_000, 30_000, 30_000), esperas)
    }

    @Test
    fun reproducaoEstavelZeraASequencia() = runTest {
        val e = FakeEngine()
        val c = controller(e)
        c.start(request)
        e.listener!!.onFailure(StreamError.Other("x"))
        advanceTimeBy(1_000)
        runCurrent()
        e.listener!!.onPlaying()
        advanceTimeBy(10_000)
        e.listener!!.onFailure(StreamError.Other("x"))
        assertEquals(PlayerState.Reconnecting(1, 1_000), c.state.value)
    }

    @Test
    fun voltaSemAcaoDoUsuario() = runTest {
        val e = FakeEngine()
        val c = controller(e)
        c.start(request)
        e.listener!!.onFailure(StreamError.Unreachable("x"))
        advanceTimeBy(1_000)
        runCurrent()
        e.listener!!.onPlaying()
        assertEquals(PlayerState.Playing, c.state.value)
    }

    @Test
    fun autenticacaoECodecNaoReconectam() = runTest {
        for (erro in listOf(StreamError.AuthenticationFailed(), StreamError.UnsupportedCodec("mjpeg"), StreamError.NoVideo())) {
            val e = FakeEngine()
            val c = controller(e)
            c.start(request)
            e.listener!!.onFailure(erro)
            assertEquals(PlayerState.Failed(erro), c.state.value)
            advanceTimeBy(60_000)
            runCurrent()
            assertEquals(1, e.opens, "não tenta de novo sozinho")
        }
    }

    @Test
    fun decoderProntoSemQuadroParaComFormatoNaoSuportado() = runTest {
        val e = FakeEngine()
        val c = controller(e)
        c.start(request)
        e.listener!!.onInfo(StreamInfo("hevc", 1280, 720, "c2.exynos.hevc.decoder", true))
        advanceTimeBy(FIRST_FRAME_TIMEOUT_MS - 1)
        runCurrent()
        assertEquals(PlayerState.Connecting, c.state.value)
        advanceTimeBy(1)
        runCurrent()
        val s = assertIs<PlayerState.Failed>(c.state.value)
        assertEquals("Formato de vídeo não suportado: hevc", s.error.message)
        assertEquals(false, e.isOpen)
        advanceTimeBy(60_000)
        runCurrent()
        assertEquals(1, e.opens, "não tenta de novo sozinho")
    }

    @Test
    fun primeiroQuadroCancelaOPrazo() = runTest {
        val e = FakeEngine()
        val c = controller(e)
        c.start(request)
        e.listener!!.onInfo(StreamInfo("hevc", 1280, 720, "c2.exynos.hevc.decoder", true))
        advanceTimeBy(FIRST_FRAME_TIMEOUT_MS - 1)
        e.listener!!.onPlaying()
        advanceTimeBy(60_000)
        runCurrent()
        assertEquals(PlayerState.Playing, c.state.value)
    }

    @Test
    fun pararCancelaOPrazoDoPrimeiroQuadro() = runTest {
        val e = FakeEngine()
        val c = controller(e)
        c.start(request)
        e.listener!!.onInfo(StreamInfo("hevc", 1280, 720, "c2.exynos.hevc.decoder", true))
        c.stop()
        advanceTimeBy(60_000)
        runCurrent()
        assertEquals(PlayerState.Idle, c.state.value)
    }

    @Test
    fun pararDuranteAEsperaCancelaAReconexao() = runTest {
        val e = FakeEngine()
        val c = controller(e)
        c.start(request)
        e.listener!!.onFailure(StreamError.Unreachable("x"))
        c.stop()
        assertEquals(PlayerState.Idle, c.state.value)
        advanceTimeBy(60_000)
        runCurrent()
        assertEquals(1, e.opens)
    }

    @Test
    fun pararFechaAConexaoEIgnoraEventosAtrasados() = runTest {
        val e = FakeEngine()
        val c = controller(e)
        c.start(request)
        val antigo = e.listener!!
        c.stop()
        assertEquals(1, e.closes)
        antigo.onPlaying()
        antigo.onFailure(StreamError.Unreachable("x"))
        assertEquals(PlayerState.Idle, c.state.value)
    }

    @Test
    fun tentarDeNovoRecomecaDoZero() = runTest {
        val e = FakeEngine()
        val c = controller(e)
        c.start(request)
        e.listener!!.onFailure(StreamError.AuthenticationFailed())
        c.retry()
        assertEquals(PlayerState.Connecting, c.state.value)
        assertEquals(2, e.opens)
    }
}
