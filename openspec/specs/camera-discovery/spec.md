# camera-discovery Specification

## Purpose

Busca de câmeras na rede local (WS-Discovery por multicast e unicast na sub-rede privada), só quando o usuário pede, e consulta ONVIF que obtém as URLs dos streams principal e secundário a partir do IP, usuário e senha, com escolha de fabricante quando a câmera não responde ao ONVIF. As credenciais vão sempre só para o host da câmera. Criada pela change descoberta-onvif (desktop).

## Requirements

### Requirement: Busca de câmeras na rede

O app desktop SHALL procurar câmeras ONVIF na rede local por WS-Discovery
somente quando o usuário pedir, em todas as interfaces de rede ativas, por
cerca de 3 segundos. Nenhuma busca SHALL acontecer no boot.

#### Scenario: Câmeras encontradas

- **WHEN** o usuário clica em "Procurar câmeras" e há câmeras ONVIF na rede local
- **THEN** em até 4 segundos o app lista cada câmera uma única vez, com o nome sugerido e o IP
- **AND** câmeras já cadastradas aparecem marcadas como "Já cadastrada"

#### Scenario: Nenhuma câmera encontrada

- **WHEN** a busca termina sem respostas
- **THEN** o app mostra "Nenhuma câmera encontrada" com dicas (ONVIF ativo na câmera, mesma rede, firewall) e a opção de cadastrar pelo IP

#### Scenario: Boot sem rede

- **WHEN** o app é aberto
- **THEN** nenhuma mensagem de descoberta é enviada até o usuário pedir uma busca

#### Scenario: Resposta inválida ou de outra busca

- **WHEN** chega uma resposta UDP malformada, maior que o limite ou com `RelatesTo` diferente da busca atual
- **THEN** a resposta é descartada sem erro e sem afetar a lista

### Requirement: Consulta dos streams pela câmera

A partir do endereço da câmera (vindo da busca ou digitado) e das credenciais,
o app SHALL obter pelo ONVIF a URL RTSP do stream principal (perfil de maior
resolução) e, quando existir, a do secundário (perfil de menor resolução),
sem o usuário digitar o caminho RTSP.

#### Scenario: Cadastro pela busca

- **WHEN** o usuário escolhe uma câmera encontrada, informa usuário e senha e salva
- **THEN** a câmera é cadastrada com a URL do stream principal obtida da câmera e, se houver, a do secundário

#### Scenario: Só o IP, porta ONVIF desconhecida

- **WHEN** o usuário digita só `192.168.1.164`, informa usuário e senha e salva
- **THEN** o app encontra o serviço ONVIF da câmera nas portas usuais (inclusive 2020, da Tapo) e cadastra os streams obtidos

#### Scenario: Consulta não bloqueia a interface

- **WHEN** a consulta está em andamento
- **THEN** o app mostra "Consultando a câmera…", continua respondendo, e o usuário pode cancelar

#### Scenario: Câmera devolve URL com host inválido

- **WHEN** a URL RTSP devolvida usa `0.0.0.0` ou `localhost` como host
- **THEN** o app usa o host da câmera no lugar

### Requirement: Autenticação ONVIF sem senha em texto puro

O app SHALL autenticar as consultas ONVIF com WS-UsernameToken PasswordDigest,
usando o relógio da câmera, e SHALL usar HTTP Digest quando a câmera o
exigir. A senha SHALL NOT ser enviada em texto puro nem aparecer em logs ou
mensagens de erro.

#### Scenario: Relógio da câmera errado

- **WHEN** o relógio da câmera difere do relógio do computador em horas
- **THEN** a autenticação funciona, porque o horário enviado segue o relógio da câmera

#### Scenario: Câmera exige HTTP Digest

- **WHEN** a câmera responde 401 pedindo HTTP Digest
- **THEN** o app repete a chamada uma vez com HTTP Digest e a consulta funciona

#### Scenario: Usuário ou senha errados

- **WHEN** a câmera recusa as credenciais
- **THEN** o app mostra "Usuário ou senha inválidos." e não salva a câmera

### Requirement: Fabricante quando não há ONVIF

Quando a câmera não responder ao ONVIF, o app SHALL oferecer uma lista de
fabricantes (TP-Link Tapo, Hikvision, Dahua/Intelbras/Amcrest, Reolink,
Uniview e Axis) que completa os caminhos RTSP principal e secundário a partir
do IP, e SHALL continuar aceitando a URL RTSP completa.

#### Scenario: ONVIF desligado

- **WHEN** nenhuma porta ONVIF responde para o IP informado
- **THEN** o app explica que a câmera não respondeu ao ONVIF e mostra a lista de fabricantes
- **AND** ao escolher "Hikvision" a câmera é cadastrada com `rtsp://<IP>:554/Streaming/Channels/101` e secundário `.../102`

#### Scenario: URL completa continua valendo

- **WHEN** o usuário digita uma URL RTSP completa
- **THEN** o cadastro funciona como antes, sem consulta ONVIF
