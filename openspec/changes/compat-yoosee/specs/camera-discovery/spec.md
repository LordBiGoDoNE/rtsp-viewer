## MODIFIED Requirements

### Requirement: Consulta dos streams pela câmera

A partir do endereço da câmera (vindo da busca ou digitado) e das credenciais,
o app SHALL obter pelo ONVIF a URL RTSP do stream principal (perfil de maior
resolução) e, quando existir, a do secundário (perfil de menor resolução),
sem o usuário digitar o caminho RTSP. Sem porta no endereço, o app SHALL
sondar as portas 80, 2020, 5000, 8000, 8080 e 8899.

#### Scenario: Cadastro pela busca

- **WHEN** o usuário escolhe uma câmera encontrada, informa usuário e senha e salva
- **THEN** a câmera é cadastrada com a URL do stream principal obtida da câmera e, se houver, a do secundário

#### Scenario: Só o IP, porta ONVIF desconhecida

- **WHEN** o usuário digita só `192.168.1.164`, informa usuário e senha e salva
- **THEN** o app encontra o serviço ONVIF da câmera nas portas usuais (inclusive 2020, da Tapo) e cadastra os streams obtidos

#### Scenario: Só o IP de uma Yoosee

- **WHEN** o usuário digita só o IP de uma câmera com ONVIF na porta 5000, informa usuário e senha e salva
- **THEN** o app encontra o serviço ONVIF na porta 5000 e cadastra os streams obtidos, no mesmo prazo das outras portas

#### Scenario: Consulta não bloqueia a interface

- **WHEN** a consulta está em andamento
- **THEN** o app mostra "Consultando a câmera…", continua respondendo, e o usuário pode cancelar

#### Scenario: Câmera devolve URL com host inválido

- **WHEN** a URL RTSP devolvida usa `0.0.0.0` ou `localhost` como host
- **THEN** o app usa o host da câmera no lugar

### Requirement: Fabricante quando não há ONVIF

Quando a câmera não responder ao ONVIF, o app SHALL oferecer uma lista de
fabricantes (TP-Link Tapo, Hikvision, Dahua/Intelbras/Amcrest, Reolink,
Uniview, Axis e Yoosee) que completa os caminhos RTSP principal e secundário a
partir do IP, e SHALL continuar aceitando a URL RTSP completa.

#### Scenario: ONVIF desligado

- **WHEN** nenhuma porta ONVIF responde para o IP informado
- **THEN** o app explica que a câmera não respondeu ao ONVIF e mostra a lista de fabricantes
- **AND** ao escolher "Hikvision" a câmera é cadastrada com `rtsp://<IP>:554/Streaming/Channels/101` e secundário `.../102`

#### Scenario: Yoosee pela lista de fabricantes

- **WHEN** o usuário escolhe "Yoosee" na lista de fabricantes
- **THEN** a câmera é cadastrada com `rtsp://<IP>:554/onvif1` e secundário `rtsp://<IP>:554/onvif2`

#### Scenario: URL completa continua valendo

- **WHEN** o usuário digita uma URL RTSP completa
- **THEN** o cadastro funciona como antes, sem consulta ONVIF
