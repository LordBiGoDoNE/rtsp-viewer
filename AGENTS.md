# Regras do projeto rtsp-viewer

As **regras globais** do usuário valem primeiro: `~/.claude/rules/*.md` e os agentes em `~/.claude/agents/`.

Este arquivo só **detalha** o que é próprio do projeto e registra as **exceções** aprovadas. Conflito sem exceção registrada abaixo: vale a regra global (`precedencia-de-regras.md`).

## Contexto

- Client leve para câmeras RTSP no Windows, Linux e Android. Leveza, simplicidade e boot rápido são requisitos permanentes.
- Stack, estrutura e sequência de specs: `openspec/config.yaml`. O projeto não tem backend; a regra global de backend em Java não se aplica. A escolha de Rust (desktop) e Kotlin (Android) foi aprovada pelo usuário em 2026-11-24 (client sem backend, com requisito de leveza e boot rápido); o ADR com a justificativa é criado em `docs/adr/` pela spec `mvp-desktop-rtsp`.

## Arquitetura (detalhamento de `arquitetura-backend.md`, no que se aplica a um client)

- Desktop: o crate `core` (domínio e aplicação, com as portas) não depende de UI, FFmpeg, rede nem sistema de arquivos. `media`, `onvif` e `app` são adaptadores e dependem do `core`, nunca o contrário. A direção é garantida pelas dependências do Cargo.
- Android: mesma separação por pacote (`domain`, `application`, `infrastructure`, `ui`).
- Senhas de câmeras ficam atrás da porta `CredentialStore` (cofre do sistema no desktop, Keystore no Android); nunca em texto puro no arquivo de configuração, em logs nem em mensagens de erro.
- Identidade visual vem de `design/tokens.json`; os temas gerados para Slint e Compose não são editados à mão.

## Teste local (detalhamento de `teste-local.md`)

- Câmera falsa para testes e para o teste de ponta a ponta: servidor RTSP local (MediaMTX em Docker) recebendo um padrão de teste publicado pelo `ffmpeg`. Nunca versionar endereço, usuário ou senha de câmeras reais.

## Documentos de estado

- Estado entregue: `README.md` e `openspec/config.yaml`.
- Registro de entregas: `docs/entregas.md`
- Branches permanentes: nenhuma além de `main`.

## Exceções às regras globais

Nenhuma.
