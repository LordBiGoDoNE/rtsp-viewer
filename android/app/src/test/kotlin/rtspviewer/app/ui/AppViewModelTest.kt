package rtspviewer.app.ui

import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertIs
import kotlin.test.assertNull
import kotlin.test.assertTrue
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import org.junit.jupiter.api.AfterEach
import org.junit.jupiter.api.BeforeEach
import org.junit.jupiter.api.Test
import rtspviewer.app.player.PlayerState
import rtspviewer.core.application.DiscoveryFailure
import rtspviewer.core.application.RepositoryWarning
import rtspviewer.core.application.ResolveError
import rtspviewer.core.application.StreamTarget
import rtspviewer.core.domain.Camera
import rtspviewer.core.domain.Password
import rtspviewer.core.domain.Vendor

@OptIn(ExperimentalCoroutinesApi::class)
class AppViewModelTest {
    private val dispatcher = StandardTestDispatcher()
    private val credentials = MemoryCredentials()
    private val resolver = FakeResolver()
    private val discovery = FakeDiscovery()
    private val engine = FakeEngine()
    private lateinit var repository: MemoryRepository
    private lateinit var vm: AppViewModel

    @BeforeEach
    fun prepara() = Dispatchers.setMain(dispatcher)

    @AfterEach
    fun limpa() = Dispatchers.resetMain()

    private fun TestScope.start(vararg cameras: Camera, warning: RepositoryWarning? = null) {
        repository = MemoryRepository(cameras.toList(), warning)
        vm = AppViewModel(AppPorts(repository, credentials, resolver, discovery, engine, io = dispatcher))
        advanceUntilIdle()
    }

    private val state get() = vm.state.value

    private fun fill(name: String, address: String, user: String = "", password: String = "") {
        vm.setName(name)
        vm.setAddress(address)
        vm.setUsername(user)
        vm.setPassword(password)
    }

    // ------------------------------------------------------------ boot e lista

    @Test
    fun bootLeAListaSemRedeNemCofre() = runTest(dispatcher) {
        start(camera("Garagem", "rtsp://192.168.0.10:554/stream1", "admin"))
        assertTrue(state.loaded)
        assertEquals(Screen.List, state.screen)
        assertEquals(listOf("Garagem"), state.cameras.map { it.name })
        assertEquals("rtsp://192.168.0.10:554/stream1", state.cameras.single().address)
        assertEquals(1, repository.loads)
        assertEquals(0, credentials.calls, "o boot não decifra senhas")
        assertEquals(0, resolver.calls.size, "o boot não consulta câmeras")
        assertEquals(0, discovery.calls, "o boot não procura câmeras")
        assertTrue(engine.opened.isEmpty(), "o boot não abre RTSP")
    }

    @Test
    fun arquivoCorrompidoMostraAvisoQueFecha() = runTest(dispatcher) {
        start(warning = RepositoryWarning.CorruptedFileRecovered("/dados/cameras.json.corrompido-1"))
        assertEquals(
            "A lista de câmeras não pôde ser lida e foi guardada em /dados/cameras.json.corrompido-1. " +
                "Começando com a lista vazia.",
            state.banner,
        )
        assertTrue(state.cameras.isEmpty())
        vm.dismissBanner()
        assertNull(state.banner)
    }

    @Test
    fun remocaoSoDepoisDaConfirmacao() = runTest(dispatcher) {
        val c = camera("Garagem", "rtsp://192.168.0.10/s", "admin")
        credentials.passwords[c.id] = Password("p1")
        start(c)
        vm.requestRemove(c.id)
        assertEquals("Garagem", state.confirmRemove?.name)
        assertTrue(vm.back(), "voltar fecha a confirmação")
        assertNull(state.confirmRemove)
        advanceUntilIdle()
        assertEquals(1, repository.saved.size, "cancelar não remove")

        vm.requestRemove(c.id)
        vm.confirmRemove()
        advanceUntilIdle()
        assertTrue(state.cameras.isEmpty())
        assertTrue(repository.saved.isEmpty())
        assertTrue(credentials.passwords.isEmpty(), "a senha sai do cofre")
    }

    // ------------------------------------------------------------ formulário

    @Test
    fun adicionaPorUrlCompletaSemConsulta() = runTest(dispatcher) {
        start()
        vm.openNewForm()
        assertEquals("Nova câmera", state.form.title)
        fill("Garagem", "rtsp://192.168.0.10:554/stream1", "admin", "p1")
        vm.save()
        advanceUntilIdle()
        assertEquals(Screen.List, state.screen)
        val saved = repository.saved.single()
        assertEquals("Garagem", saved.name.value)
        assertEquals(Password("p1"), credentials.passwords[saved.id])
        assertTrue(resolver.calls.isEmpty())
        assertEquals("", state.form.password, "a senha não fica no estado")
    }

    @Test
    fun errosDeValidacaoVaoParaOCampo() = runTest(dispatcher) {
        start()
        vm.openNewForm()
        fill("", "ftp://192.168.0.10")
        vm.save()
        advanceUntilIdle()
        assertEquals(Screen.Form, state.screen)
        assertEquals("Informe um nome.", state.form.nameError)
        assertEquals("Use um IP, um nome de host ou uma URL rtsp:// completa.", state.form.addressError)

        fill("", "rtsp://192.168.0.10:99999/s")
        vm.save()
        advanceUntilIdle()
        assertEquals("Informe um nome.", state.form.nameError)
        assertEquals("Porta inválida: use um número entre 1 e 65535.", state.form.addressError)
        assertEquals(FormPhase.Editing, state.form.phase)
        assertTrue(repository.saved.isEmpty())
    }

    @Test
    fun edicaoComSenhaVaziaMantemASenhaAtual() = runTest(dispatcher) {
        val c = camera("Garagem", "rtsp://192.168.0.10/s", "admin")
        credentials.passwords[c.id] = Password("antiga")
        start(c)
        vm.openEditForm(c.id)
        assertEquals("Editar câmera", state.form.title)
        assertEquals("rtsp://192.168.0.10/s", state.form.address)
        assertEquals("admin", state.form.username)
        assertEquals("", state.form.password)
        vm.setName("Portão")
        vm.save()
        advanceUntilIdle()
        assertEquals(listOf("Portão"), state.cameras.map { it.name })
        assertEquals(c.id, repository.saved.single().id)
        assertEquals(Password("antiga"), credentials.passwords[c.id])
    }

    @Test
    fun falhaNoCofreAoAdicionarPassaAEditarACamera() = runTest(dispatcher) {
        start()
        credentials.failSet = "Keystore indisponível"
        vm.openNewForm()
        fill("Garagem", "rtsp://192.168.0.10/s", "admin", "p1")
        vm.save()
        advanceUntilIdle()
        assertEquals(Screen.Form, state.screen)
        assertEquals(repository.saved.single().id, state.form.editing)
        assertEquals("Editar câmera", state.form.title)
        assertEquals("Câmera salva, mas a senha não pôde ser guardada no cofre: Keystore indisponível", state.form.formError)
    }

    // ------------------------------------------------------------ consulta ONVIF

    @Test
    fun consultaOnvifGravaPrincipalSecundarioENomeSugerido() = runTest(dispatcher) {
        start()
        resolver.answer = { streams("192.168.0.10", "TP-Link C200") }
        vm.openNewForm()
        fill("", "192.168.0.10", "admin", "p1")
        vm.save()
        assertEquals(FormPhase.LookingUp, state.form.phase)
        advanceUntilIdle()
        val call = resolver.calls.single()
        assertEquals(StreamTarget.Address(rtspviewer.core.domain.DeviceAddress.parse("192.168.0.10")), call.target)
        assertEquals("admin", call.username)
        assertEquals(Password("p1"), call.password)
        assertEquals(Screen.List, state.screen)
        val saved = repository.saved.single()
        assertEquals("TP-Link C200", saved.name.value)
        assertEquals("rtsp://192.168.0.10:554/stream1", saved.url.toString())
        assertEquals("rtsp://192.168.0.10:554/stream2", saved.substream.toString())
        assertEquals(Password("p1"), credentials.passwords[saved.id])
    }

    @Test
    fun semNomeNemSugestaoUsaCameraEEndereco() = runTest(dispatcher) {
        start()
        resolver.answer = { streams("192.168.0.10") }
        vm.openNewForm()
        fill("  ", "192.168.0.10")
        vm.save()
        advanceUntilIdle()
        assertEquals("Câmera 192.168.0.10", repository.saved.single().name.value)
    }

    @Test
    fun nomeDigitadoPrevaleceSobreOSugerido() = runTest(dispatcher) {
        start()
        resolver.answer = { streams("192.168.0.10", "TP-Link C200") }
        vm.openNewForm()
        fill("Garagem", "192.168.0.10")
        vm.save()
        advanceUntilIdle()
        assertEquals("Garagem", repository.saved.single().name.value)
    }

    @Test
    fun autenticacaoRecusadaNaoSalvaEApagaASenha() = runTest(dispatcher) {
        start()
        resolver.answer = { throw ResolveError.AuthenticationFailed() }
        vm.openNewForm()
        fill("", "192.168.0.10", "admin", "errada")
        vm.save()
        advanceUntilIdle()
        assertEquals(Screen.Form, state.screen)
        assertEquals(FormPhase.Editing, state.form.phase)
        assertEquals("Usuário ou senha inválidos.", state.form.formError)
        assertEquals("", state.form.password, "a senha sai do campo ao fim da consulta")
        assertTrue(repository.saved.isEmpty())
    }

    @Test
    fun semOnvifEscolherHikvisionMontaOsCaminhos() = runTest(dispatcher) {
        start()
        resolver.answer = { throw ResolveError.Unreachable("tempo esgotado") }
        vm.openNewForm()
        fill("", "192.168.0.20", "admin", "p1")
        vm.save()
        advanceUntilIdle()
        assertEquals(FormPhase.ChooseVendor, state.form.phase)
        assertEquals("Motivo: A câmera não respondeu: tempo esgotado", state.form.vendorReason)
        assertEquals("", state.form.password)
        vm.chooseVendor(Vendor.Hikvision)
        advanceUntilIdle()
        assertEquals(Screen.List, state.screen)
        val saved = repository.saved.single()
        assertEquals("Câmera 192.168.0.20", saved.name.value)
        assertEquals("rtsp://192.168.0.20:554/Streaming/Channels/101", saved.url.toString())
        assertEquals("rtsp://192.168.0.20:554/Streaming/Channels/102", saved.substream.toString())
        assertEquals(Password("p1"), credentials.passwords[saved.id], "a senha digitada vai para o cofre")
    }

    @Test
    fun semOnvifSemMotivoECancelarVoltaAoFormulario() = runTest(dispatcher) {
        start()
        resolver.answer = { throw ResolveError.NotOnvif() }
        vm.openNewForm()
        fill("Garagem", "192.168.0.20")
        vm.save()
        advanceUntilIdle()
        assertEquals(FormPhase.ChooseVendor, state.form.phase)
        assertEquals("", state.form.vendorReason)
        assertTrue(vm.back(), "voltar faz o papel do Esc")
        assertEquals(Screen.Form, state.screen)
        assertEquals(FormPhase.Editing, state.form.phase)
        assertEquals("Garagem", state.form.name)
        vm.chooseVendor(Vendor.Hikvision)
        advanceUntilIdle()
        assertTrue(repository.saved.isEmpty(), "fabricante depois do cancelar não grava")
    }

    @Test
    fun cancelarConsultaIgnoraOResultado() = runTest(dispatcher) {
        start()
        resolver.answer = { streams("192.168.0.10", "TP-Link C200") }
        // O usuário cancela enquanto a câmera ainda responde.
        resolver.onCall = { vm.cancelLookup() }
        vm.openNewForm()
        fill("", "192.168.0.10", "admin", "p1")
        vm.save()
        advanceUntilIdle()
        assertEquals(1, resolver.calls.size)
        assertEquals(Screen.Form, state.screen)
        assertEquals(FormPhase.Editing, state.form.phase)
        assertEquals("192.168.0.10", state.form.address, "os campos ficam como estavam")
        assertEquals("p1", state.form.password)
        assertTrue(repository.saved.isEmpty())
        assertNull(state.form.formError)
    }

    @Test
    fun voltarDuranteAConsultaCancela() = runTest(dispatcher) {
        start()
        resolver.answer = { streams("192.168.0.10") }
        vm.openNewForm()
        fill("", "192.168.0.10")
        vm.save()
        assertTrue(vm.back())
        advanceUntilIdle()
        assertEquals(Screen.Form, state.screen)
        assertEquals(FormPhase.Editing, state.form.phase)
        assertTrue(repository.saved.isEmpty())
    }

    @Test
    fun edicaoComUsuarioESemSenhaPedeASenhaSemConsultar() = runTest(dispatcher) {
        val c = camera("Garagem", "rtsp://192.168.0.10/s", "admin")
        start(c)
        vm.openEditForm(c.id)
        vm.setAddress("192.168.0.10")
        vm.save()
        advanceUntilIdle()
        assertEquals("Informe a senha para consultar a câmera.", state.form.formError)
        assertTrue(resolver.calls.isEmpty())
    }

    // ------------------------------------------------------------ busca

    @Test
    fun buscaListaEncontradasEMarcaJaCadastrada() = runTest(dispatcher) {
        start(camera("Garagem", "rtsp://192.168.0.10:554/stream1"))
        assertEquals(0, discovery.calls)
        discovery.answer = { listOf(found("192.168.0.10", "TP-Link C200"), found("192.168.0.11")) }
        vm.openDiscovery()
        assertEquals(Screen.Discover, state.screen)
        assertEquals(DiscoverState.Searching, state.discover)
        advanceUntilIdle()
        val rows = assertIs<DiscoverState.Found>(state.discover).rows
        assertEquals(
            listOf(
                FoundRow(0, "TP-Link C200", "192.168.0.10", registered = true),
                FoundRow(1, "Câmera ONVIF", "192.168.0.11", registered = false),
            ),
            rows,
        )
    }

    @Test
    fun buscaVaziaEComErro() = runTest(dispatcher) {
        start()
        vm.openDiscovery()
        advanceUntilIdle()
        assertEquals(DiscoverState.Empty, state.discover)
        discovery.answer = { throw DiscoveryFailure("sem permissão") }
        vm.startDiscovery()
        advanceUntilIdle()
        assertEquals(DiscoverState.Failed("Não foi possível procurar câmeras: sem permissão"), state.discover)
    }

    @Test
    fun buscaAbandonadaEIgnorada() = runTest(dispatcher) {
        start()
        discovery.answer = { listOf(found("192.168.0.11")) }
        // O usuário volta à lista enquanto a busca ainda roda.
        discovery.onCall = { vm.back() }
        vm.openDiscovery()
        advanceUntilIdle()
        assertEquals(1, discovery.calls)
        assertEquals(Screen.List, state.screen)
        assertEquals(DiscoverState.Searching, state.discover, "o resultado da busca abandonada não aparece")
    }

    @Test
    fun procurarDeNovoIgnoraABuscaAnterior() = runTest(dispatcher) {
        start()
        discovery.answer = { n -> listOf(found("192.168.0.${10 + n}")) }
        discovery.onCall = { n -> if (n == 1) vm.startDiscovery() }
        vm.openDiscovery()
        advanceUntilIdle()
        assertEquals(2, discovery.calls)
        val rows = assertIs<DiscoverState.Found>(state.discover).rows
        assertEquals(listOf("192.168.0.12"), rows.map { it.host })
    }

    @Test
    fun adicionarEncontradaConsultaPelaUrlDeServico() = runTest(dispatcher) {
        start()
        discovery.answer = { listOf(found("192.168.0.11", "Tapo C200")) }
        vm.openDiscovery()
        advanceUntilIdle()
        vm.addFound(0)
        assertEquals(Screen.Form, state.screen)
        assertEquals("Tapo C200", state.form.name)
        assertEquals("192.168.0.11", state.form.address)
        resolver.answer = { streams("192.168.0.11") }
        vm.setUsername("admin")
        vm.setPassword("p1")
        vm.save()
        advanceUntilIdle()
        assertEquals(StreamTarget.ServiceUrl("http://192.168.0.11:2020/onvif/device_service"), resolver.calls.single().target)
        assertEquals("Tapo C200", repository.saved.single().name.value)
    }

    @Test
    fun enderecoAlteradoDepoisDaBuscaConsultaOEnderecoDigitado() = runTest(dispatcher) {
        start()
        discovery.answer = { listOf(found("192.168.0.11")) }
        vm.openDiscovery()
        advanceUntilIdle()
        vm.addFound(0)
        vm.setAddress("192.168.0.12")
        resolver.answer = { streams("192.168.0.12") }
        vm.save()
        advanceUntilIdle()
        assertIs<StreamTarget.Address>(resolver.calls.single().target)
    }

    // ------------------------------------------------------------ visualização

    @Test
    fun abrirCameraConectaComCredenciaisEVoltarPara() = runTest(dispatcher) {
        val c = camera("Garagem", "rtsp://192.168.0.10:554/stream1", "admin")
        credentials.passwords[c.id] = Password("p1")
        start(c)
        vm.openCamera(c.id)
        assertEquals(Screen.Viewer, state.screen)
        assertEquals(ViewStatus.Message("Conectando…"), Texts.status(state.viewer, vm.player.state.value))
        advanceUntilIdle()
        assertEquals("rtsp://admin:p1@192.168.0.10:554/stream1", engine.opened.single().expose())
        assertEquals(PlayerState.Connecting, vm.player.state.value)
        engine.listener!!.onPlaying()
        assertEquals(ViewStatus.None, Texts.status(state.viewer, vm.player.state.value))
        val closes = engine.closes
        vm.backToList()
        assertEquals(Screen.List, state.screen)
        assertEquals(PlayerState.Idle, vm.player.state.value)
        assertEquals(closes + 1, engine.closes, "o player fecha ao sair da tela")
    }

    @Test
    fun aberturaAntigaEIgnoradaAoVoltar() = runTest(dispatcher) {
        val c = camera("Garagem", "rtsp://192.168.0.10/s")
        start(c)
        vm.openCamera(c.id)
        vm.backToList()
        advanceUntilIdle()
        assertTrue(engine.opened.isEmpty(), "a abertura cancelada não conecta")
        assertEquals(Screen.List, state.screen)
    }

    @Test
    fun semSenhaPedeAoUsuarioEConecta() = runTest(dispatcher) {
        val c = camera("Garagem", "rtsp://192.168.0.10/s", "admin")
        start(c)
        vm.openCamera(c.id)
        advanceUntilIdle()
        assertEquals("admin", state.viewer.passwordUser)
        assertTrue(engine.opened.isEmpty())
        assertEquals(ViewStatus.None, Texts.status(state.viewer, vm.player.state.value))
        vm.submitPassword("digitada")
        advanceUntilIdle()
        assertNull(state.viewer.passwordUser)
        assertEquals("rtsp://admin:digitada@192.168.0.10/s", engine.opened.single().expose())
        assertEquals(Password("digitada"), credentials.passwords[c.id], "a senha fica no cofre")
    }

    @Test
    fun cancelarSenhaVoltaALista() = runTest(dispatcher) {
        val c = camera("Garagem", "rtsp://192.168.0.10/s", "admin")
        start(c)
        vm.openCamera(c.id)
        advanceUntilIdle()
        assertTrue(vm.back())
        assertEquals(Screen.List, state.screen)
        assertTrue(engine.opened.isEmpty())
    }

    @Test
    fun telaCheiaEVoltar() = runTest(dispatcher) {
        val c = camera("Garagem", "rtsp://192.168.0.10/s")
        start(c)
        vm.openCamera(c.id)
        advanceUntilIdle()
        vm.toggleFullscreen()
        assertTrue(state.viewer.fullscreen)
        assertTrue(vm.back(), "voltar sai da tela cheia")
        assertFalse(state.viewer.fullscreen)
        assertEquals(Screen.Viewer, state.screen)
        assertTrue(vm.back(), "segundo voltar volta à lista")
        assertEquals(Screen.List, state.screen)
        assertFalse(vm.back(), "na lista, o sistema fecha o app")
    }

    @Test
    fun segundoPlanoEncerraEVoltarReconecta() = runTest(dispatcher) {
        val c = camera("Garagem", "rtsp://192.168.0.10/s")
        start(c)
        vm.onAppStopped()
        vm.onAppStarted()
        advanceUntilIdle()
        assertTrue(engine.opened.isEmpty(), "fora da visualização não conecta")

        vm.openCamera(c.id)
        advanceUntilIdle()
        engine.listener!!.onPlaying()
        val closes = engine.closes
        vm.onAppStopped()
        assertEquals(closes + 1, engine.closes, "a conexão é encerrada")
        assertEquals(PlayerState.Idle, vm.player.state.value)
        vm.onAppStarted()
        advanceUntilIdle()
        assertEquals(2, engine.opened.size, "ao voltar, conecta de novo")
        assertEquals(Screen.Viewer, state.screen)
    }

    @Test
    fun erroDoPlayerMostraMensagemETentarDeNovo() = runTest(dispatcher) {
        val c = camera("Garagem", "rtsp://192.168.0.10/s", "admin")
        credentials.passwords[c.id] = Password("errada")
        start(c)
        vm.openCamera(c.id)
        advanceUntilIdle()
        engine.listener!!.onFailure(rtspviewer.core.application.StreamError.AuthenticationFailed())
        assertEquals(ViewStatus.Failed("Usuário ou senha inválidos."), Texts.status(state.viewer, vm.player.state.value))
        vm.retry()
        advanceUntilIdle()
        assertEquals(2, engine.opened.size)
    }
}
