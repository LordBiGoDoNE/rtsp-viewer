# Design

## Contexto

O app já cadastra câmeras por URL, por ONVIF (busca ou só o IP) e por
fabricante, e reproduz RTSP sempre sobre TCP (FFmpeg com `rtsp_transport=tcp`
no desktop; Media3 com `setForceUseRtpTcp(true)` no Android). As câmeras
Yoosee usam caminhos próprios (`/onvif1`, `/onvif2`) e ONVIF na porta 5000.
Câmeras baratas desse tipo às vezes recusam o RTP intercalado no TCP.

O usuário não tem uma Yoosee: tudo é validado com uma câmera falsa que imita o
comportamento esperado. O comportamento real fica como limitação declarada até
o teste com uma câmera dele.

## Objetivos

- **Cadastro:** cadastrar uma Yoosee só com o IP, por ONVIF na porta 5000 ou
  pela escolha de fabricante.
- **Abrir câmeras só UDP:** câmeras que só aceitam RTP por UDP abrem sem
  configuração.
- **Nada piora para as câmeras atuais:**
  - TCP continua primeiro;
  - a reserva só entra depois de uma falha;
  - a latência e o boot ficam iguais para quem funciona por TCP.

## Não objetivos

- Nuvem P2P da Yoosee, PTZ, áudio e preferência manual de transporte.

## Decisões

### D1. Fabricante Yoosee

- **Domínio:** `Vendor::Yoosee` / `Vendor.Yoosee`, rótulo "Yoosee", principal
  `/onvif1` e secundário `/onvif2`, na porta 554 ou na porta digitada (mesma
  regra dos outros fabricantes).
- **Ordem na lista:** a mesma nos dois apps, com Yoosee depois de Axis. A
  lista atual não muda de posição.
- **Dica:** nos textos de ajuda da busca e da escolha de fabricante (desktop e
  Android, `design/telas.md`), "na Yoosee, use o usuário admin e a senha do
  dispositivo definida no app Yoosee".
  - A fonte do texto é comunitária. Ele é conferido quando o usuário tiver uma
    câmera; até lá, consta como não verificado no README.

### D2. Porta ONVIF 5000

- **Portas:** `DEFAULT_PORTS` passa a `[80, 2020, 5000, 8000, 8080, 8899]` no
  desktop e no Android.
- **Sondagem:** continua em paralelo, com o mesmo prazo total de 10 s, então
  uma porta a mais não muda o tempo da consulta.
- **Ordem de preferência:** quando mais de uma porta responde, vale a primeira
  da lista, como hoje.

### D3. Reserva UDP no desktop (FFmpeg)

- **Primeira tentativa:** a sessão abre com `rtsp_transport=tcp`, como hoje.
- **Quando passar para UDP:** se a tentativa TCP falhar **antes do primeiro
  pacote de vídeo**, a mesma sessão reabre uma vez com `rtsp_transport=udp`,
  sem esperar o intervalo de reconexão. A falha pode ser:
  - erro de RTSP no `SETUP`, como 461 "Unsupported Transport" ou outro status
    que não seja 401, 403 ou 404;
  - conexão fechada pela câmera depois do `DESCRIBE`;
  - nenhum pacote em 5 s depois do `PLAY`.
- **Sem reserva:**
  - erros classificados como autenticação, caminho inexistente, sem vídeo ou
    codec não suportado (`classify`);
  - falha de conexão TCP na porta RTSP, porque a câmera está inacessível e o
    UDP não ajudaria; segue a reconexão normal.
- **Opções no UDP:** as mesmas de baixa latência, mais `reorder_queue_size`
  pequeno e `buffer_size` adequado ao 1080p. A faixa de portas locais fica
  com o padrão do FFmpeg.
  - O FFmpeg envia pacotes de "furo" do NAT a partir das portas RTP/RTCP, o
    que abre o estado no firewall com estado do Windows.
  - Na primeira vez, o Windows pode mostrar o aviso do firewall para o
    `rtsp-viewer.exe`. Isso é registrado no README e no ADR 0002, sem regra
    de firewall instalada pelo app.
- **Memória do transporte:**
  - o transporte que deu o primeiro quadro fica lembrado por câmera (pela URL,
    sem credenciais) num mapa em memória do player;
  - a próxima abertura, ou reconexão, começa por ele;
  - se o lembrado falhar antes do primeiro pacote, tenta o outro uma vez;
  - nada vai para disco.
- **Estado na tela:** "Informações" ganha a linha "Transporte: TCP" ou
  "Transporte: UDP".
- **Testes:** contra a câmera falsa só UDP:
  - abre por UDP;
  - a câmera falsa normal continua em TCP;
  - autenticação errada na câmera só UDP mostra "Usuário ou senha inválidos."
    sem tentar UDP em laço.
  - Testes unitários da decisão (`should_try_udp`) cobrem cada erro.

### D4. Reserva UDP no Android (Media3)

- **Primeira tentativa:** igual ao desktop, por TCP
  (`setForceUseRtpTcp(true)`).
- **Quando passar para UDP:** se a tentativa TCP falhar antes do primeiro
  quadro, o `PlayerController` recria a fonte com `setForceUseRtpTcp(false)`,
  uma vez, na mesma sessão. As falhas são:
  - `RtspMediaSource.RtspPlaybackException` ou erro de status RTSP no
    `SETUP`;
  - nenhum dado em 5 s depois do `PLAY`.
  - Sem `force`, o Media3 usa UDP e, se nada chegar, ele mesmo volta ao TCP
    depois do prazo dele; o app não depende disso.
- **Sem reserva:** as mesmas exceções do desktop. A decisão é uma função pura
  (`shouldTryUdp`), com testes JVM.
- **Memória:** a mesma do desktop, em memória, no `AppContainer` durante a
  vida do processo.
- **Prazo do primeiro quadro (15 s):** passa a contar a partir da tentativa
  que conectou. A reserva não encurta nem soma prazos além dos 5 s da espera
  por dados.
- **Rede:**
  - o socket UDP segue a rede ativa, sem `bindSocket` novo; o Media3 abre os
    sockets UDP dele;
  - sem permissão nova: `INTERNET` cobre o UDP.
- **Testes:** instrumentados contra a câmera falsa só UDP (emulador no CI, com
  a câmera em `10.0.2.2`, e celular).
  - Ressalva: o UDP do emulador passa pelo NAT do emulador. Se o RTP por UDP
    não atravessar esse NAT, o teste instrumentado UDP roda só no celular, e o
    CI cobre a decisão no JVM. Isso é decidido no grupo 3 e registrado aqui.

### D5. Câmera falsa "Yoosee"

- **Serviço:** novo `mediamtx-udp` em `test-camera/compose.yaml`, em
  `:8556`, com:
  - `rtspTransports: [udp]` e portas RTP/RTCP próprias (`8010`/`8011`);
  - caminhos `onvif1` (H.264 720p) e `onvif2` (H.264 360p), publicados por
    `ffmpeg` como os atuais;
  - leitura com usuário `admin` e senha fictícia `yoosee123`.
- **Simulador ONVIF:** usa o que já existe, com `--port 5000 --main ...
  --sub ...` apontando para ela. O roteiro traz o comando.
- **CI:** os jobs que sobem a câmera falsa passam a subir também o
  `mediamtx-udp`.

### D6. Responsabilidades

- **`core`:** só o fabricante (dados) e nada de transporte.
- **`media` e `app/player`:** a reserva é detalhe do adaptador de vídeo.
- **Porta `StreamPlayer`:** não muda; o estado visível continua o mesmo, mais
  a linha de informação.
- **Teste de arquitetura:** os existentes continuam valendo.

## Riscos

- **Comportamento real desconhecido:** a Yoosee pode recusar TCP de um jeito
  não coberto, como responder 200 ao `SETUP` TCP e fechar no `PLAY`. Os 5 s
  sem pacotes cobrem o caso de "aceita e não manda". Outros casos ficam para o
  teste com uma câmera real.
- **UDP em Wi-Fi ruim:** perda de pacotes vira artefatos na imagem. O TCP
  continua o padrão para as câmeras que o aceitam.
- **Firewall do Windows:** o aviso pode aparecer na primeira conexão UDP, e
  com o acesso negado o vídeo não chega. Nesse caso, o player mostra
  "Câmera inacessível" depois dos 5 s, e o README explica.
