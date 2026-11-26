package rtspviewer.app.player

import java.io.IOException
import java.net.InetAddress
import java.net.InetSocketAddress
import java.net.Socket
import java.net.SocketAddress
import java.net.SocketException
import javax.net.SocketFactory

/**
 * Sockets da conexão RTSP de um player. O `RtspClient` do Media3 1.11 conecta
 * com `createSocket(host, porta)` na thread de reprodução, sem prazo: com uma
 * câmera que não responde, essa thread fica presa e o `release()` do player só
 * termina por tempo esgotado, com a tentativa de conexão ainda aberta. Aqui a
 * conexão tem prazo ([connectTimeoutMs]) e [closeAll] fecha, de qualquer
 * thread, os sockets abertos ou ainda conectando (o `connect` em andamento
 * falha na hora). Depois de [closeAll], novas conexões são recusadas.
 */
internal class RtspSockets(private val connectTimeoutMs: Int) : SocketFactory() {
    private val sockets = mutableSetOf<Socket>()
    private var closed = false

    override fun createSocket(): Socket = track(Socket())

    override fun createSocket(host: String, port: Int): Socket = connect(InetSocketAddress(host, port))

    override fun createSocket(host: String, port: Int, localHost: InetAddress?, localPort: Int): Socket =
        connect(InetSocketAddress(host, port), InetSocketAddress(localHost, localPort))

    override fun createSocket(host: InetAddress, port: Int): Socket = connect(InetSocketAddress(host, port))

    override fun createSocket(address: InetAddress, port: Int, localAddress: InetAddress?, localPort: Int): Socket =
        connect(InetSocketAddress(address, port), InetSocketAddress(localAddress, localPort))

    /** Fecha as conexões ainda não estabelecidas (o `connect` em andamento falha). */
    fun abortPending() = closeWhere { !it.isConnected }

    /** Fecha todas as conexões e recusa as próximas. Idempotente. */
    fun closeAll() {
        synchronized(sockets) { closed = true }
        closeWhere { true }
    }

    private fun connect(remote: SocketAddress, local: SocketAddress? = null): Socket {
        val socket = track(Socket())
        try {
            local?.let(socket::bind)
            socket.connect(remote, connectTimeoutMs)
            return socket
        } catch (e: IOException) {
            untrack(socket)
            closeQuietly(socket)
            throw e
        }
    }

    private fun track(socket: Socket): Socket {
        synchronized(sockets) {
            if (!closed) {
                sockets += socket
                return socket
            }
        }
        closeQuietly(socket)
        throw SocketException("conexão encerrada")
    }

    private fun untrack(socket: Socket) {
        synchronized(sockets) { sockets -= socket }
    }

    private fun closeWhere(predicate: (Socket) -> Boolean) {
        val selected = synchronized(sockets) { sockets.filter(predicate).also { sockets -= it.toSet() } }
        selected.forEach(::closeQuietly)
    }

    private fun closeQuietly(socket: Socket) {
        try {
            socket.close()
        } catch (_: IOException) {
            // Já fechado ou rede indisponível: nada a fazer.
        }
    }
}
