package rtspviewer.app.ui

import androidx.activity.ComponentActivity
import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertTextContains
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onFirst
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.performTextInput
import androidx.test.ext.junit.runners.AndroidJUnit4
import kotlinx.coroutines.Dispatchers
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import rtspviewer.core.application.ResolveError
import rtspviewer.core.domain.Camera

/** Fluxos principais da UI com portas falsas, sem rede (design/telas.md). */
@OptIn(ExperimentalTestApi::class)
@RunWith(AndroidJUnit4::class)
class UiFlowsTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    private val credentials = MemoryCredentials()
    private val resolver = FakeResolver()
    private val discovery = FakeDiscovery()
    private val engine = FakeEngine()
    private lateinit var repository: MemoryRepository

    private fun start(vararg cameras: Camera) {
        repository = MemoryRepository(cameras.toList())
        val vm = AppViewModel(AppPorts(repository, credentials, resolver, discovery, engine, io = Dispatchers.IO))
        compose.setContent { RtspViewerApp(vm) }
        compose.waitUntilExactlyOneExists(hasText("Câmeras"))
    }

    private fun waitFor(text: String) = compose.waitUntilAtLeastOneExists(hasText(text), TIMEOUT_MS)

    /**
     * "Salvar" fica no fim do formulário rolável: com o teclado aberto (mais
     * alto no campo de senha, com a barra de sugestões), ele pode sair da área
     * visível, e um clique sem rolar não chega ao botão. O usuário rola; o
     * teste também.
     */
    private fun clickSave() = compose.onNodeWithText("Salvar").performScrollTo().performClick()

    private fun pressBack() =compose.runOnUiThread { compose.activity.onBackPressedDispatcher.onBackPressed() }

    @Test
    fun listaVaziaAdicionarPorUrlEApareceNaLista() {
        start()
        waitFor("Nenhuma câmera cadastrada.")
        compose.onAllNodesWithText("Adicionar câmera").onFirst().performClick()
        compose.onNodeWithText("Nova câmera").assertIsDisplayed()
        compose.onNodeWithTag("campo-nome").performTextInput("Garagem")
        compose.onNodeWithTag("campo-endereco").performTextInput("rtsp://192.168.0.10:554/stream1")
        clickSave()
        waitFor("Garagem")
        compose.onNodeWithText("Câmeras").assertIsDisplayed()
        compose.onNodeWithText("rtsp://192.168.0.10:554/stream1").assertIsDisplayed()
        assertEquals(1, repository.saved.size)
        assertTrue("URL completa não consulta a câmera", resolver.calls.isEmpty())
    }

    @Test
    fun remocaoPedeConfirmacao() {
        start(camera("Garagem", "rtsp://192.168.0.10/s"))
        waitFor("Garagem")
        compose.onNodeWithContentDescription("Ações de Garagem").performClick()
        compose.onNodeWithText("Remover").performClick()
        compose.onNodeWithText("Remover Garagem?").assertIsDisplayed()
        compose.onNodeWithText("A câmera e a senha salva serão apagadas.").assertIsDisplayed()
        compose.onNodeWithText("Cancelar").performClick()
        compose.onNodeWithText("Garagem").assertIsDisplayed()
        assertEquals(1, repository.saved.size)

        compose.onNodeWithContentDescription("Ações de Garagem").performClick()
        compose.onNodeWithText("Remover").performClick()
        compose.onNodeWithText("Remover").performClick()
        waitFor("Nenhuma câmera cadastrada.")
        assertTrue(repository.saved.isEmpty())
    }

    @Test
    fun buscaMostraEncontradasEJaCadastrada() {
        start(camera("Garagem", "rtsp://192.168.0.10:554/stream1"))
        discovery.answer = { listOf(found("192.168.0.10", "TP-Link C200"), found("192.168.0.11")) }
        compose.onNodeWithContentDescription("Mais opções").performClick()
        compose.onNodeWithText("Procurar câmeras").performClick()
        waitFor("Já cadastrada")
        compose.onNodeWithText("TP-Link C200").assertIsDisplayed()
        compose.onNodeWithText("Câmera ONVIF").assertIsDisplayed()
        compose.onNodeWithText("192.168.0.11").assertIsDisplayed()
        compose.onNodeWithText("Adicionar").performClick()
        compose.onNodeWithText("Nova câmera").assertIsDisplayed()
        compose.onNodeWithTag("campo-endereco").assertTextContains("192.168.0.11")
    }

    @Test
    fun buscaSemCamerasMostraDicasDoAndroid() {
        start()
        compose.onNodeWithText("Procurar câmeras").performClick()
        waitFor("Nenhuma câmera encontrada.")
        compose.onNodeWithText("• o celular está conectado ao Wi-Fi da mesma rede da câmera.").assertIsDisplayed()
        compose.onNodeWithText("Cadastrar pelo IP").performClick()
        compose.onNodeWithText("Nova câmera").assertIsDisplayed()
    }

    @Test
    fun erroDeAutenticacaoNoFormulario() {
        start()
        resolver.answer = { throw ResolveError.AuthenticationFailed() }
        compose.onAllNodesWithText("Adicionar câmera").onFirst().performClick()
        compose.onNodeWithTag("campo-endereco").performTextInput("192.168.0.10")
        compose.onNodeWithTag("campo-usuario").performTextInput("admin")
        compose.onNodeWithTag("campo-senha").performTextInput("errada")
        clickSave()
        waitFor("Usuário ou senha inválidos.")
        compose.onNodeWithText("Nova câmera").assertIsDisplayed()
        assertTrue(repository.saved.isEmpty())
        assertEquals(1, resolver.calls.size)
    }

    @Test
    fun senhaPedidaTelaCheiaEVoltar() {
        start(camera("Garagem", "rtsp://192.168.0.10/s", "admin"))
        waitFor("Garagem")
        compose.onNodeWithText("Garagem").performClick()
        waitFor("Senha de admin")
        compose.onNodeWithTag("campo-senha-pedida").performTextInput("digitada")
        compose.onNodeWithText("Conectar").performClick()
        waitFor("Conectando…")
        compose.waitUntil(TIMEOUT_MS) { engine.opened.isNotEmpty() }
        assertEquals("rtsp://admin:digitada@192.168.0.10/s", engine.opened.single().expose())

        compose.onNodeWithText("Tela cheia").performClick()
        compose.onNodeWithText("Informações").assertDoesNotExist()
        pressBack()
        compose.onNodeWithText("Informações").assertIsDisplayed()
        pressBack()
        waitFor("Câmeras")
        compose.onNodeWithText("Garagem").assertIsDisplayed()
    }

    private companion object {
        const val TIMEOUT_MS = 5_000L
    }
}
