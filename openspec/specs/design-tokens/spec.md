# design-tokens Specification

## Purpose

Identidade visual única do app: design/tokens.json é a fonte de cores, tipografia, espaçamentos e tamanhos, e design/telas.md descreve as telas de forma independente de plataforma, para desktop e Android. Criada pela change mvp-desktop-rtsp.

## Requirements

### Requirement: Tokens de design como fonte única

`design/tokens.json` SHALL ser a única fonte de cores, tipografia,
espaçamentos, raios e tamanhos de componentes da interface. O tema do Slint e o
tema do Compose SHALL ser gerados a partir dele durante o build, sem arquivo
gerado versionado.

#### Scenario: Alterar uma cor

- **WHEN** um valor de cor muda em `design/tokens.json`
- **THEN** o próximo build do desktop e do Android usa a nova cor sem nenhuma outra edição

#### Scenario: Token inválido

- **WHEN** `design/tokens.json` tem um valor malformado ou falta um token obrigatório
- **THEN** o build falha com mensagem que indica o token

#### Scenario: Cor fixa no código da UI

- **WHEN** um arquivo `.slint` do app usa uma cor literal em vez do tema
- **THEN** a verificação do CI falha

#### Scenario: Cor fixa no código Compose

- **WHEN** o código Kotlin da UI do Android usa uma cor literal em vez do tema gerado
- **THEN** a verificação do CI falha

### Requirement: Descrição única das telas

`design/telas.md` SHALL descrever cada tela do app (estrutura, estados e
textos) de forma independente de plataforma, servindo ao desktop e ao Android.

#### Scenario: Telas do MVP documentadas

- **WHEN** a spec é concluída
- **THEN** `design/telas.md` descreve lista de câmeras (inclusive vazia), formulário e visualização com todos os estados
