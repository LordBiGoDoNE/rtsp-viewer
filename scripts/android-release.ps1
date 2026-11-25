<#
.SYNOPSIS
  Gera o APK de release assinado com a keystore do usuário (ADR 0006).

.DESCRIPTION
  Pede a senha da keystore criada por scripts/android-keystore.ps1, passa-a ao
  Gradle só pelo ambiente deste processo e gera
  android/app/build/outputs/apk/release/app-release.apk. Nada é gravado em disco
  além do APK. Para instalar: adb install -r <apk>.
#>
param(
    [string]$Keystore = (Join-Path $env:USERPROFILE '.android-keys\rtsp-viewer.jks'),
    [string]$Alias = 'rtsp-viewer'
)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'env-android.ps1') | Out-Null
if (-not (Test-Path $Keystore)) { throw "Keystore não encontrada: $Keystore. Rode scripts/android-keystore.ps1." }

$secure = Read-Host 'Senha da keystore' -AsSecureString
$env:RTSP_VIEWER_KEYSTORE = $Keystore
$env:RTSP_VIEWER_KEY_ALIAS = $Alias
$env:RTSP_VIEWER_KEYSTORE_PASSWORD = [Net.NetworkCredential]::new('', $secure).Password
$android = Join-Path (Split-Path -Parent $PSScriptRoot) 'android'
try {
    & (Join-Path $android 'gradlew.bat') -p $android --no-configuration-cache :app:assembleRelease
    if ($LASTEXITCODE -ne 0) { throw "Gradle falhou ($LASTEXITCODE)" }
} finally {
    Remove-Item Env:RTSP_VIEWER_KEYSTORE_PASSWORD, Env:RTSP_VIEWER_KEYSTORE, Env:RTSP_VIEWER_KEY_ALIAS -ErrorAction SilentlyContinue
}
$apk = Join-Path $android 'app\build\outputs\apk\release\app-release.apk'
$apksigner = Get-ChildItem (Join-Path $env:ANDROID_HOME 'build-tools') -Directory | Sort-Object Name -Descending |
    Select-Object -First 1 | ForEach-Object { Join-Path $_.FullName 'apksigner.bat' }
& $apksigner verify --print-certs $apk | Select-Object -First 3
Write-Host ("APK: {0} ({1:N1} MB)" -f $apk, ((Get-Item $apk).Length / 1MB))
