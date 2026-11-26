package rtspviewer.app.infrastructure

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import java.io.File
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import rtspviewer.core.domain.Camera
import rtspviewer.core.domain.CameraId
import rtspviewer.core.domain.CameraName
import rtspviewer.core.domain.Password
import rtspviewer.core.domain.RtspUrl

/** Cofre com o Keystore real e persistência no armazenamento privado do app. */
@RunWith(AndroidJUnit4::class)
class DeviceStorageTest {
    private val context = InstrumentationRegistry.getInstrumentation().targetContext
    private lateinit var dir: File
    private val cipher = KeystoreSecretCipher("rtsp-viewer-test-${System.nanoTime()}")

    @Before
    fun prepara() {
        dir = File(context.filesDir, "teste-${System.nanoTime()}").apply { mkdirs() }
    }

    @After
    fun limpa() {
        dir.deleteRecursively()
        cipher.deleteKey()
    }

    @Test
    fun keystoreCifraEDecifra() {
        val store = FileCredentialStore(dir, cipher)
        val id = CameraId.generate()
        store.set(id, Password("s3gr3do-ção"))
        assertEquals("s3gr3do-ção", store.get(id)!!.expose())
        assertFalse(store.file.readText().contains("s3gr3do"))
    }

    @Test
    fun chaveRemovidaFazASenhaSerPedidaDeNovo() {
        val store = FileCredentialStore(dir, cipher)
        val id = CameraId.generate()
        store.set(id, Password("x"))
        cipher.deleteKey()
        assertNull(store.get(id))
        store.set(id, Password("y"))
        assertEquals("y", store.get(id)!!.expose())
    }

    @Test
    fun ivMudaACadaGravacao() {
        val store = FileCredentialStore(dir, cipher)
        val id = CameraId.generate()
        store.set(id, Password("mesma"))
        val primeiro = store.file.readText()
        store.set(id, Password("mesma"))
        assertTrue(primeiro != store.file.readText())
    }

    @Test
    fun listaNoArmazenamentoPrivado() {
        val repo = JsonCameraRepository(dir)
        val cam = Camera.create(CameraName.parse("Garagem"), RtspUrl.parse("rtsp://10.0.2.2:8554/h264").url, "admin")
        repo.save(listOf(cam))
        assertEquals(listOf(cam), repo.load().cameras)
        repo.file.writeText("{ quebrado")
        val loaded = repo.load()
        assertTrue(loaded.cameras.isEmpty())
        assertNotNull(loaded.warning)
    }
}
