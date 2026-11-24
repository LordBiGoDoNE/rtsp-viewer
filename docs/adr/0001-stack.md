# ADR 0001: Stack do desktop e do Android

- Estado: aceito
- Data: 2026-11-24
- Aprovação: usuário, 2026-11-24 (planejamento do projeto e spec `mvp-desktop-rtsp`)

## Contexto

O rtsp-viewer é um client RTSP para Windows, Linux e Android, sem backend.
Requisitos permanentes: leve, simples e boot rápido. A regra global de stack
(`arquitetura-backend.md`) define Java como padrão de **backend** e permite
outra tecnologia mais adequada ao produto, com justificativa e aprovação.
Este projeto não tem backend; este ADR registra a justificativa mesmo assim.

## Decisão

Modelo híbrido:

| Plataforma | Stack | Pasta |
| --- | --- | --- |
| Windows e Linux | Rust + Slint (UI) + FFmpeg (RTSP, decodificação, gravação) | `desktop/` |
| Android | Kotlin + Jetpack Compose + Media3/ExoPlayer | `android/` |

A identidade visual é compartilhada por tokens de design (ADR 0004).

## Alternativas consideradas

- **Kotlin/Compose Multiplatform em tudo:** Android excelente, mas desktop sobre
  JVM, com boot de 1 a 3 s e pacote de 100 MB+. Contraria a leveza.
- **Java (JavaFX) no desktop:** mesmo custo da JVM.
- **Rust + Slint em tudo:** UI idêntica e código único, mas Android com
  `NativeActivity`, JNI manual, FFmpeg compilado com NDK e mais consumo de
  bateria (cópia de quadros pela CPU).
- **Flutter + media_kit:** UI única, porém mais pesado no desktop, dependente de
  um projeto comunitário para o player e com gravação a resolver à parte.
- **Qt/QML:** leve e maduro, mas C++ com toolchain e build Android mais caros e
  licença LGPL.
- **Go (Fyne/Gio):** bom em rede, sem player de vídeo pronto e com Android pouco
  maduro.
- **Tauri/Electron:** webview não reproduz RTSP; exigiria conversão por servidor.

## Consequências

- Desktop compila para binário nativo pequeno, sem runtime, com boot rápido.
- Android usa o caminho oficial (MediaCodec direto na `Surface`), com menor
  consumo de bateria e acesso direto às APIs do sistema.
- Custo: duas UIs. Cada tela é feita duas vezes, a partir de `design/telas.md`.
- Lógica de domínio duplicada em Rust e Kotlin. Se a duplicação pesar (ONVIF,
  por exemplo), expor o núcleo Rust ao Android via UniFFI é decisão futura,
  em ADR próprio.
- No desktop, o crate `core` não depende de UI, FFmpeg, cofre, disco nem rede;
  um teste de arquitetura garante isso.
