<#
.SYNOPSIS
  Ambiente de build do desktop no Windows. Use com dot-source:  . scripts/env.ps1

.DESCRIPTION
  Define FFMPEG_DIR (bindings do ffmpeg-sys-next), LIBCLANG_PATH (bindgen) e
  coloca as DLLs do FFmpeg no PATH para rodar testes e o app.
#>
$Root = Split-Path -Parent $PSScriptRoot
$Ffmpeg = Join-Path $Root 'third_party/ffmpeg'

if (-not (Test-Path (Join-Path $Ffmpeg 'include'))) {
    Write-Warning "FFmpeg ausente. Rode primeiro: scripts/fetch-ffmpeg.ps1"
    return
}

$env:FFMPEG_DIR = (Resolve-Path $Ffmpeg).Path
$env:PATH = (Join-Path $env:FFMPEG_DIR 'bin') + [IO.Path]::PathSeparator + $env:PATH

if (-not $env:LIBCLANG_PATH) {
    $llvm = Join-Path $env:ProgramFiles 'LLVM\bin'
    if (Test-Path (Join-Path $llvm 'libclang.dll')) {
        $env:LIBCLANG_PATH = $llvm
    } else {
        Write-Warning "libclang.dll não encontrada. Instale o LLVM ou defina LIBCLANG_PATH."
    }
}

Write-Host "FFMPEG_DIR=$env:FFMPEG_DIR"
Write-Host "LIBCLANG_PATH=$env:LIBCLANG_PATH"
