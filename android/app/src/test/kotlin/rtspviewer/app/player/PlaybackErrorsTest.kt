package rtspviewer.app.player

import androidx.media3.common.PlaybackException
import androidx.media3.exoplayer.rtsp.RtspMediaSource
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertIs
import rtspviewer.core.application.StreamError

class PlaybackErrorsTest {
    private fun erro(code: Int, cause: Throwable? = null, msg: String = "falha") = PlaybackException(msg, cause, code)

    @Test
    fun respostaRtsp401ViraSenhaInvalida() {
        val e = erro(PlaybackException.ERROR_CODE_IO_UNSPECIFIED, RtspMediaSource.RtspPlaybackException("DESCRIBE 401"))
        assertIs<StreamError.AuthenticationFailed>(mapPlaybackError(e))
    }

    @Test
    fun outroStatusRtspNaoEAutenticacao() {
        val e = erro(PlaybackException.ERROR_CODE_IO_UNSPECIFIED, RtspMediaSource.RtspPlaybackException("DESCRIBE 404"))
        assertIs<StreamError.Unreachable>(mapPlaybackError(e))
    }

    @Test
    fun redeEPrazoReconectam() {
        for (code in listOf(PlaybackException.ERROR_CODE_IO_NETWORK_CONNECTION_FAILED, PlaybackException.ERROR_CODE_TIMEOUT)) {
            val r = mapPlaybackError(erro(code))
            assertIs<StreamError.Unreachable>(r)
            assertEquals(true, r.isRetryable)
        }
    }

    @Test
    fun decoderSemSuporteViraCodecNaoSuportado() {
        val r = mapPlaybackError(erro(PlaybackException.ERROR_CODE_DECODING_FORMAT_UNSUPPORTED))
        assertIs<StreamError.UnsupportedCodec>(r)
        assertFalse(r.isRetryable)
    }

    @Test
    fun mensagemNaoMostraCredenciais() {
        val e = erro(PlaybackException.ERROR_CODE_IO_NETWORK_CONNECTION_FAILED, msg = "falha rtsp://admin:s3gr3do@10.0.0.2/s")
        val r = mapPlaybackError(e)
        assertFalse(r.message!!.contains("s3gr3do"), r.message)
    }

    @Test
    fun nomesDeCodec() {
        assertEquals("h264", codecName("video/avc"))
        assertEquals("hevc", codecName("video/hevc"))
        assertEquals("mjpeg", codecName("video/mjpeg"))
        assertEquals("desconhecido", codecName(null))
    }
}
