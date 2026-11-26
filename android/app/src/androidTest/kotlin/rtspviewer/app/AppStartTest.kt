package rtspviewer.app

import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/** O app abre com o tema gerado e mostra a lista (grupo 1). */
@RunWith(AndroidJUnit4::class)
class AppStartTest {
    @get:Rule
    val compose = createAndroidComposeRule<MainActivity>()

    @Test
    fun abreNaListaDeCameras() {
        compose.onNodeWithText("Câmeras").assertIsDisplayed()
        compose.onNodeWithText("Nenhuma câmera cadastrada.").assertIsDisplayed()
    }
}
