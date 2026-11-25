package rtspviewer.app.player

import androidx.annotation.OptIn
import androidx.media3.common.C
import androidx.media3.common.Format
import androidx.media3.common.MimeTypes
import androidx.media3.common.PlaybackException
import androidx.media3.common.TrackGroup
import androidx.media3.common.Tracks
import androidx.media3.common.util.UnstableApi
import androidx.media3.exoplayer.rtsp.RtspMediaSource
import java.io.IOException
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertIs
import kotlin.test.assertNull
import rtspviewer.core.application.StreamError

@OptIn(UnstableApi::class)
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
    fun nenhumaTrilhaReproduzivelViraCodecNaoSuportadoSemReconectar() {
        // Media3 1.11: RtspClient descarta trilhas sem depacketizador (MJPEG) e,
        // sem nenhuma, falha a preparação com IOException("No playable track.").
        val e = erro(PlaybackException.ERROR_CODE_IO_UNSPECIFIED, IOException("No playable track."), "Source error")
        val r = assertIs<StreamError.UnsupportedCodec>(mapPlaybackError(e))
        assertEquals("Formato de vídeo não suportado: desconhecido", r.message)
        assertFalse(r.isRetryable)
    }

    private fun formato(mime: String) = Format.Builder().setSampleMimeType(mime).build()

    private fun grupo(mime: String, suporte: Int) =
        Tracks.Group(TrackGroup(formato(mime)), false, intArrayOf(suporte), booleanArrayOf(suporte == C.FORMAT_HANDLED))

    @Test
    fun trilhasDeVideoSuportadasSeguem() {
        val t = Tracks(listOf(grupo(MimeTypes.VIDEO_H264, C.FORMAT_HANDLED), grupo(MimeTypes.AUDIO_AAC, C.FORMAT_HANDLED)))
        assertNull(tracksFailure(t))
        assertNull(tracksFailure(Tracks.EMPTY), "trilhas ainda desconhecidas")
    }

    @Test
    fun videoSemDecoderViraCodecNaoSuportado() {
        val t = Tracks(listOf(grupo(MimeTypes.VIDEO_H265, C.FORMAT_UNSUPPORTED_SUBTYPE)))
        val r = assertIs<StreamError.UnsupportedCodec>(tracksFailure(t))
        assertEquals("Formato de vídeo não suportado: hevc", r.message)
    }

    @Test
    fun algumaTrilhaDeVideoSuportadaBasta() {
        val t = Tracks(listOf(grupo(MimeTypes.VIDEO_MP4V, C.FORMAT_UNSUPPORTED_TYPE), grupo(MimeTypes.VIDEO_H264, C.FORMAT_HANDLED)))
        assertNull(tracksFailure(t))
    }

    @Test
    fun soAudioViraSemVideo() {
        // Também o caso de vídeo sem depacketizador no Media3 com áudio suportado.
        val t = Tracks(listOf(grupo(MimeTypes.AUDIO_AAC, C.FORMAT_HANDLED)))
        assertIs<StreamError.NoVideo>(tracksFailure(t))
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
