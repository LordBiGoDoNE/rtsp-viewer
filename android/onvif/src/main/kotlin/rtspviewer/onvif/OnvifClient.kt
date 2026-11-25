package rtspviewer.onvif

import java.net.InetAddress
import java.net.UnknownHostException
import java.util.concurrent.LinkedBlockingQueue
import java.util.concurrent.TimeUnit
import kotlin.concurrent.thread
import rtspviewer.core.domain.InvalidUrlException
import rtspviewer.core.domain.RtspUrl
import rtspviewer.core.domain.redactCredentials

/**
 * Cliente ONVIF (`desktop/crates/onvif/src/client.rs`, design D1–D3 da
 * `descoberta-onvif`): localiza o serviço do dispositivo, mede o relógio da
 * câmera e obtém as URLs RTSP dos streams principal e secundário.
 */

/** Falhas da consulta, com as mesmas mensagens do desktop. */
sealed class OnvifException(message: String) : Exception(message) {
    class Unreachable(val detail: String) : OnvifException("A câmera não respondeu: $detail")

    class NotOnvif : OnvifException("O endereço não respondeu como câmera ONVIF.")

    class AuthenticationFailed : OnvifException("Usuário ou senha inválidos.")

    class NoVideoProfiles : OnvifException("A câmera não informou nenhum perfil de vídeo.")

    class Protocol(val detail: String) : OnvifException("Resposta ONVIF inesperada: $detail")
}

data class OnvifConfig(
    /** Timeout de conexão de cada chamada. */
    val connectTimeoutMs: Long = 2_000,
    /**
     * Prazo total de uma consulta inteira ([OnvifClient.locate] ou
     * [OnvifClient.fetchStreams], com todas as chamadas dela), não de cada chamada.
     */
    val totalTimeoutMs: Long = 10_000,
    /** Portas tentadas quando o endereço não traz porta. */
    val probePorts: List<Int> = DEFAULT_PORTS,
    val probeConnectTimeoutMs: Long = 1_000,
) {
    companion object {
        /** Portas ONVIF usuais, na ordem de preferência (2020 é a da TP-Link Tapo). */
        val DEFAULT_PORTS = listOf(80, 2020, 8000, 8080, 8899)
    }
}

/**
 * Resolve o host da câmera para o IP a que as chamadas HTTP de uma consulta
 * vão. A resolução é feita uma vez por consulta e todas as chamadas dela usam
 * o mesmo IP: quem resolve também decide se o destino é aceito (no app, só a
 * rede local; ADR 0006). Recusa → [OnvifException], sem nada enviado.
 */
fun interface HostResolver {
    /** @throws OnvifException */
    fun resolve(host: String): InetAddress

    companion object {
        /** Resolução do sistema, sem restrição de destino. */
        val SYSTEM = HostResolver { host ->
            LocalAddresses.parseLiteral(host)?.let { return@HostResolver InetAddress.getByAddress(it) }
            try {
                InetAddress.getByName(host)
            } catch (_: UnknownHostException) {
                throw OnvifException.Unreachable("endereço não encontrado: $host")
            } catch (_: SecurityException) {
                throw OnvifException.Unreachable("endereço não encontrado: $host")
            }
        }
    }
}

/** Serviço de dispositivo ONVIF localizado. */
data class DeviceService(
    /** `http://host:porta/caminho`. */
    val url: String,
    /** Host da câmera, destino de todas as chamadas e das URLs RTSP devolvidas. */
    val host: String,
) {
    companion object {
        /**
         * Lê `http://host[:porta][/caminho]`. Recusa (`null`) o que pode levar a
         * conexão a um host diferente do `host` devolvido: credenciais embutidas
         * (`user@host`), host com caracteres fora de nome/IPv4, IPv6 entre
         * colchetes ilegível e porta inválida (ADR 0005).
         */
        fun fromUrl(url: String): DeviceService? {
            if (!url.startsWith("http://")) return null
            val rest = url.substring("http://".length)
            val authority = rest.substring(0, authorityEnd(rest))
            if (authority.contains('@')) return null
            val host: String
            val port: String?
            if (authority.startsWith('[')) {
                val close = authority.indexOf(']')
                if (close < 0) return null
                host = authority.substring(1, close)
                LocalAddresses.parseIpv6(host) ?: return null
                val after = authority.substring(close + 1)
                port = when {
                    after.isEmpty() -> null
                    after.startsWith(':') -> after.substring(1)
                    else -> return null
                }
            } else {
                val colon = authority.indexOf(':')
                host = if (colon >= 0) authority.substring(0, colon) else authority
                port = if (colon >= 0) authority.substring(colon + 1) else null
            }
            if (port != null && parsePort(port) == null) return null
            val validName = host.isNotEmpty() && host.all { it in 'a'..'z' || it in 'A'..'Z' || it in '0'..'9' || it == '.' || it == '-' }
            if (!host.contains(':') && !validName) return null
            return DeviceService(url, host)
        }

        fun forHost(host: String, port: Int): DeviceService =
            DeviceService("http://${bracketed(host)}:$port/onvif/device_service", host)
    }
}

data class StreamProfile(val url: RtspUrl, val width: Int, val height: Int, val encoding: String?)

data class StreamSet(
    val manufacturer: String?,
    val model: String?,
    val main: StreamProfile,
    val sub: StreamProfile?,
)

internal data class ProfileInfo(val token: String, val width: Int, val height: Int, val encoding: String?)

class OnvifClient(
    private val config: OnvifConfig = OnvifConfig(),
    private val resolver: HostResolver = HostResolver.SYSTEM,
) {
    private sealed interface AuthMode {
        data object WsSecurity : AuthMode

        class HttpDigest(val challenge: DigestChallenge) : AuthMode
    }

    /** Estado de autenticação de uma consulta (uma câmera). */
    private class Session(
        val creds: Credentials?,
        val clock: ClockOffset,
        var mode: AuthMode,
        /** Fim do prazo total da consulta (`System.nanoTime`). */
        val deadline: Long,
        /** IP conferido do host da câmera. */
        val address: InetAddress,
    )

    /**
     * Localiza o serviço ONVIF do host. Com porta, só ela é tentada; sem
     * porta, as portas usuais são tentadas em paralelo e vale a primeira que
     * responder a `GetSystemDateAndTime` (design D3).
     *
     * @throws OnvifException
     */
    fun locate(host: String, port: Int?): DeviceService {
        val ports = if (port != null) listOf(port) else config.probePorts
        if (ports.isEmpty()) throw OnvifException.NotOnvif()
        val address = resolver.resolve(host)
        val deadline = System.nanoTime() + TimeUnit.MILLISECONDS.toNanos(config.totalTimeoutMs)
        val results = LinkedBlockingQueue<Pair<DeviceService, Boolean>>()
        for (p in ports) {
            val device = DeviceService.forHost(host, p)
            thread(isDaemon = true, name = "onvif-sonda-$p") {
                val ok = try {
                    systemTime(device.url, address, deadline, config.probeConnectTimeoutMs)
                    true
                } catch (_: Exception) {
                    false
                }
                results.put(device to ok)
            }
        }
        var answered = 0
        while (true) {
            val remaining = deadline - System.nanoTime()
            if (remaining <= 0) break
            val (device, ok) = results.poll(remaining, TimeUnit.NANOSECONDS) ?: break
            answered++
            if (ok) return device
            if (answered == ports.size) break
        }
        throw OnvifException.NotOnvif()
    }

    /**
     * Obtém fabricante, modelo e os streams principal e secundário.
     *
     * @throws OnvifException
     */
    fun fetchStreams(device: DeviceService, creds: Credentials?): StreamSet {
        // Prazo total da consulta inteira; cada chamada recebe o restante.
        val deadline = System.nanoTime() + TimeUnit.MILLISECONDS.toNanos(config.totalTimeoutMs)
        val address = resolver.resolve(device.host)
        // Relógio da câmera, sem autenticação; falha aqui não impede a
        // consulta (a câmera pode estar com a hora certa).
        val clock = try {
            ClockOffset.between(systemTime(device.url, address, deadline, config.connectTimeoutMs), nowSeconds())
        } catch (_: OnvifException) {
            ClockOffset.ZERO
        }
        val session = Session(creds, clock, AuthMode.WsSecurity, deadline, address)

        var manufacturer: String? = null
        var model: String? = null
        try {
            val body = call(device.url, Soap.getDeviceInformation(), session)
            manufacturer = body.find("Manufacturer")?.textTrimmed()?.ifEmpty { null }
            model = body.find("Model")?.textTrimmed()?.ifEmpty { null }
        } catch (e: OnvifException.AuthenticationFailed) {
            throw e
        } catch (_: OnvifException) {
            // Informação opcional.
        }

        val mediaUrl = try {
            call(device.url, Soap.getCapabilitiesMedia(), session)
                .find("Media")?.child("XAddr")?.textTrimmed()
                ?.let { mediaServiceUrl(it, device.host) }
                ?: device.url
        } catch (e: OnvifException.AuthenticationFailed) {
            throw e
        } catch (_: OnvifException) {
            device.url
        }

        val profiles = parseProfiles(call(mediaUrl, Soap.getProfiles(), session))
        val (main, sub) = choose(profiles) ?: throw OnvifException.NoVideoProfiles()
        val mainProfile = streamProfile(mediaUrl, main, device.host, session)
        val subProfile = sub?.let {
            try {
                streamProfile(mediaUrl, it, device.host, session)
            } catch (_: OnvifException) {
                null
            }
        }
        return StreamSet(manufacturer, model, mainProfile, subProfile)
    }

    private fun streamProfile(mediaUrl: String, p: ProfileInfo, cameraHost: String, session: Session): StreamProfile {
        val body = call(mediaUrl, Soap.getStreamUri(p.token), session)
        val uri = body.find("MediaUri")?.child("Uri")?.textTrimmed()
            ?: throw OnvifException.Protocol("GetStreamUri sem Uri")
        return StreamProfile(sanitizeStreamUrl(uri, cameraHost), p.width, p.height, p.encoding)
    }

    /**
     * Chamada autenticada: WS-Security; se a câmera responder 401 com desafio
     * Digest, repete uma vez com HTTP Digest e mantém esse modo na sessão.
     */
    private fun call(url: String, body: String, session: Session): XmlElement {
        var digestRetryDone = false
        while (true) {
            val creds = session.creds
            var header: String? = null
            var authorization: String? = null
            if (creds != null) {
                when (val mode = session.mode) {
                    AuthMode.WsSecurity -> {
                        val created = session.clock.cameraTime(nowSeconds())
                        header = Auth.wsSecurityHeader(creds, Auth.randomBytes(16), created)
                    }
                    is AuthMode.HttpDigest -> {
                        val cnonce = Auth.hex(Auth.randomBytes(8))
                        authorization = mode.challenge.authorization(creds, "POST", pathOf(url), cnonce)
                    }
                }
            }
            val reply = Http.post(
                url, session.address, Soap.envelope(header, body), authorization, session.deadline,
                config.connectTimeoutMs,
            )
            if (reply.status == 401) {
                val challenge = reply.wwwAuthenticate?.let {
                    try {
                        DigestChallenge.parse(it)
                    } catch (_: DigestChallenge.ChallengeException) {
                        null
                    }
                }
                if (challenge != null && creds != null && !digestRetryDone) {
                    session.mode = AuthMode.HttpDigest(challenge)
                    digestRetryDone = true
                    continue
                }
                throw OnvifException.AuthenticationFailed()
            }
            return when (val r = Soap.readReply(reply.body)) {
                is Soap.Reply.Body ->
                    if (reply.status in 200..299) r.element else throw OnvifException.Protocol("HTTP ${reply.status}")
                is Soap.Reply.FaultReply ->
                    if (r.fault.isNotAuthorized()) {
                        throw OnvifException.AuthenticationFailed()
                    } else {
                        throw OnvifException.Protocol(redactCredentials(r.fault.reason))
                    }
                null -> throw OnvifException.NotOnvif()
            }
        }
    }

    /** `GetSystemDateAndTime` sem autenticação → hora UTC da câmera. */
    private fun systemTime(url: String, address: InetAddress, deadline: Long, connectTimeoutMs: Long): Long {
        val reply = Http.post(
            url, address, Soap.envelope(null, Soap.getSystemDateAndTime()), null, deadline, connectTimeoutMs,
        )
        val body = (Soap.readReply(reply.body) as? Soap.Reply.Body)?.element ?: throw OnvifException.NotOnvif()
        val utc = body.find("UTCDateTime") ?: throw OnvifException.Protocol("sem UTCDateTime")
        fun get(vararg path: String): Long =
            utc.path(*path)?.textTrimmed()?.toLongOrNull() ?: throw OnvifException.Protocol("data inválida")
        fun small(v: Long): Int = if (v in 0..Int.MAX_VALUE) v.toInt() else throw OnvifException.Protocol("data inválida")
        val y = get("Date", "Year")
        val mo = get("Date", "Month")
        val d = get("Date", "Day")
        val h = get("Time", "Hour")
        val mi = get("Time", "Minute")
        val s = get("Time", "Second")
        return Auth.utcToUnix(y, small(mo), small(d), small(h), small(mi), small(s))
            ?: throw OnvifException.Protocol("data inválida")
    }

    internal companion object {
        fun nowSeconds(): Long = System.currentTimeMillis() / 1000

        fun pathOf(url: String): String {
            if (!url.startsWith("http://")) return "/"
            val rest = url.substring("http://".length)
            val i = rest.indexOf('/')
            return if (i < 0) "/" else rest.substring(i)
        }

        fun parseProfiles(body: XmlElement): List<ProfileInfo> = body.findAll("Profiles").mapNotNull { p ->
            val token = p.attr("token") ?: return@mapNotNull null
            val vec = p.child("VideoEncoderConfiguration") ?: return@mapNotNull null
            val res = vec.child("Resolution") ?: return@mapNotNull null
            val width = res.child("Width")?.textTrimmed()?.toIntOrNull()?.takeIf { it >= 0 } ?: return@mapNotNull null
            val height = res.child("Height")?.textTrimmed()?.toIntOrNull()?.takeIf { it >= 0 } ?: return@mapNotNull null
            ProfileInfo(token, width, height, vec.child("Encoding")?.textTrimmed())
        }

        /**
         * Principal = maior resolução; secundário = menor, se for diferente
         * (design D3). Empate: vale a ordem do dispositivo.
         */
        fun choose(profiles: List<ProfileInfo>): Pair<ProfileInfo, ProfileInfo?>? {
            fun area(p: ProfileInfo) = p.width.toLong() * p.height.toLong()
            var main = profiles.firstOrNull() ?: return null
            var sub = main
            for (p in profiles) {
                if (area(p) > area(main)) main = p
                if (area(p) < area(sub)) sub = p
            }
            return main to sub.takeIf { area(it) < area(main) }
        }

        /**
         * Valida a URL RTSP devolvida, descarta credenciais embutidas e usa sempre o
         * host da câmera, mantendo esquema, porta e caminho/consulta (design D3,
         * ADR 0005). A URL é entrada de rede não confiável e a senha do cadastro vai
         * junto ao abrir o stream: o host anunciado (seja `0.0.0.0`, loopback, IP
         * interno atrás de NAT ou outro host qualquer) nunca é seguido.
         */
        fun sanitizeStreamUrl(uri: String, cameraHost: String): RtspUrl {
            val parsed = parseRtsp(uri)
            if (parsed.host == cameraHost) return parsed
            val oldAuthority = "//" + bracketed(parsed.host)
            val newAuthority = "//" + bracketed(cameraHost)
            return parseRtsp(parsed.toString().replaceFirst(oldAuthority, newAuthority))
        }

        private fun parseRtsp(text: String): RtspUrl = try {
            RtspUrl.parse(text).url
        } catch (e: InvalidUrlException) {
            throw OnvifException.Protocol("URL RTSP inválida (${e.message})")
        }

        /**
         * Endereço do serviço de mídia a partir do `XAddr` anunciado pela câmera.
         *
         * O `XAddr` é entrada de rede não confiável (ADR 0005): as chamadas seguintes
         * levam o digest das credenciais, então nunca vão a outro host. A URL usada é
         * sempre `http://<host da câmera>:<porta do XAddr><caminho do XAddr>`. `XAddr`
         * que não é `http` ou que não se consegue ler → `null` (vale o serviço de
         * dispositivo).
         */
        fun mediaServiceUrl(xaddr: String, cameraHost: String): String? {
            if (!xaddr.startsWith("http://")) return null
            val rest = xaddr.substring("http://".length)
            val end = authorityEnd(rest)
            val authority = rest.substring(0, end)
            val path = if (end < rest.length) rest.substring(end) else "/"
            // Descarta credenciais embutidas (`user:pass@`).
            val hostport = authority.substringAfterLast('@')
            val portText = if (hostport.startsWith('[')) {
                val close = hostport.indexOf(']')
                if (close < 0) return null
                hostport.substring(close + 1).let { if (it.startsWith(':')) it.substring(1) else null }
            } else {
                hostport.indexOf(':').let { if (it < 0) null else hostport.substring(it + 1) }
            }
            val port = if (portText != null) parsePort(portText) ?: return null else 80
            if (!path.startsWith('/') || path.any { it.isISOControl() || it.isWhitespace() }) return null
            return "http://${bracketed(cameraHost)}:$port$path"
        }
    }
}

internal fun bracketed(host: String): String = if (host.contains(':')) "[$host]" else host

internal fun authorityEnd(rest: String): Int =
    rest.indexOfFirst { it == '/' || it == '?' || it == '#' }.let { if (it < 0) rest.length else it }

/** Porta 1..65535 só com dígitos ASCII; `null` se inválida. */
internal fun parsePort(p: String): Int? {
    if (p.isEmpty() || !p.all { it in '0'..'9' }) return null
    val digits = p.trimStart('0')
    if (digits.length > 5) return null
    return digits.ifEmpty { "0" }.toInt().takeIf { it in 1..65535 }
}
