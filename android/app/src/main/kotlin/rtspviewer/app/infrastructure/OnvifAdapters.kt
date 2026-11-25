package rtspviewer.app.infrastructure

import java.net.InetAddress
import java.net.InetSocketAddress
import java.net.UnknownHostException
import rtspviewer.core.application.CameraDiscovery
import rtspviewer.core.application.DiscoveredCamera
import rtspviewer.core.application.DiscoveryFailure
import rtspviewer.core.application.ResolveError
import rtspviewer.core.application.ResolvedStreams
import rtspviewer.core.application.StreamResolver
import rtspviewer.core.application.StreamTarget
import rtspviewer.core.domain.Password
import rtspviewer.onvif.Credentials
import rtspviewer.onvif.DeviceService
import rtspviewer.onvif.Discovery
import rtspviewer.onvif.DiscoveryConfig
import rtspviewer.onvif.DiscoveryException
import rtspviewer.onvif.HostResolver
import rtspviewer.onvif.Ipv4Interface
import rtspviewer.onvif.LocalAddresses
import rtspviewer.onvif.OnvifClient
import rtspviewer.onvif.OnvifConfig
import rtspviewer.onvif.OnvifException
import rtspviewer.onvif.StreamSet

/**
 * Adaptadores das portas `StreamResolver` e `CameraDiscovery` com o módulo
 * `onvif` (espelham `desktop/crates/app/src/infrastructure/onvif_adapters.rs`).
 * Bloqueiam: o app os chama fora da thread da UI. Sem dependência do SDK do
 * Android (testados no JVM); a parte do Android fica em `AndroidNetwork.kt`.
 */

/**
 * Regra do ADR 0006 (design D8): o ONVIF em HTTP do app só vai para endereços
 * privados, link-local ou loopback, conferidos **depois** da resolução do
 * nome. O cliente conecta ao IP devolvido aqui, sem nova consulta DNS, então
 * um nome que resolve para IP público não recebe nada.
 */
class LocalNetworkResolver(
    private val lookup: (String) -> Array<InetAddress> = InetAddress::getAllByName,
) : HostResolver {
    override fun resolve(host: String): InetAddress {
        val literal = LocalAddresses.parseLiteral(host)
        val addresses = if (literal != null) {
            arrayOf(InetAddress.getByAddress(literal))
        } else {
            try {
                lookup(host)
            } catch (_: UnknownHostException) {
                throw OnvifException.Unreachable("endereço $host não encontrado")
            } catch (_: SecurityException) {
                throw OnvifException.Unreachable("endereço $host não encontrado")
            }
        }
        return addresses.firstOrNull(LocalAddresses::isLocal)
            ?: throw OnvifException.Unreachable(
                "$host não é um endereço da rede local. O app só consulta câmeras em endereços " +
                    "privados, link-local ou loopback.",
            )
    }
}

class OnvifStreamResolver(
    private val client: OnvifClient = OnvifClient(OnvifConfig(), LocalNetworkResolver()),
) : StreamResolver {
    override fun resolve(target: StreamTarget, username: String?, password: Password?): ResolvedStreams {
        try {
            val device = when (target) {
                is StreamTarget.Address -> client.locate(target.address.host, target.address.port)
                is StreamTarget.ServiceUrl -> DeviceService.fromUrl(target.url) ?: throw ResolveError.NotOnvif()
            }
            val creds = username?.let { Credentials(it, password ?: Password("")) }
            return toResolved(client.fetchStreams(device, creds))
        } catch (e: OnvifException) {
            throw mapError(e)
        }
    }

    internal companion object {
        fun toResolved(set: StreamSet): ResolvedStreams {
            val m = set.manufacturer
            val model = set.model
            val name = when {
                m != null && model != null && model.lowercase().contains(m.lowercase()) -> model
                m != null && model != null -> "$m $model"
                else -> model ?: m
            }
            val suggested = name?.let { takeCodePoints(it, 60).trim() }?.ifEmpty { null }
            return ResolvedStreams(set.main.url, set.sub?.url, suggested)
        }

        fun mapError(e: OnvifException): ResolveError = when (e) {
            is OnvifException.NotOnvif -> ResolveError.NotOnvif()
            is OnvifException.AuthenticationFailed -> ResolveError.AuthenticationFailed()
            is OnvifException.NoVideoProfiles -> ResolveError.NoVideoProfiles()
            is OnvifException.Unreachable -> ResolveError.Unreachable(e.detail)
            is OnvifException.Protocol -> ResolveError.Protocol(e.detail)
        }

        private fun takeCodePoints(s: String, n: Int): String =
            if (s.codePointCount(0, s.length) > n) s.substring(0, s.offsetByCodePoints(0, n)) else s
    }
}

/** Redes IPv4 de onde a busca envia (no Android, Wi-Fi e Ethernet ativas). */
fun interface NetworkInterfaces {
    fun list(): List<Ipv4Interface>
}

/** Trava que mantém o multicast ligado durante a busca (no Android, o `WifiManager.MulticastLock`). */
interface MulticastGuard {
    fun acquire()

    fun release()
}

/**
 * Busca WS-Discovery (3 s) pelas redes dadas, com a trava de multicast só
 * enquanto dura: ela é solta ao terminar, em erro e em cancelamento
 * (interrupção da thread).
 */
class OnvifCameraDiscovery(
    private val interfaces: NetworkInterfaces,
    private val multicast: MulticastGuard,
    private val durationMs: Long = 3_000,
    private val target: InetSocketAddress = Discovery.MULTICAST,
    /** Destinos unicast injetados (testes); `null` = a sub-rede de cada rede. */
    private val unicast: List<InetSocketAddress>? = null,
) : CameraDiscovery {
    override fun discover(): List<DiscoveredCamera> {
        var held = false
        try {
            // Sem Wi-Fi nem Ethernet (só dados móveis): nada é enviado e a busca
            // termina vazia, para a tela mostrar a dica do Wi-Fi (spec
            // `camera-discovery`, ajuste do grupo 5).
            val networks = interfaces.list()
            if (networks.isEmpty()) return emptyList()
            val config = DiscoveryConfig(networks, durationMs, target, unicast)
            multicast.acquire()
            held = true
            return Discovery.discover(config).map { DiscoveredCamera(it.host, it.deviceUrl, it.name) }
        } catch (e: DiscoveryException) {
            throw DiscoveryFailure(e.message.orEmpty())
        } finally {
            if (held) multicast.release()
        }
    }
}
