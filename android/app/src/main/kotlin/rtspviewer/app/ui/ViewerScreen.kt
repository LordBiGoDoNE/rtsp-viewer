package rtspviewer.app.ui

import android.content.pm.ActivityInfo
import android.view.SurfaceView
import androidx.activity.compose.LocalActivity
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.viewinterop.AndroidView
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsControllerCompat
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import rtspviewer.app.player.PlayerState
import rtspviewer.app.player.StreamInfo
import rtspviewer.app.ui.theme.Tokens

/** Tela 3: visualização (design/telas.md, seções 3 e 6). */
@Composable
internal fun ViewerScreen(viewer: ViewerState, vm: AppViewModel) {
    val playerState by vm.player.state.collectAsStateWithLifecycle()
    val info by vm.player.info.collectAsStateWithLifecycle()
    val status = Texts.status(viewer, playerState)
    ImmersiveMode(viewer.fullscreen)
    val insets = if (viewer.fullscreen) Modifier else Modifier.safeDrawingPadding()
    Column(Modifier.fillMaxSize().background(Tokens.Colors.videoBackground).then(insets)) {
        if (!viewer.fullscreen) {
            Toolbar {
                QuietButton("Voltar", vm::backToList)
                ToolbarTitle(viewer.name, Modifier.weight(1f))
                QuietButton("Informações", vm::toggleInfo)
                QuietButton("Tela cheia", vm::toggleFullscreen)
            }
        }
        Box(Modifier.fillMaxWidth().weight(1f), contentAlignment = Alignment.Center) {
            Video(info, playing = playerState == PlayerState.Playing, attach = vm::attachSurface)
            // Camada só para o duplo toque, por cima do vídeo e abaixo das mensagens.
            Box(
                Modifier
                    .matchParentSize()
                    .testTag("video")
                    .pointerInput(vm) { detectTapGestures(onDoubleTap = { vm.toggleFullscreen() }) },
            )
            StatusMessage(status, vm::retry)
            if (viewer.showInfo) {
                Column(Modifier.align(Alignment.TopEnd).padding(Tokens.Space.m).overlayPanel(Tokens.Size.infoWidth)) {
                    for (line in Texts.info(info, viewer.displayUrl)) {
                        Text(line, style = MaterialTheme.typography.bodySmall, color = Tokens.Colors.text)
                    }
                }
            }
        }
    }
    viewer.passwordUser?.let { user -> PasswordDialog(user, vm::submitPassword, vm::cancelPassword) }
}

/** Vídeo numa `SurfaceView` (sem cópia de quadros), mantendo a proporção. */
@Composable
private fun Video(info: StreamInfo?, playing: Boolean, attach: (SurfaceView?) -> Unit) {
    val ratio = info?.takeIf { it.width > 0 && it.height > 0 }?.let { it.width.toFloat() / it.height }
    val size = if (ratio != null) Modifier.aspectRatio(ratio) else Modifier.fillMaxSize()
    AndroidView(
        factory = { context -> SurfaceView(context).also(attach) },
        modifier = size,
        onRelease = { attach(null) },
        // A tela fica ligada só enquanto o vídeo está em reprodução.
        update = { it.keepScreenOn = playing },
    )
}

@Composable
private fun StatusMessage(status: ViewStatus, onRetry: () -> Unit) {
    when (status) {
        ViewStatus.None -> Unit
        is ViewStatus.Message -> Box(Modifier.padding(Tokens.Space.l).overlayPanel(Tokens.Size.statusWidth)) {
            Text(status.text, color = Tokens.Colors.text, textAlign = TextAlign.Center)
        }
        is ViewStatus.Failed -> Column(
            Modifier.padding(Tokens.Space.l).overlayPanel(Tokens.Size.statusWidth),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(Tokens.Space.m),
        ) {
            Text(status.text, color = Tokens.Colors.text, textAlign = TextAlign.Center)
            PrimaryButton("Tentar de novo", onRetry)
        }
    }
}

@Composable
private fun PasswordDialog(user: String, onSubmit: (String) -> Unit, onCancel: () -> Unit) {
    // Só na composição, nunca no estado salvo da Activity.
    var password by remember { mutableStateOf("") }
    AppDialog(
        onDismiss = onCancel,
        title = Texts.passwordTitle(user),
        text = {
            SecretField(
                label = "Senha",
                value = password,
                onChange = { password = it },
                tag = "campo-senha-pedida",
                onDone = { onSubmit(password) },
            )
        },
        confirm = { PrimaryButton("Conectar", { onSubmit(password) }) },
        dismiss = { QuietButton("Cancelar", onCancel) },
    )
}

/**
 * Tela cheia imersiva: barras do sistema escondidas (um deslize as mostra por
 * instantes) e rotação pelo sensor; ao sair, tudo volta ao normal.
 */
@Composable
private fun ImmersiveMode(on: Boolean) {
    val activity = LocalActivity.current ?: return
    DisposableEffect(activity, on) {
        if (!on) return@DisposableEffect onDispose {}
        val window = activity.window
        val controller = WindowCompat.getInsetsController(window, window.decorView)
        controller.systemBarsBehavior = WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
        controller.hide(WindowInsetsCompat.Type.systemBars())
        activity.requestedOrientation = ActivityInfo.SCREEN_ORIENTATION_FULL_SENSOR
        onDispose {
            controller.show(WindowInsetsCompat.Type.systemBars())
            activity.requestedOrientation = ActivityInfo.SCREEN_ORIENTATION_UNSPECIFIED
        }
    }
}
