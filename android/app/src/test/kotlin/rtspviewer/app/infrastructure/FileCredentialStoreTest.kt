package rtspviewer.app.infrastructure

import java.io.File
import java.nio.file.Files
import java.security.ProviderException
import java.security.SecureRandom
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec
import kotlin.test.AfterTest
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertFalse
import kotlin.test.assertNull
import kotlin.test.assertTrue
import rtspviewer.core.application.CameraCatalog
import rtspviewer.core.application.CameraDraft
import rtspviewer.core.application.CatalogError
import rtspviewer.core.application.CredentialException
import rtspviewer.core.application.PasswordInput
import rtspviewer.core.domain.CameraId
import rtspviewer.core.domain.Password

/** AES-GCM em software, no lugar do Keystore, para testar o cofre no JVM. */
class SoftwareCipher(var key: SecretKey = newKey()) : SecretCipher {
    override fun seal(plain: ByteArray, aad: ByteArray): Sealed {
        val iv = ByteArray(12).also(SecureRandom()::nextBytes)
        val c = Cipher.getInstance("AES/GCM/NoPadding")
        c.init(Cipher.ENCRYPT_MODE, key, GCMParameterSpec(128, iv))
        c.updateAAD(aad)
        return Sealed(iv, c.doFinal(plain))
    }

    override fun open(sealed: Sealed, aad: ByteArray): ByteArray {
        val c = Cipher.getInstance("AES/GCM/NoPadding")
        c.init(Cipher.DECRYPT_MODE, key, GCMParameterSpec(128, sealed.iv))
        c.updateAAD(aad)
        return c.doFinal(sealed.data)
    }

    companion object {
        fun newKey(): SecretKey = KeyGenerator.getInstance("AES").apply { init(256) }.generateKey()
    }
}

/** Simula falha do provedor do Keystore (exceção não verificada). */
class ProviderFailingCipher : SecretCipher {
    override fun seal(plain: ByteArray, aad: ByteArray): Sealed = throw ProviderException("Keystore operation failed")

    override fun open(sealed: Sealed, aad: ByteArray): ByteArray = throw ProviderException("Keystore operation failed")
}

class FileCredentialStoreTest {
    private val dirs = mutableListOf<File>()

    private fun store(cipher: SecretCipher = SoftwareCipher()): FileCredentialStore =
        FileCredentialStore(Files.createTempDirectory("rtsp-viewer-cred").toFile().also { dirs += it }, cipher)

    @AfterTest
    fun limpa() {
        dirs.forEach { it.deleteRecursively() }
    }

    @Test
    fun guardaELeASenha() {
        val s = store()
        val id = CameraId.generate()
        assertNull(s.get(id))
        s.set(id, Password("s3gr3do-ção"))
        assertEquals("s3gr3do-ção", s.get(id)!!.expose())
    }

    @Test
    fun arquivoSoTemTextoCifrado() {
        val s = store()
        s.set(CameraId.generate(), Password("s3gr3do"))
        val text = s.file.readText()
        assertFalse(text.contains("s3gr3do"))
        assertTrue(text.contains("\"iv\""))
    }

    @Test
    fun apagaSoADaCamera() {
        val s = store()
        val a = CameraId.generate()
        val b = CameraId.generate()
        s.set(a, Password("a"))
        s.set(b, Password("b"))
        s.delete(a)
        s.delete(a)
        assertNull(s.get(a))
        assertEquals("b", s.get(b)!!.expose())
    }

    @Test
    fun senhaQueNaoDecifraViraAusente() {
        val cipher = SoftwareCipher()
        val s = store(cipher)
        val id = CameraId.generate()
        s.set(id, Password("x"))
        cipher.key = SoftwareCipher.newKey() // chave invalidada
        assertNull(s.get(id))
        s.set(id, Password("nova"))
        assertEquals("nova", s.get(id)!!.expose())
    }

    @Test
    fun textoCifradoNaoServeParaOutraCamera() {
        val s = store()
        val a = CameraId.generate()
        val b = CameraId.generate()
        s.set(a, Password("x"))
        // Copia a entrada de A para B no arquivo: o AAD (id) não confere.
        val text = s.file.readText().replace(a.value, b.value)
        s.file.writeText(text)
        assertNull(s.get(b))
    }

    @Test
    fun arquivoCorrompidoEPreservadoESenhasSaoPedidasDeNovo() {
        val s = store()
        s.file.parentFile.mkdirs()
        s.file.writeText("{ quebrado")
        assertNull(s.get(CameraId.generate()))
        assertFalse(s.file.exists())
        assertTrue(s.file.parentFile.listFiles()!!.any { it.name.startsWith("credentials.json.corrompido-") })
    }

    @Test
    fun falhaDoProvedorAoLerViraSenhaAusente() {
        val dir = Files.createTempDirectory("rtsp-viewer-cred").toFile().also { dirs += it }
        val id = CameraId.generate()
        FileCredentialStore(dir, SoftwareCipher()).set(id, Password("x"))
        assertNull(FileCredentialStore(dir, ProviderFailingCipher()).get(id))
    }

    @Test
    fun falhaDoProvedorAoGravarViraCredentialException() {
        val s = store(ProviderFailingCipher())
        assertFailsWith<CredentialException> { s.set(CameraId.generate(), Password("x")) }
        assertFalse(s.file.exists())
    }

    @Test
    fun falhaDoProvedorAoAdicionarCameraSalvaSemSenha() {
        val dir = Files.createTempDirectory("rtsp-viewer-cred").toFile().also { dirs += it }
        val (catalog, _) = CameraCatalog.load(JsonCameraRepository(dir), FileCredentialStore(dir, ProviderFailingCipher()))
        val err = assertFailsWith<CatalogError.AddedWithoutPassword> {
            catalog.add(
                CameraDraft(
                    name = "Garagem",
                    url = "rtsp://192.168.0.10:554/stream1",
                    username = "admin",
                    password = PasswordInput.Set(Password("s3gr3do")),
                ),
            )
        }
        assertEquals(listOf(err.id), catalog.cameras.map { it.id })
        assertEquals(listOf(err.id), JsonCameraRepository(dir).load().cameras.map { it.id })
    }

    @Test
    fun duasCorrupcoesNoMesmoSegundoPreservamAsDuasCopias() {
        val dir = Files.createTempDirectory("rtsp-viewer-cred").toFile().also { dirs += it }
        val s = FileCredentialStore(dir, SoftwareCipher(), clock = { 1_700_000_000_000L })
        s.file.writeText("{ quebrado 1")
        assertNull(s.get(CameraId.generate()))
        s.file.writeText("{ quebrado 2")
        assertNull(s.get(CameraId.generate()))
        val backups = dir.listFiles()!!.filter { it.name.startsWith("credentials.json.corrompido-") }
        assertEquals(setOf("{ quebrado 1", "{ quebrado 2"), backups.map { it.readText() }.toSet())
        assertTrue(backups.any { it.name.endsWith("-2") })
    }
}
