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
  (`ffmpeg-n8.1.3-27-g8dca1586f4-win64-lgpl-shared-8.1.zip`, release
  `autobuild-2026-10-31-12-59`). Vai para `third_party/ffmpeg/` (fora do git).
  Fixa-se sempre o **último autobuild de um mês**: pela política de retenção
  do BtbN (README do repositório, conferido em 2026-11-24), o último build de
  cada mês fica publicado por dois anos, e os builds diários só pelos últimos
  14 dias. Um build diário fixado quebraria o download (404) em cerca de duas
  semanas, mesmo com o hash fixo.
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
- Risco de disponibilidade: o build fixado some dois anos depois do mês de
  publicação (ou antes, se o BtbN mudar a política). O cache do CI
  (`third_party/ffmpeg`, chave pelo hash do script) só adia a falha. Quando o
  download falhar, o script interrompe o build e indica o procedimento:
  escolher o autobuild de fim de mês mais recente, copiar nome e SHA-256 do
  `checksums.sha256` da release para o script e registrar a troca aqui.
  Atualizar periodicamente (pelo menos uma vez por ano) evita a quebra.
- Recomendação não adotada (exige autorização do usuário): espelhar o pacote
  fixado numa release própria do repositório, eliminando a dependência da
  retenção do BtbN.
- O build LGPL do BtbN traz todos os codecs; o tamanho do pacote é medido no
  grupo 5 da spec. Se passar de 30 MB compactado, abre-se issue para um build
  mínimo próprio (spec separada).
- No Linux o app usa o FFmpeg da distribuição; versões diferentes são cobertas
  pela detecção de versão do binding.
