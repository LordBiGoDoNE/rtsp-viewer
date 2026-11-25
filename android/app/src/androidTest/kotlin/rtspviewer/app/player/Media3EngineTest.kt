package rtspviewer.app.player

import android.graphics.SurfaceTexture
import android.view.Surface
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeout
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import rtspviewer.core.application.StreamError
import rtspviewer.core.application.StreamRequest
import rtspviewer.core.domain.CameraId
import rtspviewer.core.domain.Password
import rtspviewer.core.domain.RtspUrl

/**
 * Player contra a câmera falsa (`test-camera/`). O endereço vem do argumento
 * `rtspBase` da instrumentação (no CI, `rtsp://10.0.2.2:8554`; no celular, o IP
 * do PC). Sem o argumento, os testes são ignorados.
 */
@RunWith(AndroidJUnit4::class)
class Media3EngineTest {
    private val instrumentation = InstrumentationRegistry.getInstrumentation()
    private val base: String? = InstrumentationRegistry.getArguments().getString("rtspBase")
    private lateinit var scope: CoroutineScope
    private lateinit var engine: Media3Engine
    private lateinit var controller: PlayerController
    private lateinit var texture: SurfaceTexture

    @Before
    fun prepara() {
        assumeTrue("defina -e rtspBase rtsp://<host>:8554", base != null)
        scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
        instrumentation.runOnMainSync {
            texture = SurfaceTexture(0)
            engine = Media3Engine(instrumentation.targetContext).apply { setSurface(Surface(texture)) }
            controller = PlayerController(engine, scope)
        }
    }

    @After
    fun limpa() {
        if (base == null) return
        instrumentation.runOnMainSync {
            controller.stop()
            texture.release()
        }
        scope.cancel()
    }

    private fun request(path: String, user: String? = null, pass: String? = null) = StreamRequest(
        CameraId.generate(),
        RtspUrl.parse("$base/$path").url.withCredentials(user, pass?.let(::Password)),
        "$base/$path",
    )

    private fun abre(req: StreamRequest) = runBlocking { withContext(Dispatchers.Main) { controller.start(req) } }

    private fun <T> espera(ms: Long, bloco: suspend () -> T): T = runBlocking { withTimeout(ms) { bloco() } }

    @Test
    fun abreH264EMostraInformacoes() {
        val inicio = System.nanoTime()
        abre(request("h264"))
        espera(10_000) { controller.state.first { it == PlayerState.Playing } }
        val segundos = (System.nanoTime() - inicio) / 1e9
        val info = espera(5_000) { controller.info.first { it != null }!! }
        assertEquals("h264", info.codec)
        assertEquals(1280, info.width)
        assertEquals(720, info.height)
        assertTrue("abriu em ${"%.2f".format(segundos)} s", segundos < 4.0)
    }

    @Test
    fun abreH265() {
        abre(request("h265"))
        espera(10_000) { controller.state.first { it == PlayerState.Playing } }
        assertEquals("hevc", espera(5_000) { controller.info.first { it != null }!! }.codec)
    }

    @Test
    fun senhaErradaNaoReconecta() {
        abre(request("auth", "teste", "errada"))
        val s = espera(10_000) { controller.state.first { it is PlayerState.Failed } } as PlayerState.Failed
        assertTrue(s.error.toString(), s.error is StreamError.AuthenticationFailed)
    }

    @Test
    fun senhaCertaReproduz() {
        abre(request("auth", "teste", "teste123"))
        espera(10_000) { controller.state.first { it == PlayerState.Playing } }
    }

    @Test
    fun cameraInacessivelTentaDeNovo() {
        // Mesmo host, porta fechada: conexão recusada.
        val host = RtspUrl.parse(base!!).url.host
        abre(StreamRequest(CameraId.generate(), RtspUrl.parse("rtsp://$host:9/s").url.withCredentials(null, null), "rtsp://$host:9/s"))
        val s = espera(15_000) { controller.state.first { it is PlayerState.Reconnecting || it is PlayerState.Failed } }
        assertTrue(s.toString(), s is PlayerState.Reconnecting)
    }

    @Test
    fun pararEncerraEmMenosDeUmSegundo() {
        abre(request("h264"))
        espera(10_000) { controller.state.first { it == PlayerState.Playing } }
        val inicio = System.nanoTime()
        instrumentation.runOnMainSync { controller.stop() }
        val ms = (System.nanoTime() - inicio) / 1e6
        assertTrue("parou em $ms ms", ms < 1_000)
    }
}
