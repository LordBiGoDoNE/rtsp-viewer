<#
.SYNOPSIS
  Monta o pacote .zip do desktop para Windows x64 (spec desktop-distribution).

.DESCRIPTION
  Copia o executável release, somente as DLLs do FFmpeg que ele usa (e as
  dependências delas), os avisos de licença e um LEIA-ME. Não publica nada:
  o .zip fica em dist/ (CI: artefato do workflow).

  Pré-requisitos: scripts/fetch-ffmpeg.ps1 e `cargo build --release` em desktop/.
#>
[CmdletBinding()]
param(
    [string]$OutDir = (Join-Path (Split-Path -Parent $PSScriptRoot) 'dist')
)

$ErrorActionPreference = 'Stop'
$Root    = Split-Path -Parent $PSScriptRoot
$Exe     = Join-Path $Root 'desktop/target/release/rtsp-viewer.exe'
# FFMPEG_DIR (definido por scripts/env.ps1) tem precedência sobre o padrão.
$Ffmpeg  = if ($env:FFMPEG_DIR) { $env:FFMPEG_DIR } else { Join-Path $Root 'third_party/ffmpeg' }
$Version = (Select-String -Path (Join-Path $Root 'desktop/Cargo.toml') -Pattern '^version = "(.+)"').Matches[0].Groups[1].Value

if (-not (Test-Path $Exe)) { throw "Executável ausente: $Exe (rode cargo build --release em desktop/)" }
if (-not (Test-Path (Join-Path $Ffmpeg 'bin'))) { throw "FFmpeg ausente: rode scripts/fetch-ffmpeg.ps1" }

# DLLs usadas: as do executável e, recursivamente, as que elas importam.
# Bibliotecas usadas pelo app (crates/media): avcodec, avformat, avutil e
# swscale; o avcodec importa o swresample.
$Needed = @('avcodec', 'avformat', 'avutil', 'swscale', 'swresample')
$Dlls = foreach ($name in $Needed) {
    $found = Get-ChildItem (Join-Path $Ffmpeg 'bin') -Filter "$name-*.dll"
    if ($found.Count -ne 1) { throw "Esperava uma DLL $name-*.dll, achei $($found.Count)" }
    $found
}

$Name  = "rtsp-viewer-$Version-windows-x64"
$Stage = Join-Path $OutDir $Name
if (Test-Path $Stage) { Remove-Item -Recurse -Force $Stage }
New-Item -ItemType Directory -Force $Stage | Out-Null

Copy-Item $Exe $Stage
$Dlls | Copy-Item -Destination $Stage
Copy-Item (Join-Path $Ffmpeg 'LICENSE.txt') (Join-Path $Stage 'FFmpeg-LICENSE.txt')
$FfmpegBuild = (Get-Content (Join-Path $Ffmpeg '.versao') -Raw).Trim()

@"
rtsp-viewer $Version
Client leve para câmeras RTSP.

Uso: extraia a pasta e execute rtsp-viewer.exe. Não precisa instalar nada.
Requisitos: Windows 10 ou 11, 64 bits.

A lista de câmeras fica em %APPDATA%\rtsp-viewer\config\cameras.json.
As senhas ficam no Gerenciador de Credenciais do Windows, nunca nesse arquivo.

Componentes de terceiros
------------------------
FFmpeg (LGPL 2.1 ou posterior), ligado dinamicamente pelas DLLs desta pasta,
sem modificações. Build: $FfmpegBuild
(https://github.com/BtbN/FFmpeg-Builds). Código-fonte: https://ffmpeg.org/download.html
Licença: FFmpeg-LICENSE.txt. As DLLs podem ser trocadas por outro build
compatível do FFmpeg.

Slint (https://slint.dev), sob a Slint Royalty-free License 2.0; atribuição na
tela "Sobre" do app.
"@ | Set-Content -Encoding utf8 (Join-Path $Stage 'LEIA-ME.txt')

$Zip = Join-Path $OutDir "$Name.zip"
if (Test-Path $Zip) { Remove-Item $Zip }
Compress-Archive -Path (Join-Path $Stage '*') -DestinationPath $Zip -CompressionLevel Optimal

$staged = (Get-ChildItem $Stage -File | Measure-Object Length -Sum).Sum
"Pacote: $Zip"
"Tamanho compactado: {0:N1} MB (descompactado: {1:N1} MB)" -f ((Get-Item $Zip).Length / 1MB), ($staged / 1MB)
