<#
.SYNOPSIS
  Confere o pacote Windows montado por scripts/package-windows.ps1 (spec
  desktop-distribution: "executável e somente as DLLs do FFmpeg usadas", sem
  depender de nada instalado além do próprio Windows 10/11 x64).

.DESCRIPTION
  Falha se:
  - o conjunto de arquivos do pacote mudar (executável, as cinco DLLs do
    FFmpeg, avisos de licença e LEIA-ME);
  - o executável ou alguma DLL importar uma DLL que não esteja no pacote nem
    na lista de DLLs do sistema abaixo (por exemplo VCRUNTIME140.dll ou
    MSVCP140.dll, do Visual C++ Redistributable);
  - alguma DLL do pacote não for alcançada a partir das importações do
    executável (DLL sobrando).

  Usa o llvm-objdump do LLVM (LIBCLANG_PATH, definido por scripts/env.ps1).
  Roda localmente e no CI.
#>
[CmdletBinding()]
param(
    [string]$Stage
)

$ErrorActionPreference = 'Stop'
$Root = Split-Path -Parent $PSScriptRoot
if (-not $Stage) {
    $Stage = Get-ChildItem (Join-Path $Root 'dist') -Directory -Filter 'rtsp-viewer-*-windows-x64' |
        Select-Object -First 1 -ExpandProperty FullName
}
if (-not $Stage -or -not (Test-Path $Stage)) { throw 'Pacote ausente: rode scripts/package-windows.ps1' }
$ObjDump = Join-Path $env:LIBCLANG_PATH 'llvm-objdump.exe'
if (-not (Test-Path $ObjDump)) { throw "llvm-objdump ausente em LIBCLANG_PATH ($env:LIBCLANG_PATH)" }

# DLLs que fazem parte do Windows 10/11 x64. Lista derivada do que o
# executável e as DLLs do FFmpeg (build fixado em scripts/fetch-ffmpeg.ps1)
# importam hoje; uma DLL nova fora dela faz a checagem falhar e exige revisão.
# api-ms-win-* são os conjuntos de API do sistema (inclusive a UCRT).
$SystemDlls = @(
    'advapi32', 'bcrypt', 'bcryptprimitives', 'cfgmgr32', 'combase', 'comctl32',
    'crypt32', 'd2d1', 'dnsapi', 'dwmapi', 'dwrite', 'gdi32', 'imm32', 'iphlpapi',
    'kernel32', 'msimg32', 'ncrypt', 'ntdll', 'ole32', 'oleaut32', 'opengl32',
    'rpcrt4', 'secur32', 'shell32', 'shlwapi', 'uiautomationcore', 'user32',
    'userenv', 'usp10', 'uxtheme', 'ws2_32'
) | ForEach-Object { "$_.dll" }

# Conteúdo esperado do pacote; as DLLs do FFmpeg têm o sufixo da versão ABI.
$Expected = @(
    '^rtsp-viewer\.exe$', '^avcodec-\d+\.dll$', '^avformat-\d+\.dll$',
    '^avutil-\d+\.dll$', '^swscale-\d+\.dll$', '^swresample-\d+\.dll$',
    '^FFmpeg-LICENSE\.txt$', '^LEIA-ME\.txt$'
)

$errors = [System.Collections.Generic.List[string]]::new()
$files = Get-ChildItem $Stage -Recurse -File | ForEach-Object { $_.Name }
foreach ($pattern in $Expected) {
    $n = @($files | Where-Object { $_ -match $pattern }).Count
    if ($n -ne 1) { $errors.Add("esperava um arquivo $pattern no pacote, achei $n") }
}
foreach ($f in $files) {
    if (-not ($Expected | Where-Object { $f -match $_ })) { $errors.Add("arquivo inesperado no pacote: $f") }
}
if (Get-ChildItem $Stage -Directory) { $errors.Add('o pacote não deve ter subpastas') }

# Falha (em vez de devolver lista vazia) se o llvm-objdump falhar ou não achar
# nenhuma importação: $ErrorActionPreference não cobre programas nativos, e uma
# lista vazia deixaria passar uma importação indevida sem aviso.
function Get-Imports([string]$Path) {
    $out = & $ObjDump -p $Path 2>&1
    if ($LASTEXITCODE -ne 0) {
        throw "llvm-objdump falhou (código $LASTEXITCODE) ao ler ${Path}: $(($out | Out-String).Trim())"
    }
    $imports = @($out | Select-String -CaseSensitive 'DLL Name: (\S+)' |
        ForEach-Object { $_.Matches[0].Groups[1].Value.ToLowerInvariant() } | Sort-Object -Unique)
    if ($imports.Count -eq 0) { throw "nenhuma importação (DLL Name) encontrada em $Path" }
    $imports
}

$packaged = @{}
Get-ChildItem $Stage -File -Filter '*.dll' | ForEach-Object { $packaged[$_.Name.ToLowerInvariant()] = $_.FullName }

# Percorre as importações a partir do executável.
$reached = @{}
$queue = [System.Collections.Generic.Queue[string]]::new()
$queue.Enqueue((Join-Path $Stage 'rtsp-viewer.exe'))
while ($queue.Count -gt 0) {
    $bin = $queue.Dequeue()
    $binName = Split-Path -Leaf $bin
    foreach ($dll in Get-Imports $bin) {
        if ($dll -eq $binName.ToLowerInvariant()) { continue }
        if ($packaged.ContainsKey($dll)) {
            if (-not $reached.ContainsKey($dll)) { $reached[$dll] = $true; $queue.Enqueue($packaged[$dll]) }
        } elseif ($dll -like 'api-ms-win-*' -or $SystemDlls -contains $dll) {
            continue
        } else {
            $errors.Add("$binName importa $dll, que não está no pacote nem é DLL do sistema")
        }
    }
}
foreach ($dll in $packaged.Keys) {
    if (-not $reached.ContainsKey($dll)) { $errors.Add("DLL não usada pelo executável: $dll") }
}

if ($errors.Count -gt 0) {
    $errors | ForEach-Object { Write-Host "ERRO: $_" }
    throw "Pacote Windows inválido ($($errors.Count) problema(s)): $Stage"
}
"Pacote OK: $Stage"
"DLLs do pacote: $(($reached.Keys | Sort-Object) -join ', ')"
