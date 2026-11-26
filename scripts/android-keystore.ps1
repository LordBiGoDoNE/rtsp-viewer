<#
.SYNOPSIS
  Cria, uma vez, a keystore de assinatura do APK Android (ADR 0006).

.DESCRIPTION
  A keystore fica fora do repositório, em %USERPROFILE%\.android-keys\rtsp-viewer.jks
  (ou no caminho de -Path). A senha é digitada pelo usuário e não é gravada em
  lugar nenhum. Guarde uma cópia da keystore e da senha: sem elas, novas versões
  não instalam por cima da atual.
#>
param(
    [string]$Path = (Join-Path $env:USERPROFILE '.android-keys\rtsp-viewer.jks'),
    [string]$Alias = 'rtsp-viewer'
)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'env-android.ps1') | Out-Null

if (Test-Path $Path) { throw "Já existe uma keystore em $Path. Não sobrescrevo." }
$keytool = Join-Path $env:JAVA_HOME 'bin\keytool.exe'
if (-not (Test-Path $keytool)) { throw "keytool não encontrado em $keytool" }

$p1 = Read-Host 'Senha da keystore (mínimo 8 caracteres)' -AsSecureString
$p2 = Read-Host 'Repita a senha' -AsSecureString
$plain1 = [Net.NetworkCredential]::new('', $p1).Password
$plain2 = [Net.NetworkCredential]::new('', $p2).Password
if ($plain1 -ne $plain2) { throw 'As senhas não conferem.' }
if ($plain1.Length -lt 8) { throw 'Use pelo menos 8 caracteres.' }

New-Item -ItemType Directory -Force (Split-Path $Path) | Out-Null
$env:RTSP_VIEWER_KEYTOOL_PASS = $plain1
try {
    & $keytool -genkeypair -keystore $Path -alias $Alias -keyalg RSA -keysize 4096 -validity 10000 `
        -dname 'CN=rtsp-viewer' -storetype PKCS12 `
        -storepass:env RTSP_VIEWER_KEYTOOL_PASS -keypass:env RTSP_VIEWER_KEYTOOL_PASS
    if ($LASTEXITCODE -ne 0) { throw "keytool falhou ($LASTEXITCODE)" }
} finally {
    Remove-Item Env:RTSP_VIEWER_KEYTOOL_PASS -ErrorAction SilentlyContinue
}
Write-Host "Keystore criada em $Path (alias $Alias). Guarde uma cópia dela e da senha."
