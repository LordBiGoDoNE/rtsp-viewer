package rtspviewer.core.application

import rtspviewer.core.domain.Camera
import rtspviewer.core.domain.CameraId
import rtspviewer.core.domain.CameraName
import rtspviewer.core.domain.InvalidNameException
import rtspviewer.core.domain.InvalidUrlException
import rtspviewer.core.domain.Password
import rtspviewer.core.domain.RtspUrl
import rtspviewer.core.domain.SubstreamHostMismatchException

/** Dados do formulário de câmera, como digitados. */
data class CameraDraft(
    val name: String = "",
    val url: String = "",
    val username: String = "",
    val password: PasswordInput = PasswordInput.Unchanged,
    val substream: SubstreamInput = SubstreamInput.Unchanged,
)

/** O que fazer com o stream secundário ao salvar. */
sealed interface SubstreamInput {
    /** Manter o atual (ou nenhum). Se a URL principal mudar, ele é descartado. */
    data object Unchanged : SubstreamInput

    /** URL RTSP do secundário (credenciais embutidas descartadas), mesmo host do principal. */
    data class Set(val url: String) : SubstreamInput

    data object Clear : SubstreamInput
}

/** O que fazer com a senha ao salvar. */
sealed interface PasswordInput {
    data object Unchanged : PasswordInput

    /** Nova senha; vazia equivale a [Clear]. */
    data class Set(val password: Password) : PasswordInput

    data object Clear : PasswordInput
}

enum class Field { Name, Url }

data class FieldError(val field: Field, val message: String)

sealed class CatalogError(message: String) : Exception(message) {
    class Invalid(val errors: List<FieldError>) : CatalogError("Corrija os campos destacados.")
    class NotFound : CatalogError("Câmera não encontrada.")
    class Storage(detail: String) : CatalogError("Não foi possível salvar a lista de câmeras: $detail")
    class Credentials(detail: String) : CatalogError(detail)

    /**
     * A câmera nova foi salva (com este [id]), mas a senha não chegou ao cofre.
     * A UI trata a câmera como criada e oferece reaplicar a senha com `edit`.
     */
    class AddedWithoutPassword(val id: CameraId, reason: String) :
        CatalogError("Câmera salva, mas a senha não pôde ser guardada no cofre: $reason")

    class NeedsPassword : CatalogError("Informe a senha da câmera.")
}

/** Caso de uso: lista de câmeras em memória, sincronizada com o repositório e o cofre. */
class CameraCatalog private constructor(
    private val repository: CameraRepository,
    private val credentials: CredentialStore,
    private var list: List<Camera>,
) {
    /**
     * Cópias das câmeras: mutá-las não altera a lista em memória nem o arquivo.
     * Mudanças passam por [add], [edit] e [remove], que persistem.
     */
    val cameras: List<Camera> get() = list.map { it.copy() }

    fun get(id: CameraId): Camera? = find(id)?.copy()

    /** `false` quando as senhas não sobrevivem ao fechamento do app. */
    val passwordsArePersistent: Boolean get() = credentials.isPersistent

    fun add(draft: CameraDraft): CameraId {
        val valid = validate(draft)
        val camera = Camera.create(valid.name, valid.url, valid.username)
        valid.substream?.let { changeSubstream(camera, it.url) }
        // Sem usuário, a senha não é usada: não vai para o cofre.
        val password = if (camera.requiresCredentials()) valid.password else PasswordInput.Unchanged
        persist(list + camera)
        try {
            applyPassword(camera.id, password)
        } catch (e: CatalogError.Credentials) {
            throw CatalogError.AddedWithoutPassword(camera.id, e.message.orEmpty())
        }
        return camera.id
    }

    fun edit(id: CameraId, draft: CameraDraft) {
        val idx = indexOf(id)
        val valid = validate(draft)
        val next = list.map { it.copy() }
        val camera = next[idx]
        camera.rename(valid.name)
        // `changeUrl` descarta o secundário quando a URL muda; um `Set` explícito
        // é aplicado depois, já contra a URL nova.
        camera.changeUrl(valid.url)
        camera.changeUsername(valid.username)
        valid.substream?.let { changeSubstream(camera, it.url) }
        // Câmera que passa a não ter usuário não deve manter senha no cofre.
        val password = if (camera.requiresCredentials()) valid.password else PasswordInput.Clear
        persist(next)
        try {
            applyPassword(id, password)
        } catch (e: CatalogError.Credentials) {
            throw CatalogError.Credentials("Câmera salva, mas a senha não pôde ser guardada no cofre: ${e.message}")
        }
    }

    /**
     * Remove a câmera da lista e a senha dela do cofre. A lista é gravada
     * primeiro; se só a senha falhar, a câmera já saiu da lista e o erro diz isso.
     */
    fun remove(id: CameraId) {
        val idx = indexOf(id)
        persist(list.filterIndexed { i, _ -> i != idx })
        try {
            credentials.delete(id)
        } catch (e: CredentialException) {
            throw CatalogError.Credentials("Câmera removida, mas a senha não pôde ser apagada do cofre: ${e.message}")
        }
    }

    /**
     * Monta o pedido de reprodução. [typedPassword] é a senha digitada quando o
     * app a pediu ([CatalogError.NeedsPassword]); ela fica no cofre para as
     * próximas aberturas.
     */
    fun connectionRequest(id: CameraId, typedPassword: Password? = null): StreamRequest {
        val camera = find(id) ?: throw CatalogError.NotFound()
        if (!camera.requiresCredentials()) {
            // Sem usuário, conecta sem credenciais; senha antiga é ignorada.
            return StreamRequest(id, camera.url.withCredentials(null, null), camera.url.toString())
        }
        val password = try {
            if (typedPassword != null) {
                credentials.set(id, typedPassword)
                typedPassword
            } else {
                credentials.get(id)
            }
        } catch (e: CredentialException) {
            throw CatalogError.Credentials(e.message.orEmpty())
        } ?: throw CatalogError.NeedsPassword()
        return StreamRequest(id, camera.url.withCredentials(camera.username, password), camera.url.toString())
    }

    private fun find(id: CameraId): Camera? = list.firstOrNull { it.id == id }

    private fun indexOf(id: CameraId): Int =
        list.indexOfFirst { it.id == id }.takeIf { it >= 0 } ?: throw CatalogError.NotFound()

    private fun persist(next: List<Camera>) {
        try {
            repository.save(next)
        } catch (e: RepositoryException) {
            throw CatalogError.Storage(e.message.orEmpty())
        }
        list = next
    }

    private fun applyPassword(id: CameraId, input: PasswordInput) {
        try {
            when (input) {
                PasswordInput.Unchanged -> Unit
                is PasswordInput.Set -> if (input.password.isEmpty()) credentials.delete(id) else credentials.set(id, input.password)
                PasswordInput.Clear -> credentials.delete(id)
            }
        } catch (e: CredentialException) {
            throw CatalogError.Credentials(e.message.orEmpty())
        }
    }

    private class Substream(val url: RtspUrl?)

    private class ValidDraft(
        val name: CameraName,
        val url: RtspUrl,
        val username: String?,
        val password: PasswordInput,
        /** `null` = manter; senão o novo valor (que pode ser nenhum). */
        val substream: Substream?,
    )

    private fun changeSubstream(camera: Camera, url: RtspUrl?) {
        try {
            camera.changeSubstream(url)
        } catch (e: SubstreamHostMismatchException) {
            throw CatalogError.Invalid(listOf(FieldError(Field.Url, "Stream secundário: ${e.message}")))
        }
    }

    private fun validate(draft: CameraDraft): ValidDraft {
        val errors = mutableListOf<FieldError>()
        val name = try {
            CameraName.parse(draft.name)
        } catch (e: InvalidNameException) {
            errors += FieldError(Field.Name, e.error.message)
            null
        }
        val parsed = try {
            RtspUrl.parse(draft.url)
        } catch (e: InvalidUrlException) {
            errors += FieldError(Field.Url, e.error.message)
            null
        }
        var substreamOk = true
        val substream = when (val s = draft.substream) {
            SubstreamInput.Unchanged -> null
            SubstreamInput.Clear -> Substream(null)
            is SubstreamInput.Set -> try {
                Substream(RtspUrl.parse(s.url).url)
            } catch (e: InvalidUrlException) {
                errors += FieldError(Field.Url, "Stream secundário: ${e.error.message}")
                substreamOk = false
                null
            }
        }
        if (name == null || parsed == null || !substreamOk) throw CatalogError.Invalid(errors)
        // Credenciais embutidas na URL têm precedência sobre os campos.
        val username = parsed.username ?: draft.username
        val password = parsed.password?.let { PasswordInput.Set(it) } ?: draft.password
        return ValidDraft(name, parsed.url, username, password, substream)
    }

    companion object {
        /**
         * Carrega a lista. O aviso (arquivo corrompido recuperado) deve ser
         * mostrado ao usuário.
         */
        fun load(repository: CameraRepository, credentials: CredentialStore): Pair<CameraCatalog, RepositoryWarning?> {
            val loaded = try {
                repository.load()
            } catch (e: RepositoryException) {
                throw CatalogError.Storage(e.message.orEmpty())
            }
            return CameraCatalog(repository, credentials, loaded.cameras) to loaded.warning
        }
    }
}
