package rtspviewer.onvif

import java.net.InetAddress
import java.net.ServerSocket
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertFalse
import kotlin.test.assertNotNull
import kotlin.test.assertNull
import kotlin.test.assertTrue
import rtspviewer.core.domain.Password
import rtspviewer.onvif.support.Prefixes
import rtspviewer.onvif.support.SimAuth
import rtspviewer.onvif.support.SimConfig
import rtspviewer.onvif.support.SimProfile
import rtspviewer.onvif.support.Simulator
import rtspviewer.onvif.support.UNRESTRICTED

/**
 * Cliente ONVIF contra o simulador em processo (mesmos casos de
 * `desktop/crates/onvif/tests/client.rs`; spec `camera-discovery`, requisitos
 * "Consulta dos streams pela câmera" e "Autenticação ONVIF sem senha em texto
 * puro").
 */
class ClientSimulatorTest {
    private fun creds(cfg: SimConfig) = Credentials(cfg.username, Password(cfg.password))

    private fun client(resolver: HostResolver = UNRESTRICTED) = OnvifClient(
        OnvifConfig(connectTimeoutMs = 2_000, totalTimeoutMs = 5_000, probePorts = emptyList(), probeConnectTimeoutMs = 500),
        resolver,
    )

    private fun device(sim: Simulator) = assertNotNull(DeviceService.fromUrl(sim.deviceUrl()))

    private fun fetch(cfg: SimConfig, block: (Simulator, StreamSet) -> Unit = { _, _ -> }) =
        Simulator(cfg).use { sim -> client().fetchStreams(device(sim), creds(cfg)).also { block(sim, it) } }

    @Test
    fun obtemPrincipalESecundarioComWsSecurity() {
        val set = fetch(SimConfig())
        assertEquals("Simulador", set.manufacturer)
        assertEquals("Camera ONVIF de teste", set.model)
        assertEquals("rtsp://127.0.0.1:8554/main", set.main.url.toString())
        assertEquals(1920 to 1080, set.main.width to set.main.height)
        val sub = assertNotNull(set.sub)
        assertEquals("rtsp://127.0.0.1:8554/sub", sub.url.toString())
        assertEquals(640 to 360, sub.width to sub.height)
    }

    /** Respostas com outros prefixos de namespace (estilo de outros firmwares; valores fictícios). */
    @Test
    fun respostasComOutrosPrefixos() {
        val set = fetch(SimConfig(prefixes = Prefixes(env = "SOAP-ENV", tds = "ns1", trt = "ns2", tt = "ns3")))
        assertEquals("rtsp://127.0.0.1:8554/main", set.main.url.toString())
        assertEquals("rtsp://127.0.0.1:8554/sub", set.sub?.url.toString())
    }

    @Test
    fun senhaNuncaTrafegaEmTextoPuro() {
        val cfg = SimConfig()
        fetch(cfg) { sim, _ ->
            val traffic = sim.allRequests()
            assertTrue(traffic.contains("PasswordDigest"))
            assertFalse(traffic.contains(cfg.password), "senha em texto puro na rede")
            assertFalse(traffic.contains("PasswordText"))
        }
    }

    @Test
    fun relogioDaCameraComHorasDeDiferenca() {
        for (offset in listOf(3 * 3600L, -5 * 3600L, 400 * 86_400L)) {
            fetch(SimConfig(clockOffset = offset))
        }
    }

    @Test
    fun cameraQueExigeHttpDigest() {
        val cfg = SimConfig(auth = SimAuth.HttpDigest)
        fetch(cfg) { sim, set ->
            assertEquals("rtsp://127.0.0.1:8554/main", set.main.url.toString())
            val traffic = sim.allRequests()
            assertTrue(traffic.lowercase().contains("authorization: digest"))
            assertFalse(traffic.contains(cfg.password))
        }
    }

    @Test
    fun senhaErradaComWsSecurityEComDigest() {
        for (auth in listOf(SimAuth.WsSecurity, SimAuth.HttpDigest)) {
            val cfg = SimConfig(auth = auth)
            Simulator(cfg).use { sim ->
                val wrong = Credentials(cfg.username, Password("errada-123"))
                val e = assertFailsWith<OnvifException.AuthenticationFailed>(auth.name) {
                    client().fetchStreams(device(sim), wrong)
                }
                assertFalse(e.message.orEmpty().contains("errada-123"))
                assertFalse(sim.allRequests().contains("errada-123"))
            }
        }
    }

    @Test
    fun semCredenciaisEmCameraQueExige() {
        Simulator(SimConfig()).use { sim ->
            assertFailsWith<OnvifException.AuthenticationFailed> { client().fetchStreams(device(sim), null) }
        }
    }

    @Test
    fun cameraAbertaSemCredenciais() {
        Simulator(SimConfig(auth = SimAuth.Open)).use { sim -> client().fetchStreams(device(sim), null) }
    }

    @Test
    fun hostsInvalidosTrocadosPeloDaCamera() {
        val set = fetch(SimConfig(mediaHostZero = true, profiles = listOf(SimProfile("main", 1280, 720, "rtsp://0.0.0.0:554/stream1"))))
        assertEquals("rtsp://127.0.0.1:554/stream1", set.main.url.toString())
        assertNull(set.sub)
    }

    /**
     * XAddr de mídia apontando para outro host (câmera maliciosa ou NAT): as
     * chamadas autenticadas seguem para o host da câmera, nunca para o
     * anunciado (ADR 0005). 192.0.2.10 é TEST-NET-1: se o cliente o seguisse,
     * a consulta não chegaria aos perfis.
     */
    @Test
    fun xaddrDeMidiaEmOutroHostNaoRecebeCredenciais() {
        for (auth in listOf(SimAuth.WsSecurity, SimAuth.HttpDigest)) {
            fetch(SimConfig(auth = auth, mediaHost = "192.0.2.10")) { sim, set ->
                assertEquals("rtsp://127.0.0.1:8554/main", set.main.url.toString(), auth.name)
                val traffic = sim.allRequests()
                assertTrue(traffic.contains("GetProfiles"), auth.name)
                assertTrue(traffic.contains("GetStreamUri"), auth.name)
            }
        }
    }

    /** O prazo total vale para a consulta inteira, não para cada chamada (D1): seis chamadas de 400 ms passam de 1,5 s. */
    @Test
    fun prazoTotalValeParaAConsultaInteira() {
        val cfg = SimConfig(responseDelayMs = 400)
        Simulator(cfg).use { sim ->
            val c = OnvifClient(OnvifConfig(connectTimeoutMs = 2_000, totalTimeoutMs = 1_500, probePorts = emptyList()), UNRESTRICTED)
            val start = System.nanoTime()
            assertFailsWith<OnvifException.Unreachable> { c.fetchStreams(device(sim), creds(cfg)) }
            val elapsedMs = (System.nanoTime() - start) / 1_000_000
            assertTrue(elapsedMs < 2_500, "$elapsedMs ms")
        }
    }

    /**
     * URL RTSP de `GetStreamUri` com outro host: a senha do cadastro vai junto
     * ao abrir o stream, então o host é sempre o da câmera; esquema, porta e
     * caminho/consulta são mantidos (ADR 0005).
     */
    @Test
    fun urlRtspEmOutroHostUsaOHostDaCamera() {
        for ((uri, esperada) in listOf(
            "rtsp://203.0.113.7:554/live/ch0?subtype=0" to "rtsp://127.0.0.1:554/live/ch0?subtype=0",
            "rtsps://camera.example:8555/stream" to "rtsps://127.0.0.1:8555/stream",
            "rtsp://u:p@[2001:db8::9]/s" to "rtsp://127.0.0.1/s",
        )) {
            val set = fetch(SimConfig(profiles = listOf(SimProfile("main", 1280, 720, uri))))
            assertEquals(esperada, set.main.url.toString(), uri)
        }
    }

    @Test
    fun credenciaisEmbutidasNaUrlDevolvidaSaoDescartadas() {
        val set = fetch(SimConfig(profiles = listOf(SimProfile("main", 1280, 720, "rtsp://admin:outra@{host}:554/s"))))
        assertEquals("rtsp://127.0.0.1:554/s", set.main.url.toString())
    }

    @Test
    fun semPerfisDeVideo() {
        assertFailsWith<OnvifException.NoVideoProfiles> { fetch(SimConfig(profiles = emptyList())) }
    }

    @Test
    fun respostaGrandeDemaisERecusada() {
        assertFailsWith<OnvifException.Protocol> { fetch(SimConfig(oversizedProfiles = true)) }
    }

    @Test
    fun servidorQueNaoEOnvif() {
        Simulator(SimConfig(notOnvif = true)).use { sim ->
            assertFailsWith<OnvifException.NotOnvif> { client().locate("127.0.0.1", sim.port) }
        }
    }

    /** Porta livre sem servidor (para simular porta fechada). */
    private fun closedPort(): Int = ServerSocket(0, 1, InetAddress.getLoopbackAddress()).use { it.localPort }

    @Test
    fun localizaAPortaOnvifEntreAsUsuais() {
        Simulator(SimConfig()).use { sim ->
            val c = OnvifClient(OnvifConfig(probePorts = listOf(closedPort(), sim.port, closedPort()), probeConnectTimeoutMs = 500), UNRESTRICTED)
            assertEquals(sim.deviceUrl(), c.locate("127.0.0.1", null).url)
        }
    }

    @Test
    fun nenhumaPortaResponde() {
        val c = OnvifClient(
            OnvifConfig(probePorts = listOf(closedPort(), closedPort()), probeConnectTimeoutMs = 300, totalTimeoutMs = 2_000),
            UNRESTRICTED,
        )
        assertFailsWith<OnvifException.NotOnvif> { c.locate("127.0.0.1", null) }
    }

    /**
     * Destino recusado por quem resolve (no app, a regra da rede local do ADR
     * 0006): a consulta falha antes de qualquer envio.
     */
    @Test
    fun destinoRecusadoNaoRecebeNada() {
        val cfg = SimConfig()
        Simulator(cfg).use { sim ->
            val recusa = HostResolver { throw OnvifException.Unreachable("fora da rede local") }
            val e = assertFailsWith<OnvifException.Unreachable> { client(recusa).fetchStreams(device(sim), creds(cfg)) }
            assertEquals("fora da rede local", e.detail)
            assertFailsWith<OnvifException.Unreachable> {
                OnvifClient(OnvifConfig(probePorts = listOf(sim.port)), recusa).locate("127.0.0.1", null)
            }
            assertTrue(sim.requests.isEmpty())
        }
    }

    /**
     * Redirecionamento HTTP nunca é seguido (ADR 0006, tarefa 3.5): a câmera
     * responde 3xx com `Location` para outro servidor e esse servidor não recebe
     * nenhum pedido — nem a sondagem, nem as chamadas com WS-Security, nem a
     * repetição com HTTP Digest (`Authorization`). A consulta termina em erro.
     */
    @Test
    fun redirecionamentoNaoESeguido() {
        for (status in listOf(301, 302, 303, 307, 308)) {
            for (auth in listOf(SimAuth.WsSecurity, SimAuth.HttpDigest, SimAuth.Open)) {
                val caso = "$status $auth"
                Simulator(SimConfig(auth = SimAuth.Open)).use { destino ->
                    val cfg = SimConfig(auth = auth, redirect = status to destino.deviceUrl())
                    Simulator(cfg).use { camera ->
                        assertFailsWith<OnvifException.NotOnvif>(caso) { client().locate("127.0.0.1", camera.port) }
                        assertFailsWith<OnvifException.NotOnvif>(caso) {
                            client().fetchStreams(device(camera), creds(cfg))
                        }
                        val trafego = camera.allRequests()
                        assertTrue(trafego.contains("GetProfiles"), caso)
                        if (auth == SimAuth.HttpDigest) {
                            assertTrue(trafego.lowercase().contains("authorization: digest"), caso)
                        }
                        assertTrue(destino.requests.isEmpty(), "$caso: destino do redirecionamento recebeu pedido")
                    }
                }
            }
        }
    }

    /**
     * Nome resolvido uma vez por consulta: as chamadas vão ao IP conferido e a
     * URL RTSP fica com o nome da câmera (o que o usuário digitou).
     */
    @Test
    fun chamadasVaoAoIpResolvidoEUrlFicaComONome() {
        val cfg = SimConfig()
        Simulator(cfg).use { sim ->
            val pedidos = mutableListOf<String>()
            val resolver = HostResolver { host ->
                pedidos.add(host)
                InetAddress.getLoopbackAddress()
            }
            val c = OnvifClient(OnvifConfig(probePorts = listOf(sim.port)), resolver)
            val d = c.locate("camera.test", null)
            assertEquals("http://camera.test:${sim.port}/onvif/device_service", d.url)
            val set = c.fetchStreams(d, creds(cfg))
            assertEquals("rtsp://camera.test:8554/main", set.main.url.toString())
            assertEquals(listOf("camera.test", "camera.test"), pedidos)
        }
    }
}
