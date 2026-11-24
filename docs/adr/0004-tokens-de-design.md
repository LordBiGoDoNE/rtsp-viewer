# ADR 0004: Tokens de design compartilhados

- Estado: aceito
- Data: 2026-11-24

## Contexto

Desktop (Slint) e Android (Compose) têm UIs separadas (ADR 0001). O usuário
quer as duas com a mesma identidade visual.

## Decisão

- `design/tokens.json` é a única fonte de cores, tipografia, espaçamentos,
  raios e tamanhos de componentes.
- Cada plataforma gera o próprio tema no build, sem arquivo gerado versionado:
  - desktop: `build.rs` do crate `app` gera um `global Theme` em `.slint` no
    `OUT_DIR`;
  - Android (spec `app-android`): task Gradle gera o tema do Compose.
- Token inválido ou ausente faz o build falhar com mensagem clara.
- O CI falha se um `.slint` do app usar cor literal em vez do tema.
- `design/telas.md` descreve as telas, estados e textos de forma independente de
  plataforma; toda mudança de tela altera esse documento e as duas UIs.

## Consequências

- Mesma identidade, não pixels idênticos: texto, rolagem e animações seguem cada
  plataforma.
- Gerar no build elimina o risco de tema desatualizado, ao custo de um passo a
  mais no build.
