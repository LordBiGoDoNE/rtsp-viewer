package rtspviewer.app.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextAlign
import rtspviewer.app.ui.theme.Tokens

/** Licenças de terceiros do Android (design/telas.md, seção 6). */
internal val THIRD_PARTY_LICENSES = listOf(
    "Media3 (AndroidX) — Apache-2.0",
    "Jetpack Compose — Apache-2.0",
    "AndroidX — Apache-2.0",
    "Kotlin e kotlinx — Apache-2.0",
)

/** Tela 4: sobre (design/telas.md, seções 4 e 6). */
@Composable
internal fun AboutScreen(vm: AppViewModel) {
    val version = remember(vm) { vm.version() }
    Column(
        Modifier
            .fillMaxSize()
            .safeDrawingPadding()
            .verticalScroll(rememberScrollState())
            .padding(Tokens.Space.xl),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(Tokens.Space.l, Alignment.CenterVertically),
    ) {
        Text(
            "rtsp-viewer $version".trim(),
            style = MaterialTheme.typography.headlineSmall,
            color = Tokens.Colors.text,
            textAlign = TextAlign.Center,
        )
        Text("Client leve para câmeras RTSP.", color = Tokens.Colors.textMuted, textAlign = TextAlign.Center)
        Column(
            Modifier.widthIn(max = Tokens.Size.aboutWidth).fillMaxWidth(),
            verticalArrangement = Arrangement.spacedBy(Tokens.Space.xs),
        ) {
            Text("Licenças de terceiros", style = MaterialTheme.typography.titleMedium, color = Tokens.Colors.text)
            for (license in THIRD_PARTY_LICENSES) {
                Text(license, style = MaterialTheme.typography.bodySmall, color = Tokens.Colors.textMuted)
            }
        }
        PrimaryButton("Voltar", vm::closeAbout)
    }
}
