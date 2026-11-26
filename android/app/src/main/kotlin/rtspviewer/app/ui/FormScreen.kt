package rtspviewer.app.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
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
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.input.KeyboardType
import rtspviewer.app.ui.theme.Tokens
import rtspviewer.core.domain.Vendor

/** Tela 2: formulário, consulta ONVIF e escolha de fabricante (design/telas.md, seção 2). */
@Composable
internal fun FormScreen(form: FormState, vm: AppViewModel) {
    val choosing = form.phase == FormPhase.ChooseVendor
    Column(Modifier.fillMaxSize().safeDrawingPadding()) {
        Toolbar { ToolbarTitle(if (choosing) "Escolha o fabricante" else form.title, Modifier.weight(1f)) }
        Column(
            Modifier
                .fillMaxSize()
                .verticalScroll(rememberScrollState())
                .padding(Tokens.Space.l),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            Column(
                Modifier.widthIn(max = Tokens.Size.formWidth).fillMaxWidth(),
                verticalArrangement = Arrangement.spacedBy(Tokens.Space.m),
            ) {
                if (choosing) VendorChoice(form, vm) else Fields(form, vm)
            }
        }
    }
    if (form.phase == FormPhase.LookingUp) {
        AppDialog(
            onDismiss = vm::cancelLookup,
            dismissOnClickOutside = false,
            text = { Text("Consultando a câmera…") },
            confirm = { QuietButton("Cancelar", vm::cancelLookup) },
        )
    }
}

@Composable
private fun Fields(form: FormState, vm: AppViewModel) {
    val editing = form.editing != null
    FormField(
        label = "Nome",
        value = form.name,
        onChange = vm::setName,
        tag = "campo-nome",
        error = form.nameError,
        help = if (editing) null else "Com só o IP, pode ficar vazio: o nome vem da câmera.",
    )
    FormField(
        label = "Endereço",
        value = form.address,
        onChange = vm::setAddress,
        tag = "campo-endereco",
        error = form.addressError,
        help = "IP da câmera ou URL RTSP completa",
        placeholder = "192.168.0.10 ou rtsp://192.168.0.10:554/stream1",
        keyboardType = KeyboardType.Uri,
    )
    FormField(label = "Usuário (opcional)", value = form.username, onChange = vm::setUsername, tag = "campo-usuario")
    SecretField(
        label = "Senha (opcional)",
        value = form.password,
        onChange = vm::setPassword,
        tag = "campo-senha",
        help = if (editing) "Deixe vazio para manter a senha atual" else null,
        onDone = vm::save,
    )
    if (!form.passwordsPersistent) {
        Text(
            "Este sistema não tem cofre de senhas: a senha será lembrada só até fechar o app.",
            color = Tokens.Colors.warning,
        )
    }
    form.formError?.let { Text(it, color = Tokens.Colors.danger) }
    Row(
        Modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.spacedBy(Tokens.Space.s, Alignment.End),
    ) {
        QuietButton("Cancelar", vm::closeForm)
        PrimaryButton("Salvar", vm::save, enabled = form.phase == FormPhase.Editing)
    }
}

@Composable
private fun VendorChoice(form: FormState, vm: AppViewModel) {
    Text(
        "A câmera não respondeu ao ONVIF. Escolha o fabricante para usar os caminhos RTSP conhecidos.",
        color = Tokens.Colors.text,
    )
    if (form.vendorReason.isNotEmpty()) {
        Text(form.vendorReason, style = MaterialTheme.typography.bodySmall, color = Tokens.Colors.textMuted)
    }
    for (vendor in Vendor.entries) {
        SecondaryButton(vendor.label, { vm.chooseVendor(vendor) }, Modifier.fillMaxWidth())
    }
    Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.End) {
        QuietButton("Cancelar", vm::cancelLookup)
    }
}
