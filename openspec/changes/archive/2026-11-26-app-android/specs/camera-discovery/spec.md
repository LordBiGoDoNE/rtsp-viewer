## MODIFIED Requirements

### Requirement: Busca de câmeras na rede

O app, no desktop e no Android, SHALL procurar câmeras ONVIF na rede local por
WS-Discovery somente quando o usuário pedir, por cerca de 3 segundos: no
desktop em todas as interfaces de rede ativas, no Android nas redes Wi-Fi e
Ethernet ativas do aparelho. Nenhuma busca SHALL acontecer no boot.

#### Scenario: Câmeras encontradas

- **WHEN** o usuário clica em "Procurar câmeras" e há câmeras ONVIF na rede local
- **THEN** em até 4 segundos o app lista cada câmera uma única vez, com o nome sugerido e o IP
- **AND** câmeras já cadastradas aparecem marcadas como "Já cadastrada"

#### Scenario: Nenhuma câmera encontrada

- **WHEN** a busca termina sem respostas
- **THEN** o app mostra "Nenhuma câmera encontrada" com dicas (ONVIF ativo na câmera, mesma rede e, no desktop, firewall; no Android, Wi-Fi da mesma rede) e a opção de cadastrar pelo IP

#### Scenario: Boot sem rede

- **WHEN** o app é aberto
- **THEN** nenhuma mensagem de descoberta é enviada até o usuário pedir uma busca

#### Scenario: Resposta inválida ou de outra busca

- **WHEN** chega uma resposta UDP malformada, maior que o limite ou com `RelatesTo` diferente da busca atual
- **THEN** a resposta é descartada sem erro e sem afetar a lista

#### Scenario: Busca no Android sem Wi-Fi nem Ethernet

- **WHEN** o celular está só nos dados móveis e o usuário pede a busca
- **THEN** o app mostra "Nenhuma câmera encontrada" com a dica de conectar ao Wi-Fi da mesma rede, sem enviar nada pela rede móvel
