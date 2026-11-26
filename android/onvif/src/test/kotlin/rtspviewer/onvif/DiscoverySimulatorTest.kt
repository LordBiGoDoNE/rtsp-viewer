package rtspviewer.onvif

import java.net.DatagramSocket
import java.net.Inet4Address
import java.net.InetAddress
import java.net.InetSocketAddress
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertTrue
import rtspviewer.onvif.support.DiscoveryReply
import rtspviewer.onvif.support.DiscoveryResponder

/**
 * Busca WS-Discovery contra o respondedor do simulador, por unicast em
 * loopback (mesmos casos de `desktop/crates/onvif/tests/discovery.rs`; spec
 * `camera-discovery`, requisito "Busca de câmeras na rede").
 */
class DiscoverySimulatorTest {
    private val loopback = InetAddress.getByAddress(byteArrayOf(127, 0, 0, 1)) as Inet4Address

    private fun config(port: Int, unicast: List<InetSocketAddress>? = null) = DiscoveryConfig(
        interfaces = listOf(Ipv4Interface(loopback, null)),
        durationMs = 800,
        target = InetSocketAddress(loopback, port),
        unicast = unicast,
    )

    private val tapo = DiscoveryReply.Match(
        endpoint = "urn:uuid:tapo-1",
        xaddr = "http://{ip}:2020/onvif/device_service",
        scopes = "onvif://www.onvif.org/name/TP-IPC onvif://www.onvif.org/hardware/C200",
    )

    private fun responder(vararg replies: DiscoveryReply) = DiscoveryResponder("127.0.0.1", replies.toList())

    private fun <T> timed(limitMs: Long, block: () -> T): T {
        val start = System.nanoTime()
        val r = block()
        val elapsed = (System.nanoTime() - start) / 1_000_000
        assertTrue(elapsed < limitMs, "$elapsed ms")
        return r
    }

    @Test
    fun encontraCameraESugereNome() {
        responder(tapo).use { r ->
            val found = Discovery.discover(config(r.port))
            assertEquals(1, found.size)
            assertEquals("http://127.0.0.1:2020/onvif/device_service", found[0].deviceUrl)
            assertEquals("TP-IPC", found[0].name)
            assertEquals("127.0.0.1", found[0].host)
        }
    }

    @Test
    fun deduplicaEIgnoraRespostasInvalidas() {
        responder(tapo, DiscoveryReply.Malformed, tapo, DiscoveryReply.WrongRelatesTo).use { r ->
            val found = Discovery.discover(config(r.port))
            assertEquals(1, found.size, "$found")
            assertEquals("urn:uuid:tapo-1", found[0].endpoint)
        }
    }

    @Test
    fun variasCameras() {
        val b = DiscoveryReply.Match("urn:b", "http://{ip}:8000/onvif/device_service", "onvif://www.onvif.org/name/B")
        responder(b, tapo).use { r -> assertEquals(2, Discovery.discover(config(r.port)).size) }
    }

    @Test
    fun floodDeDispositivosFicaLimitadoENoPrazo() {
        responder(DiscoveryReply.Flood(Discovery.MAX_DEVICES + 200)).use { r ->
            val found = timed(1_500) { Discovery.discover(config(r.port)) }
            // O sistema pode descartar datagramas no loopback; o que importa é o teto.
            assertTrue(found.isNotEmpty())
            assertTrue(found.size <= Discovery.MAX_DEVICES, "${found.size}")
        }
    }

    @Test
    fun floodDeLixoNaoImpedeABusca() {
        responder(tapo, DiscoveryReply.Junk(200)).use { r ->
            val found = timed(1_500) { Discovery.discover(config(r.port)) }
            assertEquals(1, found.size, "$found")
            assertEquals("urn:uuid:tapo-1", found[0].endpoint)
        }
    }

    @Test
    fun semRespostasTerminaNoPrazo() {
        // Porta sem ninguém respondendo: a busca termina vazia, no prazo, sem erro.
        DatagramSocket(InetSocketAddress(loopback, 0)).use { silent ->
            assertTrue(timed(1_500) { Discovery.discover(config(silent.localPort)) }.isEmpty())
        }
    }

    /**
     * Câmera que ignora o multicast (como a Tapo C320WS) e só responde ao
     * Probe unicast: o destino "multicast" é uma porta muda e a câmera só é
     * alcançada pela varredura unicast, entre 200 destinos sem ninguém.
     */
    @Test
    fun encontraCameraQueSoRespondeAUnicast() {
        DatagramSocket(InetSocketAddress(loopback, 0)).use { mudo ->
            responder(tapo).use { camera ->
                val vazios = (0 until 200).map { DatagramSocket(InetSocketAddress(loopback, 0)) }
                val alvos = vazios.map { InetSocketAddress(loopback, it.localPort) }.toMutableList()
                vazios.forEach { it.close() }
                alvos.add(100, InetSocketAddress(loopback, camera.port))
                val found = timed(1_500) { Discovery.discover(config(mudo.localPort, alvos)) }
                assertEquals(1, found.size, "$found")
                assertEquals("urn:uuid:tapo-1", found[0].endpoint)
            }
        }
    }

    /** A mesma câmera responde ao multicast e ao unicast: uma única entrada. */
    @Test
    fun multicastEUnicastDaMesmaCameraDeduplicam() {
        responder(tapo).use { camera ->
            val found = Discovery.discover(config(camera.port, listOf(InetSocketAddress(loopback, camera.port))))
            assertEquals(1, found.size, "$found")
        }
    }

    @Test
    fun semInterfacesEErro() {
        assertFailsWith<DiscoveryException.NoInterface> {
            Discovery.discover(DiscoveryConfig(interfaces = emptyList()))
        }
    }

    /** Interromper a thread encerra a busca antes do prazo. */
    @Test
    fun interrupcaoEncerraABusca() {
        DatagramSocket(InetSocketAddress(loopback, 0)).use { silent ->
            var error: Throwable? = null
            val t = Thread {
                try {
                    Discovery.discover(config(silent.localPort).let { DiscoveryConfig(it.interfaces, 10_000, it.target) })
                } catch (e: Throwable) {
                    error = e
                }
            }
            val start = System.nanoTime()
            t.start()
            Thread.sleep(200)
            t.interrupt()
            t.join(2_000)
            assertTrue(!t.isAlive && (System.nanoTime() - start) / 1_000_000 < 2_000)
            assertTrue(error is InterruptedException, "$error")
        }
    }
}
