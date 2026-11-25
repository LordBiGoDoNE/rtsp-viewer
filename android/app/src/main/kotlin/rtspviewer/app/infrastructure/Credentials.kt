package rtspviewer.app.infrastructure

import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import java.io.File
import java.io.FileNotFoundException
import java.io.IOException
import java.security.GeneralSecurityException
import java.security.KeyStore
import java.util.Base64
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import rtspviewer.core.application.CredentialException
import rtspviewer.core.application.CredentialStore
import rtspviewer.core.domain.CameraId
import rtspviewer.core.domain.Password

/** Texto cifrado com o IV usado (AES-GCM). */
class Sealed(val iv: ByteArray, val data: ByteArray)

/** Cifra autenticada das senhas. [aad] amarra o texto cifrado à câmera. */
interface SecretCipher {
    /** @throws GeneralSecurityException */
    fun seal(plain: ByteArray, aad: ByteArray): Sealed

    /** @throws GeneralSecurityException quando a chave falta ou o texto não confere */
    fun open(sealed: Sealed, aad: ByteArray): ByteArray
}

/**
 * AES-256-GCM com chave não exportável do Android Keystore (design D2). A chave
 * é criada na primeira gravação e nunca sai do Keystore.
 */
class KeystoreSecretCipher(private val alias: String = DEFAULT_ALIAS) : SecretCipher {
    override fun seal(plain: ByteArray, aad: ByteArray): Sealed {
        val cipher = Cipher.getInstance(TRANSFORMATION)
        cipher.init(Cipher.ENCRYPT_MODE, existingKey() ?: generateKey())
        cipher.updateAAD(aad)
        val data = cipher.doFinal(plain)
        return Sealed(cipher.iv, data)
    }

    override fun open(sealed: Sealed, aad: ByteArray): ByteArray {
        val key = existingKey() ?: throw GeneralSecurityException("chave ausente")
        val cipher = Cipher.getInstance(TRANSFORMATION)
        cipher.init(Cipher.DECRYPT_MODE, key, GCMParameterSpec(TAG_BITS, sealed.iv))
        cipher.updateAAD(aad)
        return cipher.doFinal(sealed.data)
    }

    private fun existingKey(): SecretKey? {
        val ks = KeyStore.getInstance(KEYSTORE).apply { load(null) }
        return ks.getKey(alias, null) as? SecretKey
    }

    private fun generateKey(): SecretKey {
        val generator = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, KEYSTORE)
        generator.init(
            KeyGenParameterSpec.Builder(alias, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .setKeySize(256)
                .setRandomizedEncryptionRequired(true)
                .build(),
        )
        return generator.generateKey()
    }

    /** Remove a chave (testes). */
    fun deleteKey() {
        KeyStore.getInstance(KEYSTORE).apply { load(null) }.deleteEntry(alias)
    }

    companion object {
        const val DEFAULT_ALIAS = "rtsp-viewer-credentials"
        private const val KEYSTORE = "AndroidKeyStore"
        private const val TRANSFORMATION = "AES/GCM/NoPadding"
        private const val TAG_BITS = 128
    }
}

/**
 * Cofre das senhas: só o texto cifrado e o IV ficam em `credentials.json`, no
 * armazenamento privado do app, por id da câmera. Senha que não decifra
 * (chave invalidada, arquivo de outro aparelho) é tratada como ausente, e o app
 * pede a senha de novo.
 */
class FileCredentialStore(
    private val dir: File,
    private val cipher: SecretCipher,
    private val clock: () -> Long = System::currentTimeMillis,
) : CredentialStore {
    val file: File get() = File(dir, FILE_NAME)

    override val isPersistent: Boolean = true

    @Synchronized
    override fun get(id: CameraId): Password? {
        val entry = read()[id.value] ?: return null
        return try {
            val sealed = Sealed(B64D.decode(entry.iv), B64D.decode(entry.data))
            val plain = cipher.open(sealed, aad(id))
            try {
                Password(plain.toString(Charsets.UTF_8))
            } finally {
                plain.fill(0)
            }
        } catch (_: GeneralSecurityException) {
            null
        } catch (_: IllegalArgumentException) {
            null
        }
    }

    @Synchronized
    override fun set(id: CameraId, password: Password) {
        val plain = password.expose().toByteArray(Charsets.UTF_8)
        val sealed = try {
            cipher.seal(plain, aad(id))
        } catch (e: GeneralSecurityException) {
            throw CredentialException("Não foi possível cifrar a senha: ${e.javaClass.simpleName}")
        } finally {
            plain.fill(0)
        }
        write(read() + (id.value to EntryDto(B64E.encodeToString(sealed.iv), B64E.encodeToString(sealed.data))))
    }

    @Synchronized
    override fun delete(id: CameraId) {
        val entries = read()
        if (id.value in entries) write(entries - id.value)
    }

    private fun aad(id: CameraId) = "rtsp-viewer:${id.value}".toByteArray(Charsets.UTF_8)

    private fun read(): Map<String, EntryDto> {
        val text = try {
            file.readText(Charsets.UTF_8)
        } catch (_: FileNotFoundException) {
            return emptyMap()
        } catch (e: IOException) {
            throw CredentialException("Não foi possível ler o cofre de senhas: ${e.message}")
        }
        return try {
            val dto = JSON.decodeFromString(FileDto.serializer(), text)
            require(dto.version == VERSION)
            dto.entries
        } catch (_: Exception) {
            // Ilegível: preservado à parte; as senhas serão pedidas de novo.
            try {
                preserveCorrupted(file, clock())
            } catch (e: IOException) {
                throw CredentialException("Não foi possível preservar o cofre corrompido: ${e.message}")
            }
            emptyMap()
        }
    }

    private fun write(entries: Map<String, EntryDto>) {
        try {
            writeAtomic(file, JSON.encodeToString(FileDto.serializer(), FileDto(VERSION, entries)).toByteArray(Charsets.UTF_8))
        } catch (e: IOException) {
            throw CredentialException("Não foi possível gravar o cofre de senhas: ${e.message}")
        }
    }

    @Serializable
    private data class FileDto(val version: Int, val entries: Map<String, EntryDto>)

    @Serializable
    private data class EntryDto(val iv: String, val data: String)

    private companion object {
        const val FILE_NAME = "credentials.json"
        const val VERSION = 1
        val JSON = Json { ignoreUnknownKeys = true }
        val B64E: Base64.Encoder = Base64.getEncoder()
        val B64D: Base64.Decoder = Base64.getDecoder()
    }
}
