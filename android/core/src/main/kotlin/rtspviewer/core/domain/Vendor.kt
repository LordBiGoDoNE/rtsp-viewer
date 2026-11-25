package rtspviewer.core.domain

/**
 * Caminhos RTSP conhecidos por fabricante (design D5 da `descoberta-onvif`),
 * usados quando a câmera não responde ao ONVIF.
 */
enum class Vendor(val label: String, private val mainPath: String, private val subPath: String?) {
    Tapo("TP-Link Tapo", "/stream1", "/stream2"),
    Hikvision("Hikvision", "/Streaming/Channels/101", "/Streaming/Channels/102"),
    Dahua(
        "Dahua / Intelbras / Amcrest",
        "/cam/realmonitor?channel=1&subtype=0",
        "/cam/realmonitor?channel=1&subtype=1",
    ),
    Reolink("Reolink", "/h264Preview_01_main", "/h264Preview_01_sub"),
    Uniview("Uniview", "/media/video1", "/media/video2"),
    Axis("Axis", "/axis-media/media.amp", null),
    ;

    /**
     * URLs RTSP principal e secundária para o endereço. A porta do endereço,
     * se houver, é tratada como porta RTSP; senão vale 554.
     */
    fun streamUrls(address: DeviceAddress): Pair<RtspUrl, RtspUrl?> {
        val port = address.port ?: DEFAULT_RTSP_PORT
        fun build(path: String) = RtspUrl.parse("rtsp://${address.hostForUrl()}:$port$path").url
        return build(mainPath) to subPath?.let(::build)
    }

    companion object {
        /** Porta RTSP padrão quando o endereço não informa outra. */
        const val DEFAULT_RTSP_PORT = 554
    }
}
