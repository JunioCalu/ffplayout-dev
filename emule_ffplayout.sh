#!/bin/bash

# Arquivos de entrada
FILE1="/var/lib/ffplayout/tv-media/1/עוד מעט זה כאן.mp4"
FILE2="/var/lib/ffplayout/tv-media/1/interlaced2.mp4"

# Parâmetros de decodificação
PARAMS_FULL="-threads 4 -init_hw_device cuda=cuda:reset -filter_hw_device cuda -hwaccel cuvid -hwaccel_output_format cuda -c:v h264_cuvid -deint 2 -fix_sub_duration -drop_second_field true"
PARAMS_BASIC="-init_hw_device cuda=cuda:reset -filter_hw_device cuda"

# Parte comum do comando de filtro e codificação do decoder
DECODER_FILTERS="-filter_complex \"[0:v:0]format=yuv420p,hwupload_cuda,scale_npp=format=yuv420p,scale_npp=1920:1080:interp_algo=super:force_original_aspect_ratio=decrease,hwdownload,pad=1920:1080:(ow-iw)/2:(oh-ih)/2:color=black,setdar=16:9,setsar=1:1,hwupload_cuda[video];movie=/var/lib/ffplayout/tv-media/1/00-assets/logo.png,format=rgba,colorchannelmixer=aa=0.7,hwupload_cuda[logo_alpha];[video][logo_alpha]overlay_cuda=W-w-12:12[vout0];[0:a:0]anull[aout0]\" -map \"[vout0]\" -map \"[aout0]\" -r 30 -c:v h264_nvenc -b:v 6000k -minrate 6000k -maxrate 6000k -bufsize 3000k -c:a aac -strict -2 -b:a 328k -ar 48000 -ac 2 -f mpegts -"

# Comando completo do encoder
ENCODER_CMD="ffmpeg -hide_banner -nostats -v level+error -threads 4 -init_hw_device cuda=cuda:reset -filter_hw_device cuda -hwaccel cuvid -c:v h264_cuvid -deint 2 -fix_sub_duration -drop_second_field true -re -i pipe:0 -map \"0:v\" -map \"0:a:0\" -c:v:0 h264_nvenc -aspect 16:9 -r:v:0 30 -b:v:0 3M -maxrate:0 4M -bufsize:0 2M -profile:v:0 main -level 41 -preset:v:0 p6 -ar:0 48000 -b:a:0 196k -c:a:0 aac -flags +cgop -f hls -hls_time 6 -hls_list_size 600 -hls_flags append_list+delete_segments+omit_endlist -hls_segment_filename \"/usr/share/ffplayout/public/1/live/stream-%d.ts\" \"/usr/share/ffplayout/public/1/live/stream.m3u8\""

# Variáveis para rastrear PIDs
ENCODER_PID=""
DECODER_PID=""
PIPE_NAME="/tmp/ffmpeg_pipe_$$"

# Função para matar todos os processos ao sair
cleanup() {
    echo "Terminando todos os processos..."
    
    # Matar o decoder se ainda estiver em execução
    if [ ! -z "$DECODER_PID" ] && ps -p $DECODER_PID > /dev/null 2>&1; then
        echo "Terminando processo decoder (PID: $DECODER_PID)..."
        kill $DECODER_PID 2>/dev/null
        sleep 1
        kill -9 $DECODER_PID 2>/dev/null
    fi
    
    # Matar o encoder ao encerrar o script
    if [ ! -z "$ENCODER_PID" ] && ps -p $ENCODER_PID > /dev/null 2>&1; then
        echo "Terminando processo encoder (PID: $ENCODER_PID)..."
        kill $ENCODER_PID 2>/dev/null
        sleep 1
        kill -9 $ENCODER_PID 2>/dev/null
    fi
    
    # Limpar pipe
    if [ -e "$PIPE_NAME" ]; then
        rm -f "$PIPE_NAME"
    fi
    
    exit 0
}

# Registrar função de limpeza para sinais de término
trap cleanup EXIT INT TERM

# Função para executar apenas o decoder com parâmetros específicos
run_decoder() {
    local input_file=$1
    local decode_params=$2
    local duration=$3  # Duração em segundos
    local start_offset=$4  # Posição inicial (opcional)
    
    echo "===================================="
    echo "Executando decoder com:"
    echo "Arquivo: $input_file"
    echo "Parâmetros: $decode_params"
    echo "Duração: $duration segundos"
    if [ ! -z "$start_offset" ]; then
        echo "Início a partir de: $start_offset segundos"
    fi
    echo "===================================="
    
    # Verificar e terminar qualquer processo decoder anterior
    if [ ! -z "$DECODER_PID" ] && ps -p $DECODER_PID > /dev/null 2>&1; then
        echo "Terminando processo decoder anterior (PID: $DECODER_PID)..."
        kill $DECODER_PID 2>/dev/null
        sleep 1
        # Força a terminação se ainda estiver rodando
        if ps -p $DECODER_PID > /dev/null 2>&1; then
            kill -9 $DECODER_PID 2>/dev/null
        fi
    fi
    
    # Construir o comando do decoder
    DECODER_CMD="ffmpeg -hide_banner -nostats -v level+error $decode_params"
    
    # Adicionar offset de início se fornecido
    if [ ! -z "$start_offset" ]; then
        DECODER_CMD="$DECODER_CMD -ss $start_offset"
    fi
    
    # Adicionar input e duração
    DECODER_CMD="$DECODER_CMD -i \"$input_file\" -t $duration $DECODER_FILTERS"
    
    # Ajustar para usar o pipe nomeado já criado
    MOD_DECODER_CMD="${DECODER_CMD/\-/$PIPE_NAME}"
    
    # Executar o decoder e guardar o PID
    eval "$MOD_DECODER_CMD" &
    DECODER_PID=$!
    
    echo "Processo Decoder iniciado com PID: $DECODER_PID"
    
    # Aguardar a duração do clipe mais uma pequena margem
    WAIT_TIME=$((duration + 1))
    echo "Aguardando $WAIT_TIME segundos para conclusão do decoder..."
    sleep $WAIT_TIME
    
    # Verificar se o decoder terminou naturalmente
    if ps -p $DECODER_PID > /dev/null 2>&1; then
        echo "Terminando processo decoder (PID: $DECODER_PID)..."
        kill $DECODER_PID 2>/dev/null
        sleep 1
        
        # Força a terminação se ainda estiver rodando
        if ps -p $DECODER_PID > /dev/null 2>&1; then
            kill -9 $DECODER_PID 2>/dev/null
        fi
    else
        echo "Decoder terminou naturalmente."
    fi
    
    DECODER_PID=""
    echo "Pronto para o próximo decoder."
    sleep 1
}

# Inicializar o pipe nomeado
echo "Criando pipe nomeado: $PIPE_NAME"
mkfifo "$PIPE_NAME"

# Iniciar o encoder com o pipe como entrada (apenas uma vez)
echo "Iniciando o processo encoder persistente..."
MOD_ENCODER_CMD="${ENCODER_CMD/pipe:0/$PIPE_NAME}"
eval "$MOD_ENCODER_CMD" &
ENCODER_PID=$!
echo "Processo Encoder iniciado com PID: $ENCODER_PID"

# Aguardar um momento para o encoder inicializar
sleep 2

# Verificar se o encoder está rodando
if ! ps -p $ENCODER_PID > /dev/null 2>&1; then
    echo "ERRO: O processo encoder falhou ao iniciar. Verifique os logs."
    cleanup
    exit 1
fi

echo "Encoder iniciado com sucesso. Iniciando sequência de decoders..."
sleep 1

# Loop principal de decoders
while true; do
    # Caso 1: Arquivo 1 com parâmetros completos (5 segundos a partir do início)
    run_decoder "$FILE1" "$PARAMS_FULL" 5
    
    # Verificar se o encoder ainda está rodando
    if ! ps -p $ENCODER_PID > /dev/null 2>&1; then
        echo "ERRO: O processo encoder foi encerrado. Reiniciando..."
        MOD_ENCODER_CMD="${ENCODER_CMD/pipe:0/$PIPE_NAME}"
        eval "$MOD_ENCODER_CMD" &
        ENCODER_PID=$!
        sleep 2
    fi
    
    # Caso 2: Arquivo 2 com parâmetros completos (5 segundos a partir do meio)
    run_decoder "$FILE2" "$PARAMS_FULL" 5
    
    # Verificar encoder
    if ! ps -p $ENCODER_PID > /dev/null 2>&1; then
        echo "ERRO: O processo encoder foi encerrado. Reiniciando..."
        MOD_ENCODER_CMD="${ENCODER_CMD/pipe:0/$PIPE_NAME}"
        eval "$MOD_ENCODER_CMD" &
        ENCODER_PID=$!
        sleep 2
    fi
    
    # Caso 3: Arquivo 2 com parâmetros básicos (5 segundos a partir de outro ponto)
    run_decoder "$FILE2" "$PARAMS_BASIC" 5
    
    # Verificar encoder
    if ! ps -p $ENCODER_PID > /dev/null 2>&1; then
        echo "ERRO: O processo encoder foi encerrado. Reiniciando..."
        MOD_ENCODER_CMD="${ENCODER_CMD/pipe:0/$PIPE_NAME}"
        eval "$MOD_ENCODER_CMD" &
        ENCODER_PID=$!
        sleep 2
    fi
    
    # Caso 4: Arquivo 1 com parâmetros completos (5 segundos a partir do início)
    run_decoder "$FILE1" "$PARAMS_FULL" 5
    
    # Verificar encoder
    if ! ps -p $ENCODER_PID > /dev/null 2>&1; then
        echo "ERRO: O processo encoder foi encerrado. Reiniciando..."
        MOD_ENCODER_CMD="${ENCODER_CMD/pipe:0/$PIPE_NAME}"
        eval "$MOD_ENCODER_CMD" &
        ENCODER_PID=$!
        sleep 2
    fi
    
    # Caso 5: Arquivo 2 com parâmetros básicos (5 segundos a partir do início)
    run_decoder "$FILE2" "$PARAMS_BASIC" 5
    
    # Verificar encoder
    if ! ps -p $ENCODER_PID > /dev/null 2>&1; then
        echo "ERRO: O processo encoder foi encerrado. Reiniciando..."
        MOD_ENCODER_CMD="${ENCODER_CMD/pipe:0/$PIPE_NAME}"
        eval "$MOD_ENCODER_CMD" &
        ENCODER_PID=$!
        sleep 2
    fi
    
    echo "Ciclo completo, reiniciando a sequência..."
done