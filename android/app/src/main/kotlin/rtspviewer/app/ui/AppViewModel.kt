package rtspviewer.app.ui

import android.view.SurfaceView
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.runInterruptible
import kotlinx.coroutines.withContext
import rtspviewer.app.player.PlayerController
import rtspviewer.app.player.VideoEngine
import rtspviewer.core.application.AddressInput
import rtspviewer.core.application.CameraCatalog
import rtspviewer.core.application.CameraDiscovery
import rtspviewer.core.application.CameraDraft
import rtspviewer.core.application.CameraRepository
import rtspviewer.core.application.CatalogError
import rtspviewer.core.application.CredentialStore
import rtspviewer.core.application.DiscoveredCamera
import rtspviewer.core.application.LookupOutcome
import rtspviewer.core.application.PasswordInput
import rtspviewer.core.application.ResolveError
import rtspviewer.core.application.StreamRequest
import rtspviewer.core.application.StreamResolver
import rtspviewer.core.application.StreamTarget
import rtspviewer.core.application.SubstreamInput
import rtspviewer.core.application.classifyAddress
import rtspviewer.core.application.lookup
import rtspviewer.core.application.vendorStreams
import rtspviewer.core.domain.CameraId
import rtspviewer.core.domain.CameraName
import rtspviewer.core.domain.DeviceAddress
import rtspviewer.core.domain.InvalidAddressException
import rtspviewer.core.domain.InvalidNameException
import rtspviewer.core.domain.Password
import rtspviewer.core.domain.RtspUrl
import rtspviewer.core.domain.Vendor

/**
 * Dependências da UI (design D5). O app usa as reais (`AppContainer`); os
 * testes passam portas falsas, sem rede, arquivo nem Keystore.
 */
class AppPorts(
    val repository: CameraRepository,
    val credentials: CredentialStore,
    val resolver: StreamResolver,
    val discovery: CameraDiscovery,
    val engine: VideoEngine,
    /** Liga o vídeo a uma `SurfaceView` (ou solta, com `null`). */
    val attachSurface: (SurfaceView?) -> Unit = {},
    /** Onde rodam o catálogo (arquivo e cofre), a consulta e a busca. */
    val io: CoroutineDispatcher = Dispatchers.IO,
    /** Versão exibida em "Sobre"; lida só quando a tela abre. */
    val version: () -> String = { "" },
)

enum class Screen { List, Form, Viewer, Discover, About }

/** Linha da lista: dados sem credenciais. */
data class CameraRow(
    val id: CameraId,
    val name: String,
    /** URL principal sem credenciais. */
    val address: String,
    val host: String,
    val username: String?,
)

enum class FormPhase { Editing, Saving, LookingUp, ChooseVendor }

data class FormState(
    val editing: CameraId? = null,
    val name: String = "",
    val address: String = "",
    val username: String = "",
    val password: String = "",
    val nameError: String? = null,
    val addressError: String? = null,
    val formError: String? = null,
    val phase: FormPhase = FormPhase.Editing,
    val vendorReason: String = "",
    val passwordsPersistent: Boolean = true,
) {
    val title: String get() = if (editing != null) "Editar câmera" else "Nova câmera"

    /** Sem os campos digitados: a senha nunca vai para log. */
    override fun toString(): String = "FormState(editing=$editing, phase=$phase)"
}

sealed interface DiscoverState {
    data object Searching : DiscoverState

    data class Found(val rows: List<FoundRow>) : DiscoverState

    data object Empty : DiscoverState

    data class Failed(val message: String) : DiscoverState
}

data class FoundRow(val index: Int, val name: String, val host: String, val registered: Boolean)

data class ViewerState(
    val cameraId: CameraId? = null,
    val name: String = "",
    /** Endereço sem credenciais, para o painel "Informações". */
    val displayUrl: String = "",
    val showInfo: Boolean = false,
    val fullscreen: Boolean = false,
    /** Usuário do diálogo "Senha de <usuário>", quando aberto. */
    val passwordUser: String? = null,
    /** Erro ao montar o pedido de reprodução (cofre, câmera removida). */
    val error: String? = null,
)

data class UiState(
    val screen: Screen = Screen.List,
    /** A lista já foi lida (antes disso a tela fica sem conteúdo). */
    val loaded: Boolean = false,
    val cameras: List<CameraRow> = emptyList(),
    val banner: String? = null,
    val confirmRemove: CameraRow? = null,
    val form: FormState = FormState(),
    val discover: DiscoverState = DiscoverState.Searching,
    val viewer: ViewerState = ViewerState(),
)

/**
 * Estado e navegação da UI (design D5), sem biblioteca de navegação. Chamar só
 * na thread principal. O catálogo roda fora dela, uma operação por vez; a
 * busca, a consulta e a abertura de câmera têm sessão: o resultado de uma
 * operação cancelada ou antiga é ignorado.
 *
 * O boot só lê a lista (fora da thread principal): não abre rede nem decifra
 * senhas (spec `android-distribution`, "Boot rápido no Android").
 */
class AppViewModel(private val ports: AppPorts) : ViewModel() {
    private val catalogContext = ports.io.limitedParallelism(1)
    private val catalog = CompletableDeferred<CameraCatalog?>()
    private var loadError: String? = null

    val player = PlayerController(ports.engine, viewModelScope)

    private val _state = MutableStateFlow(UiState())
    val state: StateFlow<UiState> = _state.asStateFlow()

    private var formSession = 0L

    private var lookupSession = 0L
    private var lookupJob: Job? = null
    private var pending: PendingLookup? = null

    /** Endereço digitado e URL de serviço ONVIF da câmera escolhida na busca. */
    private var foundTarget: Pair<String, String>? = null

    private var discoverySession = 0L
    private var discoveryJob: Job? = null
    private var found: List<DiscoveredCamera> = emptyList()

    private var openSession = 0L
    private var openJob: Job? = null
    private var backgrounded = false

    private class PendingLookup(
        val name: String,
        val address: DeviceAddress,
        val username: String,
        val password: Password?,
    )

    init {
        viewModelScope.launch { load() }
    }

    fun version(): String = ports.version()

    fun attachSurface(view: SurfaceView?) = ports.attachSurface(view)

    // ------------------------------------------------------------ lista

    private suspend fun load() {
        try {
            val (loaded, warning, rows) = withContext(catalogContext) {
                val (c, w) = CameraCatalog.load(ports.repository, ports.credentials)
                Triple(c, w, rows(c))
            }
            catalog.complete(loaded)
            update { it.copy(loaded = true, cameras = rows, banner = warning?.let(Texts::banner)) }
        } catch (e: CatalogError) {
            loadError = e.message
            catalog.complete(null)
            update { it.copy(loaded = true, banner = e.message) }
        }
    }

    fun dismissBanner() = update { it.copy(banner = null) }

    fun openAbout() = update { it.copy(screen = Screen.About) }

    fun closeAbout() = update { it.copy(screen = Screen.List) }

    fun requestRemove(id: CameraId) {
        val row = state.value.cameras.firstOrNull { it.id == id } ?: return
        update { it.copy(confirmRemove = row) }
    }

    fun dismissRemove() = update { it.copy(confirmRemove = null) }

    fun confirmRemove() {
        val row = state.value.confirmRemove ?: return
        update { it.copy(confirmRemove = null) }
        viewModelScope.launch {
            val (error, rows) = onCatalog { c ->
                val error = try {
                    c.remove(row.id)
                    null
                } catch (e: CatalogError) {
                    e.message
                }
                error to rows(c)
            } ?: (loadError to state.value.cameras)
            update { it.copy(cameras = rows, banner = error ?: it.banner) }
        }
    }

    // ------------------------------------------------------------ formulário

    fun openNewForm() = startForm(null)

    fun openEditForm(id: CameraId) = startForm(id)

    private fun startForm(id: CameraId?) {
        leaveDiscovery()
        abandonLookup()
        foundTarget = null
        formSession++
        val row = id?.let { i -> state.value.cameras.firstOrNull { it.id == i } }
        update {
            it.copy(
                screen = Screen.Form,
                form = FormState(
                    editing = row?.id,
                    name = row?.name.orEmpty(),
                    address = row?.address.orEmpty(),
                    username = row?.username.orEmpty(),
                    passwordsPersistent = ports.credentials.isPersistent,
                ),
            )
        }
    }

    fun setName(value: String) = updateForm { it.copy(name = value) }

    fun setAddress(value: String) = updateForm { it.copy(address = value) }

    fun setUsername(value: String) = updateForm { it.copy(username = value) }

    fun setPassword(value: String) = updateForm { it.copy(password = value) }

    /** "Cancelar" do formulário. */
    fun closeForm() {
        abandonLookup()
        foundTarget = null
        formSession++
        update { it.copy(screen = Screen.List, form = FormState()) }
    }

    /** "Salvar": URL completa grava na hora; só o endereço consulta a câmera. */
    fun save() {
        val form = state.value.form
        if (state.value.screen != Screen.Form || form.phase != FormPhase.Editing) return
        val input = try {
            classifyAddress(form.address)
        } catch (e: InvalidAddressException) {
            updateForm { it.copy(nameError = nameError(form.name), addressError = e.error.message, formError = null) }
            return
        }
        when (input) {
            AddressInput.Rtsp -> saveDraft(
                CameraDraft(
                    name = form.name,
                    url = form.address,
                    username = form.username,
                    password = if (form.password.isEmpty()) PasswordInput.Unchanged else PasswordInput.Set(Password(form.password)),
                    // URL completa: o secundário atual fica enquanto a principal não muda.
                    substream = SubstreamInput.Unchanged,
                ),
            )
            is AddressInput.Device -> startLookup(form, input.address)
        }
    }

    private fun saveDraft(draft: CameraDraft) {
        val editing = state.value.form.editing
        val session = formSession
        updateForm { it.copy(phase = FormPhase.Saving) }
        viewModelScope.launch {
            val outcome = onCatalog { c ->
                val error = try {
                    if (editing != null) c.edit(editing, draft) else c.add(draft)
                    null
                } catch (e: CatalogError) {
                    e
                }
                error to rows(c)
            }
            if (outcome == null) {
                if (session == formSession) updateForm { it.copy(phase = FormPhase.Editing, formError = loadError) }
                return@launch
            }
            val (error, rows) = outcome
            update { it.copy(cameras = rows) }
            if (session != formSession) return@launch
            when (error) {
                null -> leaveForm(banner = null)
                // A câmera foi salva; só a senha falhou. O formulário passa a
                // editar essa câmera, para uma nova tentativa não duplicá-la.
                is CatalogError.AddedWithoutPassword -> updateForm {
                    it.copy(editing = error.id, phase = FormPhase.Editing, formError = error.message)
                }
                // Na edição, a lista já foi gravada; só a senha falhou.
                is CatalogError.Credentials -> leaveForm(banner = error.message)
                else -> {
                    val (name, url, general) = Texts.formErrors(error)
                    updateForm {
                        it.copy(phase = FormPhase.Editing, nameError = name, addressError = url, formError = general)
                    }
                }
            }
        }
    }

    private fun leaveForm(banner: String?) {
        formSession++
        update { it.copy(screen = Screen.List, form = FormState(), banner = banner ?: it.banner) }
    }

    // ------------------------------------------------------------ consulta ONVIF

    private fun startLookup(form: FormState, address: DeviceAddress) {
        val name = form.name.trim()
        val nameError = if (name.isEmpty()) null else nameError(name)
        updateForm { it.copy(nameError = nameError, addressError = null, formError = null) }
        if (nameError != null) return
        if (form.editing != null && form.username.isNotBlank() && form.password.isEmpty()) {
            updateForm { it.copy(formError = Texts.LOOKUP_NEEDS_PASSWORD) }
            return
        }
        val target = foundTarget
            ?.takeIf { (typed, _) -> typed.trim() == form.address.trim() }
            ?.let { (_, service) -> StreamTarget.ServiceUrl(service) }
            ?: StreamTarget.Address(address)
        val p = PendingLookup(name, address, form.username, form.password.takeIf { it.isNotEmpty() }?.let(::Password))
        pending = p
        val session = ++lookupSession
        updateForm { it.copy(phase = FormPhase.LookingUp) }
        lookupJob = viewModelScope.launch {
            val outcome = try {
                runInterruptible(ports.io) { lookup(ports.resolver, target, p.username, p.password) }
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                LookupOutcome.NeedsVendor(ResolveError.Protocol(e.message ?: e.javaClass.simpleName))
            }
            if (session != lookupSession) return@launch
            lookupJob = null
            lookupDone(p, outcome)
        }
    }

    private fun lookupDone(p: PendingLookup, outcome: LookupOutcome) {
        // A consulta terminou: a senha sai da tela (fica só no rascunho).
        updateForm { it.copy(password = "", phase = FormPhase.Editing) }
        when (outcome) {
            is LookupOutcome.Resolved -> {
                pending = null
                val streams = outcome.streams
                val name = p.name.ifEmpty { Texts.suggestedName(streams.suggestedName) ?: Texts.fallbackName(p.address) }
                saveResolved(p, name, streams.main, streams.sub)
            }
            is LookupOutcome.NeedsVendor -> updateForm {
                it.copy(phase = FormPhase.ChooseVendor, vendorReason = Texts.vendorReason(outcome.reason))
            }
            LookupOutcome.AuthenticationFailed -> {
                pending = null
                updateForm { it.copy(formError = ResolveError.AuthenticationFailed().message) }
            }
        }
    }

    /** Botão de fabricante na escolha de fabricante. */
    fun chooseVendor(vendor: Vendor) {
        if (state.value.form.phase != FormPhase.ChooseVendor) return
        val p = pending ?: return
        pending = null
        updateForm { it.copy(phase = FormPhase.Editing, vendorReason = "") }
        val (main, sub) = vendorStreams(vendor, p.address)
        saveResolved(p, p.name.ifEmpty { Texts.fallbackName(p.address) }, main, sub)
    }

    private fun saveResolved(p: PendingLookup, name: String, main: RtspUrl, sub: RtspUrl?) = saveDraft(
        CameraDraft(
            name = name,
            url = main.toString(),
            username = p.username,
            password = p.password?.let { PasswordInput.Set(it) } ?: PasswordInput.Unchanged,
            substream = sub?.let { SubstreamInput.Set(it.toString()) } ?: SubstreamInput.Clear,
        ),
    )

    /** "Cancelar" da consulta ou da escolha de fabricante: volta ao formulário como estava. */
    fun cancelLookup() = abandonLookup()

    /** Invalida a consulta em andamento: o resultado que chegar é ignorado. */
    private fun abandonLookup() {
        lookupSession++
        lookupJob?.cancel()
        lookupJob = null
        pending = null
        updateForm {
            val phase = if (it.phase == FormPhase.LookingUp || it.phase == FormPhase.ChooseVendor) FormPhase.Editing else it.phase
            it.copy(phase = phase, vendorReason = "")
        }
    }

    // ------------------------------------------------------------ busca na rede

    fun openDiscovery() {
        update { it.copy(screen = Screen.Discover) }
        startDiscovery()
    }

    /** A busca só roda por ação do usuário (nunca no boot). */
    fun startDiscovery() {
        discoveryJob?.cancel()
        val session = ++discoverySession
        found = emptyList()
        update { it.copy(discover = DiscoverState.Searching) }
        discoveryJob = viewModelScope.launch {
            val result = try {
                // Interrompível: cancelar solta o MulticastLock na hora.
                Result.success(runInterruptible(ports.io) { ports.discovery.discover() })
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                Result.failure(e)
            }
            if (session != discoverySession) return@launch
            discoveryJob = null
            showFound(result)
        }
    }

    private fun showFound(result: Result<List<DiscoveredCamera>>) {
        val list = result.getOrElse { e ->
            update { it.copy(discover = DiscoverState.Failed(Texts.discoveryError(e.message ?: e.javaClass.simpleName))) }
            return
        }
        found = list
        val rows = list.mapIndexed { i, d -> FoundRow(i, Texts.foundName(d.suggestedName), d.host, isRegistered(d.host)) }
        update { it.copy(discover = if (rows.isEmpty()) DiscoverState.Empty else DiscoverState.Found(rows)) }
    }

    /** Já existe câmera com este host na URL principal. */
    private fun isRegistered(host: String): Boolean {
        val h = host.trim('[', ']')
        return state.value.cameras.any { it.host.equals(h, ignoreCase = true) }
    }

    /** "Adicionar" de uma câmera encontrada: a consulta usa a URL de serviço anunciada. */
    fun addFound(index: Int) {
        val camera = found.getOrNull(index) ?: return
        startForm(null)
        val address = if (camera.host.contains(':') && !camera.host.startsWith('[')) "[${camera.host}]" else camera.host
        updateForm { it.copy(name = Texts.suggestedName(camera.suggestedName).orEmpty(), address = address) }
        foundTarget = address to camera.deviceUrl
    }

    /** "Cadastrar pelo IP": formulário vazio. */
    fun registerByAddress() = startForm(null)

    fun closeDiscovery() {
        leaveDiscovery()
        update { it.copy(screen = Screen.List) }
    }

    /** Invalida a busca em andamento: o resultado que chegar é ignorado. */
    private fun leaveDiscovery() {
        discoverySession++
        discoveryJob?.cancel()
        discoveryJob = null
        found = emptyList()
    }

    // ------------------------------------------------------------ visualização

    fun openCamera(id: CameraId) {
        val row = state.value.cameras.firstOrNull { it.id == id } ?: return
        update { it.copy(screen = Screen.Viewer, viewer = ViewerState(cameraId = id, name = row.name, displayUrl = row.address)) }
        connect(null)
    }

    /** Monta o pedido (cofre fora da thread principal) e abre o player. */
    private fun connect(typed: Password?) {
        val id = state.value.viewer.cameraId ?: return
        stopPlayback()
        val session = openSession
        updateViewer { it.copy(error = null, passwordUser = null) }
        openJob = viewModelScope.launch {
            val result = onCatalog { c ->
                try {
                    Result.success(c.connectionRequest(id, typed))
                } catch (e: CatalogError) {
                    Result.failure<StreamRequest>(e)
                }
            }
            if (session != openSession) return@launch
            openJob = null
            if (result == null) {
                updateViewer { it.copy(error = loadError) }
                return@launch
            }
            result.fold(
                onSuccess = { request ->
                    updateViewer { it.copy(displayUrl = request.displayUrl) }
                    player.start(request)
                },
                onFailure = { e ->
                    if (e is CatalogError.NeedsPassword) {
                        val user = state.value.cameras.firstOrNull { it.id == id }?.username.orEmpty()
                        updateViewer { it.copy(passwordUser = user) }
                    } else {
                        updateViewer { it.copy(error = e.message) }
                    }
                },
            )
        }
    }

    /** Encerra a reprodução e invalida a abertura em andamento. */
    private fun stopPlayback() {
        openSession++
        openJob?.cancel()
        openJob = null
        player.stop()
    }

    /** "Tentar de novo". */
    fun retry() = connect(null)

    /** "Conectar" do diálogo "Senha de <usuário>". */
    fun submitPassword(password: String) {
        updateViewer { it.copy(passwordUser = null) }
        connect(Password(password))
    }

    fun cancelPassword() = backToList()

    fun backToList() {
        stopPlayback()
        update { it.copy(screen = Screen.List, viewer = ViewerState()) }
    }

    fun toggleFullscreen() = updateViewer { it.copy(fullscreen = !it.fullscreen) }

    fun toggleInfo() = updateViewer { it.copy(showInfo = !it.showInfo) }

    /** App em segundo plano (ON_STOP): a conexão é encerrada. */
    fun onAppStopped() {
        if (state.value.screen != Screen.Viewer || backgrounded) return
        backgrounded = true
        stopPlayback()
    }

    /** App de volta (ON_START): a câmera conecta de novo, sem ação do usuário. */
    fun onAppStarted() {
        if (!backgrounded) return
        backgrounded = false
        val s = state.value
        if (s.screen == Screen.Viewer && s.viewer.passwordUser == null) connect(null)
    }

    /**
     * Voltar (botão ou gesto do sistema), o Esc do desktop: fecha o que estiver
     * por cima. `false` na lista sem diálogo (o sistema fecha o app).
     */
    fun back(): Boolean {
        val s = state.value
        when (s.screen) {
            Screen.List -> if (s.confirmRemove != null) dismissRemove() else return false
            Screen.Form ->
                if (s.form.phase == FormPhase.LookingUp || s.form.phase == FormPhase.ChooseVendor) cancelLookup() else closeForm()
            Screen.About -> closeAbout()
            Screen.Discover -> closeDiscovery()
            Screen.Viewer -> when {
                s.viewer.passwordUser != null -> cancelPassword()
                s.viewer.fullscreen -> toggleFullscreen()
                else -> backToList()
            }
        }
        return true
    }

    override fun onCleared() {
        leaveDiscovery()
        abandonLookup()
        stopPlayback()
    }

    // ------------------------------------------------------------ apoio

    /** Roda [block] com o catálogo, fora da thread principal e uma operação por vez. */
    private suspend fun <T> onCatalog(block: (CameraCatalog) -> T): T? {
        val c = catalog.await() ?: return null
        return withContext(catalogContext) { block(c) }
    }

    private fun rows(c: CameraCatalog): List<CameraRow> =
        c.cameras.map { CameraRow(it.id, it.name.value, it.url.toString(), it.url.host, it.username) }

    private fun nameError(name: String): String? = try {
        CameraName.parse(name)
        null
    } catch (e: InvalidNameException) {
        e.error.message
    }

    private inline fun update(f: (UiState) -> UiState) = _state.update(f)

    private inline fun updateForm(crossinline f: (FormState) -> FormState) = _state.update { it.copy(form = f(it.form)) }

    private inline fun updateViewer(crossinline f: (ViewerState) -> ViewerState) =
        _state.update { it.copy(viewer = f(it.viewer)) }

    companion object {
        /** Fábrica do `ViewModel`; [ports] só é chamada quando ele é criado. */
        fun factory(ports: () -> AppPorts) = viewModelFactory { initializer { AppViewModel(ports()) } }
    }
}
