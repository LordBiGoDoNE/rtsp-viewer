package rtspviewer.app.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.text.input.KeyboardActionHandler
import androidx.compose.foundation.text.input.TextFieldState
import androidx.compose.foundation.text.input.clearText
import androidx.compose.material3.OutlinedSecureTextField
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.snapshotFlow
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.editableText
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.window.DialogProperties
import rtspviewer.app.ui.theme.Tokens

private val ControlShape = RoundedCornerShape(Tokens.Radius.s)
private val PanelShape = RoundedCornerShape(Tokens.Radius.m)

/** Barra superior (`size.toolbar-height`, fundo `surface`). */
@Composable
internal fun Toolbar(content: @Composable RowScope.() -> Unit) {
    Row(
        Modifier
            .fillMaxWidth()
            .height(Tokens.Size.toolbarHeight)
            .background(Tokens.Colors.surface)
            .padding(horizontal = Tokens.Space.s),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(Tokens.Space.xs),
        content = content,
    )
}

@Composable
internal fun ToolbarTitle(text: String, modifier: Modifier = Modifier) {
    Text(
        text,
        modifier.padding(horizontal = Tokens.Space.s),
        style = MaterialTheme.typography.titleLarge,
        color = Tokens.Colors.text,
        maxLines = 1,
        overflow = TextOverflow.Ellipsis,
    )
}

@Composable
internal fun PrimaryButton(text: String, onClick: () -> Unit, modifier: Modifier = Modifier, enabled: Boolean = true) {
    Button(
        onClick = onClick,
        modifier = modifier.heightIn(min = Tokens.Size.controlHeight),
        enabled = enabled,
        shape = ControlShape,
        colors = ButtonDefaults.buttonColors(containerColor = Tokens.Colors.primary, contentColor = Tokens.Colors.onPrimary),
        contentPadding = PaddingValues(horizontal = Tokens.Space.l),
    ) { Text(text, maxLines = 1) }
}

/** Botão comum (fundo `surface-raised`), por exemplo os fabricantes. */
@Composable
internal fun SecondaryButton(text: String, onClick: () -> Unit, modifier: Modifier = Modifier) {
    Button(
        onClick = onClick,
        modifier = modifier.heightIn(min = Tokens.Size.controlHeight),
        shape = ControlShape,
        colors = ButtonDefaults.buttonColors(containerColor = Tokens.Colors.surfaceRaised, contentColor = Tokens.Colors.text),
        contentPadding = PaddingValues(horizontal = Tokens.Space.l),
    ) { Text(text, maxLines = 1) }
}

@Composable
internal fun DangerButton(text: String, onClick: () -> Unit, modifier: Modifier = Modifier) {
    Button(
        onClick = onClick,
        modifier = modifier.heightIn(min = Tokens.Size.controlHeight),
        shape = ControlShape,
        colors = ButtonDefaults.buttonColors(containerColor = Tokens.Colors.danger, contentColor = Tokens.Colors.onDanger),
        contentPadding = PaddingValues(horizontal = Tokens.Space.l),
    ) { Text(text, maxLines = 1) }
}

/** Botão discreto (só texto). */
@Composable
internal fun QuietButton(text: String, onClick: () -> Unit, modifier: Modifier = Modifier, enabled: Boolean = true) {
    TextButton(
        onClick = onClick,
        modifier = modifier.heightIn(min = Tokens.Size.controlHeight),
        enabled = enabled,
        shape = ControlShape,
        colors = ButtonDefaults.textButtonColors(contentColor = Tokens.Colors.text),
        contentPadding = PaddingValues(horizontal = Tokens.Space.m),
    ) { Text(text, maxLines = 1) }
}

/** Botão "⋮" que abre um menu; [description] é o nome para acessibilidade. */
@Composable
internal fun MenuButton(description: String, items: List<Pair<String, () -> Unit>>) {
    var open by remember { mutableStateOf(false) }
    Box {
        TextButton(
            onClick = { open = true },
            modifier = Modifier.semantics { contentDescription = description },
            shape = ControlShape,
            colors = ButtonDefaults.textButtonColors(contentColor = Tokens.Colors.text),
        ) { Text("⋮", style = MaterialTheme.typography.titleLarge) }
        DropdownMenu(
            expanded = open,
            onDismissRequest = { open = false },
            containerColor = Tokens.Colors.surfaceRaised,
        ) {
            for ((label, action) in items) {
                DropdownMenuItem(
                    text = { Text(label, color = Tokens.Colors.text) },
                    onClick = {
                        open = false
                        action()
                    },
                )
            }
        }
    }
}

/** Diálogo (`size.dialog-width`, fundo `surface-raised`). */
@Composable
internal fun AppDialog(
    onDismiss: () -> Unit,
    confirm: @Composable () -> Unit,
    title: String? = null,
    dismiss: (@Composable () -> Unit)? = null,
    dismissOnClickOutside: Boolean = true,
    text: (@Composable () -> Unit)? = null,
) {
    AlertDialog(
        onDismissRequest = onDismiss,
        confirmButton = confirm,
        dismissButton = dismiss,
        modifier = Modifier.widthIn(max = Tokens.Size.dialogWidth),
        title = title?.let { t -> { Text(t, style = MaterialTheme.typography.titleLarge) } },
        text = text,
        shape = PanelShape,
        containerColor = Tokens.Colors.surfaceRaised,
        titleContentColor = Tokens.Colors.text,
        textContentColor = Tokens.Colors.text,
        properties = DialogProperties(dismissOnClickOutside = dismissOnClickOutside),
    )
}

/** Campo de texto com rótulo, apoio ou erro (em `danger`) abaixo. */
@Composable
internal fun FormField(
    label: String,
    value: String,
    onChange: (String) -> Unit,
    tag: String,
    modifier: Modifier = Modifier,
    error: String? = null,
    help: String? = null,
    placeholder: String? = null,
    keyboardType: KeyboardType = KeyboardType.Text,
    onDone: (() -> Unit)? = null,
) {
    OutlinedTextField(
        value = value,
        onValueChange = onChange,
        modifier = modifier.fillMaxWidth().testTag(tag),
        label = { Text(label) },
        placeholder = placeholder?.let { p -> { Text(p, color = Tokens.Colors.textMuted) } },
        supportingText = (error ?: help)?.let { t -> { Text(t) } },
        isError = error != null,
        singleLine = true,
        keyboardOptions = KeyboardOptions(
            keyboardType = keyboardType,
            imeAction = if (onDone != null) ImeAction.Done else ImeAction.Next,
            autoCorrectEnabled = false,
        ),
        keyboardActions = KeyboardActions(onDone = { onDone?.invoke() }),
        shape = ControlShape,
    )
}

private const val MASK = "•"

/**
 * Campo de senha (oculto). O texto fica num [TextFieldState] só da composição
 * (não vai para o estado salvo da Activity) e é repassado a [onChange]; quando
 * [value] volta a vazio (o app apagou a senha), o campo é limpo.
 */
@Composable
internal fun SecretField(
    label: String,
    value: String,
    onChange: (String) -> Unit,
    tag: String,
    modifier: Modifier = Modifier,
    help: String? = null,
    onDone: (() -> Unit)? = null,
) {
    val state = remember { TextFieldState(value) }
    val currentOnChange by rememberUpdatedState(onChange)
    LaunchedEffect(state) {
        snapshotFlow { state.text.toString() }.collect { currentOnChange(it) }
    }
    LaunchedEffect(value) {
        if (value.isEmpty() && state.text.isNotEmpty()) state.clearText()
    }
    OutlinedSecureTextField(
        state = state,
        // O campo já é marcado como senha, mas o Compose entrega o texto em claro
        // à árvore de acessibilidade (leitores de tela, `uiautomator dump`). Como
        // o EditText nativo, a acessibilidade recebe só os pontos, um por
        // caractere; o texto real continua no TextFieldState.
        modifier = modifier
            .fillMaxWidth()
            .testTag(tag)
            .semantics { editableText = AnnotatedString(MASK.repeat(state.text.length)) },
        label = { Text(label) },
        supportingText = help?.let { t -> { Text(t) } },
        keyboardOptions = KeyboardOptions(
            keyboardType = KeyboardType.Password,
            imeAction = if (onDone != null) ImeAction.Done else ImeAction.Next,
        ),
        onKeyboardAction = onDone?.let { done -> KeyboardActionHandler { done() } },
        shape = ControlShape,
    )
}

/** Faixa de aviso no topo do conteúdo (cor `warning`), com "Fechar". */
@Composable
internal fun Banner(text: String, onClose: () -> Unit) {
    Row(
        Modifier
            .fillMaxWidth()
            .padding(Tokens.Space.m)
            .background(Tokens.Colors.surfaceRaised, PanelShape)
            .border(Dp.Hairline, Tokens.Colors.warning, PanelShape)
            .padding(start = Tokens.Space.m),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(Tokens.Space.m),
    ) {
        Text(text, Modifier.weight(1f).padding(vertical = Tokens.Space.m), color = Tokens.Colors.warning)
        QuietButton("Fechar", onClose)
    }
}

/** Fundo de mensagens e painéis sobre o vídeo (`overlay`). */
internal fun Modifier.overlayPanel(maxWidth: Dp): Modifier =
    this
        .widthIn(max = maxWidth)
        .background(Tokens.Colors.overlay, PanelShape)
        .padding(Tokens.Space.l)
