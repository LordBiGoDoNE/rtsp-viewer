## ADDED Requirements

### Requirement: Tokens de design como fonte única

`design/tokens.json` SHALL ser a única fonte de cores, tipografia,
espaçamentos, raios e tamanhos de componentes da interface. O tema do Slint
SHALL ser gerado a partir dele durante o build, sem arquivo gerado versionado.

#### Scenario: Alterar uma cor

- **WHEN** um valor de cor muda em `design/tokens.json`
- **THEN** o próximo build do app usa a nova cor sem nenhuma outra edição

#### Scenario: Token inválido

- **WHEN** `design/tokens.json` tem um valor malformado ou falta um token obrigatório
- **THEN** o build falha com mensagem que indica o token

#### Scenario: Cor fixa no código da UI

- **WHEN** um arquivo `.slint` do app usa uma cor literal em vez do tema
- **THEN** a verificação do CI falha

### Requirement: Descrição única das telas

`design/telas.md` SHALL descrever cada tela do app (estrutura, estados e
textos) de forma independente de plataforma, servindo ao desktop e ao Android.

#### Scenario: Telas do MVP documentadas

- **WHEN** a spec é concluída
- **THEN** `design/telas.md` descreve lista de câmeras (inclusive vazia), formulário e visualização com todos os estados
