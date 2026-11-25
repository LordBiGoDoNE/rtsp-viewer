package rtspviewer.app.player

import android.content.Context
import android.os.Handler
import androidx.annotation.OptIn
import androidx.media3.common.Format
import androidx.media3.common.MimeTypes
import androidx.media3.common.util.UnstableApi
import androidx.media3.exoplayer.DecoderReuseEvaluation
import androidx.media3.exoplayer.DefaultRenderersFactory
import androidx.media3.exoplayer.FormatHolder
import androidx.media3.exoplayer.Renderer
import androidx.media3.exoplayer.mediacodec.MediaCodecSelector
import androidx.media3.exoplayer.video.MediaCodecVideoRenderer
import androidx.media3.exoplayer.video.VideoRendererEventListener

/**
 * H.265 por RTSP no Media3 1.11: o `RtspMediaTrack` entrega VPS, SPS e PPS do
 * SDP (`sprop-vps/sps/pps`) como três entradas de `initializationData`, que
 * viram `csd-0`, `csd-1` e `csd-2` no `MediaCodec`. Para HEVC o Android espera
 * os três juntos em `csd-0`; decoders que seguem isso à risca (o
 * `c2.exynos.hevc.decoder` do Galaxy A56, por exemplo) recusam o fluxo com
 * `CodecException 0x80000000` logo ao iniciar. Aqui os conjuntos de parâmetros
 * são unidos numa só entrada, como faz o extrator de MP4 do próprio Media3.
 */
@OptIn(UnstableApi::class)
fun withSingleHevcCsd(format: Format): Format {
    if (format.sampleMimeType != MimeTypes.VIDEO_H265 || format.initializationData.size < 2) return format
    val joined = format.initializationData.fold(ByteArray(0)) { acc, nal -> acc + withStartCode(nal) }
    return format.buildUpon().setInitializationData(listOf(joined)).build()
}

private val START_CODE = byteArrayOf(0, 0, 0, 1)

private fun withStartCode(nal: ByteArray): ByteArray {
    val hasStartCode = (nal.size >= 4 && nal[0] == 0.toByte() && nal[1] == 0.toByte() && nal[2] == 0.toByte() && nal[3] == 1.toByte()) ||
        (nal.size >= 3 && nal[0] == 0.toByte() && nal[1] == 0.toByte() && nal[2] == 1.toByte())
    return if (hasStartCode) nal else START_CODE + nal
}

/** Renderer de vídeo padrão do Media3, com o formato H.265 corrigido por [withSingleHevcCsd]. */
@OptIn(UnstableApi::class)
private class HevcCsdVideoRenderer(builder: Builder) : MediaCodecVideoRenderer(builder) {
    override fun onInputFormatChanged(formatHolder: FormatHolder): DecoderReuseEvaluation? {
        formatHolder.format = formatHolder.format?.let(::withSingleHevcCsd)
        return super.onInputFormatChanged(formatHolder)
    }
}

/**
 * Fábrica de renderers do app: a padrão do Media3, com o renderer de vídeo
 * trocado por [HevcCsdVideoRenderer] (mesma configuração do
 * `DefaultRenderersFactory` 1.11). Sem renderers de extensão.
 */
@OptIn(UnstableApi::class)
internal class AppRenderersFactory(context: Context) : DefaultRenderersFactory(context) {
    override fun buildVideoRenderers(
        context: Context,
        extensionRendererMode: Int,
        mediaCodecSelector: MediaCodecSelector,
        enableDecoderFallback: Boolean,
        eventHandler: Handler,
        eventListener: VideoRendererEventListener,
        allowedVideoJoiningTimeMs: Long,
        out: ArrayList<Renderer>,
    ) {
        val builder = MediaCodecVideoRenderer.Builder(context)
            .setCodecAdapterFactory(codecAdapterFactory)
            .setMediaCodecSelector(mediaCodecSelector)
            .setAllowedJoiningTimeMs(allowedVideoJoiningTimeMs)
            .setEnableDecoderFallback(enableDecoderFallback)
            .setEventHandler(eventHandler)
            .setEventListener(eventListener)
            .setMaxDroppedFramesToNotify(MAX_DROPPED_FRAMES_TO_NOTIFY)
        out.add(HevcCsdVideoRenderer(builder))
    }

    private companion object {
        /** Mesmo valor do `DefaultRenderersFactory`. */
        const val MAX_DROPPED_FRAMES_TO_NOTIFY = 50
    }
}
