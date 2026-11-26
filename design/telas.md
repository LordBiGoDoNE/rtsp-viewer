# Telas do rtsp-viewer

Descrição independente de plataforma, válida para o desktop (Slint) e o Android
(Compose); as diferenças do Android estão na seção 6. Cores, fontes, espaçamentos e tamanhos (inclusive da janela e dos
painéis) vêm de `design/tokens.json` (ADR 0004); a UI não usa medidas literais.
Toda mudança de tela altera este documento e as duas UIs.

**Janela** (desktop): tamanho inicial `size.window-width` × `size.window-height`;
mínimo `size.window-min-width` × `size.window-min-height`.

Tema escuro único nesta versão. Textos em pt-BR, exatamente como abaixo.

## 1. Lista de câmeras (tela inicial)

**Barra superior** (`size.toolbar-height`, fundo `surface`): título "Câmeras" à
esquerda; à direita, os botões discretos "Sobre" e "Procurar câmeras" (tela 5)
e o botão primário "Adicionar câmera".

**Conteúdo:** uma linha por câmera (`size.list-item-height`), na ordem de
cadastro:

- nome (`font.size-body`, negrito, `text`);
- endereço sem credenciais (`font.size-small`, `text-muted`);
- à direita, ações "Editar" e "Remover" (discretas).

Clique (ou Enter) na linha abre a câmera (tela 3). A linha sob o cursor usa
`surface-hover`.

**Estado vazio:** centralizado, "Nenhuma câmera cadastrada." e, abaixo, o botão
primário "Adicionar câmera" e o botão "Procurar câmeras".

**Avisos** (faixa no topo do conteúdo, cor `warning`, com "Fechar"):

- arquivo corrompido: "A lista de câmeras não pôde ser lida e foi guardada em
  <caminho>. Começando com a lista vazia.";
- erro ao salvar ou no cofre: a mensagem do erro.

**Confirmação de remoção** (diálogo, `size.dialog-width`): "Remover
<nome>?" / "A câmera e a senha salva serão apagadas." / botões "Cancelar" e
"Remover" (`danger`).

## 2. Formulário de câmera (adicionar / editar)

Título "Nova câmera" ou "Editar câmera". Campos (largura `size.form-width`):

| Campo | Rótulo | Observação |
| --- | --- | --- |
| Nome | "Nome" | até 60 caracteres; obrigatório com URL RTSP completa. Câmera nova: texto de apoio "Com só o IP, pode ficar vazio: o nome vem da câmera." |
| Endereço | "Endereço" | IP, nome de host, `host:porta` ou URL RTSP completa; exemplo no campo vazio: `192.168.0.10 ou rtsp://192.168.0.10:554/stream1`; texto de apoio "IP da câmera ou URL RTSP completa" |
| Usuário | "Usuário (opcional)" | |
| Senha | "Senha (opcional)" | oculta; ao editar, vazio mantém a senha atual e o texto de apoio diz "Deixe vazio para manter a senha atual" |

Se a URL tiver `usuário:senha@`, as credenciais vão para os campos próprios ao
salvar (a URL fica limpa).

Erros aparecem abaixo do campo, em `danger`, com o texto vindo da validação
(por exemplo "Informe um nome.", "O endereço deve começar com rtsp:// ou
rtsps://.", "Use um IP, um nome de host ou uma URL rtsp:// completa.",
"Porta inválida: use um número entre 1 e 65535.").

**Aviso sem cofre** (quando o sistema não tem cofre de senhas): "Este sistema
não tem cofre de senhas: a senha será lembrada só até fechar o app."

Botões: "Cancelar" e "Salvar" (primário). Enter salva; Esc cancela.

### 2.1 Cadastro só com o endereço (consulta ONVIF)

Com URL RTSP completa, "Salvar" grava na hora, sem consulta. Com só o
endereço (IP, nome ou `host:porta`, sem esquema), "Salvar" consulta a câmera
pelo ONVIF em segundo plano:

- **Consultando** (diálogo, `size.dialog-width`): "Consultando a câmera…" e o
  botão "Cancelar". A janela continua respondendo. "Cancelar" ou Esc volta ao
  formulário com os campos como estavam; o resultado que chegar depois é
  ignorado.
- **Câmera respondeu:** grava a URL principal (perfil de maior resolução) e, se
  houver, a secundária, sem credenciais, e volta à lista. Nome vazio recebe o
  nome informado pela câmera (fabricante e modelo) ou, sem ele, "Câmera
  <endereço>".
- **Usuário ou senha recusados:** o formulário mostra "Usuário ou senha
  inválidos." (em `danger`, acima dos botões) e nada é gravado.
- **Câmera não respondeu ao ONVIF:** o formulário dá lugar à escolha de
  fabricante (abaixo).
- **Edição sem senha digitada:** a consulta precisa da senha; com usuário
  preenchido e senha vazia, o formulário mostra "Informe a senha para consultar
  a câmera." e não consulta.

A senha digitada é apagada do campo quando a consulta termina (com qualquer
resultado); ao voltar ao formulário depois de uma recusa ou da escolha de
fabricante, ela é digitada de novo.

**Escolha de fabricante** (no lugar do formulário, mesma largura):

- título "Escolha o fabricante";
- texto "A câmera não respondeu ao ONVIF. Escolha o fabricante para usar os
  caminhos RTSP conhecidos.";
- quando o motivo não é a própria falta de ONVIF, linha `font.size-small`,
  `text-muted`: "Motivo: <mensagem>" (por exemplo "Motivo: A câmera não
  respondeu: tempo esgotado");
- um botão por fabricante, nesta ordem: "TP-Link Tapo", "Hikvision", "Dahua /
  Intelbras / Amcrest", "Reolink", "Uniview", "Axis";
- botão "Cancelar", que volta ao formulário (Esc faz o mesmo).

Escolher um fabricante grava a câmera com os caminhos dele na porta 554 (ou na
porta digitada no endereço), por exemplo Hikvision:
`rtsp://<IP>:554/Streaming/Channels/101` e secundário
`rtsp://<IP>:554/Streaming/Channels/102`, e volta à lista.

## 3. Visualização

Fundo `video-background`. O vídeo ocupa a área toda, mantendo a proporção.

**Barra superior** (escondida em tela cheia): "Voltar", nome da câmera, e à
direita "Informações" e "Tela cheia".

**Estados** (mensagem centralizada sobre o vídeo, fundo `overlay`, largura até
`size.status-width`):

| Estado | Texto |
| --- | --- |
| Conectando | "Conectando…" |
| Reproduzindo | (sem mensagem) |
| Reconectando | "Reconectando (tentativa N)…" |
| Erro | mensagem do erro, por exemplo "Usuário ou senha inválidos." ou "Formato de vídeo não suportado: mjpeg", e o botão "Tentar de novo" |

**Informações** (painel no canto, largura `size.info-width`, alterna com o
botão): codec, resolução,
decodificação ("hardware (d3d11va)" ou "software") e endereço sem credenciais.

**Senha pedida** (câmera com usuário e sem senha disponível): diálogo "Senha
de <usuário>", campo oculto, botões "Cancelar" e "Conectar".

**Atalhos:** F11 ou duplo clique alternam tela cheia; Esc sai da tela cheia e,
fora dela, volta à lista.

## 4. Sobre

Nome "rtsp-viewer", versão, "Client leve para câmeras RTSP." e o componente de
atribuição do Slint (obrigatório, ADR 0003; largura até `size.about-width`).
Botão "Voltar".

## 5. Procurar câmeras

Aberta pelo botão "Procurar câmeras" da lista. A busca (WS-Discovery, cerca de
3 s) roda só ao entrar nesta tela ou em "Procurar de novo"; nunca no boot.

**Barra superior** (`size.toolbar-height`, fundo `surface`): "Voltar"
(discreto), título "Procurar câmeras" e, à direita, "Procurar de novo"
(desabilitado enquanto procura). "Voltar" e Esc voltam à lista; o resultado de
uma busca abandonada é ignorado.

**Estados:**

| Estado | Conteúdo |
| --- | --- |
| Procurando | centralizado, "Procurando câmeras na rede…" (`text-muted`) |
| Encontradas | uma linha por câmera (`size.list-item-height`): nome sugerido pela câmera ou "Câmera ONVIF" (negrito) e o IP (`font.size-small`, `text-muted`); à direita, "Já cadastrada" (`text-muted`) quando já existe câmera cadastrada com o mesmo host na URL principal, senão o botão primário "Adicionar" |
| Nenhuma encontrada | centralizado, "Nenhuma câmera encontrada." (`font.size-title`) e as dicas abaixo, em `text-muted`; botão "Cadastrar pelo IP" |
| Erro | centralizado, "Não foi possível procurar câmeras: <mensagem>" em `danger`; botão "Cadastrar pelo IP" |

Dicas de "Nenhuma câmera encontrada", uma por linha:

- "Confira se:"
- "• o ONVIF está ativo na câmera (na Tapo, crie a \"Conta da câmera\" no app
  Tapo);"
- "• o computador e a câmera estão na mesma rede;"
- "• o Firewall do Windows permite o rtsp-viewer na rede privada."

"Cadastrar pelo IP" abre o formulário vazio (tela 2). "Adicionar" abre o
formulário "Nova câmera" com o nome sugerido (se houver) e o IP no endereço; o
app guarda internamente o endereço do serviço ONVIF anunciado pela câmera (não
exibido) e o usa na consulta ao salvar, se o endereço não for alterado. O
usuário informa usuário e senha e salva (seção 2.1).

## 6. Adaptações do Android

As telas 1 a 5 valem para o Android (Compose), com estas diferenças (design D5
da spec `app-android`). Tudo o que não aparece aqui segue as seções acima.

- **Sem teclado nem cursor:** não há atalhos de teclado (F11, Enter, Esc) nem
  estado sob o cursor (`surface-hover`). O botão "Voltar" das telas e o gesto
  (ou botão) de voltar do sistema fazem o papel do Esc: fecham o diálogo
  aberto, cancelam a consulta ou a escolha de fabricante, saem da tela cheia e,
  por fim, voltam à lista. Na lista, voltar fecha o app como em qualquer app
  Android.
- **Barra da lista:** o título "Câmeras" e o botão primário "Adicionar câmera";
  "Procurar câmeras" e "Sobre" ficam no menu "⋮" (descrição de acessibilidade
  "Mais opções") à direita, porque os quatro não cabem na largura de um
  celular. O estado vazio continua com "Adicionar câmera" e "Procurar câmeras".
- **Linha da lista:** um toque abre a câmera. "Editar" e "Remover" ficam num
  menu da própria linha, aberto pelo botão "⋮" à direita (descrição de
  acessibilidade "Ações de <nome>").
- **Larguras:** formulário, diálogos, avisos e painéis usam os tamanhos dos
  tokens como largura máxima e ocupam a largura da tela quando ela é menor.
- **Formulário:** a senha é um campo de senha do teclado do Android; a ação
  "Concluir" do teclado no último campo equivale ao Enter (salva).
- **Visualização:** "Tela cheia" ou duplo toque no vídeo alternam a tela cheia
  imersiva: barras do sistema escondidas (um deslize da borda as mostra por
  instantes) e a barra superior do app escondida. Em tela cheia a rotação segue
  o sensor do aparelho (retrato ou paisagem); ao sair, volta ao comportamento
  normal do aparelho. A tela fica ligada enquanto o vídeo está em reprodução.
  Ao levar o app para segundo plano, a conexão é encerrada; ao voltar, a
  câmera conecta de novo sem ação do usuário. O diálogo "Senha de <usuário>"
  tem o campo oculto "Senha".
- **Procurar câmeras:** a busca usa só as redes Wi-Fi e Ethernet do aparelho.
  Sem nenhuma delas (só dados móveis), nada é enviado e a tela mostra "Nenhuma
  câmera encontrada.". As dicas, uma por linha, são:
  - "Confira se:"
  - "• o ONVIF está ativo na câmera (na Tapo, crie a \"Conta da câmera\" no app
    Tapo);"
  - "• o celular está conectado ao Wi-Fi da mesma rede da câmera."
- **Sobre:** "rtsp-viewer <versão>", "Client leve para câmeras RTSP." e, no
  lugar do componente do Slint (que o Android não usa), o título "Licenças de
  terceiros" com uma linha por componente: "Media3 (AndroidX) — Apache-2.0",
  "Jetpack Compose — Apache-2.0", "AndroidX — Apache-2.0" e "Kotlin e kotlinx
  — Apache-2.0". Botão "Voltar".
