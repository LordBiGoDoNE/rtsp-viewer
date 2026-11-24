# Design

## Contexto

Projeto novo, sem código. Requisitos permanentes: leve, simples e boot rápido.
Stack decidida pelo usuário em 2026-11-24 (ver `openspec/config.yaml`). Versões
conferidas em 2026-11-24: Slint na série 1.17; `ffmpeg-next` 9.x em modo de
manutenção; o fork `ffmpeg-the-third` 5.x acompanha o FFmpeg 8.1. As versões
exatas são fixadas no grupo 1 com base no estado do dia.

## Objetivos

- Janela utilizável em menos de 500 ms, sem esperar rede.
- Primeira imagem de uma câmera local em menos de 2 s e latência de até 1 s.
- Uma câmera 1080p H.264/H.265 com CPU baixa, usando hardware quando houver.
- Núcleo testável sem UI nem FFmpeg, reaproveitável pelas specs seguintes.

## Não objetivos

Ver "Out of scope" da proposta.

## Estrutura

```
desktop/
  Cargo.toml            workspace, perfil release
  rust-toolchain.toml   versão fixa do Rust
  crates/core/          domínio e aplicação, portas (sem dependências de I/O)
  crates/media/         adaptador FFmpeg (StreamPlayer)
  crates/app/           binário: UI Slint, wiring, adaptadores de disco e cofre
design/
  tokens.json           cores, tipografia, espaçamentos, raios, tamanhos
  telas.md              telas, estados e textos (vale para desktop e Android)
docs/adr/               decisões de arquitetura
scripts/                fetch-ffmpeg.ps1, câmera falsa, checagem openspec
test-camera/            docker compose com MediaMTX + publicador ffmpeg
```

Direção de dependências: `app` → `media` → `core`; `app` → `core`. O `core` só
depende de bibliotecas puras (por exemplo `thiserror` e `uuid`); os DTOs de
persistência com `serde` ficam no `app`. Um teste de arquitetura em `app` lê
`cargo metadata` e falha se `core` depender de `slint`, `ffmpeg*`, `keyring`,
`directories` ou de crates de rede.

## Decisões

### D1. Thread por stream, sem runtime async

O player roda numa thread própria: abre a conexão, lê pacotes, decodifica e
publica o quadro mais recente. A UI recebe quadros por
`slint::invoke_from_event_loop` com política "último quadro vence": se a UI
estiver ocupada, quadros antigos são descartados. Estados (conectando,
reproduzindo, reconectando, erro) chegam pelo mesmo caminho. Motivo: menos
dependências, boot menor e comportamento previsível. Parar o player sinaliza a
thread por uma flag atômica e por `interrupt_callback` do FFmpeg, para não
travar em leitura de rede.

### D2. Abertura de baixa latência

Opções do FFmpeg: `rtsp_transport=tcp`, `fflags=nobuffer`, `flags=low_delay`,
`probesize` e `analyzeduration` reduzidos, `timeout` de socket de 5 s. Sem fila
de quadros além do último. Reconexão com backoff de 1, 2, 4, 8, 16 e 30 s
(teto), reiniciada após 10 s de reprodução estável.

### D3. Decodificação e binding do FFmpeg

- Hardware primeiro: D3D11VA no Windows e VAAPI no Linux, via `hw_device_ctx`.
  Quadros são copiados para a CPU (`av_hwframe_transfer_data`) e convertidos
  para RGBA com `swscale`. Sem hardware ou com falha na abertura, cai para
  software sem intervenção do usuário; o modo em uso aparece nas informações
  da câmera.
- Exibição zero-copy (textura da GPU direto no Slint) fica fora desta spec.
- Binding: o grupo 1 escolhe entre `ffmpeg-next` e `ffmpeg-the-third` pelo
  critério "suporta a versão de FFmpeg usada nos dois sistemas e tem manutenção
  ativa", registrado em ADR. Os dois expõem a mesma API base.

### D4. Obtenção do FFmpeg e meta de tamanho

- Windows: build LGPL compartilhado do FFmpeg, versão e SHA-256 fixados em
  `scripts/fetch-ffmpeg.ps1`, baixado para `third_party/ffmpeg/` (ignorado no
  git). O pacote leva somente as DLLs usadas (avcodec, avformat, avutil,
  swscale, swresample).
- Linux: bibliotecas do sistema (`libavcodec-dev` etc. no Ubuntu 24.04); o
  `.deb` declara as dependências.
- Meta: pacote Windows compactado com até 30 MB. Se a medição do grupo 5
  passar da meta, abre-se uma issue para um build mínimo próprio do FFmpeg,
  que vira spec separada; a meta não bloqueia esta spec, mas o número real fica
  registrado no README e no `review.md`.
- Ligação dinâmica do FFmpeg, mantendo a conformidade com a LGPL.

### D5. Câmeras, credenciais e persistência

- `RtspUrl` aceita `rtsp://` e `rtsps://`, exige host, aceita porta e caminho.
  Credenciais embutidas na URL (`rtsp://user:senha@host`) são extraídas no
  cadastro: o usuário vai para o campo próprio, a senha para o cofre, e a URL
  salva fica sem credenciais.
- `Camera`: `CameraId` (UUID), nome (1 a 60 caracteres, sem só espaços),
  `RtspUrl`, usuário opcional. Nomes repetidos são permitidos.
- Persistência: `cameras.json` no diretório de configuração do usuário
  (crate `directories`), com campo `version` para migrações futuras. Escrita
  atômica (arquivo temporário + rename). Arquivo corrompido: a app abre com
  lista vazia, preserva o arquivo como `cameras.json.corrompido-<timestamp>` e
  avisa o usuário.
- Senhas: crate `keyring` (Windows Credential Manager; Secret Service no Linux),
  chave `rtsp-viewer/<CameraId>`. Sem cofre disponível no Linux, a senha não é
  salva: a app avisa e pede a senha ao abrir a câmera, mantendo-a só em memória
  durante a sessão.
- A URL com senha só existe em memória, no momento de abrir o stream. Logs e
  mensagens usam a URL sem credenciais.

### D6. UI e tokens de design

- Telas: lista de câmeras (com estado vazio), formulário (adicionar/editar),
  visualização (com tela cheia). Descritas em `design/telas.md`.
- `design/tokens.json` é lido por `build.rs` do crate `app`, que gera um
  `global Theme` em `.slint` no `OUT_DIR`. Nada gerado é versionado, então não
  há como ficar desatualizado. A spec `app-android` fará o mesmo com uma task
  Gradle.
- Tema escuro único nesta spec (câmeras são vistas melhor em fundo escuro);
  tema claro fica para depois.
- Atalhos: Enter abre a câmera selecionada, F11 ou duplo clique alterna tela
  cheia, Esc sai da tela cheia e depois volta à lista.
- Boot: a janela é mostrada antes de ler o cofre; a lista vem do JSON (rápido)
  e nenhuma conexão é aberta automaticamente.

### D7. Licença do Slint

Licença royalty-free do Slint, que permite app desktop e mobile, inclusive
fechada, com atribuição: a tela "Sobre" mostra o componente `AboutSlint`.
Confirmada pelo usuário em 2026-11-24 e registrada em ADR. Mudar para GPLv3 ou
comercial exige decisão do usuário.

### D8. CI e câmera falsa

- `test-camera/compose.yaml`: MediaMTX + um contêiner `ffmpeg` publicando
  `testsrc2` com relógio (`drawtext`) em H.264 e H.265, em `rtsp://localhost:8554/h264`
  e `/h265`, mais um caminho com autenticação usando credenciais fictícias.
- Workflow `ci.yml`:
  - `desktop-linux` (ubuntu-24.04): `cargo fmt --check`, `cargo clippy -D warnings`,
    `cargo test`, testes de integração do `media` contra a câmera falsa,
    `cargo deb`.
  - `desktop-windows` (windows-latest): FFmpeg via script, `cargo build --release`,
    `cargo test` (sem câmera falsa).
  - `openspec`: `openspec validate --all`, change entregue fora de `archive/`
    e linha no registro de entregas (`pendencias-e-encerramento.md`).
- CI não publica nada. O `.zip` e o `.deb` saem como artefatos do workflow.
- `act` roda `desktop-linux` e `openspec`; `desktop-windows` não roda no `act`
  e fica registrado como não verificado localmente.

## Riscos e mitigação

| Risco | Mitigação |
| --- | --- |
| DLLs do FFmpeg deixam o pacote acima de 30 MB | Medir no grupo 5; issue para build mínimo |
| Câmeras com RTSP fora do padrão | Mensagens de erro específicas; teste com câmera real do usuário no teste local |
| VAAPI ausente ou instável no Linux | Fallback automático para software |
| Cópia GPU→CPU custa CPU em 4K | Aceito no MVP; zero-copy avaliado na spec de grade |
| Leitura de rede bloqueando ao fechar | `interrupt_callback` + timeout |

## Questões abertas

Nenhuma bloqueante. A escolha do binding (D3) é feita no grupo 1 por critério
objetivo.
