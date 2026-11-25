# Design

## Contexto

A `mvp-desktop-rtsp` entregou cadastro por URL RTSP completa e visualização.
Esta spec acrescenta a descoberta e a consulta ONVIF (Profile S), só no
desktop. Requisitos permanentes: leve, simples e boot rápido; nada de rede no
boot. Versões conferidas em 2026-11-24 (fixadas no grupo 1):

| Crate | Versão |
| --- | --- |
| `ureq` | 3.5 |
| `quick-xml` | 0.42 |
| `sha1` | 0.11 |
| `md-5` | 0.11 |
| `base64` | 0.23 |
| `socket2` | 0.6 |
| `if-addrs` | 0.15 |

## Objetivos

- Cadastrar uma câmera ONVIF com IP, usuário e senha, sem conhecer o caminho
  RTSP.
- Encontrar as câmeras ONVIF da rede local em cerca de 3 s, por botão.
- Quando o ONVIF falhar, completar o caminho pela lista de fabricantes.
- Nenhuma senha em texto puro na rede (sem `PasswordText`), em logs ou no
  `cameras.json`.

## Não objetivos

Ver "Out of scope" da proposta.

## Estrutura

```
desktop/crates/onvif/      adaptador de rede (novo)
  src/soap.rs              envelope SOAP, WS-Security, parse com limites
  src/auth.rs              PasswordDigest, compensação de relógio, HTTP Digest
  src/client.rs            OnvifClient: device, media, perfis, stream URI
  src/discovery.rs         WS-Discovery por interface de rede
  examples/fake_onvif.rs   simulador ONVIF (descoberta + consultas)
desktop/crates/core/       portas, fabricantes e caso de uso (sem rede)
desktop/crates/app/        tela de busca, formulário, wiring
```

Direção de dependências: `app` → `onvif` → `core`. O teste de arquitetura passa
a proibir também `rtsp-viewer-onvif`, `ureq`, `quick-xml`, `socket2` e
`if-addrs` no `core`.

## Decisões

### D1. Cliente HTTP/SOAP síncrono, sem TLS

`ureq` sem as features padrão (sem `rustls` e sem `gzip`): sem runtime async,
sem TLS (ONVIF em rede local usa HTTP) e com chunked e timeouts prontos. Cada
chamada tem timeout de conexão de 2 s, e a consulta inteira (todas as chamadas
de `locate` ou de `fetch_streams`) tem prazo total de 10 s: cada chamada recebe
o que resta do prazo. O total, antes 5 s por chamada, passou a valer para a
consulta (até seis chamadas) e subiu para 10 s para acomodar câmeras lentas
(revisão do grupo 1). Respostas acima de 256 KiB são recusadas antes do parse.

### D2. Autenticação

- **WS-UsernameToken PasswordDigest:**
  `Base64(SHA1(nonce + created + senha))`, com nonce aleatório de 16 bytes.
  `PasswordText` nunca é usado.
- **Relógio:** antes da primeira chamada autenticada, `GetSystemDateAndTime`
  (sem autenticação) mede a diferença para o relógio da câmera, e o `Created`
  usa o relógio dela. Câmeras com hora errada, comuns sem NTP, deixam de
  recusar a autenticação.
- **HTTP Digest (MD5, `qop=auth`):** se a câmera responder 401 com
  `WWW-Authenticate: Digest`, a chamada é repetida uma vez com HTTP Digest, sem
  cabeçalho WS-Security. Algumas Hikvision e Dahua exigem isso.
- Credenciais vêm de `Password`, nunca entram em `Debug`, logs nem mensagens de
  erro, e as mensagens de erro passam por `redact_credentials`.

### D3. Consulta dos streams

1. Endereço do dispositivo:
   - vindo da busca: `XAddrs` do ProbeMatch;
   - digitado: `http://<host>:<porta>/onvif/device_service`. Sem porta, o app
     tenta 80, 2020 (Tapo), 8000, 8080 e 8899 em paralelo, com timeout de
     conexão de 1 s, e fica com a primeira que responder a
     `GetSystemDateAndTime`.
2. `GetDeviceInformation` (fabricante e modelo, para sugerir o nome) e
   `GetCapabilities`, para achar o serviço de mídia. Do `XAddr` de mídia valem
   só a porta e o caminho; o host é sempre o da câmera (ADR 0005).
3. `GetProfiles` e `GetStreamUri` (RTP-Unicast sobre RTSP) para os perfis de
   vídeo.
4. Principal = perfil de maior resolução. Secundário = o de menor resolução,
   se for diferente. Empate na resolução: vale a ordem do dispositivo.
5. URLs devolvidas com credenciais embutidas passam por `RtspUrl::parse`. As
   credenciais são descartadas, porque as do cadastro valem. Como essas
   credenciais vão junto ao abrir o stream, da URL valem só esquema, porta e
   caminho/consulta; o host é sempre o da câmera (ADR 0005), o que também
   resolve firmwares que anunciam `0.0.0.0`, loopback ou o IP interno atrás
   de NAT.

### D4. WS-Discovery

- Probe SOAP-over-UDP para `239.255.255.250:3702`, tipo
  `dn:NetworkVideoTransmitter`, enviado por cada interface IPv4 ativa e não
  loopback (`if-addrs` + `socket2` com `IP_MULTICAST_IF`), com TTL 1. Escuta por
  3 s.
- O mesmo Probe (mesmo `MessageID` e socket) também vai por unicast para
  `<host>:3702` de cada host da sub-rede de cada interface, porque há câmeras
  que ignoram o multicast (ex.: TP-Link Tapo C320WS, que só responde ao
  unicast) e redes ou Wi-Fi que não o repassam. Regras: só interfaces com
  endereço privado (RFC 1918; nada de link-local, loopback ou público); a
  sub-rede real com prefixo /24 ou mais longo, senão só o /24 do endereço;
  sem rede, broadcast nem endereços da própria máquina; no máximo 254 por
  interface e 1024 no total. O envio corre numa thread por interface, em
  lotes de 32 com 2 ms de pausa, depois de a escuta começar e só até o fim
  dos 3 s; o buffer de envio vai a 256 KiB porque datagramas à espera de ARP
  o ocupam (no Windows, com o padrão de 64 KiB, o envio travava ~2,5 s).
- Respostas aceitas só se forem ProbeMatch com o `RelatesTo` igual ao
  `MessageID` do probe, até 64 KiB, e de endereço privado ou link-local. O
  resultado é deduplicado pelo `EndpointReference`, com no máximo 256
  dispositivos; a fila de recepção guarda no máximo 64 respostas e descarta o
  excedente (ADR 0005).
- Do ProbeMatch saem os `XAddrs` (só `http` com host IP literal privado,
  link-local ou loopback e sem `user@`, preferindo o IP de onde veio a
  resposta; sem nenhum aceito, o ProbeMatch é descartado) e o nome sugerido, a
  partir dos scopes `onvif://www.onvif.org/name/` e `hardware/`.
- Câmeras já cadastradas, pelo mesmo host, aparecem como "Já cadastrada".
- A porta da busca é testável: os destinos do probe (multicast e unicast) são
  injetáveis, e o simulador responde por unicast em loopback nos testes.

### D5. Fabricantes

Lista estática no `core` (dado de domínio, sem I/O), com caminho principal e
secundário:

| Fabricante | Principal | Secundário |
| --- | --- | --- |
| TP-Link Tapo | `/stream1` | `/stream2` |
| Hikvision | `/Streaming/Channels/101` | `/Streaming/Channels/102` |
| Dahua / Intelbras / Amcrest | `/cam/realmonitor?channel=1&subtype=0` | `...subtype=1` |
| Reolink | `/h264Preview_01_main` | `/h264Preview_01_sub` |
| Uniview | `/media/video1` | `/media/video2` |
| Axis | `/axis-media/media.amp` | (nenhum) |

Porta RTSP padrão: 554. A lista é usada só quando o ONVIF falha, ou se o
usuário escolher um fabricante em vez de consultar.

### D6. Modelo e persistência

- `Camera` ganha `substream: Option<RtspUrl>`, alterável por método de
  intenção. As credenciais são as mesmas do principal, então o secundário
  pertence ao mesmo dispositivo: a entidade recusa um secundário com host
  diferente do principal (erro de campo no formulário). Trocar a URL principal
  descarta o secundário, a menos que um novo seja informado na mesma edição;
  ele foi derivado da URL antiga e não há como saber se continua válido.
- Na leitura do `cameras.json`, um secundário com host diferente do principal
  (arquivo editado à mão) é descartado, sem marcar o arquivo como corrompido.
- `cameras.json` ganha o campo opcional `substream_url`. A `version` continua
  1, porque o campo é opcional: arquivos antigos carregam, e versões antigas
  ignoram o campo.
- O formulário continua aceitando URL RTSP completa. Endereço só com host (IP,
  nome ou `IP:porta`, sem esquema) dispara a consulta ao salvar.

### D7. UI e concorrência

- Busca e consulta rodam numa thread própria. A UI recebe o resultado por
  `upgrade_in_event_loop`, com o mesmo padrão de sessão do player: um
  resultado de uma busca cancelada ou antiga é ignorado.
- Estados: "Procurando câmeras…", lista de encontradas, "Nenhuma câmera
  encontrada" (com dicas), "Consultando a câmera…" e erro com a lista de
  fabricantes.
- Telas descritas em `design/telas.md` (seções novas), com tokens de
  `design/tokens.json`. Tokens novos, se houver, seguem o ADR 0004.

### D8. Testes

- `onvif`:
  - unitários de XML e autenticação com respostas de exemplo (Profile S
    genérico, Tapo, Hikvision e Dahua, reconstruídas a partir da norma, sem
    dados reais);
  - testes de integração contra o simulador em processo, verificando a
    validade do PasswordDigest, a compensação de relógio, o HTTP Digest e a
    busca por unicast em loopback.
- `core`: caso de uso com portas falsas (ONVIF ok, ONVIF falha com fallback,
  autenticação recusada, sem perfis).
- `app`: testes de UI com o backend de testes do Slint (busca, "Já cadastrada",
  consulta, fallback).
- Ponta a ponta: simulador ONVIF no host, apontando para a câmera falsa.
  WS-Discovery em multicast real, ou seja, fora do Docker. O usuário testa com
  a Tapo dele.

## Riscos e mitigação

| Risco | Mitigação |
| --- | --- |
| Firewall do Windows bloqueia respostas da busca | Mensagem de "nenhuma câmera" com dica do firewall; o cadastro por IP continua funcionando |
| Câmera com hora errada recusa o PasswordDigest | Compensação pelo `GetSystemDateAndTime` (D2) |
| Variações de firmware (namespaces, prefixos) | Parse por nome local, sem prefixo; amostras de vários fabricantes nos testes |
| Resposta maliciosa na rede local | Limites de tamanho, validação de `RelatesTo`, URLs passam por `RtspUrl::parse` |
| Tapo exige "Conta da câmera" e ONVIF ativo | Roteiro e dica na tela de erro |

## Questões abertas

Nenhuma bloqueante.
