package rtspviewer.app.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import rtspviewer.app.ui.theme.Tokens

/** Tela 5: procurar câmeras (design/telas.md, seções 5 e 6). */
@Composable
internal fun DiscoverScreen(discover: DiscoverState, vm: AppViewModel) {
    Column(Modifier.fillMaxSize().safeDrawingPadding()) {
        Toolbar {
            QuietButton("Voltar", vm::closeDiscovery)
            ToolbarTitle("Procurar câmeras", Modifier.weight(1f))
            QuietButton("Procurar de novo", vm::startDiscovery, enabled = discover !is DiscoverState.Searching)
        }
        when (discover) {
            DiscoverState.Searching -> Centered {
                Text("Procurando câmeras na rede…", color = Tokens.Colors.textMuted, textAlign = TextAlign.Center)
            }
            is DiscoverState.Found -> LazyColumn(Modifier.fillMaxSize()) {
                items(discover.rows, key = { it.index }) { row -> FoundItem(row, vm) }
            }
            DiscoverState.Empty -> Centered {
                Text("Nenhuma câmera encontrada.", style = MaterialTheme.typography.titleLarge, color = Tokens.Colors.text)
                Column(Modifier.widthIn(max = Tokens.Size.formWidth)) {
                    for (hint in DISCOVERY_HINTS) Text(hint, color = Tokens.Colors.textMuted)
                }
                PrimaryButton("Cadastrar pelo IP", vm::registerByAddress)
            }
            is DiscoverState.Failed -> Centered {
                Text(discover.message, color = Tokens.Colors.danger, textAlign = TextAlign.Center)
                PrimaryButton("Cadastrar pelo IP", vm::registerByAddress)
            }
        }
    }
}

/** Dicas de "Nenhuma câmera encontrada" no Android (design/telas.md, seção 6). */
internal val DISCOVERY_HINTS = listOf(
    "Confira se:",
    "• o ONVIF está ativo na câmera (na Tapo, crie a \"Conta da câmera\" no app Tapo);",
    "• o celular está conectado ao Wi-Fi da mesma rede da câmera.",
)

@Composable
private fun Centered(content: @Composable () -> Unit) {
    Box(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(Tokens.Space.xl), contentAlignment = Alignment.Center) {
        Column(
            Modifier.fillMaxWidth(),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(Tokens.Space.l),
        ) { content() }
    }
}

@Composable
private fun FoundItem(row: FoundRow, vm: AppViewModel) {
    Row(
        Modifier
            .fillMaxWidth()
            .height(Tokens.Size.listItemHeight)
            .padding(horizontal = Tokens.Space.l),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(Tokens.Space.m),
    ) {
        Column(Modifier.weight(1f)) {
            Text(
                row.name,
                style = MaterialTheme.typography.titleMedium,
                color = Tokens.Colors.text,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            Text(row.host, style = MaterialTheme.typography.bodySmall, color = Tokens.Colors.textMuted)
        }
        if (row.registered) {
            Text("Já cadastrada", color = Tokens.Colors.textMuted)
        } else {
            PrimaryButton("Adicionar", { vm.addFound(row.index) })
        }
    }
    HorizontalDivider(color = Tokens.Colors.border)
}
