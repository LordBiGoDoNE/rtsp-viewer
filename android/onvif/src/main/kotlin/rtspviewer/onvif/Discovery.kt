package rtspviewer.onvif

import java.io.IOException
import java.net.DatagramPacket
import java.net.DatagramSocket
import java.net.Inet4Address
import java.net.InetAddress
import java.net.InetSocketAddress
import java.net.MulticastSocket
import java.net.NetworkInterface
import java.net.PortUnreachableException
import java.net.SocketTimeoutException
import java.nio.ByteBuffer
import java.nio.charset.CharacterCodingException
import java.nio.charset.CodingErrorAction
import java.util.TreeMap
import java.util.concurrent.ArrayBlockingQueue
import java.util.concurrent.TimeUnit
import kotlin.concurrent.thread

/**
 * WS-Discovery (`desktop/crates/onvif/src/discovery.rs`, design D4 da
 * `descoberta-onvif`): encontra câmeras ONVIF na rede local.
 *
 * Um Probe multicast por interface IPv4, TTL 1, e o mesmo Probe por unicast a
 * cada host da sub-rede privada da interface (câmeras que ignoram o
 * multicast, redes que não o repassam); escuta por alguns segundos. Respostas
 * são entrada não confiável: só ProbeMatch da busca atual (`RelatesTo`), de
 * origem privada, link-local ou loopback, até 64 KiB, com XAddr de host IP
 * também privado, link-local ou loopback. Fila de recepção e número de
 * dispositivos limitados.
 *
 * As interfaces vêm de quem chama (no Android, das redes Wi-Fi e Ethernet
 * ativas; nos testes, loopback).
 */

/** Interface IPv4 de onde a busca envia. */
class Ipv4Interface(
    val address: Inet4Address,
    /** Tamanho do prefixo da sub-rede; `null` = sem varredura unicast. */
    val prefixLength: Int?,
    /** Ajuste do socket antes do envio (no Android, prender o socket à rede). */
    val prepare: ((DatagramSocket) -> Unit)? = null,
) {
    override fun toString(): String = "${address.hostAddress}/${prefixLength ?: "?"}"
}

class DiscoveryConfig(
    /** Interfaces de onde enviar. */
    val interfaces: List<Ipv4Interface>,
    /** Quanto tempo esperar respostas. */
    val durationMs: Long = 3_000,
    /** Destino do Probe (multicast em produção; unicast nos testes). */
    val target: InetSocketAddress = Discovery.MULTICAST,
    /**
     * Destinos do Probe unicast, enviado de cada interface além do multicast;
     * `null` = a sub-rede privada de cada interface, na porta 3702 (design D4).
     * Injetável nos testes (loopback).
     */
    val unicast: List<InetSocketAddress>? = null,
)

data class DiscoveredDevice(
    /** Identidade estável do dispositivo (`EndpointReference/Address`). */
    val endpoint: String,
    /** Host da câmera (o do serviço de dispositivo escolhido). */
    val host: String,
    /** URL do serviço de dispositivo (http), escolhida entre os `XAddrs`. */
    val deviceUrl: String,
    /** Nome sugerido (scope `name`, senão `hardware`). */
    val name: String?,
    val hardware: String?,
)

sealed class DiscoveryException(message: String) : Exception(message) {
    class NoInterface : DiscoveryException("Nenhuma interface de rede disponível para a busca.")

    class Network(detail: String) : DiscoveryException("Falha de rede na busca: $detail")
}

object Discovery {
    /** Endereço multicast padrão do WS-Discovery. */
    val MULTICAST = InetSocketAddress(
        InetAddress.getByAddress(byteArrayOf(239.toByte(), 255.toByte(), 255.toByte(), 250.toByte())),
        3702,
    )

    /** Tamanho máximo aceito de uma resposta UDP. */
    const val MAX_DATAGRAM = 64 * 1024

    /** Respostas aguardando o parse; com [MAX_DATAGRAM], ~4 MiB sob flood. O excedente é descartado. */
    const val QUEUE_CAPACITY = 64

    /** Máximo de dispositivos distintos numa busca; os seguintes são ignorados. */
    const val MAX_DEVICES = 256

    /** Teto de destinos unicast numa busca, somando todas as interfaces. */
    const val MAX_UNICAST_TARGETS = 1024

    /** Destinos da varredura enviados sem pausa. */
    private const val SWEEP_BATCH = 32

    /** Pausa entre lotes, para não disparar centenas de resoluções ARP de uma vez. */
    private const val SWEEP_PAUSE_MS = 2L

    /** Porta do WS-Discovery nas câmeras. */
    private const val DISCOVERY_PORT = 3702

    /**
     * Buffer de envio do socket: datagramas para hosts sem ARP resolvido ficam
     * retidos e ocupam o buffer. Cabe uma sub-rede inteira (254 × ~0,6 KiB).
     */
    private const val SEND_BUFFER = 256 * 1024

    internal class Datagram(val data: ByteArray, val from: InetAddress)

    /**
     * Faz uma busca e devolve os dispositivos encontrados, sem repetição,
     * ordenados por IP. Bloqueia pelo tempo da busca; interromper a thread
     * encerra a busca (com `InterruptedException`) e fecha os sockets.
     *
     * @throws DiscoveryException
     */
    fun discover(config: DiscoveryConfig): List<DiscoveredDevice> {
        val interfaces = config.interfaces
        if (interfaces.isEmpty()) throw DiscoveryException.NoInterface()
        val plan = unicastPlan(interfaces.map { it.address to it.prefixLength }, config.unicast)
        val messageId = newMessageId()
        val probe = probeMessage(messageId).toByteArray(Charsets.UTF_8)
        val deadline = System.nanoTime() + TimeUnit.MILLISECONDS.toNanos(config.durationMs)

        // Fila limitada: um host da LAN em flood não faz a memória crescer; o
        // que não cabe é descartado na thread de recepção.
        val queue = ArrayBlockingQueue<Datagram>(QUEUE_CAPACITY)
        val sockets = mutableListOf<DatagramSocket>()
        val receivers = mutableListOf<Thread>()
        var lastError: String? = null
        try {
            for ((iface, targets) in interfaces.zip(plan)) {
                val sock = try {
                    openSocket(iface)
                } catch (e: IOException) {
                    lastError = e.message ?: e.javaClass.simpleName
                    continue
                }
                val multicastOk = try {
                    sock.send(DatagramPacket(probe, probe.size, config.target))
                    true
                } catch (e: IOException) {
                    lastError = e.message ?: e.javaClass.simpleName
                    false
                }
                // Sem multicast nesta interface, a varredura unicast ainda vale.
                if (!multicastOk && targets.isEmpty()) {
                    sock.close()
                    continue
                }
                sockets.add(sock)
                // A escuta começa antes; a varredura corre em paralelo, uma
                // thread por interface, dentro do prazo.
                if (targets.isNotEmpty()) {
                    thread(isDaemon = true, name = "ws-discovery-envio") { sendUnicast(sock, targets, probe, deadline) }
                }
                receivers += thread(isDaemon = true, name = "ws-discovery-escuta") {
                    receiveUntil(sock, deadline, messageId, queue)
                }
            }
            if (sockets.isEmpty()) throw DiscoveryException.Network(lastError.orEmpty())

            val collector = DeviceCollector(messageId)
            while (true) {
                val d = queue.poll(20, TimeUnit.MILLISECONDS)
                if (d != null) {
                    collector.add(d)
                } else if (receivers.none { it.isAlive }) {
                    while (true) collector.add(queue.poll() ?: break)
                    break
                }
            }
            return collector.result()
        } finally {
            sockets.forEach { it.close() }
        }
    }

    private fun openSocket(iface: Ipv4Interface): DatagramSocket {
        val s = MulticastSocket(InetSocketAddress(iface.address, 0))
        try {
            if (!iface.address.isLoopbackAddress) {
                val ni = NetworkInterface.getByInetAddress(iface.address)
                    ?: throw IOException("interface de ${iface.address.hostAddress} não encontrada")
                s.networkInterface = ni
            }
            s.timeToLive = 1
            // Melhor esforço: sem ele, a varredura só fica mais lenta.
            try {
                s.sendBufferSize = SEND_BUFFER
            } catch (_: IOException) {
                // ignorado
            }
            s.soTimeout = 100
            iface.prepare?.invoke(s)
            return s
        } catch (e: IOException) {
            s.close()
            throw e
        }
    }

    /**
     * Envia o Probe aos destinos unicast de uma interface, pelo socket dela
     * (mesmo `MessageID`, respostas pela mesma recepção). Erros de envio a um
     * destino são ignorados; para no prazo ou com o socket fechado.
     */
    private fun sendUnicast(sock: DatagramSocket, targets: List<InetSocketAddress>, probe: ByteArray, deadline: Long) {
        for ((i, target) in targets.withIndex()) {
            if (System.nanoTime() - deadline >= 0 || sock.isClosed) return
            try {
                sock.send(DatagramPacket(probe, probe.size, target))
            } catch (_: IOException) {
                // Destino recusado ou socket fechado: segue (ou para no próximo teste).
            }
            if ((i + 1) % SWEEP_BATCH == 0) {
                try {
                    Thread.sleep(SWEEP_PAUSE_MS)
                } catch (_: InterruptedException) {
                    return
                }
            }
        }
    }

    private fun receiveUntil(sock: DatagramSocket, deadline: Long, messageId: String, queue: ArrayBlockingQueue<Datagram>) {
        // Um byte a mais que o limite: se ele for preenchido, a resposta é
        // grande demais e é descartada.
        val buf = ByteArray(MAX_DATAGRAM + 1)
        val packet = DatagramPacket(buf, buf.size)
        while (System.nanoTime() - deadline < 0) {
            try {
                packet.setLength(buf.size)
                sock.receive(packet)
                val data = buf.copyOf(packet.length)
                // Fila cheia: descarta (flood); a busca segue.
                if (worthQueueing(data, packet.address, messageId)) queue.offer(Datagram(data, packet.address))
            } catch (_: SocketTimeoutException) {
                // Sem resposta nesse intervalo.
            } catch (_: PortUnreachableException) {
                // Destino unicast recusou; não encerra a escuta.
            } catch (_: IOException) {
                return
            }
        }
    }

    /**
     * Triagem barata antes de enfileirar: tamanho, origem confiável, UTF-8 e o
     * `MessageID` da busca presente (o `RelatesTo` o contém). O parse completo
     * fica em [parseProbeMatch].
     */
    internal fun worthQueueing(data: ByteArray, from: InetAddress, messageId: String): Boolean =
        data.size <= MAX_DATAGRAM && LocalAddresses.isLocal(from) && decodeUtf8(data)?.contains(messageId) == true

    /** Dispositivos sem repetição (por `EndpointReference`), no máximo [MAX_DEVICES], ordenados por IP. */
    internal class DeviceCollector(private val messageId: String) {
        private val found = TreeMap<String, DiscoveredDevice>()

        fun add(d: Datagram) {
            val dev = parseProbeMatch(d.data, messageId, d.from) ?: return
            if (found.size < MAX_DEVICES || found.containsKey(dev.endpoint)) found.putIfAbsent(dev.endpoint, dev)
        }

        fun result(): List<DiscoveredDevice> =
            found.values.sortedWith(compareBy<DiscoveredDevice>({ sortKey(it.host).first }, { sortKey(it.host).second }))
    }

    private fun sortKey(host: String): Pair<Long, String> {
        val v4 = LocalAddresses.parseIpv4(host) ?: return 0xFFFF_FFFFL to host
        var n = 0L
        for (b in v4) n = (n shl 8) or (b.toLong() and 0xFF)
        return n to ""
    }

    /**
     * Destinos unicast por interface, na ordem de `interfaces`: os injetados
     * (testes) em todas, ou a varredura da sub-rede privada de cada uma, sem os
     * endereços da própria máquina, sem repetir destino entre interfaces e com
     * no máximo [MAX_UNICAST_TARGETS] no total.
     */
    internal fun unicastPlan(
        interfaces: List<Pair<Inet4Address, Int?>>,
        injected: List<InetSocketAddress>?,
    ): List<List<InetSocketAddress>> {
        if (injected != null) return interfaces.map { injected }
        val seen = interfaces.map { toInt(it.first.address) }.toMutableSet()
        var total = 0
        return interfaces.map { (ip, prefix) ->
            val targets = mutableListOf<InetSocketAddress>()
            for (host in if (prefix != null) sweepHosts(ip.address, prefix) else emptyList()) {
                if (total >= MAX_UNICAST_TARGETS) break
                if (seen.add(host)) {
                    targets.add(InetSocketAddress(InetAddress.getByAddress(fromInt(host)), DISCOVERY_PORT))
                    total++
                }
            }
            targets
        }
    }

    /**
     * Hosts da sub-rede de uma interface para o Probe unicast (design D4). Só
     * endereço privado (RFC 1918): link-local, loopback e público dão lista
     * vazia. Prefixo /24 ou mais longo: a sub-rede real; mais curto (/20,
     * /16): só o /24 que contém o endereço. Sem rede, broadcast e o próprio
     * endereço; no máximo 254 hosts.
     */
    internal fun sweepHosts(ip: ByteArray, prefixLength: Int): List<Int> {
        if (!LocalAddresses.isPrivateV4(ip)) return emptyList()
        val prefix = prefixLength.coerceAtLeast(24)
        if (prefix > 30) return emptyList()
        val mask = -1 shl (32 - prefix)
        val self = toInt(ip)
        val network = self and mask
        val broadcast = network or mask.inv()
        return ((network + 1) until broadcast).filter { it != self }
    }

    private fun toInt(b: ByteArray): Int =
        ((b[0].toInt() and 0xFF) shl 24) or ((b[1].toInt() and 0xFF) shl 16) or
            ((b[2].toInt() and 0xFF) shl 8) or (b[3].toInt() and 0xFF)

    private fun fromInt(v: Int): ByteArray =
        byteArrayOf((v ushr 24).toByte(), (v ushr 16).toByte(), (v ushr 8).toByte(), v.toByte())

    internal fun newMessageId(): String {
        val h = Auth.hex(Auth.randomBytes(16))
        return "uuid:${h.substring(0, 8)}-${h.substring(8, 12)}-${h.substring(12, 16)}-" +
            "${h.substring(16, 20)}-${h.substring(20, 32)}"
    }

    fun probeMessage(messageId: String): String =
        """<?xml version="1.0" encoding="UTF-8"?>""" +
            """<s:Envelope xmlns:s="http://www.w3.org/2003/05/soap-envelope" """ +
            """xmlns:a="http://schemas.xmlsoap.org/ws/2004/08/addressing" """ +
            """xmlns:d="http://schemas.xmlsoap.org/ws/2005/04/discovery" """ +
            """xmlns:dn="http://www.onvif.org/ver10/network/wsdl">""" +
            "<s:Header><a:MessageID>${Xml.escape(messageId)}</a:MessageID>" +
            """<a:To s:mustUnderstand="1">urn:schemas-xmlsoap-org:ws:2005:04:discovery</a:To>""" +
            """<a:Action s:mustUnderstand="1">http://schemas.xmlsoap.org/ws/2005/04/discovery/Probe</a:Action>""" +
            "</s:Header><s:Body><d:Probe><d:Types>dn:NetworkVideoTransmitter</d:Types></d:Probe></s:Body></s:Envelope>"

    private fun decodeUtf8(data: ByteArray): String? = try {
        Charsets.UTF_8.newDecoder()
            .onMalformedInput(CodingErrorAction.REPORT)
            .onUnmappableCharacter(CodingErrorAction.REPORT)
            .decode(ByteBuffer.wrap(data))
            .toString()
    } catch (_: CharacterCodingException) {
        null
    }

    /** Lê um ProbeMatch; `null` para qualquer coisa que não seja uma resposta válida desta busca. */
    fun parseProbeMatch(data: ByteArray, messageId: String, from: InetAddress): DiscoveredDevice? {
        if (data.size > MAX_DATAGRAM || !LocalAddresses.isLocal(from)) return null
        val text = decodeUtf8(data) ?: return null
        val doc = try {
            Xml.parse(text)
        } catch (_: XmlException) {
            return null
        }
        if (doc.name != "Envelope") return null
        if (doc.path("Header", "RelatesTo")?.textTrimmed() != messageId) return null
        val m = doc.path("Body", "ProbeMatches", "ProbeMatch") ?: return null
        val endpoint = m.find("Address")?.textTrimmed()?.ifEmpty { null }
        val xaddrs = m.child("XAddrs")?.textTrimmed().orEmpty().split(WHITESPACE).filter { it.isNotEmpty() }
        val deviceUrl = chooseXaddr(xaddrs, from) ?: return null
        // Host da câmera = o do serviço escolhido (é o que se cadastra). Em
        // máquinas com várias interfaces, a origem do datagrama pode diferir.
        val host = DeviceService.fromUrl(deviceUrl)?.host ?: return null
        val scopes = m.child("Scopes")?.textTrimmed().orEmpty().split(WHITESPACE).filter { it.isNotEmpty() }
        fun scope(kind: String): String? {
            val prefix = "onvif://www.onvif.org/$kind/"
            return scopes.firstOrNull { it.startsWith(prefix) }
                ?.let { decodeScope(it.substring(prefix.length)) }
                ?.ifEmpty { null }
        }
        val name = scope("name")
        val hardware = scope("hardware")
        return DiscoveredDevice(
            endpoint = endpoint ?: deviceUrl,
            host = host,
            deviceUrl = deviceUrl,
            name = name ?: hardware,
            hardware = hardware,
        )
    }

    private val WHITESPACE = Regex("\\s+")

    /**
     * Só `http://` com host IP literal confiável (privado, link-local ou
     * loopback), sem credenciais embutidas: o XAddr vira o destino das
     * credenciais no cadastro, então nome DNS, IP público ou `user@host` são
     * descartados (ADR 0005). Prefere o XAddr com o mesmo IP de origem, depois
     * o primeiro IPv4, depois o primeiro IPv6. Nenhum aceito → `null`.
     */
    private fun chooseXaddr(xaddrs: List<String>, from: InetAddress): String? {
        val accepted = xaddrs.mapNotNull { x ->
            val ip = DeviceService.fromUrl(x)?.host?.let(LocalAddresses::parseLiteral) ?: return@mapNotNull null
            if (LocalAddresses.isLocal(ip)) x to ip else null
        }
        val fromBytes = from.address
        return (
            accepted.firstOrNull { it.second.contentEquals(fromBytes) }
                ?: accepted.firstOrNull { it.second.size == 4 }
                ?: accepted.firstOrNull()
            )?.first
    }

    /** Scopes ONVIF usam codificação percentual (e às vezes `_` no lugar de espaço). */
    internal fun decodeScope(raw: String): String {
        val bytes = raw.toByteArray(Charsets.UTF_8)
        val out = java.io.ByteArrayOutputStream(bytes.size)
        var i = 0
        while (i < bytes.size) {
            if (bytes[i] == '%'.code.toByte() && i + 2 < bytes.size && isHex(bytes[i + 1]) && isHex(bytes[i + 2])) {
                out.write(Character.digit(bytes[i + 1].toInt(), 16) * 16 + Character.digit(bytes[i + 2].toInt(), 16))
                i += 3
                continue
            }
            out.write(if (bytes[i] == '_'.code.toByte()) ' '.code else bytes[i].toInt())
            i++
        }
        val text = String(out.toByteArray(), Charsets.UTF_8).trim()
        // Até 60 caracteres (pontos de código, como no desktop).
        val end = if (text.codePointCount(0, text.length) > 60) text.offsetByCodePoints(0, 60) else text.length
        return text.substring(0, end)
    }

    private fun isHex(b: Byte): Boolean = b.toInt().toChar().let { it in '0'..'9' || it in 'a'..'f' || it in 'A'..'F' }
}
