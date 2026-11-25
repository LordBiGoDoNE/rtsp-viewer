# Roteiro de teste: app-android

Teste do app Android no seu celular, com a câmera falsa e o simulador ONVIF
rodando no PC e, por fim, com a sua câmera real. Cada item traz os passos e o
resultado esperado. Anote qualquer diferença: ela vira achado e é corrigida
antes da aprovação.

## Preparação

1. O celular e o PC na **mesma rede Wi-Fi/LAN**. Descubra o IP do PC na rede
   (`ipconfig`, linha "Endereço IPv4" do adaptador da sua rede); abaixo ele
   aparece como `<IP-do-PC>`.
2. Câmera falsa no PC:
   ```powershell
   docker compose -f test-camera/compose.yaml up -d
   ```
3. Simulador ONVIF no PC, anunciando o IP da rede (usuário `admin`, senha
   `admin123`, fictícios):
   ```powershell
   cd desktop
   cargo run -p rtsp-viewer-onvif --example fake_onvif -- --port 8099 --advertise <IP-do-PC>
   ```
4. App instalado no celular, de uma destas formas:
   - **APK assinado com a sua chave** (o de uso diário): uma vez
     `./scripts/android-keystore.ps1`; depois `./scripts/android-release.ps1` e
     `adb install -r android/app/build/outputs/apk/release/app-release.apk`.
   - **Build de teste** (`rtsp-viewer` com id `.e2e`, assinado com a chave de
     debug do SDK), se o agente já o deixou instalado. Os dois podem coexistir.
5. Dados de teste (fictícios):

   | Nome | Endereço | Usuário | Senha |
   | --- | --- | --- | --- |
   | Falsa H.264 | `rtsp://<IP-do-PC>:8554/h264` | | |
   | Falsa H.265 | `rtsp://<IP-do-PC>:8554/h265` | | |
   | Portão | `rtsp://<IP-do-PC>:8554/auth` | `teste` | `teste123` |
   | Simulador ONVIF | `<IP-do-PC>:8099` (só o endereço) | `admin` | `admin123` |

## 1. Abertura e lista vazia

- Abra o app. **Esperado:** abre rápido (menos de 1 s), barra "Câmeras" com
  "Adicionar câmera" e o menu "⋮"; "Nenhuma câmera cadastrada." com os botões
  "Adicionar câmera" e "Procurar câmeras". Nada de rede até você pedir.

## 2. Cadastro por URL

1. "Adicionar câmera" → "Falsa H.264" com a URL → "Salvar". **Esperado:** a
   câmera aparece na lista com o endereço sem credenciais.
2. Tente salvar com nome vazio e endereço `http://x`. **Esperado:** "Informe um
   nome." e "O endereço deve começar com rtsp:// ou rtsps://." abaixo dos
   campos.
3. Cadastre "Portão" com a URL `rtsp://teste:teste123@<IP-do-PC>:8554/auth`.
   **Esperado:** salva com usuário `teste` e a URL sem a senha.
4. Menu "⋮" da linha → "Editar": mude o nome e salve; "Remover": confirmação
   "Remover <nome>?" → "Remover". **Esperado:** some da lista.

## 3. Visualização

1. Toque em "Falsa H.264". **Esperado:** "Conectando…" e o vídeo em até 2 s,
   com o relógio na imagem; proporção preservada.
2. "Informações". **Esperado:** codec `h264`, resolução 1280×720,
   decodificação `hardware (…)` (ou `software (…)` se o aparelho não tiver), e o
   endereço sem credenciais.
3. Latência: compare o relógio da imagem (UTC) com um relógio do PC.
   **Esperado:** até 1 s de diferença.
4. Duplo toque no vídeo. **Esperado:** tela cheia sem barras do sistema; gire o
   celular, o vídeo acompanha. Gesto de voltar sai da tela cheia; outro volta à
   lista.
5. Abra "Falsa H.265". **Esperado:** vídeo com codec `hevc`.
6. Abra "Portão" com a senha errada (edite a senha para `x`). **Esperado:**
   "Usuário ou senha inválidos." e "Tentar de novo", sem reconectar sozinho.
7. Durante a reprodução, pare a câmera falsa
   (`docker compose -f test-camera/compose.yaml stop pub-h264`). **Esperado:**
   "Reconectando (tentativa N)…"; ao religar (`start pub-h264`), o vídeo volta
   sozinho.
8. Com o vídeo aberto, vá para a tela inicial do Android e volte. **Esperado:**
   o vídeo reconecta sozinho. Com o vídeo em reprodução, a tela não apaga.

## 4. Busca e cadastro só com o endereço (ONVIF)

1. Menu "⋮" → "Procurar câmeras". **Esperado:** "Procurando câmeras na rede…" e,
   em até 4 s, "Simulador_ONVIF" com o IP do PC e o botão "Adicionar".
2. "Adicionar" → usuário `admin`, senha `admin123` → "Salvar". **Esperado:**
   "Consultando a câmera…" e a câmera cadastrada com a URL principal obtida
   pelo ONVIF; abrir mostra o vídeo da câmera falsa.
3. Procure de novo. **Esperado:** o simulador aparece como "Já cadastrada".
4. "Adicionar câmera" com só `<IP-do-PC>:8099`, usuário `admin` e senha errada.
   **Esperado:** "Usuário ou senha inválidos." e nada salvo.
5. "Adicionar câmera" com só `<IP-do-PC>` (porta sem ONVIF, por exemplo pare o
   simulador). **Esperado:** "Escolha o fabricante" com o motivo; "Hikvision"
   cadastra `rtsp://<IP-do-PC>:554/Streaming/Channels/101`.
6. Desligue o Wi-Fi do celular (só dados móveis) e procure. **Esperado:**
   "Nenhuma câmera encontrada." com a dica do Wi-Fi da mesma rede.

## 5. Câmera real (Tapo)

1. Na sua Tapo, crie a "Conta da câmera" no app Tapo (configurações
   avançadas) e deixe o ONVIF ativo.
2. "Procurar câmeras". **Esperado:** a Tapo aparece (o app envia o pedido de
   busca também direto a cada IP da sub-rede).
3. "Adicionar" com o usuário e a senha da "Conta da câmera". **Esperado:** a
   câmera é cadastrada sem digitar URL e o vídeo abre.

## 6. Senhas e dados

- Feche o app pela lista de recentes e abra de novo. **Esperado:** as câmeras
  continuam; as senhas também (as câmeras abrem sem pedir senha).
- As senhas ficam só cifradas pelo Keystore do Android; o backup do app está
  desligado.

## Limpeza

- `docker compose -f test-camera/compose.yaml down` e Ctrl+C no simulador.
- Para remover o build de teste: `adb uninstall io.github.lordbigodone.rtspviewer.e2e`.
