package rtspviewer.app.infrastructure

import java.io.File
import java.nio.file.Files
import kotlin.test.AfterTest
import kotlin.test.Test
import kotlin.test.assertContentEquals
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertIs
import kotlin.test.assertNotNull
import kotlin.test.assertNull
import kotlin.test.assertTrue
import rtspviewer.core.application.RepositoryWarning
import rtspviewer.core.domain.Camera
import rtspviewer.core.domain.CameraName
import rtspviewer.core.domain.RtspUrl

/** Mesmos casos de `desktop/crates/app/src/infrastructure/json_repository.rs`. */
class JsonCameraRepositoryTest {
    private val dirs = mutableListOf<File>()

    private fun tempDir(): File = Files.createTempDirectory("rtsp-viewer-test").toFile().also { dirs += it }

    @AfterTest
    fun limpa() {
        dirs.forEach { it.deleteRecursively() }
    }

    private fun camera(name: String, url: String, user: String?) =
        Camera.create(CameraName.parse(name), RtspUrl.parse(url).url, user)

    @Test
    fun semArquivoListaVazia() {
        val loaded = JsonCameraRepository(File(tempDir(), "nao-existe")).load()
        assertTrue(loaded.cameras.isEmpty())
        assertNull(loaded.warning)
    }

    @Test
    fun gravaELeDeVolta() {
        val dir = tempDir()
        val repo = JsonCameraRepository(dir)
        val cams = listOf(
            camera("Garagem", "rtsp://192.168.0.10:554/stream1", "admin"),
            camera("Portão", "rtsps://cam.local/live", null),
        )
        repo.save(cams)
        assertEquals(cams, repo.load().cameras)
        assertFalse(File(dir, "cameras.json.tmp").exists())
    }

    @Test
    fun arquivoTemVersaoENaoTemSenha() {
        val repo = JsonCameraRepository(tempDir())
        repo.save(listOf(camera("A", "rtsp://admin:segredo@h/s", "admin")))
        val text = repo.file.readText()
        assertTrue(text.contains("\"version\": 1"), text)
        assertFalse(text.contains("segredo"))
    }

    @Test
    fun formatoIgualAoDoDesktop() {
        val repo = JsonCameraRepository(tempDir())
        repo.save(listOf(camera("A", "rtsp://h/s", null)))
        val text = repo.file.readText()
        assertFalse(text.contains("username"), "campo ausente quando não há usuário")
        assertFalse(text.contains("substream_url"))
        assertFalse(text.contains("null"))
    }

    @Test
    fun senhaComCaracteresReservadosNuncaChegaAoArquivo() {
        for (url in listOf("rtsp://admin:1234/abc@10.0.0.2/s", "rtsp://admin:1234?abc@host/s", "rtsp://admin:12345#x@192.168.0.10/stream")) {
            val repo = JsonCameraRepository(tempDir())
            val json = """{"version":1,"cameras":[{"id":"6f1c2a9e-1d55-4d3c-9b6c-0d0b9d7c2f11","name":"A","url":"$url"}]}"""
            repo.file.writeText(json)
            val loaded = repo.load()
            assertTrue(loaded.cameras.isEmpty(), url)
            assertNotNull(loaded.warning, url)
        }
    }

    @Test
    fun arquivoCorrompidoEPreservado() {
        val repo = JsonCameraRepository(tempDir(), clock = { 1_795_516_365_000 })
        repo.file.writeText("{ isto nao e json")
        val loaded = repo.load()
        assertTrue(loaded.cameras.isEmpty())
        val warning = assertIs<RepositoryWarning.CorruptedFileRecovered>(loaded.warning)
        assertTrue(warning.backup.endsWith("cameras.json.corrompido-20261124-103245"), warning.backup)
        assertContentEquals("{ isto nao e json".toByteArray(), File(warning.backup).readBytes())
        assertFalse(repo.file.exists())
    }

    @Test
    fun dadosInvalidosContamComoCorrompido() {
        for (
            conteudo in listOf(
                """{"version":2,"cameras":[]}""",
                """{"version":1,"cameras":[{"id":"x","name":"A","url":"rtsp://h"}]}""",
                """{"version":1,"cameras":[{"id":"6f1c2a9e-1d55-4d3c-9b6c-0d0b9d7c2f11","name":"A","url":"rtsp://u:p@h"}]}""",
            )
        ) {
            val repo = JsonCameraRepository(tempDir())
            repo.file.writeText(conteudo)
            assertNotNull(repo.load().warning, conteudo)
        }
    }

    @Test
    fun temporarioAbandonadoNaoAfetaOArquivo() {
        val dir = tempDir()
        val repo = JsonCameraRepository(dir)
        val cams = listOf(camera("A", "rtsp://h/s", null))
        repo.save(cams)
        File(dir, "cameras.json.tmp").writeText("{ pela metade")
        assertEquals(cams, repo.load().cameras)
        repo.save(cams)
        assertEquals(cams, repo.load().cameras)
    }

    @Test
    fun secundarioIdaEVolta() {
        val repo = JsonCameraRepository(tempDir())
        val cam = camera("Portão", "rtsp://192.168.1.164:554/stream1", "admin")
        cam.changeSubstream(RtspUrl.parse("rtsp://192.168.1.164:554/stream2").url)
        repo.save(listOf(cam))
        assertTrue(repo.file.readText().contains("\"substream_url\": \"rtsp://192.168.1.164:554/stream2\""))
        assertEquals(listOf(cam), repo.load().cameras)
    }

    @Test
    fun arquivoDaVersaoAnteriorCarregaSemAviso() {
        val repo = JsonCameraRepository(tempDir())
        repo.file.writeText(
            """{"version":1,"cameras":[{"id":"6f1c2a9e-1d55-4d3c-9b6c-0d0b9d7c2f11","name":"A","url":"rtsp://h/s","username":"admin"}]}""",
        )
        val loaded = repo.load()
        assertNull(loaded.warning)
        assertEquals(1, loaded.cameras.size)
        assertNull(loaded.cameras[0].substream)
    }

    @Test
    fun secundarioComSenhaNoArquivoContaComoCorrompido() {
        val repo = JsonCameraRepository(tempDir())
        repo.file.writeText(
            """{"version":1,"cameras":[{"id":"6f1c2a9e-1d55-4d3c-9b6c-0d0b9d7c2f11","name":"A","url":"rtsp://h/s","substream_url":"rtsp://u:p@h/s2"}]}""",
        )
        assertNotNull(repo.load().warning)
    }

    @Test
    fun secundarioDeOutroHostNoArquivoEDescartadoSemAviso() {
        val repo = JsonCameraRepository(tempDir())
        repo.file.writeText(
            """{"version":1,"cameras":[{"id":"6f1c2a9e-1d55-4d3c-9b6c-0d0b9d7c2f11","name":"A","url":"rtsp://192.168.1.164/s1","substream_url":"rtsp://192.168.1.121/s2"}]}""",
        )
        val loaded = repo.load()
        assertNull(loaded.warning)
        assertEquals("rtsp://192.168.1.164/s1", loaded.cameras.single().url.toString())
        assertNull(loaded.cameras.single().substream)
    }

    @Test
    fun carimboDeDataUtc() {
        assertEquals("20261124-103245", timestampUtc(1_795_516_365_000))
        assertEquals("19700101-000000", timestampUtc(0))
        assertEquals("20000229-000000", timestampUtc(951_782_400_000))
    }
}
