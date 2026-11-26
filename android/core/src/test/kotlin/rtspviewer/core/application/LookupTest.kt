package rtspviewer.core.application

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertIs
import kotlin.test.assertSame
import rtspviewer.core.domain.DeviceAddress
import rtspviewer.core.domain.InvalidAddressException
import rtspviewer.core.domain.Password
import rtspviewer.core.domain.RtspUrl
import rtspviewer.core.domain.Vendor

/** Mesmos casos de `lookup.rs`. */
class LookupTest {
    private class FakeResolver(private val result: () -> ResolvedStreams) : StreamResolver {
        val seen = mutableListOf<Pair<String?, Boolean>>()

        override fun resolve(target: StreamTarget, username: String?, password: Password?): ResolvedStreams {
            seen += username to (password != null)
            return result()
        }
    }

    private fun streams() = ResolvedStreams(
        RtspUrl.parse("rtsp://192.168.1.164:554/stream1").url,
        RtspUrl.parse("rtsp://192.168.1.164:554/stream2").url,
        "TP-Link C200",
    )

    private val target = StreamTarget.Address(DeviceAddress.parse("192.168.1.164"))

    @Test
    fun classificaUrlEEndereco() {
        assertSame(AddressInput.Rtsp, classifyAddress("rtsp://h/s"))
        assertSame(AddressInput.Rtsp, classifyAddress(" RTSPS://h/s"))
        assertIs<AddressInput.Device>(classifyAddress("192.168.1.164"))
        assertFailsWith<InvalidAddressException> { classifyAddress("http://x") }
        assertFailsWith<InvalidAddressException> { classifyAddress("192.168.1.164:abc") }
    }

    @Test
    fun onvifOk() {
        val f = FakeResolver(::streams)
        assertEquals(LookupOutcome.Resolved(streams()), lookup(f, target, " admin ", Password("x")))
        assertEquals("admin" to true, f.seen[0])
    }

    @Test
    fun autenticacaoRecusadaInterrompe() {
        val f = FakeResolver { throw ResolveError.AuthenticationFailed() }
        assertSame(LookupOutcome.AuthenticationFailed, lookup(f, target, "admin", Password("x")))
    }

    @Test
    fun semOnvifOfereceFabricante() {
        for (
            reason in listOf(
                ResolveError.NotOnvif(),
                ResolveError.NoVideoProfiles(),
                ResolveError.Unreachable("tempo"),
                ResolveError.Protocol("x"),
            )
        ) {
            val out = lookup(FakeResolver { throw reason }, target, null, null)
            assertEquals(LookupOutcome.NeedsVendor(reason), out)
        }
    }

    @Test
    fun senhaSemUsuarioNaoEEnviada() {
        val f = FakeResolver(::streams)
        lookup(f, target, "  ", Password("x"))
        assertEquals(null to false, f.seen[0])
    }

    @Test
    fun fabricanteMontaUrls() {
        val (m, s) = vendorStreams(Vendor.Hikvision, DeviceAddress.parse("192.168.1.164"))
        assertEquals("rtsp://192.168.1.164:554/Streaming/Channels/101", m.toString())
        assertEquals("rtsp://192.168.1.164:554/Streaming/Channels/102", s.toString())
    }

    @Test
    fun mensagensEmPortugues() {
        assertEquals("Usuário ou senha inválidos.", StreamError.AuthenticationFailed().message)
        assertEquals("Formato de vídeo não suportado: mjpeg", StreamError.UnsupportedCodec("mjpeg").message)
        assertEquals("A câmera não respondeu ao ONVIF.", ResolveError.NotOnvif().message)
    }

    @Test
    fun soErrosTransitoriosReconectam() {
        assertEquals(true, StreamError.Unreachable("x").isRetryable)
        assertEquals(true, StreamError.Other("x").isRetryable)
        assertEquals(false, StreamError.AuthenticationFailed().isRetryable)
        assertEquals(false, StreamError.UnsupportedCodec("mjpeg").isRetryable)
        assertEquals(false, StreamError.NoVideo().isRetryable)
    }
}
