# Câmera falsa

Servidor RTSP local para testes, CI e teste de ponta a ponta. Não usa nenhuma
câmera real.

## Subir e parar

```powershell
docker compose -f test-camera/compose.yaml up -d
docker compose -f test-camera/compose.yaml down
```

## Caminhos

| URL | Vídeo | Autenticação |
| --- | --- | --- |
| `rtsp://localhost:8554/h264` | H.264 1280x720, 25 fps, relógio | nenhuma |
| `rtsp://localhost:8554/h265` | H.265 1280x720, 25 fps, relógio | nenhuma |
| `rtsp://localhost:8554/auth` | H.264 640x360, 15 fps | usuário `teste`, senha `teste123` (fictícios) |
| `rtsp://localhost:8554/queda` | H.264 640x360, 15 fps | nenhuma; usado pelo teste de queda e retorno |

O relógio no canto superior esquerdo mostra a hora **UTC** com milissegundos no
momento em que o quadro foi gerado. Para medir latência, compare com a hora UTC
do sistema no mesmo instante (por exemplo, numa captura de tela que mostre os
dois).

## Simular queda da câmera

```powershell
docker compose -f test-camera/compose.yaml stop pub-h264
docker compose -f test-camera/compose.yaml start pub-h264
```

## Conferir sem o app

```powershell
ffprobe -rtsp_transport tcp rtsp://localhost:8554/h264
```
