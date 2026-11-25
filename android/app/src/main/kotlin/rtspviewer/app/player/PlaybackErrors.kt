package rtspviewer.app.player

import androidx.annotation.OptIn
import androidx.media3.common.PlaybackException
import androidx.media3.common.util.UnstableApi
import androidx.media3.exoplayer.ExoPlaybackException
import androidx.media3.exoplayer.rtsp.RtspMediaSource
import rtspviewer.core.application.StreamError
import rtspviewer.core.domain.redactCredentials

/** Nome curto do codec a partir do MIME (`video/avc` → `h264`). */
fun codecName(mime: String?): String = when (mime) {
    null -> "desconhecido"
    "video/avc" -> "h264"
    "video/hevc" -> "hevc"
    "video/mjpeg", "video/x-motion-jpeg" -> "mjpeg"
    "video/mp4v-es" -> "mpeg4"
    "video/x-vnd.on2.vp8" -> "vp8"
    "video/x-vnd.on2.vp9" -> "vp9"
    "video/av01" -> "av1"
    else -> mime.substringAfter('/')
}

/**
 * Traduz um erro do Media3 para as falhas da spec. A mensagem passa por
 * [redactCredentials]: nada de usuário ou senha em tela ou log.
 */
@OptIn(UnstableApi::class)
fun mapPlaybackError(e: PlaybackException): StreamError {
    if (causes(e).any { it is RtspMediaSource.RtspPlaybackException && AUTH_FAILED.containsMatchIn(it.message.orEmpty()) }) {
        return StreamError.AuthenticationFailed()
    }
    val detail = redactCredentials(causes(e).mapNotNull { it.message }.lastOrNull() ?: e.errorCodeName)
    return when (e.errorCode) {
        PlaybackException.ERROR_CODE_DECODER_INIT_FAILED,
        PlaybackException.ERROR_CODE_DECODER_QUERY_FAILED,
        PlaybackException.ERROR_CODE_DECODING_FORMAT_UNSUPPORTED,
        PlaybackException.ERROR_CODE_DECODING_FORMAT_EXCEEDS_CAPABILITIES,
        -> StreamError.UnsupportedCodec(codecName((e as? ExoPlaybackException)?.rendererFormat?.sampleMimeType))

        PlaybackException.ERROR_CODE_IO_NETWORK_CONNECTION_FAILED,
        PlaybackException.ERROR_CODE_IO_NETWORK_CONNECTION_TIMEOUT,
        PlaybackException.ERROR_CODE_IO_UNSPECIFIED,
        PlaybackException.ERROR_CODE_TIMEOUT,
        PlaybackException.ERROR_CODE_IO_BAD_HTTP_STATUS,
        PlaybackException.ERROR_CODE_IO_FILE_NOT_FOUND,
        -> StreamError.Unreachable(detail)

        else -> StreamError.Other(detail)
    }
}

/** `DESCRIBE 401`, `SETUP 401`… (RtspClient do Media3 1.11). */
private val AUTH_FAILED = Regex("""\b401\b""")

private fun causes(e: Throwable): Sequence<Throwable> = generateSequence(e) { it.cause.takeIf { c -> c !== it } }.take(10)
