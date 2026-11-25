package rtspviewer.app.player

import android.content.Context
import android.media.MediaCodecList
import android.os.Build
import android.view.Surface
import android.view.SurfaceView
import androidx.annotation.OptIn
import androidx.media3.common.C
import androidx.media3.common.Format
import androidx.media3.common.MediaItem
import androidx.media3.common.PlaybackException
import androidx.media3.common.Player
import androidx.media3.common.Tracks
import androidx.media3.common.util.UnstableApi
import androidx.media3.exoplayer.DecoderReuseEvaluation
import androidx.media3.exoplayer.DefaultLoadControl
import androidx.media3.exoplayer.DefaultRenderersFactory
import androidx.media3.exoplayer.ExoPlayer
import androidx.media3.exoplayer.analytics.AnalyticsListener
import androidx.media3.exoplayer.rtsp.RtspMediaSource
import rtspviewer.core.application.StreamError
import rtspviewer.core.domain.ConnectionUrl

/**
 * Motor de vídeo com Media3/ExoPlayer (design D3): RTSP sobre TCP, buffers
 * pequenos para baixa latência, `MediaCodec` por hardware com fallback para
 * software e vídeo direto numa `SurfaceView`. Usar só na thread principal.
 */
@OptIn(UnstableApi::class)
class Media3Engine(private val context: Context) : VideoEngine {
    private var player: ExoPlayer? = null
    private var surfaceView: SurfaceView? = null
    private var surface: Surface? = null

    /** Onde o vídeo é desenhado; pode mudar com a tela aberta (rotação, tela cheia). */
    fun setSurfaceView(view: SurfaceView?) {
        surfaceView = view
        surface = null
        player?.let { p -> if (view != null) p.setVideoSurfaceView(view) else p.clearVideoSurface() }
    }

    /** Alternativa à [SurfaceView] (testes). */
    fun setSurface(target: Surface?) {
        surface = target
        surfaceView = null
        player?.setVideoSurface(target)
    }

    override fun open(url: ConnectionUrl, listener: VideoEngine.Listener) {
        close()
        val p = ExoPlayer.Builder(context, renderersFactory())
            .setLoadControl(loadControl())
            .build()
        surfaceView?.let(p::setVideoSurfaceView) ?: surface?.let(p::setVideoSurface)
        p.addListener(PlayerEvents(p, listener))
        p.addAnalyticsListener(DecoderEvents(listener))
        val source = RtspMediaSource.Factory()
            .setForceUseRtpTcp(true)
            .setTimeoutMs(TIMEOUT_MS)
            .createMediaSource(MediaItem.fromUri(url.expose()))
        p.setMediaSource(source)
        p.playWhenReady = true
        p.prepare()
        player = p
    }

    override fun close() {
        val p = player ?: return
        player = null
        p.release()
    }

    private fun renderersFactory() = DefaultRenderersFactory(context)
        .setEnableDecoderFallback(true)
        .setExtensionRendererMode(DefaultRenderersFactory.EXTENSION_RENDERER_MODE_OFF)

    private fun loadControl() = DefaultLoadControl.Builder()
        .setBufferDurationsMs(MIN_BUFFER_MS, MAX_BUFFER_MS, PLAYBACK_BUFFER_MS, REBUFFER_MS)
        .setPrioritizeTimeOverSizeThresholds(true)
        .build()

    private class PlayerEvents(private val player: ExoPlayer, private val listener: VideoEngine.Listener) : Player.Listener {
        private var failed = false

        override fun onIsPlayingChanged(isPlaying: Boolean) {
            if (isPlaying && !failed) listener.onPlaying()
        }

        override fun onPlaybackStateChanged(playbackState: Int) {
            // Stream ao vivo que "termina" é queda da câmera.
            if (playbackState == Player.STATE_ENDED) fail(StreamError.Unreachable("a câmera encerrou o stream"))
        }

        override fun onPlayerError(error: PlaybackException) {
            fail(mapPlaybackError(error))
        }

        override fun onTracksChanged(tracks: Tracks) {
            if (tracks.isEmpty) return
            val video = tracks.groups.filter { it.type == C.TRACK_TYPE_VIDEO }
            when {
                video.isEmpty() -> fail(StreamError.NoVideo())
                video.none { it.isSupported } -> {
                    val mime = video.first().getTrackFormat(0).sampleMimeType
                    fail(StreamError.UnsupportedCodec(codecName(mime)))
                }
            }
        }

        private fun fail(error: StreamError) {
            if (failed) return
            failed = true
            player.playWhenReady = false
            listener.onFailure(error)
        }
    }

    private class DecoderEvents(private val listener: VideoEngine.Listener) : AnalyticsListener {
        private var decoder: String? = null
        private var format: Format? = null

        override fun onVideoDecoderInitialized(
            eventTime: AnalyticsListener.EventTime,
            decoderName: String,
            initializedTimestampMs: Long,
            initializationDurationMs: Long,
        ) {
            decoder = decoderName
            publish()
        }

        override fun onVideoInputFormatChanged(
            eventTime: AnalyticsListener.EventTime,
            format: Format,
            decoderReuseEvaluation: DecoderReuseEvaluation?,
        ) {
            this.format = format
            publish()
        }

        private fun publish() {
            val name = decoder ?: return
            val f = format ?: return
            listener.onInfo(StreamInfo(codecName(f.sampleMimeType), f.width, f.height, name, isHardware(name)))
        }
    }

    companion object {
        // Buffers pequenos: a imagem aparece logo e a latência fica baixa (D3).
        const val MIN_BUFFER_MS = 250
        const val MAX_BUFFER_MS = 1_000
        const val PLAYBACK_BUFFER_MS = 100
        const val REBUFFER_MS = 250
        const val TIMEOUT_MS = 5_000L

        /** Decoder por hardware? API 29+ pergunta ao sistema; antes, pelo nome. */
        fun isHardware(decoderName: String): Boolean {
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
                MediaCodecList(MediaCodecList.ALL_CODECS).codecInfos
                    .firstOrNull { it.name == decoderName }
                    ?.let { return it.isHardwareAccelerated }
            }
            val n = decoderName.lowercase()
            return !(n.startsWith("omx.google.") || n.startsWith("c2.android.") || n.contains(".sw."))
        }
    }
}
