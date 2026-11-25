package rtspviewer.core.domain

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertFalse
import kotlin.test.assertNotEquals
import kotlin.test.assertNull
import kotlin.test.assertTrue

private fun url(s: String) = RtspUrl.parse(s).url

/** Mesmos casos de `camera.rs`, `device_address.rs`, `vendor.rs`, `secret.rs` e `reconnect.rs`. */
class CameraTest {
    @Test
    fun nomeValidoEAparado() {
        assertEquals("Garagem", CameraName.parse("  Garagem ").value)
        assertEquals(60, CameraName.parse("á".repeat(60)).value.length)
    }

    @Test
    fun nomeInvalido() {
        assertEquals(NameError.Empty, assertFailsWith<InvalidNameException> { CameraName.parse("") }.error)
        assertEquals(NameError.Empty, assertFailsWith<InvalidNameException> { CameraName.parse(" \t ") }.error)
        assertEquals(NameError.TooLong, assertFailsWith<InvalidNameException> { CameraName.parse("x".repeat(61)) }.error)
        // 60 emojis (pares substitutos em UTF-16) ainda são 60 caracteres.
        assertEquals(60, CameraName.parse("📷".repeat(60)).value.codePointCount(0, 120))
    }

    @Test
    fun idsSaoUnicosEPersistiveis() {
        val a = CameraId.generate()
        val b = CameraId.generate()
        assertNotEquals(a, b)
        assertEquals(a, CameraId.parse(a.value))
        assertNull(CameraId.parse("nao-e-uuid"))
    }

    @Test
    fun edicaoPreservaIdentidade() {
        val c = Camera.create(CameraName.parse("A"), url("rtsp://h/s"), null)
        val id = c.id
        c.rename(CameraName.parse("B"))
        c.changeUrl(url("rtsp://h2/s"))
        c.changeUsername("  admin ")
        assertEquals(id, c.id)
        assertEquals("B", c.name.value)
        assertEquals("rtsp://h2/s", c.url.toString())
        assertEquals("admin", c.username)
        assertTrue(c.requiresCredentials())
    }

    private fun camComSecundario(): Camera =
        Camera.create(CameraName.parse("A"), url("rtsp://192.168.1.164:554/stream1"), null).also {
            it.changeSubstream(url("rtsp://192.168.1.164:554/stream2"))
        }

    @Test
    fun secundarioDeOutroHostERecusado() {
        val c = camComSecundario()
        assertFailsWith<SubstreamHostMismatchException> { c.changeSubstream(url("rtsp://192.168.1.121/stream2")) }
        assertEquals("rtsp://192.168.1.164:554/stream2", c.substream.toString())
        c.changeSubstream(null)
        assertNull(c.substream)
    }

    @Test
    fun hostEComparadoSemDiferenciarMaiusculas() {
        val c = Camera.create(CameraName.parse("A"), url("rtsp://Cam.local/s1"), null)
        c.changeSubstream(url("rtsp://cam.LOCAL/s2"))
    }

    @Test
    fun trocarUrlPrincipalDescartaSecundario() {
        val c = camComSecundario()
        c.changeUrl(url("rtsp://192.168.1.164:554/h264"))
        assertNull(c.substream)
        val d = camComSecundario()
        d.changeUrl(url("rtsp://192.168.1.121:554/stream1"))
        assertNull(d.substream)
    }

    @Test
    fun mesmaUrlPrincipalMantemSecundario() {
        val c = camComSecundario()
        c.changeUrl(url("rtsp://192.168.1.164:554/stream1"))
        assertTrue(c.substream != null)
    }

    @Test
    fun usuarioVazioViraAusente() {
        val c = Camera.create(CameraName.parse("A"), url("rtsp://h"), "  ")
        assertNull(c.username)
        assertFalse(c.requiresCredentials())
    }

    @Test
    fun copiaEIndependente() {
        val c = camComSecundario()
        val d = c.copy()
        d.rename(CameraName.parse("Outra"))
        assertEquals("A", c.name.value)
        assertEquals(c.id, d.id)
    }
}

class DeviceAddressTest {
    @Test
    fun aceitaIpNomeEPorta() {
        var a = DeviceAddress.parse(" 192.168.1.164 ")
        assertEquals("192.168.1.164" to null, a.host to a.port)
        a = DeviceAddress.parse("Cam-Portao.local:2020")
        assertEquals("cam-portao.local" to 2020, a.host to a.port)
        assertEquals("cam-portao.local:2020", a.toString())
        a = DeviceAddress.parse("[FE80::1]:80")
        assertEquals("fe80::1" to 80, a.host to a.port)
        assertEquals("[fe80::1]:80", a.toString())
    }

    @Test
    fun recusaFormatosInvalidos() {
        assertEquals(AddressError.Empty, assertFailsWith<InvalidAddressException> { DeviceAddress.parse("") }.error)
        for (s in listOf("http://x", "rtsp://h/s", "h/s", "user@h", "a b", "-h", "h.", "[zz]", "a:b:c")) {
            assertFailsWith<InvalidAddressException>(s) { DeviceAddress.parse(s) }
        }
        for (s in listOf("192.168.1.164:abc", "h:0", "h:70000", "h:", "[::1]x")) {
            assertEquals(AddressError.InvalidPort, assertFailsWith<InvalidAddressException>(s) { DeviceAddress.parse(s) }.error, s)
        }
    }

    @Test
    fun hostDeUrlDeServico() {
        assertEquals("192.168.1.164", hostOfServiceUrl("http://192.168.1.164:2020/onvif/device_service"))
        assertEquals("fe80::1", hostOfServiceUrl("http://[fe80::1]:80/x"))
        assertNull(hostOfServiceUrl("http://192.168.1.5@evil.com/x"))
        assertNull(hostOfServiceUrl("https://h/x"))
    }
}

class VendorTest {
    private fun addr(s: String) = DeviceAddress.parse(s)

    @Test
    fun hikvisionMontaPrincipalESecundario() {
        val (m, s) = Vendor.Hikvision.streamUrls(addr("192.168.1.164"))
        assertEquals("rtsp://192.168.1.164:554/Streaming/Channels/101", m.toString())
        assertEquals("rtsp://192.168.1.164:554/Streaming/Channels/102", s.toString())
    }

    @Test
    fun todosOsFabricantesGeramUrlsValidas() {
        for (v in Vendor.entries) {
            assertTrue(v.streamUrls(addr("[fe80::1]:8554")).first.toString().startsWith("rtsp://[fe80::1]:8554/"), v.label)
        }
    }

    @Test
    fun tapoEAxis() {
        val (m, s) = Vendor.Tapo.streamUrls(addr("cam.local"))
        assertEquals("rtsp://cam.local:554/stream1", m.toString())
        assertEquals("rtsp://cam.local:554/stream2", s.toString())
        assertNull(Vendor.Axis.streamUrls(addr("h")).second)
    }

    @Test
    fun ordemERotulosDaTela() {
        assertEquals(
            listOf("TP-Link Tapo", "Hikvision", "Dahua / Intelbras / Amcrest", "Reolink", "Uniview", "Axis"),
            Vendor.entries.map { it.label },
        )
    }
}

class SecretTest {
    @Test
    fun toStringDaSenhaEMascarado() {
        assertEquals("Password(***)", Password("segredo").toString())
    }

    @Test
    fun redigeCredenciaisEmTextoLivre() {
        val r = redactCredentials("falha ao abrir rtsp://admin:s3gr3do@10.0.0.2:554/stream1: 401")
        assertEquals("falha ao abrir rtsp://***@10.0.0.2:554/stream1: 401", r)
        assertFalse(r.contains("s3gr3do"))
    }

    @Test
    fun redigeVariasUrlsERtsps() {
        assertEquals(
            "a RTSP://***@h/x e rtsps://***@k:322/y?z",
            redactCredentials("a RTSP://u:p@h/x e rtsps://v:q@k:322/y?z"),
        )
    }

    @Test
    fun arrobaNaSenhaNaoVaza() {
        assertEquals("rtsp://***@host/s", redactCredentials("rtsp://admin:a@b@host/s"))
    }

    @Test
    fun senhaComBarraInterrogacaoOuCerquilhaNaoVaza() {
        val casos = listOf(
            "falha rtsp://admin:1234/abc@10.0.0.2/s: 401" to "falha rtsp://***@10.0.0.2/s: 401",
            "rtsp://admin:1234?abc@host/s" to "rtsp://***@host/s",
            "rtsp://admin:12345#x@192.168.0.10/stream e mais" to "rtsp://***@192.168.0.10/stream e mais",
            "rtsp://admin:x@y/z@host/s" to "rtsp://***@host/s",
        )
        for ((msg, esperado) in casos) assertEquals(esperado, redactCredentials(msg), msg)
    }

    @Test
    fun textoSemCredenciaisFicaIgual() {
        val msg = "rtsp://host:554/s e texto"
        assertEquals(msg, redactCredentials(msg))
    }

    @Test
    fun textoComCaracteresQueMudamDeTamanhoNaoDesalinha() {
        assertEquals("İ rtsp://***@h/s", redactCredentials("İ rtsp://u:p@h/s"))
    }

    @Test
    fun toStringDaUrlDeConexaoNaoMostraSenha() {
        val c = url("rtsp://h/s").withCredentials("a", Password("b"))
        assertEquals("ConnectionUrl(rtsp://***@h/s)", c.toString())
    }
}

class ReconnectPolicyTest {
    @Test
    fun esperaCresceAteOTeto() {
        val p = ReconnectPolicy()
        assertEquals(listOf(1L, 2, 4, 8, 16, 30, 30, 30), (0 until 8).map { p.nextDelay().second / 1000 })
    }

    @Test
    fun numeraAsTentativas() {
        val p = ReconnectPolicy()
        assertEquals(1, p.nextDelay().first)
        assertEquals(2, p.nextDelay().first)
    }

    @Test
    fun reproducaoEstavelReiniciaSequencia() {
        val p = ReconnectPolicy()
        p.nextDelay()
        p.nextDelay()
        p.recordPlayback(9_000)
        assertEquals(4_000, p.nextDelay().second)
        p.recordPlayback(10_000)
        assertEquals(1 to 1_000L, p.nextDelay())
    }
}
