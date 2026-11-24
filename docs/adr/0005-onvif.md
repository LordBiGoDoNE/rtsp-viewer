# ADR 0005: ONVIF no desktop

- Estado: aceito
- Data: 2026-11-24
- Spec: `descoberta-onvif`

## Contexto

O usuário quer cadastrar câmeras só com IP, usuário e senha, e encontrá-las
na rede. A URL RTSP muda por fabricante; o ONVIF (Profile S) permite pedir à
própria câmera as URLs dos streams, e o WS-Discovery encontra as câmeras.
Requisitos permanentes: leve, simples, sem rede no boot.

## Decisão

- Crate próprio `desktop/crates/onvif`, adaptador de rede que depende do
  `core`. O `core` não depende dele.
- Cliente escrito no projeto, só com as operações necessárias:
  `GetSystemDateAndTime`, `GetDeviceInformation`, `GetCapabilities`,
  `GetProfiles` e `GetStreamUri`. Não há geração de código a partir de WSDL:
  crates ONVIF prontas trazem runtime async e centenas de tipos sem uso.
- Bibliotecas (versões fixas no `Cargo.toml`):

  | Crate | Uso |
  | --- | --- |
  | `ureq` 3.5, sem as features padrão | HTTP síncrono, sem TLS nem gzip |
  | `quick-xml` 0.42 | XML |
  | `sha1`, `md-5`, `base64` | autenticação |
  | `getrandom` | nonces |
  | `socket2` e `if-addrs` (grupo 2) | WS-Discovery por interface |

  Todas sob MIT, Apache-2.0 ou BSD.
- Sem TLS: ONVIF em rede local usa HTTP. A senha nunca vai em texto puro: vai
  como WS-UsernameToken PasswordDigest ou HTTP Digest (MD5).
- Entrada de rede é não confiável:
  - respostas HTTP limitadas a 256 KiB e UDP a 64 KiB;
  - XML sem DOCTYPE e com profundidade limitada;
  - URLs RTSP devolvidas validadas pelo `RtspUrl` do `core`, com as
    credenciais embutidas descartadas.

## Consequências

- O app fica sem runtime async e sem pilha TLS; o custo no pacote é medido no
  grupo 5 da spec.
- ONVIF sobre HTTPS não é suportado; se for preciso, entra numa spec própria,
  com a decisão de TLS.
- Câmeras com variações de firmware são tratadas pelo parse por nome local
  (sem prefixos), com amostras de fabricantes nos testes.
- O simulador de testes fica em `crates/onvif/tests/support/simulator.rs`
  (também exposto como `examples/fake_onvif.rs`), fora do binário do app.
