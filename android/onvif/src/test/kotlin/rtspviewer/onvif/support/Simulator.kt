package rtspviewer.onvif.support

import com.sun.net.httpserver.HttpExchange
import com.sun.net.httpserver.HttpServer
import java.net.DatagramPacket
import java.net.DatagramSocket
import java.net.InetAddress
import java.net.InetSocketAddress
import java.net.SocketTimeoutException
import java.util.Base64
import java.util.concurrent.CopyOnWriteArrayList
import java.util.concurrent.ExecutorService
import java.util.concurrent.Executors
import java.util.concurrent.atomic.AtomicBoolean
import kotlin.concurrent.thread
import rtspviewer.onvif.Auth
import rtspviewer.onvif.Xml
import rtspviewer.onvif.XmlElement
import rtspviewer.onvif.XmlException

/**
 * Simulador de câmera ONVIF para os testes no JVM (equivalente a
 * `desktop/crates/onvif/tests/support/simulator.rs`, design D10). Valida de
 * verdade o PasswordDigest (com o relógio simulado) e o HTTP Digest, e guarda
 * as requisições recebidas para os testes conferirem que a senha nunca trafega
 * em texto puro. Só valores fictícios.
 */
enum class SimAuth {
    /** Aceita só WS-UsernameToken PasswordDigest. */
    WsSecurity,

    /** Responde 401 com desafio Digest e aceita só HTTP Digest. */
    HttpDigest,

    /** Não exige autenticação. */
    Open,
}

data class SimProfile(val token: String, val width: Int, val height: Int, val uri: String)

data class SimConfig(
    val username: String = "admin",
    val password: String = "s3nh@:/?#-ONVIF",
    val auth: SimAuth = SimAuth.WsSecurity,
    /** Relógio da câmera adiantado (ou atrasado) em segundos. */
    val clockOffset: Long = 0,
    val manufacturer: String = "Simulador",
    val model: String = "Camera ONVIF de teste",
    val profiles: List<SimProfile> = listOf(
        SimProfile("sub", 640, 360, "rtsp://{host}:8554/sub"),
        SimProfile("main", 1920, 1080, "rtsp://{host}:8554/main"),
    ),
    /** Anuncia o serviço de mídia com host 0.0.0.0 (firmware com NAT errado). */
    val mediaHostZero: Boolean = false,
    /** Host anunciado no XAddr de mídia (outro IP: NAT ou câmera maliciosa). */
    val mediaHost: String? = null,
    /** Atraso antes de cada resposta, para testar o prazo total da consulta. */
    val responseDelayMs: Long = 0,
    /** Responde HTML em vez de SOAP (não é câmera ONVIF). */
    val notOnvif: Boolean = false,
    /** Resposta de perfis artificialmente grande, para testar limites. */
    val oversizedProfiles: Boolean = false,
    /** Prefixos de namespace dos elementos de resposta (firmwares variam). */
    val prefixes: Prefixes = Prefixes(),
    /**
     * Redirecionamento HTTP: depois da autenticação (a repetição com HTTP
     * Digest também é redirecionada), responde com este status e `Location`.
     */
    val redirect: Pair<Int, String>? = null,
)

/** Resolução irrestrita, só para os testes (o app usa a regra da rede local). */
val UNRESTRICTED = rtspviewer.onvif.HostResolver { host -> InetAddress.getByName(host) }

data class Prefixes(val env: String = "env", val tds: String = "tds", val trt: String = "trt", val tt: String = "tt")

class Simulator(private val cfg: SimConfig) : AutoCloseable {
    private val executor: ExecutorService = Executors.newCachedThreadPool { r -> Thread(r).apply { isDaemon = true } }
    private val server: HttpServer = HttpServer.create(InetSocketAddress(InetAddress.getLoopbackAddress(), 0), 0)
    val requests = CopyOnWriteArrayList<String>()

    init {
        server.createContext("/") { ex ->
            try {
                handle(ex)
            } finally {
                ex.close()
            }
        }
        server.executor = executor
        server.start()
    }

    val port: Int get() = server.address.port

    fun deviceUrl(): String = "http://127.0.0.1:$port/onvif/device_service"

    fun allRequests(): String = requests.joinToString("\n")

    override fun close() {
        server.stop(0)
        executor.shutdownNow()
    }

    private fun handle(ex: HttpExchange) {
        val body = ex.requestBody.readBytes().toString(Charsets.UTF_8)
        val raw = StringBuilder("${ex.requestMethod} ${ex.requestURI} HTTP/1.1\r\n")
        for ((k, vs) in ex.requestHeaders) for (v in vs) raw.append("$k: $v\r\n")
        requests.add(raw.append("\r\n").append(body).toString())
        if (cfg.responseDelayMs > 0) Thread.sleep(cfg.responseDelayMs)

        if (cfg.notOnvif) return respond(ex, 200, "text/html", "<html><body>Login</body></html>")
        val doc = try {
            Xml.parse(body)
        } catch (_: XmlException) {
            return respond(ex, 400, "text/plain", "xml")
        }
        val op = doc.child("Body")?.children?.firstOrNull()?.name.orEmpty()
        val cameraNow = System.currentTimeMillis() / 1000 + cfg.clockOffset

        if (op != "GetSystemDateAndTime") {
            when (cfg.auth) {
                SimAuth.Open -> {}
                SimAuth.WsSecurity -> if (!wsSecurityOk(doc, cameraNow)) {
                    return respond(ex, 400, SOAP, notAuthorizedFault())
                }
                SimAuth.HttpDigest -> {
                    val auth = ex.requestHeaders.getFirst("Authorization")
                    if (auth == null || !httpDigestOk(auth, ex.requestURI.toString())) {
                        ex.responseHeaders.add(
                            "WWW-Authenticate",
                            """Digest realm="simulador", qop="auth", nonce="n0nce-sim", opaque="op4que"""",
                        )
                        return respond(ex, 401, "text/plain", "unauthorized")
                    }
                }
            }
        }

        cfg.redirect?.let { (status, location) ->
            ex.responseHeaders.add("Location", location)
            return respond(ex, status, "text/plain", "")
        }

        val p = cfg.prefixes
        val mediaHost = cfg.mediaHost ?: if (cfg.mediaHostZero) "0.0.0.0" else "127.0.0.1"
        val responseBody = when (op) {
            "GetSystemDateAndTime" -> {
                val s = Auth.formatUtc(cameraNow)
                "<${p.tds}:GetSystemDateAndTimeResponse><${p.tds}:SystemDateAndTime>" +
                    "<${p.tt}:DateTimeType>Manual</${p.tt}:DateTimeType><${p.tt}:UTCDateTime>" +
                    "<${p.tt}:Time><${p.tt}:Hour>${s.substring(11, 13).toInt()}</${p.tt}:Hour>" +
                    "<${p.tt}:Minute>${s.substring(14, 16).toInt()}</${p.tt}:Minute>" +
                    "<${p.tt}:Second>${s.substring(17, 19).toInt()}</${p.tt}:Second></${p.tt}:Time>" +
                    "<${p.tt}:Date><${p.tt}:Year>${s.substring(0, 4)}</${p.tt}:Year>" +
                    "<${p.tt}:Month>${s.substring(5, 7).toInt()}</${p.tt}:Month>" +
                    "<${p.tt}:Day>${s.substring(8, 10).toInt()}</${p.tt}:Day></${p.tt}:Date>" +
                    "</${p.tt}:UTCDateTime></${p.tds}:SystemDateAndTime></${p.tds}:GetSystemDateAndTimeResponse>"
            }
            "GetDeviceInformation" ->
                "<${p.tds}:GetDeviceInformationResponse><${p.tds}:Manufacturer>${Xml.escape(cfg.manufacturer)}" +
                    "</${p.tds}:Manufacturer><${p.tds}:Model>${Xml.escape(cfg.model)}</${p.tds}:Model>" +
                    "<${p.tds}:FirmwareVersion>1.0</${p.tds}:FirmwareVersion><${p.tds}:SerialNumber>SIM-1" +
                    "</${p.tds}:SerialNumber><${p.tds}:HardwareId>1</${p.tds}:HardwareId>" +
                    "</${p.tds}:GetDeviceInformationResponse>"
            "GetCapabilities" ->
                "<${p.tds}:GetCapabilitiesResponse><${p.tds}:Capabilities><${p.tt}:Media><${p.tt}:XAddr>" +
                    "http://$mediaHost:$port/onvif/media_service</${p.tt}:XAddr></${p.tt}:Media>" +
                    "</${p.tds}:Capabilities></${p.tds}:GetCapabilitiesResponse>"
            "GetProfiles" -> buildString {
                append("<${p.trt}:GetProfilesResponse>")
                for (pr in cfg.profiles) {
                    append(
                        "<${p.trt}:Profiles token=\"${pr.token}\" fixed=\"true\"><${p.tt}:Name>${pr.token}</${p.tt}:Name>" +
                            "<${p.tt}:VideoEncoderConfiguration token=\"v${pr.token}\"><${p.tt}:Encoding>H264</${p.tt}:Encoding>" +
                            "<${p.tt}:Resolution><${p.tt}:Width>${pr.width}</${p.tt}:Width><${p.tt}:Height>${pr.height}" +
                            "</${p.tt}:Height></${p.tt}:Resolution></${p.tt}:VideoEncoderConfiguration></${p.trt}:Profiles>",
                    )
                }
                if (cfg.oversizedProfiles) append("<!-- ${"x".repeat(300 * 1024)} -->")
                append("</${p.trt}:GetProfilesResponse>")
            }
            "GetStreamUri" -> {
                val token = doc.find("ProfileToken")?.textTrimmed().orEmpty()
                val pr = cfg.profiles.firstOrNull { it.token == token }
                    ?: return respond(ex, 400, SOAP, fault("env:Sender", "NoProfile"))
                "<${p.trt}:GetStreamUriResponse><${p.trt}:MediaUri><${p.tt}:Uri>" +
                    Xml.escape(pr.uri.replace("{host}", "127.0.0.1")) +
                    "</${p.tt}:Uri><${p.tt}:InvalidAfterConnect>false</${p.tt}:InvalidAfterConnect>" +
                    "</${p.trt}:MediaUri></${p.trt}:GetStreamUriResponse>"
            }
            else -> return respond(ex, 400, SOAP, fault("env:Receiver", "Action not supported"))
        }
        respond(ex, 200, SOAP, envelope(responseBody))
    }

    private fun envelope(body: String): String {
        val p = cfg.prefixes
        return """<?xml version="1.0" encoding="UTF-8"?><${p.env}:Envelope xmlns:${p.env}="http://www.w3.org/2003/05/soap-envelope" """ +
            """xmlns:${p.tds}="http://www.onvif.org/ver10/device/wsdl" xmlns:${p.trt}="http://www.onvif.org/ver10/media/wsdl" """ +
            """xmlns:${p.tt}="http://www.onvif.org/ver10/schema" xmlns:ter="http://www.onvif.org/ver10/error">""" +
            "<${p.env}:Body>$body</${p.env}:Body></${p.env}:Envelope>"
    }

    private fun fault(code: String, reason: String): String {
        val e = cfg.prefixes.env
        return envelope(
            "<$e:Fault><$e:Code><$e:Value>$code</$e:Value></$e:Code><$e:Reason><$e:Text>$reason</$e:Text></$e:Reason></$e:Fault>",
        )
    }

    private fun notAuthorizedFault(): String {
        val e = cfg.prefixes.env
        return envelope(
            "<$e:Fault><$e:Code><$e:Value>$e:Sender</$e:Value><$e:Subcode><$e:Value>ter:NotAuthorized</$e:Value>" +
                "</$e:Subcode></$e:Code><$e:Reason><$e:Text xml:lang=\"en\">Sender not Authorized</$e:Text></$e:Reason></$e:Fault>",
        )
    }

    private fun respond(ex: HttpExchange, status: Int, contentType: String, body: String) {
        val bytes = body.toByteArray(Charsets.UTF_8)
        ex.responseHeaders.add("Content-Type", contentType)
        ex.sendResponseHeaders(status, bytes.size.toLong())
        ex.responseBody.use { it.write(bytes) }
    }

    private fun wsSecurityOk(doc: XmlElement, cameraNow: Long): Boolean {
        val token = doc.find("UsernameToken") ?: return false
        fun text(n: String) = token.child(n)?.textTrimmed()
        val user = text("Username") ?: return false
        val digest = text("Password") ?: return false
        val nonceText = text("Nonce") ?: return false
        val created = text("Created") ?: return false
        if (token.child("Password")?.attr("Type")?.endsWith("#PasswordDigest") != true) return false
        val nonce = try {
            Base64.getDecoder().decode(nonceText)
        } catch (_: IllegalArgumentException) {
            return false
        }
        val createdAt = parseCreated(created) ?: return false
        // Câmeras reais recusam horário fora de uma janela curta.
        if (kotlin.math.abs(createdAt - cameraNow) > 300) return false
        return user == cfg.username && digest == Auth.passwordDigest(nonce, created, cfg.password)
    }

    private fun parseCreated(s: String): Long? {
        val t = s.removeSuffix("Z").takeIf { s.endsWith('Z') } ?: return null
        val date = t.substringBefore('T', "").split('-')
        val time = t.substringAfter('T', "").split(':')
        if (date.size != 3 || time.size != 3) return null
        return Auth.utcToUnix(
            date[0].toLongOrNull() ?: return null,
            date[1].toIntOrNull() ?: return null,
            date[2].toIntOrNull() ?: return null,
            time[0].toIntOrNull() ?: return null,
            time[1].toIntOrNull() ?: return null,
            time[2].substringBefore('.').toIntOrNull() ?: return null,
        )
    }

    private fun httpDigestOk(header: String, path: String): Boolean {
        val rest = header.removePrefix("Digest ").takeIf { header.startsWith("Digest ") } ?: return false
        fun get(k: String): String? = rest.split(", ").firstNotNullOfOrNull { kv ->
            val key = kv.substringBefore('=', "")
            if (key.trim() == k) kv.substringAfter('=').trim().trim('"') else null
        }
        val user = get("username") ?: return false
        val uri = get("uri") ?: return false
        val response = get("response") ?: return false
        val nc = get("nc") ?: return false
        val cnonce = get("cnonce") ?: return false
        if (user != cfg.username || uri != path || get("opaque") != "op4que") return false
        val ha1 = Auth.md5Hex("${cfg.username}:simulador:${cfg.password}")
        val ha2 = Auth.md5Hex("POST:$uri")
        return response == Auth.md5Hex("$ha1:n0nce-sim:$nc:$cnonce:auth:$ha2")
    }

    companion object {
        const val SOAP = "application/soap+xml; charset=utf-8"
    }
}

// ---------------------------------------------------------------- WS-Discovery

/** Resposta que o respondedor envia para cada Probe recebido. */
sealed interface DiscoveryReply {
    /** ProbeMatch válido. `{ip}` em `xaddr` vira o endereço anunciado. */
    data class Match(val endpoint: String, val xaddr: String, val scopes: String) : DiscoveryReply

    /** ProbeMatch com `RelatesTo` de outra busca. */
    data object WrongRelatesTo : DiscoveryReply

    /** Datagrama que não é XML. */
    data object Malformed : DiscoveryReply

    /** Flood: `n` ProbeMatch válidos desta busca, cada um com endpoint distinto. */
    data class Flood(val n: Int) : DiscoveryReply

    /** Flood: `n` datagramas de ~60 KiB que não são desta busca. */
    data class Junk(val n: Int) : DiscoveryReply
}

/** Respondedor WS-Discovery por unicast em loopback. */
class DiscoveryResponder(private val advertiseIp: String, private val replies: List<DiscoveryReply>) : AutoCloseable {
    private val socket = DatagramSocket(InetSocketAddress(InetAddress.getLoopbackAddress(), 0)).apply { soTimeout = 200 }
    private val stop = AtomicBoolean(false)
    val port: Int get() = socket.localPort

    init {
        thread(isDaemon = true, name = "respondedor-ws-discovery") { loop() }
    }

    override fun close() {
        stop.set(true)
        socket.close()
    }

    private fun loop() {
        val buf = ByteArray(65536)
        while (!stop.get()) {
            val packet = DatagramPacket(buf, buf.size)
            try {
                socket.receive(packet)
            } catch (_: SocketTimeoutException) {
                continue
            } catch (_: Exception) {
                return
            }
            val text = String(buf, 0, packet.length, Charsets.UTF_8)
            val doc = try {
                Xml.parse(text)
            } catch (_: XmlException) {
                continue
            }
            if (doc.find("Probe") == null) continue
            val id = doc.path("Header", "MessageID")?.textTrimmed().orEmpty()
            val to = packet.socketAddress
            fun send(s: String) {
                val b = s.toByteArray(Charsets.UTF_8)
                try {
                    socket.send(DatagramPacket(b, b.size, to))
                } catch (_: Exception) {
                    // ignorado
                }
            }
            for (r in replies) {
                when (r) {
                    is DiscoveryReply.Flood -> {
                        val xaddr = "http://$advertiseIp/onvif/device_service"
                        repeat(r.n) { i -> send(probeMatch(id, "urn:flood-$i", xaddr, "")) }
                    }
                    is DiscoveryReply.Junk -> {
                        val d = "<x>${"a".repeat(60 * 1024)}</x>"
                        repeat(r.n) { send(d) }
                    }
                    is DiscoveryReply.Match -> send(probeMatch(id, r.endpoint, r.xaddr.replace("{ip}", advertiseIp), r.scopes))
                    DiscoveryReply.WrongRelatesTo ->
                        send(probeMatch("uuid:outra-busca", "urn:uuid:intrusa", "http://10.0.0.66/onvif/device_service", ""))
                    DiscoveryReply.Malformed -> send("isto não é XML <")
                }
            }
        }
    }

    companion object {
        fun probeMatch(relates: String, endpoint: String, xaddr: String, scopes: String): String =
            """<?xml version="1.0" encoding="UTF-8"?><SOAP-ENV:Envelope xmlns:SOAP-ENV="http://www.w3.org/2003/05/soap-envelope" """ +
                """xmlns:wsa="http://schemas.xmlsoap.org/ws/2004/08/addressing" xmlns:d="http://schemas.xmlsoap.org/ws/2005/04/discovery" """ +
                """xmlns:dn="http://www.onvif.org/ver10/network/wsdl"><SOAP-ENV:Header><wsa:MessageID>uuid:resp-${System.nanoTime()}</wsa:MessageID>""" +
                "<wsa:RelatesTo>${Xml.escape(relates)}</wsa:RelatesTo>" +
                "<wsa:Action>http://schemas.xmlsoap.org/ws/2005/04/discovery/ProbeMatches</wsa:Action></SOAP-ENV:Header>" +
                "<SOAP-ENV:Body><d:ProbeMatches><d:ProbeMatch><wsa:EndpointReference><wsa:Address>${Xml.escape(endpoint)}" +
                "</wsa:Address></wsa:EndpointReference><d:Types>dn:NetworkVideoTransmitter</d:Types>" +
                "<d:Scopes>${Xml.escape(scopes)}</d:Scopes><d:XAddrs>${Xml.escape(xaddr)}</d:XAddrs>" +
                "<d:MetadataVersion>1</d:MetadataVersion></d:ProbeMatch></d:ProbeMatches></SOAP-ENV:Body></SOAP-ENV:Envelope>"
    }
}
