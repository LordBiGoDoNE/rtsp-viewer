# camera-registry Specification

## Purpose

Cadastro das câmeras do usuário: nome, URL RTSP validada e credenciais. A lista é persistida localmente sem senhas, e as senhas ficam somente no cofre do sistema. Criada pela change mvp-desktop-rtsp (desktop).

## Requirements

### Requirement: Cadastro de câmeras

O app desktop SHALL permitir adicionar, editar e remover câmeras. Cada câmera
tem nome (1 a 60 caracteres, não composto só de espaços), URL RTSP, usuário
opcional e senha opcional.

#### Scenario: Adicionar câmera válida

- **WHEN** o usuário informa nome "Garagem" e URL `rtsp://192.168.0.10:554/stream1` e salva
- **THEN** a câmera aparece na lista com o nome "Garagem"
- **AND** continua na lista depois de fechar e reabrir o app

#### Scenario: Editar câmera

- **WHEN** o usuário altera o nome ou a URL de uma câmera existente e salva
- **THEN** a lista mostra os novos dados e a mesma câmera mantém sua identidade

#### Scenario: Remover câmera exige confirmação

- **WHEN** o usuário pede para remover uma câmera
- **THEN** o app pede confirmação
- **AND** só depois da confirmação remove a câmera da lista e a senha dela do cofre

#### Scenario: Nome inválido

- **WHEN** o usuário tenta salvar com nome vazio, só com espaços ou com mais de 60 caracteres
- **THEN** o app não salva e mostra o motivo junto ao campo

### Requirement: Validação da URL RTSP

O núcleo SHALL aceitar somente URLs com esquema `rtsp` ou `rtsps` e host não
vazio, com porta e caminho opcionais. Credenciais embutidas na URL SHALL ser
extraídas: o usuário vai para o campo de usuário, a senha vai para o cofre e a
URL salva fica sem credenciais.

#### Scenario: Esquema inválido

- **WHEN** o usuário informa `http://192.168.0.10/stream`
- **THEN** o app recusa a URL e informa que o endereço deve começar com `rtsp://` ou `rtsps://`

#### Scenario: Credenciais embutidas na URL

- **WHEN** o usuário informa `rtsp://admin:segredo@192.168.0.10/stream1`
- **THEN** a URL salva é `rtsp://192.168.0.10/stream1`
- **AND** o usuário salvo é `admin`
- **AND** a senha `segredo` vai para o cofre e não aparece no arquivo de configuração

### Requirement: Senhas protegidas

Senhas de câmeras SHALL ficar somente no cofre do sistema (Windows Credential
Manager, Secret Service no Linux). Elas SHALL NOT aparecer no arquivo de
configuração, em logs nem em mensagens de erro. Sem cofre disponível, o app
SHALL manter a senha só em memória durante a sessão e avisar o usuário.

#### Scenario: Arquivo de configuração sem segredos

- **WHEN** uma câmera com senha é salva
- **THEN** o `cameras.json` não contém a senha em nenhuma forma

#### Scenario: Erro de conexão não vaza senha

- **WHEN** a conexão de uma câmera com senha falha
- **THEN** a mensagem exibida e o log mostram a URL sem credenciais

#### Scenario: Linux sem cofre

- **WHEN** o Secret Service não está disponível e o usuário salva uma câmera com senha
- **THEN** o app avisa que a senha não será lembrada
- **AND** pede a senha ao abrir a câmera, guardando-a só até o app fechar

### Requirement: Persistência confiável da lista

A lista de câmeras SHALL ser salva em `cameras.json` no diretório de
configuração do usuário, com campo de versão e escrita atômica. Um arquivo
corrompido SHALL NOT impedir o app de abrir nem ser apagado.

#### Scenario: Arquivo corrompido

- **WHEN** o `cameras.json` não pode ser lido
- **THEN** o app abre com a lista vazia e avisa o usuário
- **AND** o arquivo original é preservado com o sufixo `.corrompido-<data e hora>`

#### Scenario: Falha durante a gravação

- **WHEN** a gravação é interrompida antes de terminar
- **THEN** o `cameras.json` anterior continua íntegro

### Requirement: Cadastro só com o endereço da câmera

O formulário de câmera SHALL aceitar, no campo de endereço, uma URL RTSP
completa ou só o endereço da câmera (IP, nome de host ou `host:porta`, sem
esquema). Só o endereço SHALL disparar a consulta dos streams (capability
`camera-discovery`) ao salvar. O que é gravado continua sendo uma URL RTSP sem
credenciais.

#### Scenario: Endereço só com IP

- **WHEN** o usuário informa `192.168.1.164` no endereço e salva
- **THEN** o app consulta a câmera e grava a URL RTSP obtida, sem credenciais

#### Scenario: Endereço inválido

- **WHEN** o usuário informa um endereço que não é URL RTSP nem host válido (por exemplo `http://x` ou `192.168.1.164:abc`)
- **THEN** o app não salva e explica o formato aceito junto ao campo

### Requirement: Stream secundário opcional

Uma câmera SHALL poder ter um stream secundário opcional, com as mesmas
credenciais do principal. Ele é gravado no `cameras.json` como campo opcional,
sem senha, e um arquivo sem esse campo SHALL continuar carregando. A
visualização continua usando o stream principal.

#### Scenario: Secundário obtido pelo ONVIF

- **WHEN** a câmera tem dois perfis de vídeo com resoluções diferentes
- **THEN** o de menor resolução é gravado como stream secundário

#### Scenario: Arquivo da versão anterior

- **WHEN** o app abre um `cameras.json` gravado antes desta mudança
- **THEN** todas as câmeras carregam, sem stream secundário e sem aviso de arquivo corrompido
