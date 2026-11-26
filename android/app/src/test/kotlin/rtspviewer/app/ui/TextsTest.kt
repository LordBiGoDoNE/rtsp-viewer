package rtspviewer.app.ui

import kotlin.test.assertEquals
import org.junit.jupiter.api.Test
import rtspviewer.app.player.PlayerState
import rtspviewer.app.player.StreamInfo
import rtspviewer.core.application.CatalogError
import rtspviewer.core.application.Field
import rtspviewer.core.application.FieldError
import rtspviewer.core.application.ResolveError
import rtspviewer.core.application.StreamError
import rtspviewer.core.domain.CameraName
import rtspviewer.core.domain.DeviceAddress

/** Mesmos casos de `presentation.rs` do desktop. */
class TextsTest {
    @Test
    fun textosDosEstados() {
        val v = ViewerState()
        assertEquals(ViewStatus.Message("Conectando…"), Texts.status(v, PlayerState.Idle))
        assertEquals(ViewStatus.Message("Conectando…"), Texts.status(v, PlayerState.Connecting))
        assertEquals(ViewStatus.None, Texts.status(v, PlayerState.Playing))
        assertEquals(ViewStatus.Message("Reconectando (tentativa 3)…"), Texts.status(v, PlayerState.Reconnecting(3, 4_000)))
        assertEquals(
            ViewStatus.Failed("Formato de vídeo não suportado: mjpeg"),
            Texts.status(v, PlayerState.Failed(StreamError.UnsupportedCodec("mjpeg"))),
        )
        assertEquals(ViewStatus.None, Texts.status(v.copy(passwordUser = "admin"), PlayerState.Idle))
        assertEquals(ViewStatus.Failed("Câmera não encontrada."), Texts.status(v.copy(error = "Câmera não encontrada."), PlayerState.Idle))
    }

    @Test
    fun erroNuncaMostraSenha() {
        val failed = PlayerState.Failed(StreamError.Other("rtsp://admin:segredo@192.168.0.10/s recusou"))
        assertEquals(ViewStatus.Failed("Falha no stream: rtsp://***@192.168.0.10/s recusou"), Texts.status(ViewerState(), failed))
    }

    @Test
    fun informacoesDaCamera() {
        val info = StreamInfo("video/avc", 1920, 1080, "c2.qti.avc.decoder", hardware = true)
        assertEquals(
            listOf(
                "Codec: video/avc",
                "Resolução: 1920x1080",
                "Decodificação: hardware (c2.qti.avc.decoder)",
                "Endereço: rtsp://192.168.0.10/s",
            ),
            Texts.info(info, "rtsp://192.168.0.10/s"),
        )
        assertEquals(listOf("Endereço: rtsp://192.168.0.10/s"), Texts.info(null, "rtsp://192.168.0.10/s"))
    }

    @Test
    fun errosVaoParaOCampoCerto() {
        val e = CatalogError.Invalid(listOf(FieldError(Field.Name, "Informe um nome."), FieldError(Field.Url, "URL ruim")))
        assertEquals(Triple("Informe um nome.", "URL ruim", null), Texts.formErrors(e))
        val (_, _, general) = Texts.formErrors(CatalogError.Storage("disco cheio"))
        assertEquals("Não foi possível salvar a lista de câmeras: disco cheio", general)
    }

    @Test
    fun textosDaBuscaEDaConsulta() {
        assertEquals("TP-Link C200", Texts.foundName(" TP-Link C200 "))
        assertEquals("Câmera ONVIF", Texts.foundName("  "))
        assertEquals("Câmera ONVIF", Texts.foundName(null))
        assertEquals("Câmera 192.168.1.164", Texts.fallbackName(DeviceAddress.parse("192.168.1.164")))
        val long = "a".repeat(80) + ".local"
        val name = Texts.fallbackName(DeviceAddress.parse(long))
        assertEquals(CameraName.MAX, name.codePointCount(0, name.length))
        assertEquals("", Texts.vendorReason(ResolveError.NotOnvif()))
        assertEquals("Motivo: A câmera não respondeu: tempo esgotado", Texts.vendorReason(ResolveError.Unreachable("tempo esgotado")))
        assertEquals("Não foi possível procurar câmeras: sem interface", Texts.discoveryError("sem interface"))
    }

    @Test
    fun dicasELicencasDoAndroid() {
        assertEquals("• o celular está conectado ao Wi-Fi da mesma rede da câmera.", DISCOVERY_HINTS.last())
        assertEquals(false, DISCOVERY_HINTS.any { "Firewall" in it }, "sem a dica do firewall do Windows")
        assertEquals(false, THIRD_PARTY_LICENSES.any { "Slint" in it })
        assertEquals(4, THIRD_PARTY_LICENSES.count { it.endsWith("Apache-2.0") })
    }
}
