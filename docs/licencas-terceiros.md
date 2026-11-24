# Licenças de terceiros

## FFmpeg

O rtsp-viewer usa as bibliotecas do FFmpeg (libavcodec, libavformat,
libavutil, libswscale e libswresample) sob a **LGPL 2.1 ou posterior**, por
ligação dinâmica e sem modificações (ADR 0002).

- Windows: as DLLs do pacote vêm de um build LGPL do
  [BtbN/FFmpeg-Builds](https://github.com/BtbN/FFmpeg-Builds) (versão fixada em
  `scripts/fetch-ffmpeg.ps1`) e podem ser substituídas por outro build
  compatível. O texto da licença acompanha o pacote (`FFmpeg-LICENSE.txt`).
- Linux: o app usa as bibliotecas do FFmpeg da distribuição.
- Código-fonte do FFmpeg: <https://ffmpeg.org/download.html>.

## Slint

A interface usa o [Slint](https://slint.dev) sob a **Slint Royalty-free
License 2.0** (ADR 0003). A atribuição exigida aparece na tela "Sobre" do app.

## Crates Rust

As demais dependências Rust usam licenças permissivas (MIT, Apache-2.0 ou
equivalentes). A lista completa sai de `cargo metadata` no diretório
`desktop/`.
