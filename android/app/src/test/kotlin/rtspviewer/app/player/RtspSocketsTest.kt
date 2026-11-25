package rtspviewer.app.player

import java.net.InetAddress
import java.net.ServerSocket
import java.net.SocketException
import kotlin.concurrent.thread
import kotlin.test.Test
import kotlin.test.assertFailsWith
import kotlin.test.assertFalse
import kotlin.test.assertTrue

class RtspSocketsTest {
    @Test
    fun conectaEFechaTudo() {
        ServerSocket(0, 1, InetAddress.getLoopbackAddress()).use { server ->
            val sockets = RtspSockets(1_000)
            val s = sockets.createSocket(server.inetAddress.hostAddress!!, server.localPort)
            assertTrue(s.isConnected)
            sockets.abortPending()
            assertFalse(s.isClosed, "conexão estabelecida não é abortada")
            sockets.closeAll()
            assertTrue(s.isClosed)
        }
    }

    @Test
    fun depoisDeFecharRecusaNovasConexoes() {
        ServerSocket(0, 1, InetAddress.getLoopbackAddress()).use { server ->
            val sockets = RtspSockets(1_000)
            sockets.closeAll()
            assertFailsWith<SocketException> { sockets.createSocket(server.inetAddress, server.localPort) }
        }
    }

    @Test
    fun abortarInterrompeConexaoPendente() {
        // Endereço não roteável (RFC 5737, TEST-NET-1): o SYN fica sem resposta.
        // Se a rede do ambiente recusar na hora, o teste só confirma o prazo.
        val sockets = RtspSockets(30_000)
        var erro: Throwable? = null
        val t = thread { erro = runCatching { sockets.createSocket("192.0.2.1", 554) }.exceptionOrNull() }
        Thread.sleep(200)
        val inicio = System.nanoTime()
        sockets.abortPending()
        t.join(5_000)
        val ms = (System.nanoTime() - inicio) / 1e6
        assertFalse(t.isAlive, "connect continuou preso")
        assertTrue(ms < 1_000, "abortou em $ms ms")
        assertTrue(erro != null, "a conexão não podia ter sido estabelecida")
    }
}
