package rtspviewer.app

import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.v2.createEmptyComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.test.core.app.ActivityScenario
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import java.io.File
import org.junit.After
import org.junit.Assert.assertFalse
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import rtspviewer.app.ui.AppPorts

/**
 * O app abre na lista com o wiring real (num diretório vazio) e o boot não
 * cria o cofre: nada é decifrado nem gravado.
 */
@OptIn(ExperimentalTestApi::class)
@RunWith(AndroidJUnit4::class)
class AppStartTest {
    @get:Rule
    val compose = createEmptyComposeRule()

    private val context = InstrumentationRegistry.getInstrumentation().targetContext
    private lateinit var dir: File
    private lateinit var original: (android.content.Context) -> AppPorts

    @Before
    fun prepara() {
        dir = File(context.filesDir, "boot-${System.nanoTime()}").apply { mkdirs() }
        original = AppContainer.portsFactory
        AppContainer.portsFactory = { AppContainer.defaultPorts(it, dir) }
    }

    @After
    fun limpa() {
        AppContainer.portsFactory = original
        dir.deleteRecursively()
    }

    @Test
    fun abreNaListaDeCameras() {
        ActivityScenario.launch(MainActivity::class.java).use {
            compose.waitUntilExactlyOneExists(hasText("Nenhuma câmera cadastrada."), 5_000)
            compose.onNodeWithText("Câmeras").assertIsDisplayed()
            assertFalse("o boot não toca no cofre", File(dir, "credentials.json").exists())
        }
    }
}
