<#
.SYNOPSIS
  Mede o boot do desktop no Windows (spec desktop-distribution, "Boot rápido").

.DESCRIPTION
  Cria uma configuração isolada com 20 câmeras, inicia o executável N vezes
  com RTSP_VIEWER_BOOT_TRACE/RTSP_VIEWER_BOOT_EXIT e calcula o tempo do
  início do processo até a primeira renderização da janela. O instante de
  início é tomado antes de Start-Process, então o número é conservador.
  A primeira execução aquece o cache do sistema e é descartada (o requisito
  pede inicialização a quente).

  Não toca na lista de câmeras real do usuário nem no cofre de senhas.
#>
[CmdletBinding()]
param(
    [string]$Exe = (Join-Path (Split-Path -Parent $PSScriptRoot) 'desktop/target/release/rtsp-viewer.exe'),
    [int]$Runs = 10,
    [int]$Cameras = 20
)

$ErrorActionPreference = 'Stop'
if (-not (Test-Path $Exe)) { throw "Executável ausente: $Exe" }

$Work = Join-Path ([IO.Path]::GetTempPath()) ("rtsp-viewer-boot-" + [guid]::NewGuid())
New-Item -ItemType Directory -Path $Work | Out-Null
try {
    $list = foreach ($i in 1..$Cameras) {
        @{ id = [guid]::NewGuid().ToString(); name = "Câmera $i"; url = "rtsp://192.0.2.$i:554/stream1" }
    }
    $json = @{ version = 1; cameras = @($list) } | ConvertTo-Json -Depth 4
    [IO.File]::WriteAllText((Join-Path $Work 'cameras.json'), $json, (New-Object Text.UTF8Encoding $false))

    $env:RTSP_VIEWER_CONFIG_DIR = $Work
    $env:RTSP_VIEWER_BOOT_EXIT = '1'
    $trace = Join-Path $Work 'boot.txt'
    $env:RTSP_VIEWER_BOOT_TRACE = $trace

    $samples = @()
    foreach ($i in 0..$Runs) {
        Remove-Item $trace -ErrorAction SilentlyContinue
        $t0 = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
        $p = Start-Process $Exe -PassThru
        if (-not $p.WaitForExit(20000)) { $p.Kill(); throw "o app não fechou na execução $i" }
        $text = Get-Content $trace -Raw -ErrorAction SilentlyContinue
        $m = [regex]::Match("$text", 'first_frame_unix_ms=(\d+)')
        if (-not $m.Success) { throw "sem medição na execução $i ($text)" }
        $ms = [long]$m.Groups[1].Value - $t0
        if ($i -eq 0) { "aquecimento: $ms ms (descartado)"; continue }
        $samples += $ms
        "execução ${i}: $ms ms"
    }
    $sorted = $samples | Sort-Object
    $median = if ($sorted.Count % 2) { $sorted[[int][math]::Floor($sorted.Count / 2)] } else { ($sorted[$sorted.Count / 2 - 1] + $sorted[$sorted.Count / 2]) / 2 }
    ""
    "câmeras: $Cameras | execuções: $Runs"
    "mediana: $median ms | mínimo: $($sorted[0]) ms | máximo: $($sorted[-1]) ms"
    if ($median -lt 500) { "meta (< 500 ms): atendida" } else { "meta (< 500 ms): NÃO atendida" }
}
finally {
    Remove-Item Env:RTSP_VIEWER_CONFIG_DIR, Env:RTSP_VIEWER_BOOT_EXIT, Env:RTSP_VIEWER_BOOT_TRACE -ErrorAction SilentlyContinue
    Remove-Item -Recurse -Force $Work -ErrorAction SilentlyContinue
}
