<#
.SYNOPSIS
  Baixa o FFmpeg LGPL compartilhado (Windows x64) usado para compilar e
  empacotar o desktop, com versão e SHA-256 fixos (docs/adr/0002-ffmpeg.md).

.DESCRIPTION
  Destino: third_party/ffmpeg/ (ignorado no git), com include/, lib/ e bin/.
  Não faz nada se a versão fixada já estiver instalada. Falha se o hash não
  bater. Depois de rodar, carregue o ambiente com:  . scripts/env.ps1
#>
[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

# Versão fixada. Usar SEMPRE o último autobuild de um mês: pela política de
# retenção do BtbN/FFmpeg-Builds, ele fica publicado por dois anos, enquanto os
# builds diários somem depois de 14 dias (ver docs/adr/0002-ffmpeg.md).
# Para atualizar: escolher o autobuild de fim de mês mais recente, copiar nome e
# hash do checksums.sha256 da release e registrar a troca no ADR 0002.
$Release = 'autobuild-2026-10-31-12-59'
$Asset   = 'ffmpeg-n8.1.3-27-g8dca1586f4-win64-lgpl-shared-8.1.zip'
$Sha256  = '5cef7bc48e513f3cee7c881e8b2999d835541edb1846c3e89443a67f8f8556ab'
$Url     = "https://github.com/BtbN/FFmpeg-Builds/releases/download/$Release/$Asset"

$Root   = Split-Path -Parent $PSScriptRoot
$Dest   = Join-Path $Root 'third_party/ffmpeg'
$Marker = Join-Path $Dest '.versao'

if ((Test-Path $Marker) -and ((Get-Content $Marker -Raw).Trim() -eq $Asset)) {
    Write-Host "FFmpeg já instalado em $Dest ($Asset)."
    return
}

$Tmp = Join-Path ([IO.Path]::GetTempPath()) ("ffmpeg-" + [guid]::NewGuid())
New-Item -ItemType Directory -Path $Tmp | Out-Null
try {
    $Zip = Join-Path $Tmp $Asset
    Write-Host "Baixando $Url"
    try {
        Invoke-WebRequest -Uri $Url -OutFile $Zip -UseBasicParsing
    }
    catch {
        throw ("Falha ao baixar o FFmpeg fixado ($Release / $Asset): $($_.Exception.Message)`n" +
            "Se a release foi removida pelo BtbN (política de retenção), atualize a versão: " +
            "escolha o autobuild de fim de mês mais recente em " +
            "https://github.com/BtbN/FFmpeg-Builds/releases, copie nome e SHA-256 do " +
            "checksums.sha256 para este script e registre a troca em docs/adr/0002-ffmpeg.md.")
    }

    $Hash = (Get-FileHash -Algorithm SHA256 $Zip).Hash.ToLowerInvariant()
    if ($Hash -ne $Sha256) {
        throw "SHA-256 inesperado para ${Asset}: $Hash (esperado $Sha256)"
    }

    Expand-Archive -Path $Zip -DestinationPath $Tmp
    $Extracted = Get-ChildItem $Tmp -Directory | Select-Object -First 1
    foreach ($sub in 'include', 'lib', 'bin') {
        if (-not (Test-Path (Join-Path $Extracted.FullName $sub))) {
            throw "Pacote do FFmpeg sem a pasta '$sub'"
        }
    }

    if (Test-Path $Dest) { Remove-Item -Recurse -Force $Dest }
    New-Item -ItemType Directory -Force (Split-Path $Dest) | Out-Null
    Move-Item $Extracted.FullName $Dest
    Set-Content -Path $Marker -Value $Asset -NoNewline
    Write-Host "FFmpeg instalado em $Dest"
}
finally {
    Remove-Item -Recurse -Force $Tmp -ErrorAction SilentlyContinue
}
