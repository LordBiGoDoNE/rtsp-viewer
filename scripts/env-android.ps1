<#
.SYNOPSIS
  Ambiente de build do Android no Windows. Use com dot-source:  . scripts/env-android.ps1

.DESCRIPTION
  Define JAVA_HOME (JDK 21) e ANDROID_HOME (SDK de linha de comando) e coloca o
  adb no PATH. Não guarda nada no repositório.
#>
if (-not $env:ANDROID_HOME) {
    $sdk = Join-Path $env:LOCALAPPDATA 'Android\Sdk'
    if (Test-Path (Join-Path $sdk 'platform-tools')) { $env:ANDROID_HOME = $sdk }
    else { Write-Warning "Android SDK não encontrado em $sdk. Defina ANDROID_HOME."; return }
}

$jdk = Get-ChildItem (Join-Path $env:ProgramFiles 'Eclipse Adoptium') -Directory -Filter 'jdk-21*' -ErrorAction SilentlyContinue |
    Sort-Object Name -Descending | Select-Object -First 1
if ($jdk) { $env:JAVA_HOME = $jdk.FullName }
elseif (-not $env:JAVA_HOME) { Write-Warning 'JDK 21 não encontrado. Defina JAVA_HOME.' }

$env:PATH = (Join-Path $env:ANDROID_HOME 'platform-tools') + [IO.Path]::PathSeparator + $env:PATH

Write-Host "ANDROID_HOME=$env:ANDROID_HOME"
Write-Host "JAVA_HOME=$env:JAVA_HOME"
