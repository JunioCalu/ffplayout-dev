#!/bin/bash
echo "Verificando dispositivos CUDA disponíveis para FFmpeg:"
for i in {0..7}; do
  echo -n "Testando GPU $i: "
  if ffmpeg -hide_banner -loglevel error -init_hw_device cuda=gpu$i:$i -f lavfi -i nullsrc -t 0.1 -f null - 2>/dev/null; then
    echo "Disponível"
  else
    echo "Não disponível"
  fi
done
