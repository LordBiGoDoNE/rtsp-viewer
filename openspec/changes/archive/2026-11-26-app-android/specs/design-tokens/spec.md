## MODIFIED Requirements

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
