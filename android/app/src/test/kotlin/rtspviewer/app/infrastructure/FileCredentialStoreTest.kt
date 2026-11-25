package rtspviewer.app.infrastructure

import java.io.File
import java.nio.file.Files
import java.security.SecureRandom
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec
import kotlin.test.AfterTest
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertNull
import kotlin.test.assertTrue
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
}
