package rtspviewer.app.ui

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import rtspviewer.app.ui.theme.Tokens

/** Tela 1: lista de câmeras (design/telas.md, seções 1 e 6). */
@Composable
internal fun ListScreen(state: UiState, vm: AppViewModel) {
    Column(Modifier.fillMaxSize().safeDrawingPadding()) {
        Toolbar {
            ToolbarTitle("Câmeras", Modifier.weight(1f))
            PrimaryButton("Adicionar câmera", vm::openNewForm)
            MenuButton("Mais opções", listOf("Procurar câmeras" to vm::openDiscovery, "Sobre" to vm::openAbout))
        }
        state.banner?.let { Banner(it, vm::dismissBanner) }
        when {
            !state.loaded -> Unit
            state.cameras.isEmpty() -> EmptyList(vm)
            else -> LazyColumn(Modifier.fillMaxSize()) {
                items(state.cameras, key = { it.id.value }) { row -> CameraListItem(row, vm) }
            }
        }
    }
    state.confirmRemove?.let { row ->
        AppDialog(
            onDismiss = vm::dismissRemove,
            title = Texts.removeTitle(row.name),
            text = { Text("A câmera e a senha salva serão apagadas.") },
            confirm = { DangerButton("Remover", vm::confirmRemove) },
            dismiss = { QuietButton("Cancelar", vm::dismissRemove) },
        )
    }
}

@Composable
private fun EmptyList(vm: AppViewModel) {
    Box(Modifier.fillMaxSize().padding(Tokens.Space.xl), contentAlignment = Alignment.Center) {
        Column(
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(Tokens.Space.l),
        ) {
            Text(
                "Nenhuma câmera cadastrada.",
                style = MaterialTheme.typography.titleLarge,
                color = Tokens.Colors.textMuted,
                textAlign = TextAlign.Center,
            )
            PrimaryButton("Adicionar câmera", vm::openNewForm)
            QuietButton("Procurar câmeras", vm::openDiscovery)
        }
    }
}

@Composable
private fun CameraListItem(row: CameraRow, vm: AppViewModel) {
    Row(
        Modifier
            .fillMaxWidth()
            .height(Tokens.Size.listItemHeight)
            .clickable { vm.openCamera(row.id) }
            .padding(start = Tokens.Space.l, end = Tokens.Space.xs),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(Modifier.weight(1f)) {
            Text(
                row.name,
                style = MaterialTheme.typography.titleMedium,
                color = Tokens.Colors.text,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            Text(
                row.address,
                style = MaterialTheme.typography.bodySmall,
                color = Tokens.Colors.textMuted,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
        MenuButton(
            Texts.rowActions(row.name),
            listOf("Editar" to { vm.openEditForm(row.id) }, "Remover" to { vm.requestRemove(row.id) }),
        )
    }
    HorizontalDivider(color = Tokens.Colors.border)
}
