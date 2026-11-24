# Tasks

Issue: [#1](https://github.com/LordBiGoDoNE/rtsp-viewer/issues/1).
Épica: `epic/mvp-desktop-rtsp`. Spec formalizada em 2026-11-24.
Implementação autorizada em 2026-11-24 (grupos 1–5). Teste local pelo usuário:
sim. Licença do Slint: royalty-free (decisão do usuário de 2026-11-24). Merges
dependem da aprovação do pacote final.

Cada grupo é um PR em rascunho. Os grupos formam uma cadeia (cada um parte da
branch do anterior e aponta para ela), porque cada um usa o código do anterior.
Em cada grupo: testes do projeto, CI local via `act` e revisão por grupo
(`group-reviewer`) antes de abrir o PR. Áreas críticas: segredos (grupo 2) e
CI (grupo 1).

## 1. Fundação (`task/mvp-desktop-rtsp/fundacao`, base: épica)

- [x] 1.1 Workspace em `desktop/` com os crates `core`, `media` e `app` vazios,
  `rust-toolchain.toml` com versão fixa e perfil release (LTO, `panic = "abort"`,
  `strip`, `codegen-units = 1`). `.gitattributes` com finais de linha.
- [x] 1.2 Escolher o binding do FFmpeg (`ffmpeg-next` ou `ffmpeg-the-third`) pelo
  critério de D3 e fixar as versões de Slint e do binding.
- [x] 1.3 `scripts/fetch-ffmpeg.ps1`: baixa o build LGPL compartilhado do FFmpeg
  com versão e SHA-256 fixos para `third_party/ffmpeg/` e falha se o hash não
  bater.
- [x] 1.4 `test-camera/compose.yaml` com MediaMTX e publicadores `ffmpeg`
  (H.264, H.265 e caminho com autenticação fictícia), relógio na imagem, e
  instruções em `test-camera/README.md`.
- [x] 1.5 Workflow `.github/workflows/ci.yml` com `desktop-linux`,
  `desktop-windows` e `openspec` (D8), sem publicação; script da checagem
  "change entregue fora de `archive/`" e "linha no registro de entregas".
- [x] 1.6 ADRs em `docs/adr/`: 0001 stack (Rust/Slint no desktop, Kotlin no
  Android, motivo e aprovação do usuário em 2026-11-24), 0002 FFmpeg (binding,
  obtenção, LGPL, ligação dinâmica), 0003 licença do Slint (D7), 0004 tokens de
  design compartilhados.
- [x] 1.7 Rodar `cargo build`, `cargo clippy`, `act` (`desktop-linux` e
  `openspec`) e registrar que `desktop-windows` não roda no `act`.

## 2. Núcleo e cadastro (`task/mvp-desktop-rtsp/nucleo`, base: grupo 1)

- [ ] 2.1 `core`: `CameraId`, `CameraName`, `RtspUrl` (validação e extração de
  credenciais), `Camera` com métodos de intenção (renomear, trocar URL, trocar
  usuário) e erros de domínio tipados.
- [ ] 2.2 `core`: portas `CameraRepository`, `CredentialStore` e `StreamPlayer`
  (com os estados e eventos do player) e os casos de uso adicionar, editar,
  remover (inclusive a senha) e listar.
- [ ] 2.3 Testes do `core` sem framework: validação de nome e URL, extração de
  credenciais, casos de uso com portas em memória.
- [ ] 2.4 `app/infrastructure`: repositório JSON (`directories`, campo
  `version`, escrita atômica, preservação de arquivo corrompido) com testes em
  diretório temporário.
- [ ] 2.5 `app/infrastructure`: `CredentialStore` com `keyring`, fallback em
  memória sem cofre, e redação de credenciais em logs e mensagens; testes da
  redação e do fallback.
- [ ] 2.6 Teste de arquitetura: `core` sem dependências proibidas
  (`cargo metadata`).

## 3. Player FFmpeg (`task/mvp-desktop-rtsp/player`, base: grupo 2)

- [ ] 3.1 `media`: `FfmpegPlayer` implementando `StreamPlayer` numa thread
  própria, opções de baixa latência (D2), conversão para RGBA e entrega do
  último quadro.
- [ ] 3.2 Decodificação por hardware (D3D11VA/VAAPI) com fallback para
  software e indicação do modo em uso.
- [ ] 3.3 Estados, classificação de erros (autenticação, inacessível, codec não
  suportado) e reconexão com backoff (D2).
- [ ] 3.4 Encerramento em até 1 s com `interrupt_callback` e timeout.
- [ ] 3.5 Testes de integração contra a câmera falsa: H.264, H.265,
  autenticação recusada, queda e retorno da câmera, encerramento durante
  conexão pendente.

## 4. Interface (`task/mvp-desktop-rtsp/interface`, base: grupo 3)

- [ ] 4.1 `design/tokens.json`, `design/telas.md` e `build.rs` gerando o
  `global Theme` do Slint, com erro claro para token inválido; verificação no CI
  de cor literal em `.slint`.
- [ ] 4.2 Tela de lista (com estado vazio) e formulário de câmera com
  validação por campo e confirmação de remoção.
- [ ] 4.3 Tela de visualização com estados, informações da câmera, tela cheia
  (F11, duplo clique, Esc) e pedido de senha quando não há cofre.
- [ ] 4.4 Wiring no `main`: janela mostrada antes de qualquer acesso ao cofre,
  nenhuma conexão automática, medição de boot opcional
  (`RTSP_VIEWER_BOOT_TRACE=1`).
- [ ] 4.5 Tela "Sobre" com o componente `AboutSlint` (D7).
- [ ] 4.6 Testes da lógica de apresentação com o backend de testes do Slint.

## 5. Fechamento (`task/mvp-desktop-rtsp/fechamento`, base: grupo 4)

- [ ] 5.1 Empacotamento: `.zip` Windows com só as DLLs usadas e `.deb` com
  `cargo deb`, como artefatos do CI.
- [ ] 5.2 Medir boot (mediana de 10 execuções com 20 câmeras), tamanho dos
  pacotes e latência com a câmera falsa; registrar no README. Pacote Windows
  acima de 30 MB: abrir issue para build mínimo do FFmpeg (D4).
- [ ] 5.3 README com instalação, uso, requisitos e câmera falsa; roteiro de
  teste em `test-camera/roteiro.md`.
- [ ] 5.4 Teste de ponta a ponta do sistema local com a cadeia completa
  (`teste-local.md`).
