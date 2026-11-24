# ADR 0002: FFmpeg no desktop

- Estado: aceito
- Data: 2026-11-24

## Contexto

O desktop precisa abrir RTSP, decodificar H.264/H.265 (com hardware quando
houver) e, em specs futuras, gravar por remux. O FFmpeg cobre tudo isso. Falta
decidir o binding Rust, de onde vêm as bibliotecas e a licença.

## Decisão

### Binding: `ffmpeg-next` 9.1

Critério (design D3 da spec `mvp-desktop-rtsp`): suportar as versões de FFmpeg
dos dois sistemas e ter manutenção ativa. Situação em 2026-11-24:

| Crate | Versão | FFmpeg suportado | Último commit |
| --- | --- | --- | --- |
| `ffmpeg-next` | 9.1.0 | 3.4 em diante (detecção no build) | 2026-11-16 |
| `ffmpeg-the-third` | 6.0.0+ffmpeg-9.0 | 5.1 a 9.0 | 2026-08-09 |

Os dois atendem às versões necessárias (6.1 no Ubuntu 24.04, 8.1 no Windows).
`ffmpeg-next` foi escolhido pela manutenção mais recente e pela base de uso
maior. Ele se declara "em modo de manutenção", o que basta para o uso aqui
(API estável de demux, decode e swscale). Se deixar de acompanhar o FFmpeg,
a troca pelo fork tem API base equivalente.

Features usadas: `codec`, `format`, `software-scaling` (sem `device`, `filter`
nem `software-resampling`), para ligar só as bibliotecas necessárias.

### Origem das bibliotecas

- **Windows:** build LGPL compartilhado do BtbN/FFmpeg-Builds, versão e
  SHA-256 fixados em `scripts/fetch-ffmpeg.ps1`
  (`ffmpeg-n8.1.3-39-g660b5aa7db-win64-lgpl-shared-8.1.zip`, release
  `autobuild-2026-11-23-12-43`). Vai para `third_party/ffmpeg/` (fora do git).
  O pacote leva só as DLLs usadas.
- **Linux:** pacotes do sistema (`libavcodec-dev`, `libavformat-dev`,
  `libavutil-dev`, `libswscale-dev`). O `.deb` declara as dependências de
  runtime.
- **Bindings:** gerados no build pelo `bindgen`, que exige `libclang` (LLVM no
  Windows, `libclang-dev` no Linux).

### Licença

Somente builds **LGPL** (sem `--enable-gpl` nem `--enable-nonfree`), com
**ligação dinâmica**. O app não modifica o FFmpeg. O pacote inclui o aviso de
licença do FFmpeg.

## Consequências

- Atualizar o FFmpeg do Windows exige alterar versão e hash no script e neste
  ADR.
- O build LGPL do BtbN traz todos os codecs; o tamanho do pacote é medido no
  grupo 5 da spec. Se passar de 30 MB compactado, abre-se issue para um build
  mínimo próprio (spec separada).
- No Linux o app usa o FFmpeg da distribuição; versões diferentes são cobertas
  pela detecção de versão do binding.
