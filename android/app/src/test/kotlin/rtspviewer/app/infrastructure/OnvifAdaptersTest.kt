package rtspviewer.app.infrastructure

import java.net.DatagramSocket
import java.net.Inet4Address
import java.net.InetAddress
import java.net.InetSocketAddress
import java.net.ServerSocket
import java.net.UnknownHostException
import java.util.concurrent.atomic.AtomicInteger
import kotlin.concurrent.thread
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertFalse
import kotlin.test.assertIs
import kotlin.test.assertNull
import kotlin.test.assertTrue
import rtspviewer.core.application.ResolveError
import rtspviewer.core.application.StreamTarget
import rtspviewer.core.domain.DeviceAddress
import rtspviewer.core.domain.Password
import rtspviewer.core.domain.RtspUrl
import rtspviewer.onvif.Ipv4Interface
import rtspviewer.onvif.LocalAddresses
import rtspviewer.onvif.OnvifClient
import rtspviewer.onvif.OnvifConfig
import rtspviewer.onvif.OnvifException
import rtspviewer.onvif.StreamProfile
import rtspviewer.onvif.StreamSet

class OnvifAdaptersTest {
    private fun ip(s: String): InetAddress = InetAddress.getByAddress(LocalAddresses.parseLiteral(s)!!)

    private val loopback = ip("127.0.0.1") as Inet4Address

    // ------------------------------------------------- redes da busca (spec camera-discovery, D4)

    @Test
    fun buscaUsaWifiEEthernetSemVpn() {
        assertTrue(isLocalSearchNetwork(wifi = true, ethernet = false, vpnTransport = false, notVpn = true))
        assertTrue(isLocalSearchNetwork(wifi = false, ethernet = true, vpnTransport = false, notVpn = true))
    }

    @Test
    fun buscaIgnoraVpnQueHerdaOTransporteWifi() {
        // VPN sobre Wi-Fi: o Android repassa TRANSPORT_WIFI da rede de baixo.
        assertFalse(isLocalSearchNetwork(wifi = true, ethernet = false, vpnTransport = true, notVpn = false))
        assertFalse(isLocalSearchNetwork(wifi = true, ethernet = false, vpnTransport = true, notVpn = true))
        assertFalse(isLocalSearchNetwork(wifi = false, ethernet = true, vpnTransport = false, notVpn = false))
    }

    @Test
    fun buscaIgnoraDadosMoveisEOutrasRedes() {
        assertFalse(isLocalSearchNetwork(wifi = false, ethernet = false, vpnTransport = false, notVpn = true))
    }

    // ------------------------------------------------- regra da rede local (ADR 0006)

    @Test
    fun ipLiteralDaRedeLocalEAceitoSemConsultaDns() {
        val resolver = LocalNetworkResolver { error("sem DNS para IP literal") }
        for (ok in listOf("192.168.1.10", "10.0.0.5", "172.16.0.1", "169.254.1.1", "127.0.0.1", "fe80::1", "fd12::1")) {
            assertEquals(ip(ok), resolver.resolve(ok), ok)
        }
    }

    @Test
    fun ipLiteralPublicoERecusado() {
        val resolver = LocalNetworkResolver { error("sem DNS para IP literal") }
        for (no in listOf("8.8.8.8", "203.0.113.7", "100.64.0.1", "2001:db8::1")) {
            val e = assertFailsWith<OnvifException.Unreachable>(no) { resolver.resolve(no) }
            assertTrue(e.detail.contains("rede local"), e.detail)
        }
    }

    @Test
    fun nomeConferidoDepoisDaResolucao() {
        val publico = LocalNetworkResolver { arrayOf(ip("203.0.113.7")) }
        val e = assertFailsWith<OnvifException.Unreachable> { publico.resolve("camera.example") }
        assertTrue(e.detail.startsWith("camera.example não é um endereço da rede local"), e.detail)
        // Nome com endereço público e privado: só o privado é usado.
        val misto = LocalNetworkResolver { arrayOf(ip("203.0.113.7"), ip("192.168.1.20")) }
        assertEquals(ip("192.168.1.20"), misto.resolve("camera.lan"))
        val desconhecido = LocalNetworkResolver { throw UnknownHostException(it) }
        assertFailsWith<OnvifException.Unreachable> { desconhecido.resolve("nada.lan") }
    }

    /** Nome que resolve para IP público: `ResolveError.Unreachable` com a mensagem, sem tentar conectar. */
    @Test
    fun consultaParaNomePublicoFalhaSemEnviar() {
        val chamadas = AtomicInteger()
        val client = OnvifClient(
            OnvifConfig(),
            LocalNetworkResolver {
                chamadas.incrementAndGet()
                arrayOf(ip("203.0.113.7"))
            },
        )
        val resolver = OnvifStreamResolver(client)
        val inicio = System.nanoTime()
        val e = assertFailsWith<ResolveError.Unreachable> {
            resolver.resolve(StreamTarget.Address(DeviceAddress.parse("camera.example")), "admin", Password("x"))
        }
        // Sem sondagem das portas (que levaria ~1 s por porta fechada).
        assertTrue((System.nanoTime() - inicio) / 1_000_000 < 500)
        assertEquals(1, chamadas.get())
        assertTrue(e.message!!.startsWith("A câmera não respondeu: camera.example não é um endereço da rede local"), e.message)
        val url = assertFailsWith<ResolveError.Unreachable> {
            resolver.resolve(StreamTarget.ServiceUrl("http://203.0.113.7/onvif/device_service"), null, null)
        }
        assertTrue(url.message!!.contains("rede local"))
    }

    /** O cliente conecta ao IP conferido (aqui, loopback) para um nome qualquer. */
    @Test
    fun conexaoVaiAoEnderecoConferido() {
        ServerSocket(0, 5, InetAddress.getLoopbackAddress()).use { server ->
            val aceitas = AtomicInteger()
            thread(isDaemon = true) {
                while (true) {
                    try {
                        server.accept().use { aceitas.incrementAndGet() }
                    } catch (_: Exception) {
                        return@thread
                    }
                }
            }
            val client = OnvifClient(OnvifConfig(totalTimeoutMs = 2_000), LocalNetworkResolver { arrayOf(loopback) })
            assertFailsWith<ResolveError> {
                OnvifStreamResolver(client)
                    .resolve(StreamTarget.Address(DeviceAddress.parse("camera.lan:${server.localPort}")), null, null)
            }
            assertTrue(aceitas.get() >= 1)
        }
    }

    @Test
    fun urlDeServicoInvalidaNaoEOnvif() {
        assertFailsWith<ResolveError.NotOnvif> {
            OnvifStreamResolver().resolve(StreamTarget.ServiceUrl("http://u@192.168.1.5/x"), null, null)
        }
    }

    // ------------------------------------------------- nome sugerido e erros (como o desktop)

    private fun set(manufacturer: String?, model: String?) = StreamSet(
        manufacturer, model, StreamProfile(RtspUrl.parse("rtsp://h/s").url, 1, 1, null), null,
    )

    @Test
    fun nomeSugerido() {
        assertEquals("TP-Link C200", OnvifStreamResolver.toResolved(set("TP-Link", "C200")).suggestedName)
        assertEquals("Hikvision DS-2CD", OnvifStreamResolver.toResolved(set("HIKVISION", "Hikvision DS-2CD")).suggestedName)
        assertEquals("C200", OnvifStreamResolver.toResolved(set(null, "C200")).suggestedName)
        assertNull(OnvifStreamResolver.toResolved(set(null, null)).suggestedName)
        assertEquals(60, OnvifStreamResolver.toResolved(set(null, "x".repeat(80))).suggestedName?.length)
    }

    @Test
    fun errosMapeados() {
        assertIs<ResolveError.AuthenticationFailed>(OnvifStreamResolver.mapError(OnvifException.AuthenticationFailed()))
        assertIs<ResolveError.NotOnvif>(OnvifStreamResolver.mapError(OnvifException.NotOnvif()))
        assertIs<ResolveError.NoVideoProfiles>(OnvifStreamResolver.mapError(OnvifException.NoVideoProfiles()))
        assertEquals("A câmera não respondeu: x", OnvifStreamResolver.mapError(OnvifException.Unreachable("x")).message)
        assertEquals("Resposta inesperada da câmera: y", OnvifStreamResolver.mapError(OnvifException.Protocol("y")).message)
    }

    // ------------------------------------------------- busca e trava de multicast

    private class FakeGuard : MulticastGuard {
        var acquired = 0
        var released = 0

        override fun acquire() {
            acquired++
        }

        override fun release() {
            released++
        }
    }

    @Test
    fun travaDeMulticastSoDuranteABusca() {
        DatagramSocket(InetSocketAddress(loopback, 0)).use { silent ->
            val guard = FakeGuard()
            val discovery = OnvifCameraDiscovery(
                { listOf(Ipv4Interface(loopback, null)) }, guard, durationMs = 300,
                target = InetSocketAddress(loopback, silent.localPort),
            )
            assertTrue(discovery.discover().isEmpty())
            assertEquals(1 to 1, guard.acquired to guard.released)
        }
    }

    @Test
    fun semWifiNemEthernetTerminaVaziaSemEnviar() {
        // Só dados móveis: nenhuma interface, nada enviado, nenhuma trava; a
        // tela mostra "Nenhuma câmera encontrada." com a dica do Wi-Fi.
        val guard = FakeGuard()
        assertTrue(OnvifCameraDiscovery({ emptyList() }, guard).discover().isEmpty())
        assertEquals(0 to 0, guard.acquired to guard.released)
    }

    @Test
    fun travaSoltaNoCancelamento() {
        DatagramSocket(InetSocketAddress(loopback, 0)).use { silent ->
            val guard = FakeGuard()
            val discovery = OnvifCameraDiscovery(
                { listOf(Ipv4Interface(loopback, null)) }, guard, durationMs = 10_000,
                target = InetSocketAddress(loopback, silent.localPort),
            )
            var erro: Throwable? = null
            val t = thread {
                try {
                    discovery.discover()
                } catch (e: Throwable) {
                    erro = e
                }
            }
            Thread.sleep(200)
            t.interrupt()
            t.join(2_000)
            assertFalse(t.isAlive)
            assertIs<InterruptedException>(erro)
            assertEquals(1 to 1, guard.acquired to guard.released)
        }
    }
}
