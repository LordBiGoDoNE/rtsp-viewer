# Roteiro de teste: mvp-desktop-rtsp

Teste do app desktop no Windows com a câmera falsa (e, se quiser, com uma
câmera real). Cada item traz os passos e o resultado esperado. Anote qualquer
diferença: ela vira achado e é corrigida antes da aprovação.

## Preparação

1. Câmera falsa no ar:
   ```powershell
   docker compose -f test-camera/compose.yaml up -d
   ```
2. Abrir o app do pacote: extraia `rtsp-viewer-0.1.0-windows-x64.zip` e execute
   `rtsp-viewer.exe`.
   - Para não mexer na sua lista real, rode antes, no mesmo terminal,
     `$env:RTSP_VIEWER_CONFIG_DIR = "$env:TEMP\rtsp-viewer-teste"` e inicie o app
     por esse terminal (`.\rtsp-viewer.exe`).
3. Dados de teste (fictícios):

   | Nome | Endereço | Usuário | Senha |
   | --- | --- | --- | --- |
   | Falsa H.264 | `rtsp://localhost:8554/h264` | | |
   | Falsa H.265 | `rtsp://localhost:8554/h265` | | |
   | Portão | `rtsp://localhost:8554/auth` | `teste` | `teste123` |
   | Queda | `rtsp://localhost:8554/queda` | | |

## 1. Boot e lista vazia

- Abra o app. **Esperado:** janela em menos de meio segundo, título "Câmeras",
  "Nenhuma câmera cadastrada." e o botão "Adicionar câmera". Nenhuma conexão é
  aberta.

## 2. Cadastro

1. "Adicionar câmera" → preencha "Falsa H.264" → "Salvar". **Esperado:** a
   câmera aparece na lista com o endereço.
2. Tente salvar com nome vazio e endereço `http://x`. **Esperado:** o formulário
   continua aberto, com "Informe um nome." e "O endereço deve começar com
   rtsp:// ou rtsps://." abaixo dos campos.
3. Cadastre "Portão" digitando o endereço
   `rtsp://teste:teste123@localhost:8554/auth`, sem preencher usuário e senha.
   **Esperado:** a lista mostra `rtsp://localhost:8554/auth`, sem a senha. Em
   "Editar", o campo Usuário mostra `teste` e a senha fica vazia, com "Deixe
   vazio para manter a senha atual".
4. Feche e reabra o app. **Esperado:** as câmeras continuam lá.
5. Opcional: abra `cameras.json` (no diretório da configuração) e confirme que
   ele não tem nenhuma senha.

## 3. Visualização

1. Clique em "Falsa H.264". **Esperado:** "Conectando…" e, em seguida, o vídeo
   com o relógio, em cerca de 1 a 2 segundos.
2. "Informações". **Esperado:** codec h264, resolução 1280x720, decodificação
   "hardware (d3d11va)" (ou "software" se não houver aceleração) e o endereço.
3. Latência: o relógio no vídeo está em **UTC**. Compare com a hora UTC do
   sistema (`[DateTime]::UtcNow` no PowerShell). **Esperado:** diferença de até
   1 segundo.
4. Repita com "Falsa H.265". **Esperado:** codec hevc, vídeo normal.

## 4. Tela cheia e teclado

1. Na visualização: F11. **Esperado:** tela cheia, sem a barra superior.
2. Esc. **Esperado:** sai da tela cheia. Esc de novo: volta para a lista.
3. Duplo clique no vídeo alterna a tela cheia.
4. Na lista: setas para cima e para baixo mudam a seleção; Enter abre a câmera.

## 5. Senha

1. Cadastre "Portão 2" com endereço `rtsp://localhost:8554/auth`, usuário
   `teste` e senha vazia. Abra a câmera. **Esperado:** diálogo "Senha de
   teste".
2. Digite uma senha errada → "Conectar". **Esperado:** "Usuário ou senha
   inválidos." e o botão "Tentar de novo", sem reconectar sozinho.
3. Edite "Portão 2", informe a senha `teste123`, salve e abra. **Esperado:**
   vídeo.
4. Abra "Portão" (do passo 2.3). **Esperado:** vídeo direto, sem pedir senha
   (ela veio da URL e foi para o cofre).

## 6. Queda e retorno

1. Cadastre e abra "Queda".
2. Num terminal: `docker compose -f test-camera/compose.yaml stop pub-queda`.
   **Esperado:** em alguns segundos, "Reconectando (tentativa N)…".
3. `docker compose -f test-camera/compose.yaml start pub-queda`. **Esperado:** o
   vídeo volta sozinho.
4. Durante "Reconectando", clique em "Voltar". **Esperado:** volta à lista na
   hora.

## 7. Remoção e "Sobre"

1. "Remover" numa câmera. **Esperado:** diálogo "Remover <nome>?" com
   "Cancelar" e "Remover". Cancelar não remove; Remover remove.
2. "Sobre". **Esperado:** nome, versão, a atribuição do Slint e "Voltar".

## 8. Câmera real (opcional)

- Cadastre uma câmera sua com o endereço RTSP dela. Anote o modelo, o codec e se
  o vídeo abriu, a latência percebida e se a decodificação foi por hardware.
  Não envie o endereço nem a senha em issues ou PRs.

## 9. Câmeras na rede (ONVIF, spec descoberta-onvif)

Preparação: em outro terminal, suba o simulador ONVIF, que aponta para a
câmera falsa e responde à busca na rede (usuário `admin`, senha `admin123`,
fictícios):

```powershell
cd desktop; cargo run -p rtsp-viewer-onvif --example fake_onvif -- --port 8099
```

1. Na lista, "Procurar câmeras". **Esperado:** "Procurando câmeras na rede…" e,
   em cerca de 3 segundos, "Simulador ONVIF" com o IP e o botão "Adicionar". Se
   o Windows perguntar sobre o firewall, permita na rede privada.
2. "Adicionar" → usuário `admin`, senha `admin123` → "Salvar". **Esperado:** a
   câmera é cadastrada sem você digitar nenhuma URL; abrir mostra o vídeo da
   câmera falsa. Em "Editar", o endereço mostra a URL RTSP obtida.
3. "Procurar câmeras" de novo. **Esperado:** o simulador aparece como "Já
   cadastrada".
4. "Adicionar câmera" → endereço só com o IP do simulador e a porta
   (`<ip>:8099`), usuário `admin`, senha errada → "Salvar". **Esperado:**
   "Consultando a câmera…" e depois "Usuário ou senha inválidos."; nada é
   salvo.
5. Endereço `127.0.0.1:9` (nada responde) → "Salvar". **Esperado:** depois da
   consulta, a tela "Escolha o fabricante" com Tapo, Hikvision,
   Dahua/Intelbras/Amcrest, Reolink, Uniview e Axis. Escolher "Hikvision" salva
   com `rtsp://127.0.0.1:9/Streaming/Channels/101`. (Não vai abrir vídeo: é só
   para conferir a URL montada.)
6. Durante "Consultando a câmera…", "Cancelar" ou Esc. **Esperado:** volta ao
   formulário, sem salvar.

**Com a sua Tapo (recomendado):**

1. No app Tapo: câmera → configurações → configurações avançadas → "Conta da
   câmera". Crie usuário e senha (são os mesmos do RTSP). Confira que o
   ONVIF está ativo, se o modelo tiver essa opção.
2. "Procurar câmeras". **Esperado:** a Tapo aparece (nome do modelo ou "Câmera
   ONVIF") com o IP dela.
3. "Adicionar" com a conta da câmera. **Esperado:** cadastro sem digitar URL,
   principal `/stream1` e secundário `/stream2`, e o vídeo abre.
4. Alternativa: "Adicionar câmera" digitando só o IP (por exemplo
   `192.168.1.120`). **Esperado:** o app encontra o ONVIF na porta 2020 e
   cadastra igual.
5. Anote: se a busca achou a câmera, o tempo de cada etapa e qualquer mensagem
   inesperada. Não envie IP, usuário ou senha em issues ou PRs.

## Depois do teste

- Para limpar: apague a pasta indicada em `RTSP_VIEWER_CONFIG_DIR` e, no
  Gerenciador de Credenciais do Windows, as entradas terminadas em
  `.rtsp-viewer`.
- `docker compose -f test-camera/compose.yaml down` para a câmera falsa.
