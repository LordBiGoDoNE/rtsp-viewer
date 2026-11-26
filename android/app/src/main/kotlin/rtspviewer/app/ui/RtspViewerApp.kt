package rtspviewer.app.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import rtspviewer.app.ui.theme.RtspViewerTheme
import rtspviewer.app.ui.theme.Tokens

/**
 * Raiz da UI. Nesta etapa (grupo 1) só a lista vazia, para validar o tema
 * gerado; as telas de `design/telas.md` entram no grupo 5.
 */
@Composable
fun RtspViewerApp() {
    RtspViewerTheme {
        Column(
            Modifier
                .fillMaxSize()
                .background(Tokens.Colors.background)
                .safeDrawingPadding(),
        ) {
            Box(
                Modifier
                    .fillMaxWidth()
                    .height(Tokens.Size.toolbarHeight)
                    .background(Tokens.Colors.surface)
                    .padding(horizontal = Tokens.Space.l),
                contentAlignment = Alignment.CenterStart,
            ) {
                Text("Câmeras", style = MaterialTheme.typography.titleLarge, color = Tokens.Colors.text)
            }
            Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                Text("Nenhuma câmera cadastrada.", color = Tokens.Colors.text)
            }
        }
    }
}
