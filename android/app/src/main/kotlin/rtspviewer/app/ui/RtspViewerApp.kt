package rtspviewer.app.ui

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LifecycleEventEffect
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import rtspviewer.app.ui.theme.RtspViewerTheme
import rtspviewer.app.ui.theme.Tokens

/**
 * Raiz da UI: a tela vem do estado do [AppViewModel] (sem biblioteca de
 * navegação). O gesto de voltar faz o papel do Esc do desktop.
 */
@Composable
fun RtspViewerApp(viewModel: AppViewModel) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    // Segundo plano encerra a conexão; ao voltar, a câmera conecta de novo.
    LifecycleEventEffect(Lifecycle.Event.ON_STOP) { viewModel.onAppStopped() }
    LifecycleEventEffect(Lifecycle.Event.ON_START) { viewModel.onAppStarted() }
    BackHandler(enabled = state.screen != Screen.List || state.confirmRemove != null) { viewModel.back() }
    RtspViewerTheme {
        Box(Modifier.fillMaxSize().background(Tokens.Colors.background)) {
            when (state.screen) {
                Screen.List -> ListScreen(state, viewModel)
                Screen.Form -> FormScreen(state.form, viewModel)
                Screen.Viewer -> ViewerScreen(state.viewer, viewModel)
                Screen.Discover -> DiscoverScreen(state.discover, viewModel)
                Screen.About -> AboutScreen(viewModel)
            }
        }
    }
}
