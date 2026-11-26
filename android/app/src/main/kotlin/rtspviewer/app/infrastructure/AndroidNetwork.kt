package rtspviewer.app.infrastructure

import android.content.Context
import android.net.ConnectivityManager
import android.net.Network
import android.net.NetworkCapabilities
import android.net.wifi.WifiManager
import java.io.IOException
import java.net.DatagramSocket
import java.net.Inet4Address
import rtspviewer.core.application.CameraDiscovery
import rtspviewer.onvif.Ipv4Interface

/**
 * Rede em que a busca procura câmeras: Wi-Fi ou Ethernet, nunca VPN. Uma VPN
 * herda os transportes das redes por baixo dela (aparece como Wi-Fi), então
 * também é recusada a rede sem `NET_CAPABILITY_NOT_VPN`; senão o endereço do
 * túnel entraria na busca e os Probes iriam para a sub-rede remota.
 */
internal fun isLocalSearchNetwork(wifi: Boolean, ethernet: Boolean, vpnTransport: Boolean, notVpn: Boolean): Boolean =
    (wifi || ethernet) && !vpnTransport && notVpn

/**
 * Redes Wi-Fi e Ethernet ativas (sem VPN; [isLocalSearchNetwork]), lidas do `ConnectivityManager` (endereço IPv4
 * e prefixo das `LinkProperties`; design D4). Cada socket da busca fica preso
 * à rede dele, para o envio não sair pela rede padrão (dados móveis, VPN).
 *
 * Sem teste no JVM (depende do SDK do Android); coberto pelo teste no celular.
 */
class ConnectivityNetworkInterfaces(context: Context) : NetworkInterfaces {
    private val connectivity = context.applicationContext.getSystemService(ConnectivityManager::class.java)

    override fun list(): List<Ipv4Interface> {
        val cm = connectivity ?: return emptyList()
        val out = mutableListOf<Ipv4Interface>()
        // `allNetworks` está obsoleto desde a API 31 em favor de callbacks; a
        // busca precisa só de uma leitura pontual das redes ativas.
        @Suppress("DEPRECATION")
        val networks = cm.allNetworks
        for (network in networks) {
            val caps = cm.getNetworkCapabilities(network) ?: continue
            val local = isLocalSearchNetwork(
                wifi = caps.hasTransport(NetworkCapabilities.TRANSPORT_WIFI),
                ethernet = caps.hasTransport(NetworkCapabilities.TRANSPORT_ETHERNET),
                vpnTransport = caps.hasTransport(NetworkCapabilities.TRANSPORT_VPN),
                notVpn = caps.hasCapability(NetworkCapabilities.NET_CAPABILITY_NOT_VPN),
            )
            if (!local) continue
            val links = cm.getLinkProperties(network)?.linkAddresses ?: continue
            for (link in links) {
                val address = link.address as? Inet4Address ?: continue
                if (address.isLoopbackAddress || address.isAnyLocalAddress) continue
                out.add(Ipv4Interface(address, link.prefixLength) { socket -> bindTo(network, socket) })
            }
        }
        return out.distinctBy { it.address }
    }

    private fun bindTo(network: Network, socket: DatagramSocket) {
        try {
            network.bindSocket(socket)
        } catch (_: IOException) {
            // Melhor esforço: o socket já está ligado ao endereço da interface.
        }
    }
}

/**
 * `WifiManager.MulticastLock`: sem ele, muitos aparelhos descartam multicast
 * com a tela ligada para economizar bateria. Contado por referência, para
 * buscas sobrepostas (uma cancelada e outra nova) não soltarem a trava uma da
 * outra.
 */
class WifiMulticastGuard(context: Context) : MulticastGuard {
    private val lock = context.applicationContext.getSystemService(WifiManager::class.java)
        ?.createMulticastLock("rtsp-viewer:busca")
        ?.apply { setReferenceCounted(true) }

    override fun acquire() {
        lock?.acquire()
    }

    override fun release() {
        if (lock?.isHeld == true) lock.release()
    }
}

/** Busca de câmeras do app: redes Wi-Fi/Ethernet e trava de multicast do Android. */
fun androidCameraDiscovery(context: Context): CameraDiscovery =
    OnvifCameraDiscovery(ConnectivityNetworkInterfaces(context), WifiMulticastGuard(context))
