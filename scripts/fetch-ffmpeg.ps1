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

# Versão fixada. Para atualizar: escolher outro autobuild do BtbN/FFmpeg-Builds,
# copiar nome e hash do checksums.sha256 da release e registrar no ADR 0002.
$Release = 'autobuild-2026-11-23-12-43'
$Asset   = 'ffmpeg-n8.1.3-39-g660b5aa7db-win64-lgpl-shared-8.1.zip'
$Sha256  = '8e424751cc51a473472c5bd99a79dade55792688917762295c188a844c5eca2a'
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
    Invoke-WebRequest -Uri $Url -OutFile $Zip -UseBasicParsing

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
