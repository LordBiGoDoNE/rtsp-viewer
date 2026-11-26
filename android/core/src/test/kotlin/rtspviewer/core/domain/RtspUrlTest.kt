package rtspviewer.core.domain

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertFalse
import kotlin.test.assertNull
import kotlin.test.assertTrue

/** Mesmos casos de `desktop/crates/core/src/domain/rtsp_url.rs`. */
class RtspUrlTest {
    private fun ok(s: String) = RtspUrl.parse(s)

    private fun err(s: String) = assertFailsWith<InvalidUrlException>(s) { RtspUrl.parse(s) }.error

    @Test
    fun aceitaUrlSimples() {
        val p = ok("rtsp://192.168.0.10:554/stream1")
        assertEquals("rtsp://192.168.0.10:554/stream1", p.url.toString())
        assertEquals("192.168.0.10", p.url.host)
        assertEquals(554, p.url.port)
        assertNull(p.username)
        assertNull(p.password)
    }

    @Test
    fun aceitaRtspsHostSemPortaESemCaminho() {
        assertEquals("rtsps://camera.local", ok("rtsps://Camera.Local").url.toString())
        assertEquals("rtsp://cam/live?ch=1", ok("  RTSP://cam/live?ch=1  ").url.toString())
    }

    @Test
    fun aceitaIpv6() {
        val p = ok("rtsp://[fe80::1%25eth0]:8554/a")
        assertEquals("fe80::1%25eth0", p.url.host)
        assertEquals("rtsp://[fe80::1%25eth0]:8554/a", p.url.toString())
    }

    @Test
    fun extraiCredenciaisEmbutidas() {
        val p = ok("rtsp://admin:segredo@192.168.0.10/stream1")
        assertEquals("rtsp://192.168.0.10/stream1", p.url.toString())
        assertEquals("admin", p.username)
        assertEquals("segredo", p.password?.expose())
    }

    @Test
    fun decodificaCredenciaisEAceitaArrobaNaSenha() {
        val p = ok("rtsp://user%40x:p%3Aw@d@host/s")
        assertEquals("user@x", p.username)
        assertEquals("p:w@d", p.password!!.expose())
    }

    @Test
    fun usuarioSemSenha() {
        val p = ok("rtsp://admin@host/s")
        assertEquals("admin", p.username)
        assertNull(p.password)
    }

    @Test
    fun descartaFragmento() {
        assertEquals("rtsp://h/s", ok("rtsp://h/s#x").url.toString())
    }

    @Test
    fun rejeitaEntradasInvalidas() {
        val casos = listOf(
            "" to UrlError.Empty,
            "   " to UrlError.Empty,
            "http://192.168.0.10/stream" to UrlError.InvalidScheme,
            "192.168.0.10/stream" to UrlError.InvalidScheme,
            "rtsp://" to UrlError.InvalidHost,
            "rtsp:///s" to UrlError.InvalidHost,
            "rtsp://user@/s" to UrlError.InvalidHost,
            "rtsp://ho st/s" to UrlError.InvalidCharacters,
            "rtsp://h/a b" to UrlError.InvalidCharacters,
            "rtsp://h:0/s" to UrlError.InvalidPort,
            "rtsp://h:70000/s" to UrlError.InvalidPort,
            "rtsp://h:+5/s" to UrlError.InvalidPort,
            "rtsp://h:/s" to UrlError.InvalidPort,
            "rtsp://a:b:c/s" to UrlError.InvalidPort,
            "rtsp://[::1/s" to UrlError.InvalidHost,
            "rtsp://[abc]/s" to UrlError.InvalidHost,
            "rtsp://[::1]x/s" to UrlError.InvalidPort,
            "rtsp://u:%zz@h/s" to UrlError.InvalidCredentials,
            "rtsp://u:%ff@h/s" to UrlError.InvalidCredentials,
            "rtsp://admin:1234/abc@10.0.0.2/s" to UrlError.UnencodedCredentials,
            "rtsp://admin:1234?abc@host/s" to UrlError.UnencodedCredentials,
            "rtsp://admin:12345#x@192.168.0.10/stream" to UrlError.UnencodedCredentials,
            "rtsp://admin:x@y/z@host/s" to UrlError.UnencodedCredentials,
        )
        for ((entrada, erro) in casos) assertEquals(erro, err(entrada), "entrada: $entrada")
    }

    @Test
    fun recusaArrobaForaDaAutoridade() {
        for (
            entrada in listOf(
                "rtsp://admin:1234/abc@10.0.0.2/s",
                "rtsp://admin:1234?abc@host/s",
                "rtsp://admin:12345#x@192.168.0.10/stream",
                "rtsp://admin:x@y/z@host/s",
                "rtsp://admin:a/b?c#d@host/s",
                "rtsp://host/s?u=a@b",
            )
        ) {
            assertEquals(UrlError.UnencodedCredentials, err(entrada), "entrada: $entrada")
        }
        val msg = UrlError.UnencodedCredentials.message
        assertTrue(msg.contains("campo Senha"))
        assertFalse(msg.contains("1234"))
    }

    @Test
    fun senhaCodificadaComCaracteresReservadosContinuaAceita() {
        val p = ok("rtsp://admin:1234%2Fa%3Fb%23c%40d%3Ae@10.0.0.2/s")
        assertEquals("rtsp://10.0.0.2/s", p.url.toString())
        assertEquals("1234/a?b#c@d:e", p.password!!.expose())
    }

    @Test
    fun montaUrlDeConexaoCodificada() {
        val p = ok("rtsp://h:554/s")
        val c = p.url.withCredentials("ad min", Password("p:w@d/x"))
        assertEquals("rtsp://ad%20min:p%3Aw%40d%2Fx@h:554/s", c.expose())
        val idaEVolta = RtspUrl.parse(c.expose())
        assertEquals("ad min", idaEVolta.username)
        assertEquals("p:w@d/x", idaEVolta.password!!.expose())
    }

    @Test
    fun urlDeConexaoSemCredenciais() {
        assertEquals("rtsp://h/s", ok("rtsp://h/s").url.withCredentials(null, null).expose())
    }

    @Test
    fun senhaSemUsuarioEIgnorada() {
        assertEquals("rtsp://h/s", ok("rtsp://h/s").url.withCredentials(null, Password("antiga")).expose())
    }

    @Test
    fun caracteresNaoAsciiNaSenhaFazemIdaEVolta() {
        val c = ok("rtsp://h/s").url.withCredentials("joão", Password("ção€"))
        val p = RtspUrl.parse(c.expose())
        assertEquals("joão", p.username)
        assertEquals("ção€", p.password!!.expose())
    }
}
