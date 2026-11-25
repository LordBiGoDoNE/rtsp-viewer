package rtspviewer.onvif

import java.net.InetAddress
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertNotNull
import kotlin.test.assertNull

/** Mesmos casos dos testes de unidade de `client.rs` do desktop. */
class ClientRulesTest {
    private fun p(token: String, w: Int, h: Int) = ProfileInfo(token, w, h, null)

    @Test
    fun escolhePrincipalESecundario() {
        val (m, s) = assertNotNull(OnvifClient.choose(listOf(p("sub", 640, 360), p("main", 2560, 1440), p("mid", 1280, 720))))
        assertEquals("main" to "sub", m.token to s?.token)
    }

    @Test
    fun perfilUnicoOuIguaisSemSecundario() {
        assertNull(OnvifClient.choose(listOf(p("a", 1920, 1080)))?.second)
        val (m, s) = assertNotNull(OnvifClient.choose(listOf(p("a", 1920, 1080), p("b", 1920, 1080))))
        assertEquals("a", m.token)
        assertNull(s)
        assertNull(OnvifClient.choose(emptyList()))
    }

    @Test
    fun lePerfisEIgnoraOsSemVideo() {
        val body = Xml.parse(
            """<Body><GetProfilesResponse>
              <Profiles token="p1" fixed="true"><Name>main</Name><VideoEncoderConfiguration>
                <Encoding>H264</Encoding><Resolution><Width>1920</Width><Height>1080</Height></Resolution>
              </VideoEncoderConfiguration></Profiles>
              <Profiles token="audio"><Name>a</Name></Profiles>
              <Profiles token="p2"><VideoEncoderConfiguration><Encoding>H265</Encoding>
                <Resolution><Width>640</Width><Height>360</Height></Resolution></VideoEncoderConfiguration></Profiles>
            </GetProfilesResponse></Body>""",
        )
        val ps = OnvifClient.parseProfiles(body)
        assertEquals(2, ps.size)
        assertEquals(ProfileInfo("p1", 1920, 1080, "H264"), ps[0])
    }

    /** Amostras fictícias com prefixos de outros firmwares (`ns2:`/`ns3:`, sem prefixo). */
    @Test
    fun lePerfisComPrefixosDeOutrosFirmwares() {
        val ns = Xml.parse(
            """<SOAP-ENV:Envelope xmlns:SOAP-ENV="http://www.w3.org/2003/05/soap-envelope"><SOAP-ENV:Body>
              <ns2:GetProfilesResponse xmlns:ns2="http://www.onvif.org/ver10/media/wsdl" xmlns:ns3="http://www.onvif.org/ver10/schema">
              <ns2:Profiles fixed="true" token="Profile_1"><ns3:Name>mainStream</ns3:Name>
              <ns3:VideoEncoderConfiguration token="VideoEncoderToken_1"><ns3:Encoding>H264</ns3:Encoding>
              <ns3:Resolution><ns3:Width>2560</ns3:Width><ns3:Height>1440</ns3:Height></ns3:Resolution>
              </ns3:VideoEncoderConfiguration></ns2:Profiles>
              <ns2:Profiles fixed="true" token="Profile_2"><ns3:VideoEncoderConfiguration><ns3:Encoding>H265</ns3:Encoding>
              <ns3:Resolution><ns3:Width>704</ns3:Width><ns3:Height>576</ns3:Height></ns3:Resolution>
              </ns3:VideoEncoderConfiguration></ns2:Profiles></ns2:GetProfilesResponse></SOAP-ENV:Body></SOAP-ENV:Envelope>""",
        )
        val (m, s) = assertNotNull(OnvifClient.choose(OnvifClient.parseProfiles(ns)))
        assertEquals("Profile_1" to "Profile_2", m.token to s?.token)
        assertEquals("H265", s?.encoding)
    }

    @Test
    fun urlRtspComHostInvalidoOuCredenciais() {
        val s = { u: String, h: String -> OnvifClient.sanitizeStreamUrl(u, h).toString() }
        assertEquals("rtsp://192.168.1.164:554/stream1", s("rtsp://0.0.0.0:554/stream1", "192.168.1.164"))
        assertEquals("rtsp://192.168.1.164/s", s("rtsp://admin:segredo@192.168.1.164/s", "192.168.1.164"))
        assertEquals("rtsp://[fe80::1]/s", s("rtsp://localhost/s", "fe80::1"))
        assertFailsWith<OnvifException.Protocol> { OnvifClient.sanitizeStreamUrl("http://x/s", "h") }
    }

    @Test
    fun urlRtspFicaSempreNoHostDaCamera() {
        val s = { u: String, h: String -> OnvifClient.sanitizeStreamUrl(u, h).toString() }
        // Outro IP (NAT ou câmera maliciosa) ou nome DNS: host da câmera,
        // esquema, porta e caminho/consulta originais.
        assertEquals("rtsp://192.168.1.164:554/live?ch=1", s("rtsp://203.0.113.7:554/live?ch=1", "192.168.1.164"))
        assertEquals("rtsps://192.168.1.164/s", s("rtsps://camera.example/s", "192.168.1.164"))
        assertEquals("rtsp://192.168.1.164:8554", s("rtsp://10.0.0.9:8554", "192.168.1.164"))
        // IPv6 nos dois sentidos, com colchetes corretos.
        assertEquals("rtsp://192.168.1.164:554/s", s("rtsp://[2001:db8::9]:554/s", "192.168.1.164"))
        assertEquals("rtsp://[fe80::1]:554/s", s("rtsp://203.0.113.7:554/s", "fe80::1"))
        assertEquals("rtsp://[fe80::1]/s", s("rtsp://u:p@[2001:db8::9]/s", "fe80::1"))
        // Host anunciado parecido com o da câmera e repetido no caminho: só a
        // autoridade é trocada.
        assertEquals("rtsp://192.168.1.164/192.168.1.12", s("rtsp://192.168.1.12/192.168.1.12", "192.168.1.164"))
    }

    @Test
    fun servicosECaminhos() {
        val d = DeviceService.forHost("192.168.1.164", 2020)
        assertEquals("http://192.168.1.164:2020/onvif/device_service", d.url)
        assertEquals(d, DeviceService.fromUrl(d.url))
        assertEquals("http://[fe80::1]:80/onvif/device_service", DeviceService.forHost("fe80::1", 80).url)
        assertEquals("fe80::1", DeviceService.fromUrl("http://[fe80::1]:80/x")?.host)
        assertEquals("cam.lan", DeviceService.fromUrl("http://cam.lan?x")?.host)
        // `@` depois de `?` ou `#` já não é authority (RFC 3986).
        for (u in listOf("http://192.168.1.5?@evil.com/x", "http://192.168.1.5#@evil.com/x")) {
            assertEquals("192.168.1.5", DeviceService.fromUrl(u)?.host, u)
        }
        // Credenciais embutidas, host ou porta ilegíveis: a conexão poderia ir
        // a outro destino que não o `host` devolvido.
        for (bad in listOf(
            "http://192.168.1.5@evil.com/x",
            "http://user:pass@192.168.1.5/x",
            "http://[fe80::1]@evil.com/x",
            "http://[fe80::zz]/x",
            "http://[fe80::1]x/x",
            "http://192.168.1.5:99999/x",
            "http://192.168.1.5:0/x",
            "http://192.168.1.5:ab/x",
            "http://evil.com\\@192.168.1.5/x",
            "http:///x",
            "https://192.168.1.5/x",
        )) {
            assertNull(DeviceService.fromUrl(bad), bad)
        }
        assertEquals("/onvif/media_service", OnvifClient.pathOf("http://h:80/onvif/media_service"))
    }

    @Test
    fun servicoDeMidiaFicaSempreNoHostDaCamera() {
        val m = { x: String, h: String -> OnvifClient.mediaServiceUrl(x, h) }
        assertEquals("http://10.0.0.5:8000/onvif/media", m("http://0.0.0.0:8000/onvif/media", "10.0.0.5"))
        assertEquals("http://10.0.0.5:2020/onvif/service", m("http://10.0.0.5:2020/onvif/service", "10.0.0.5"))
        assertEquals("http://192.168.1.164:8080/onvif/Media", m("http://203.0.113.7:8080/onvif/Media", "192.168.1.164"))
        assertEquals("http://192.168.1.164:80/x?y=1", m("http://evil.example/x?y=1", "192.168.1.164"))
        assertEquals("http://[fe80::1]:81/media", m("http://u:p@[2001:db8::9]:81/media", "fe80::1"))
        assertEquals("http://10.0.0.5:80/", m("http://attacker.lan", "10.0.0.5"))
        // Não http, porta inválida ou caminho estranho: vale o dispositivo.
        assertNull(m("https://10.0.0.5/onvif/media", "10.0.0.5"))
        assertNull(m("http://10.0.0.5:99999/m", "10.0.0.5"))
        assertNull(m("http://10.0.0.5:0/m", "10.0.0.5"))
        assertNull(m("http://10.0.0.5/a b", "10.0.0.5"))
    }

    @Test
    fun conexaoVaiAoIpConferidoMantendoPortaECaminho() {
        val v4 = InetAddress.getByAddress(byteArrayOf(192.toByte(), 168.toByte(), 1, 9))
        assertEquals("http://192.168.1.9:2020/onvif/device_service", Http.connectUrl("http://camera.lan:2020/onvif/device_service", v4))
        assertEquals("http://192.168.1.9/", Http.connectUrl("http://camera.lan", v4))
        val v6 = InetAddress.getByAddress(LocalAddresses.parseIpv6("fe80::1"))
        assertEquals("http://[fe80:0:0:0:0:0:0:1]:80/x", Http.connectUrl("http://[fe80::1]:80/x", v6))
        assertNull(Http.connectUrl("https://h/x", v4))
        assertNull(Http.connectUrl("http://u@h/x", v4))
    }
}
