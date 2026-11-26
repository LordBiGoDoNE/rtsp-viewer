package rtspviewer.app.ui.theme

import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Typography
import androidx.compose.material3.darkColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.text.TextStyle

/** Tema escuro único, todo a partir de `Tokens` (gerado de design/tokens.json). */
@Composable
fun RtspViewerTheme(content: @Composable () -> Unit) {
    val colors = darkColorScheme(
        primary = Tokens.Colors.primary,
        onPrimary = Tokens.Colors.onPrimary,
        background = Tokens.Colors.background,
        onBackground = Tokens.Colors.text,
        surface = Tokens.Colors.surface,
        onSurface = Tokens.Colors.text,
        surfaceVariant = Tokens.Colors.surfaceRaised,
        onSurfaceVariant = Tokens.Colors.textMuted,
        error = Tokens.Colors.danger,
        onError = Tokens.Colors.onDanger,
        outline = Tokens.Colors.border,
    )
    val body = TextStyle(fontSize = Tokens.Font.sizeBody, fontWeight = Tokens.Font.weightRegular)
    val typography = Typography(
        bodyLarge = body,
        bodyMedium = body,
        bodySmall = body.copy(fontSize = Tokens.Font.sizeSmall),
        labelLarge = body.copy(fontWeight = Tokens.Font.weightBold),
        labelMedium = body.copy(fontSize = Tokens.Font.sizeSmall),
        titleLarge = body.copy(fontSize = Tokens.Font.sizeTitle, fontWeight = Tokens.Font.weightBold),
        titleMedium = body.copy(fontWeight = Tokens.Font.weightBold),
        headlineSmall = body.copy(fontSize = Tokens.Font.sizeDisplay, fontWeight = Tokens.Font.weightBold),
    )
    MaterialTheme(colorScheme = colors, typography = typography, content = content)
}
