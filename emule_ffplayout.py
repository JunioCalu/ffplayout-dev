#!/usr/bin/env python3

import os
import signal
import subprocess
import time
import atexit
import sys
import threading
import argparse
import psutil

class CircularLogBuffer:
    """
    Implementa um buffer circular para armazenar as últimas N linhas de log.
    Útil para manter um histórico recente de logs mesmo após um processo encerrar.
    """
    def __init__(self, max_size=20):
        self.buffer = []
        self.max_size = max_size
    
    def add(self, line):
        """Adiciona uma linha ao buffer, removendo a mais antiga se necessário"""
        if line:
            self.buffer.append(line)
            if len(self.buffer) > self.max_size:
                self.buffer.pop(0)
    
    def get_all(self):
        """Retorna todas as linhas do buffer"""
        return self.buffer
    
    def clear(self):
        """Limpa o buffer"""
        self.buffer = []
    
    def __str__(self):
        """Retorna uma representação em string do buffer"""
        return "\n".join(self.buffer)

def kill_for_rapid_resource_release(pid):
    try:
        # Primeiro STOP para pausar execução e evitar mais alocações
        os.kill(pid, signal.SIGSTOP)
        
        process = psutil.Process(pid)
        
        # Mata filhos primeiro (eles podem estar segurando recursos)
        children = process.children(recursive=True)
        for child in children:
            os.kill(child.pid, signal.SIGKILL)
            
        # Depois mata o processo principal
        os.kill(pid, signal.SIGKILL)
    except (psutil.NoSuchProcess, OSError):
        pass

class FFmpegPipeline:
    def __init__(self, log_timing=False, termination_wait=1, log_encoder_timing=False, 
                 log_kill_timing=False, log_terminate_timing=False, log_process_timing=False, 
                 log_total_timing=False, debug_decoder="warning", debug_encoder="warning",
                 log_encoder_stdout=False, log_decoder_stdout=False,
                 encoder_stdout_buffer_size=20, decoder_stdout_buffer_size=20,
                 encoder_stderr_buffer_size=20, decoder_stderr_buffer_size=20):
        # Configurações de timing
        self.log_timing = log_timing
        self.termination_wait = termination_wait
        self.log_encoder_timing = log_encoder_timing
        self.log_kill_timing = log_kill_timing
        self.log_terminate_timing = log_terminate_timing
        self.log_process_timing = log_process_timing
        self.log_total_timing = log_total_timing
        
        # Configurações para tempos de espera fixos
        self.encoder_warmup_delay = 3  # Tempo de espera após iniciar o encoder antes de iniciar o decoder
        self.encoder_restart_delay = 5  # Tempo de espera após reiniciar o encoder
        
        # Configurações de logging
        self.log_encoder_stdout = log_encoder_stdout  # Se True, exibe logs de stdout do encoder em tempo real
        self.log_decoder_stdout = log_decoder_stdout  # Se True, exibe logs de stdout do decoder em tempo real
        self.encoder_stdout_buffer_size = encoder_stdout_buffer_size  # Tamanho do buffer de stdout do encoder
        self.decoder_stdout_buffer_size = decoder_stdout_buffer_size  # Tamanho do buffer de stdout do decoder
        self.encoder_stderr_buffer_size = encoder_stderr_buffer_size  # Tamanho do buffer de stderr do encoder
        self.decoder_stderr_buffer_size = decoder_stderr_buffer_size  # Tamanho do buffer de stderr do decoder
        
        # Configurações de debug
        self.debug_decoder = debug_decoder
        self.debug_encoder = debug_encoder
        
        # Arquivos de entrada
        self.FILE1 = "/var/lib/ffplayout/tv-media/1/עוד מעט זה כאן.mp4"
        self.FILE2 = "/var/lib/ffplayout/tv-media/1/interlaced2.mp4"
        
        # Parâmetros de decodificação
        self.PARAMS_FULL = "-threads 4 -init_hw_device cuda=cuda:reset -filter_hw_device cuda -hwaccel cuvid -hwaccel_output_format cuda -c:v h264_cuvid -deint 2 -fix_sub_duration -drop_second_field true"
        self.PARAMS_BASIC = "-init_hw_device cuda=cuda:reset -filter_hw_device cuda"
        
        # Filtros para PARAMS_BASIC (inclui format=yuv420p,hwupload_cuda)
        self.DECODER_FILTERS_BASIC = '-filter_complex "[0:v:0]format=yuv420p,hwupload_cuda,scale_npp=format=yuv420p,scale_npp=1280:720:interp_algo=super:force_original_aspect_ratio=decrease,hwdownload,pad=1280:720:(ow-iw)/2:(oh-ih)/2:color=black,setdar=16:9,setsar=1:1,hwupload_cuda[video];movie=/var/lib/ffplayout/tv-media/1/00-assets/logo.png,format=rgba,colorchannelmixer=aa=0.7,hwupload_cuda[logo_alpha];[video][logo_alpha]overlay_cuda=W-w-12:12[vout0];[0:a:0]anull[aout0]" -map "[vout0]" -map "[aout0]" -r 30 -c:v h264_nvenc -b:v 6000k -minrate 6000k -maxrate 6000k -bufsize 3000k -c:a aac -strict -2 -b:a 328k -ar 48000 -ac 2 -f mpegts OUTPUT_PLACEHOLDER'
        
        # Filtros para PARAMS_FULL (sem format=yuv420p,hwupload_cuda)
        self.DECODER_FILTERS_FULL = '-filter_complex "[0:v:0]scale_npp=format=yuv420p,scale_npp=1280:720:interp_algo=super:force_original_aspect_ratio=decrease,hwdownload,pad=1280:720:(ow-iw)/2:(oh-ih)/2:color=black,setdar=16:9,setsar=1:1,hwupload_cuda[video];movie=/var/lib/ffplayout/tv-media/1/00-assets/logo.png,format=rgba,colorchannelmixer=aa=0.7,hwupload_cuda[logo_alpha];[video][logo_alpha]overlay_cuda=W-w-12:12[vout0];[0:a:0]anull[aout0]" -map "[vout0]" -map "[aout0]" -r 30 -c:v h264_nvenc -b:v 6000k -minrate 6000k -maxrate 6000k -bufsize 3000k -c:a aac -strict -2 -b:a 328k -ar 48000 -ac 2 -f mpegts OUTPUT_PLACEHOLDER'
        
        # Comando completo do encoder (com nível de log configurável)
        self.ENCODER_CMD = 'ffmpeg -y -v {debug_level} -threads 4 -init_hw_device cuda=cuda:reset -filter_hw_device cuda -hwaccel cuvid -c:v h264_cuvid -deint 2 -fix_sub_duration -drop_second_field true -re -i pipe:0 -map "0:v" -map "0:a:0" -c:v:0 h264_nvenc -aspect 16:9 -r:v:0 30 -b:v:0 3M -maxrate:0 4M -bufsize:0 2M -profile:v:0 main -level 41 -preset:v:0 p6 -ar:0 48000 -b:a:0 196k -c:a:0 aac -flags +cgop -f hls -hls_time 6 -hls_list_size 600 -hls_flags append_list+delete_segments+omit_endlist -hls_segment_filename "/usr/share/ffplayout/public/1/live/stream-%d.ts" "/usr/share/ffplayout/public/1/live/stream.m3u8"'
        
        # Variáveis de estado
        self.encoder_process = None
        self.decoder_process = None
        self.pipe_name = f"/tmp/ffmpeg_pipe_{os.getpid()}"
        self.encoder_start_time = None
        self.encoder_total_start_time = None
        self.encoder_process_start_time = None
        
        # Criar o named pipe uma única vez na inicialização
        self.create_pipe()
        
        # Buffers de log
        self.encoder_stdout_buffer = None
        self.encoder_stderr_buffer = None
        self.decoder_stdout_buffer = None
        self.decoder_stderr_buffer = None
        
        # Variáveis para controle das threads de log
        self.encoder_log_threads_active = [False]
        self.decoder_log_threads_active = [False]
        self.encoder_stdout_thread = None
        self.encoder_stderr_thread = None
        self.decoder_stdout_thread = None
        self.decoder_stderr_thread = None
        
        # Registrar função de limpeza
        atexit.register(self.cleanup)
        signal.signal(signal.SIGINT, self.signal_handler)
        signal.signal(signal.SIGTERM, self.signal_handler)

    def signal_handler(self, sig, frame):
        print(f"Recebido sinal {sig}, encerrando...")
        self.cleanup()
        sys.exit(0)

    def cleanup(self):
        print("Realizando limpeza...")
        
        cleanup_start = time.time()
        
        # Parar as threads de log
        self.encoder_log_threads_active[0] = False
        self.decoder_log_threads_active[0] = False
        
        # Encerrar decoder se estiver rodando
        if self.decoder_process and self.decoder_process.poll() is None:
            print(f"Terminando processo decoder (PID: {self.decoder_process.pid})...")
            try:
                termination_start = time.time()
                
                # Usar a nova função para matar o processo rapidamente
                kill_for_rapid_resource_release(self.decoder_process.pid)
                
                if self.log_timing:
                    termination_time = time.time() - termination_start
                    print(f"  [TIMING] Tempo total para encerrar o decoder: {termination_time:.2f} segundos")
            except Exception as e:
                print(f"Erro ao terminar decoder: {e}")
        
        # Encerrar encoder se estiver rodando
        if self.encoder_process and self.encoder_process.poll() is None:
            # Registrar tempo de processamento puro do encoder
            if self.log_process_timing and self.encoder_process_start_time:
                encoder_process_time = time.time() - self.encoder_process_start_time
                print(f"[PROCESS TIMING] Tempo total de processamento puro do encoder: {encoder_process_time:.2f} segundos")
            
            # Registrar tempo total até aqui antes de iniciar encerramento
            if self.log_total_timing and self.encoder_total_start_time:
                total_time_before_terminate = time.time() - self.encoder_total_start_time
                print(f"[TOTAL TIMING] Tempo total do encoder antes do encerramento: {total_time_before_terminate:.2f} segundos")
            
            print(f"Terminando processo encoder (PID: {self.encoder_process.pid})...")
            try:
                termination_start = time.time()
                
                # Imprimir buffer de logs antes de encerrar se houver erros
                if self.encoder_stdout_buffer or self.encoder_stderr_buffer:
                    self.print_log_buffers("Encoder", True)
                
                # Usar a nova função para matar o processo rapidamente
                kill_for_rapid_resource_release(self.encoder_process.pid)
                
                # Registrar tempo total incluindo terminate
                if self.log_total_timing and self.encoder_start_time:
                    total_encoder_time = time.time() - self.encoder_start_time
                    print(f"  [TOTAL TIMING] Tempo total de execução do encoder: {total_encoder_time:.2f} segundos")
            except Exception as e:
                print(f"Erro ao terminar encoder: {e}")
        
        # Remover pipe apenas ao finalizar completamente o programa
        if os.path.exists(self.pipe_name):
            try:
                os.unlink(self.pipe_name)
                print(f"Pipe removido: {self.pipe_name}")
            except Exception as e:
                print(f"Erro ao remover pipe: {e}")
        
        if self.log_timing:
            cleanup_time = time.time() - cleanup_start
            print(f"[TIMING] Tempo total de limpeza: {cleanup_time:.2f} segundos")

    def create_pipe(self):
        """Cria o named pipe para comunicação entre decoder e encoder"""
        if os.path.exists(self.pipe_name):
            try:
                os.unlink(self.pipe_name)
                print(f"Pipe antigo removido: {self.pipe_name}")
            except Exception as e:
                print(f"Erro ao remover pipe antigo: {e}")
        
        try:
            os.mkfifo(self.pipe_name)
            print(f"Pipe criado: {self.pipe_name}")
        except Exception as e:
            print(f"Erro ao criar pipe: {e}")
            sys.exit(1)
    
    def print_log_buffers(self, process_name, print_all=False):
        """
        Imprime o conteúdo dos buffers de log, útil após uma falha para diagnóstico.
        
        Args:
            process_name: Nome do processo ('Encoder' ou 'Decoder')
            print_all: Se True, imprime todo o buffer mesmo se não houver erros
        """
        if process_name.lower() == 'encoder':
            stdout_buffer = self.encoder_stdout_buffer
            stderr_buffer = self.encoder_stderr_buffer
        else:  # Decoder
            stdout_buffer = self.decoder_stdout_buffer
            stderr_buffer = self.decoder_stderr_buffer
        
        if not stdout_buffer and not stderr_buffer:
            return
        
        print(f"\n{'='*20} ÚLTIMOS LOGS DO {process_name.upper()} {'='*20}")
        
        # Exibir stdout se houver conteúdo
        if stdout_buffer:
            stdout_lines = stdout_buffer.get_all()
            if stdout_lines:
                print(f"\n--- {process_name} STDOUT (últimas {len(stdout_lines)} linhas) ---")
                for line in stdout_lines:
                    print(f"  STDOUT: {line}")
        
        # Exibir stderr se houver conteúdo
        if stderr_buffer:
            stderr_lines = stderr_buffer.get_all()
            if stderr_lines:
                print(f"\n--- {process_name} STDERR (últimas {len(stderr_lines)} linhas) ---")
                for line in stderr_lines:
                    # Destacar linhas de erro
                    if "error" in line.lower() or "fatal" in line.lower() or "broken pipe" in line.lower():
                        print(f"  \033[91mSTDERR: {line}\033[0m")
                    else:
                        print(f"  STDERR: {line}")
        
        print(f"{'='*60}\n")

    def start_process_log_threads(self, process, process_name):
        """
        Inicia threads para capturar tanto stdout quanto stderr de um processo,
        mantendo buffers circulares das últimas N linhas.
        
        Args:
            process: O processo cujos logs serão capturados
            process_name: Nome a ser usado nos logs ('Encoder' ou 'Decoder')
        """
        is_encoder = process_name.lower() == 'encoder'
        
        # Desativar threads anteriores
        if is_encoder:
            self.encoder_log_threads_active[0] = False
            if self.encoder_stdout_thread and self.encoder_stdout_thread.is_alive():
                self.encoder_stdout_thread.join(timeout=0.5)
            if self.encoder_stderr_thread and self.encoder_stderr_thread.is_alive():
                self.encoder_stderr_thread.join(timeout=0.5)
            
            # Criar novos buffers
            self.encoder_stdout_buffer = CircularLogBuffer(max_size=self.encoder_stdout_buffer_size)
            self.encoder_stderr_buffer = CircularLogBuffer(max_size=self.encoder_stderr_buffer_size)
            
            # Ativar novas threads
            self.encoder_log_threads_active[0] = True
            active_flag = self.encoder_log_threads_active
            stdout_buffer = self.encoder_stdout_buffer
            stderr_buffer = self.encoder_stderr_buffer
            log_stdout = self.log_encoder_stdout
        else:  # Decoder
            self.decoder_log_threads_active[0] = False
            if self.decoder_stdout_thread and self.decoder_stdout_thread.is_alive():
                self.decoder_stdout_thread.join(timeout=0.5)
            if self.decoder_stderr_thread and self.decoder_stderr_thread.is_alive():
                self.decoder_stderr_thread.join(timeout=0.5)
            
            # Criar novos buffers
            self.decoder_stdout_buffer = CircularLogBuffer(max_size=self.decoder_stdout_buffer_size)
            self.decoder_stderr_buffer = CircularLogBuffer(max_size=self.decoder_stderr_buffer_size)
            
            # Ativar novas threads
            self.decoder_log_threads_active[0] = True
            active_flag = self.decoder_log_threads_active
            stdout_buffer = self.decoder_stdout_buffer
            stderr_buffer = self.decoder_stderr_buffer
            log_stdout = self.log_decoder_stdout
        
        # Cores para diferentes tipos de log
        if is_encoder:
            color_code = "\033[94m"  # Azul para encoder
        else:
            color_code = "\033[92m"  # Verde para decoder
            
        error_color = "\033[91m"  # Vermelho para erros
        reset_color = "\033[0m"
        
        # Thread para capturar stdout
        def read_stdout():
            try:
                for line in iter(process.stdout.readline, ''):
                    if not active_flag[0]:
                        break
                    line = line.strip()
                    if line:
                        # Adicionar ao buffer circular
                        stdout_buffer.add(line)
                        
                        # Encerra imediatamente o decoder ao detectar "no frame" pela primeira vez
                        if not is_encoder and "no frame" in line.lower():
                            print(f"{error_color}[TERMINANDO IMEDIATAMENTE]\033[0m: Encerrando o decoder devido a erro 'no frame' detectado no stdout.")
                            kill_timestamp = time.time()
                            kill_for_rapid_resource_release(process.pid)
                            print(f"Decoder encerrado em {time.time() - kill_timestamp:.3f}s após detecção de erro 'no frame'")
                            return  # Encerra a thread
                        
                        # Exibir em tempo real se solicitado
                        if log_stdout:
                            sys.stdout.write(f"{color_code}[{process_name} OUT]\033[0m: {line}\n")
                            sys.stdout.flush()
                        
                        # Apenas colorir erros sem tomar ações
                        if ("error" in line.lower() or "fatal" in line.lower() or 
                            "invalid" in line.lower() or "failed" in line.lower()):
                            sys.stdout.write(f"{error_color}[{process_name} OUT ERROR]\033[0m: {line}\n")
                            sys.stdout.flush()
            except Exception as e:
                print(f"Erro na thread de stdout do {process_name}: {e}")
        
        # Thread para capturar stderr
        def read_stderr():
            try:
                for line in iter(process.stderr.readline, ''):
                    if not active_flag[0]:
                        break
                    line = line.strip()
                    if line:
                        # Adicionar ao buffer circular
                        stderr_buffer.add(line)
                        
                        # Encerra imediatamente o decoder ao detectar "no frame" pela primeira vez
                        if not is_encoder and "no frame" in line.lower():
                            print(f"{error_color}[TERMINANDO IMEDIATAMENTE]\033[0m: Encerrando o decoder devido a erro 'no frame' detectado no stderr.")
                            kill_timestamp = time.time()
                            kill_for_rapid_resource_release(process.pid)
                            print(f"Decoder encerrado em {time.time() - kill_timestamp:.3f}s após detecção de erro 'no frame'")
                            return  # Encerra a thread
                        
                        # Detecção de broken pipe (apenas para log)
                        if "Broken pipe" in line or "pipe:0" in line:
                            if not is_encoder:  # Se for o decoder
                                print(f"\033[91m[ALERTA]\033[0m: Detectado erro de Broken Pipe! O encoder pode ter falhado.")
                        
                        # Exibir em stderr
                        if "error" in line.lower() or "fatal" in line.lower() or "broken pipe" in line.lower() or "pipe:" in line.lower():
                            sys.stdout.write(f"{error_color}[{process_name} ERROR]\033[0m: {line}\n")
                        else:
                            sys.stdout.write(f"{color_code}[{process_name}]\033[0m: {line}\n")
                        sys.stdout.flush()
            except Exception as e:
                print(f"Erro na thread de stderr do {process_name}: {e}")
        
        # Iniciar as threads
        stdout_thread = threading.Thread(target=read_stdout)
        stdout_thread.daemon = True
        stdout_thread.start()
        
        stderr_thread = threading.Thread(target=read_stderr)
        stderr_thread.daemon = True
        stderr_thread.start()
        
        # Atualizar referências das threads
        if is_encoder:
            self.encoder_stdout_thread = stdout_thread
            self.encoder_stderr_thread = stderr_thread
        else:
            self.decoder_stdout_thread = stdout_thread
            self.decoder_stderr_thread = stderr_thread

    def start_encoder(self):
        """Inicia o processo encoder que permanecerá ativo"""
        # Verificar se o pipe existe (não recriar)
        if not os.path.exists(self.pipe_name):
            print(f"ERRO: O pipe {self.pipe_name} não existe. Criando-o novamente...")
            self.create_pipe()
        
        # Ajustar o comando para usar o pipe e o nível de debug configurado
        encoder_cmd = self.ENCODER_CMD.format(debug_level=self.debug_encoder).replace("pipe:0", self.pipe_name)
        
        print("Iniciando o processo encoder persistente...")
        print(f"Comando: {encoder_cmd}")
        
        start_time = time.time()
        self.encoder_start_time = start_time
        self.encoder_total_start_time = start_time  # Para registro de tempo total
        self.encoder_process_start_time = None  # Tempo de início do processamento efetivo
        
        try:
            # Iniciar o encoder com captura de saída para depuração
            self.encoder_process = subprocess.Popen(
                encoder_cmd,
                shell=True,
                stdin=subprocess.PIPE,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
                bufsize=1,
                universal_newlines=True
            )
            print(f"Processo Encoder iniciado com PID: {self.encoder_process.pid}")
            
            # Iniciar threads para capturar stdout e stderr
            self.start_process_log_threads(self.encoder_process, "Encoder")
            
            # Aguardar um momento para o encoder inicializar
            print(f"Aguardando {self.encoder_warmup_delay} segundos para o encoder inicializar...")
            
            # Espera vigilante - verificar se o encoder falha durante a inicialização
            for i in range(self.encoder_warmup_delay):
                if self.encoder_process.poll() is not None:
                    break
                time.sleep(1)
                print(f"Inicialização do encoder: {i+1}/{self.encoder_warmup_delay}s")
            
            # Verificar se o encoder está rodando
            if self.encoder_process.poll() is not None:
                print(f"ERRO: O processo encoder falhou ao iniciar. Código de saída: {self.encoder_process.returncode}")
                # Mostrar logs armazenados para diagnóstico
                self.print_log_buffers("Encoder", True)
                
                if self.log_encoder_timing:
                    startup_time = time.time() - start_time
                    print(f"  [TIMING] Tempo até falha do encoder: {startup_time:.2f} segundos")
                return False
            
            # Marcar o início do processamento efetivo
            self.encoder_process_start_time = time.time()
            
            if self.log_encoder_timing:
                startup_time = time.time() - start_time
                print(f"  [TIMING] Tempo de inicialização do encoder: {startup_time:.2f} segundos")
            
            print("Encoder inicializado com sucesso!")
            return True
            
        except Exception as e:
            print(f"Erro ao iniciar encoder: {e}")
            if self.log_encoder_timing:
                error_time = time.time() - start_time
                print(f"  [TIMING] Tempo até erro do encoder: {error_time:.2f} segundos")
            return False

    def check_encoder(self):
        """Verifica se o encoder ainda está rodando e reinicia se necessário"""
        # Verificar se encoder está rodando
        if self.encoder_process is None or self.encoder_process.poll() is not None:
            encoder_runtime = None
            encoder_process_runtime = None
            
            if self.log_encoder_timing and self.encoder_start_time:
                encoder_runtime = time.time() - self.encoder_start_time
                print(f"  [TIMING] Tempo de execução do encoder antes do encerramento: {encoder_runtime:.2f} segundos")
            
            if self.log_process_timing and self.encoder_process_start_time:
                encoder_process_runtime = time.time() - self.encoder_process_start_time
                print(f"  [PROCESS TIMING] Tempo de processamento puro do encoder: {encoder_process_runtime:.2f} segundos")
            
            if self.log_total_timing and self.encoder_total_start_time:
                total_encoder_time = time.time() - self.encoder_total_start_time
                print(f"  [TOTAL TIMING] Tempo total do encoder até encerramento: {total_encoder_time:.2f} segundos")
                
                # Comparar tempo de processamento puro com tempo total se ambos estiverem disponíveis
                if encoder_process_runtime:
                    overhead_time = total_encoder_time - encoder_process_runtime
                    overhead_percent = (overhead_time / total_encoder_time) * 100
                    print(f"  [TOTAL vs PROCESS] Overhead de {overhead_time:.2f} segundos ({overhead_percent:.1f}%) utilizado em operações não-processamento")
            
            # Verificar código de saída
            exit_code = self.encoder_process.returncode if self.encoder_process else "N/A"
            print(f"\033[91m[ALERTA]\033[0m: O processo encoder foi encerrado com código {exit_code}. Reiniciando...")
            
            # Mostrar os últimos logs do encoder antes de reiniciar
            self.print_log_buffers("Encoder", True)
            
            # Desativar as threads de log anteriores
            self.encoder_log_threads_active[0] = False
            
            # Verificar se o pipe ainda existe
            if not os.path.exists(self.pipe_name):
                print(f"AVISO: O pipe {self.pipe_name} não existe mais. Recriando...")
                self.create_pipe()
            
            # Reiniciar o encoder
            restart_start = time.time()
            self.encoder_start_time = restart_start
            self.encoder_total_start_time = restart_start
            
            # Ajustar o comando para usar o pipe e o nível de debug configurado
            encoder_cmd = self.ENCODER_CMD.format(debug_level=self.debug_encoder).replace("pipe:0", self.pipe_name)
            
            print(f"Reiniciando encoder com comando: {encoder_cmd}")
            
            self.encoder_process = subprocess.Popen(
                encoder_cmd,
                shell=True,
                stdin=subprocess.PIPE,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
                bufsize=1,
                universal_newlines=True
            )
            print(f"Processo Encoder reiniciado com PID: {self.encoder_process.pid}")
            
            # Iniciar novas threads para logs
            self.start_process_log_threads(self.encoder_process, "Encoder")
            
            # Esperar mais tempo depois da reinicialização para garantir que o encoder estabilize
            print(f"Aguardando {self.encoder_restart_delay} segundos para o encoder reiniciado estabilizar...")
            for i in range(self.encoder_restart_delay):
                if self.encoder_process.poll() is not None:
                    break
                time.sleep(1)
                print(f"Reinicialização do encoder: {i+1}/{self.encoder_restart_delay}s")
            
            if self.encoder_process.poll() is not None:
                print(f"ERRO: Falha ao reiniciar encoder. Código: {self.encoder_process.returncode}")
                # Mostrar logs armazenados para diagnóstico
                self.print_log_buffers("Encoder", True)
                
                if self.log_encoder_timing:
                    restart_time = time.time() - restart_start
                    print(f"  [TIMING] Tempo até falha na reinicialização: {restart_time:.2f} segundos")
                return False
            
            # Marcar o início do processamento efetivo para o encoder reiniciado
            self.encoder_process_start_time = time.time()
            
            if self.log_encoder_timing:
                restart_time = time.time() - restart_start
                print(f"  [TIMING] Tempo para reiniciar o encoder: {restart_time:.2f} segundos")
                if encoder_runtime:
                    print(f"  [TIMING] Tempo total do encoder anterior: {encoder_runtime:.2f} segundos")
            
            return True
        
        return True
            
    def run_decoder(self, input_file, decode_params, duration, start_offset=None):
        """Executa um processo decoder com os parâmetros especificados"""
        print("=" * 50)
        print(f"Executando decoder com:")
        print(f"Arquivo: {input_file}")
        print(f"Parâmetros: {decode_params}")
        print(f"Duração: {duration} segundos")
        if start_offset is not None:
            print(f"Início a partir de: {start_offset} segundos")
        print("=" * 50)
        
        # Verificar se o encoder está em execução antes de iniciar
        if not self.check_encoder():
            print("Encoder não está em execução. Reiniciando encoder antes de executar decoder.")
            if not self.start_encoder():
                print("Não foi possível iniciar o encoder. Continuando mesmo assim...")
        
        # Variáveis para registro de tempo
        process_start_time = time.time()
        total_start_time = process_start_time  # Para registrar tempo total com terminate/kill
        processing_start_time = None
        processing_end_time = None
        
        # MODIFICADO: Remover desativação da thread de log anterior do decoder
        
        # Verificar se há um decoder anterior em execução - apenas aguardar sem encerrar
        if self.decoder_process and self.decoder_process.poll() is None:
            print(f"AVISO: Processo decoder anterior (PID: {self.decoder_process.pid}) ainda está em execução.")
            print("Aguardando sua conclusão natural antes de iniciar o próximo decoder...")
            
            # Aguardar até que o decoder anterior termine
            while self.decoder_process and self.decoder_process.poll() is None:
                time.sleep(1)
                # Verificar encoder a cada 5 segundos durante espera
                if int(time.time() - process_start_time) % 5 == 0:
                    if self.encoder_process.poll() is not None:
                        print("\033[91m[ALERTA]\033[0m: Encoder encerrou durante a espera pelo decoder anterior!")
                        self.check_encoder()
            
            # Registrar código de saída do decoder anterior 
            if self.decoder_process:
                print(f"Decoder anterior terminou com código: {self.decoder_process.returncode}")
        
        # Construir o comando do decoder - Adicionando -y para forçar sobrescrita e nível de debug configurado
        decoder_cmd = f"ffmpeg -y -v {self.debug_decoder} {decode_params}"
        
        # Adicionar offset de início se fornecido
        if start_offset is not None:
            decoder_cmd += f" -ss {start_offset}"
        
        # Adicionar input e duração
        # Escolher o filtro adequado com base nos parâmetros utilizados
        if decode_params == self.PARAMS_FULL:
            decoder_filters = self.DECODER_FILTERS_FULL.replace("OUTPUT_PLACEHOLDER", self.pipe_name)
        else:
            decoder_filters = self.DECODER_FILTERS_BASIC.replace("OUTPUT_PLACEHOLDER", self.pipe_name)
            
        decoder_cmd += f" -i \"{input_file}\" -t {duration} {decoder_filters}"
        
        # Adicionar analiseduration e probesize para melhorar a detecção de formato para o arquivo interlaced2
        if input_file == self.FILE2:  # Para o arquivo interlaced2.mp4
            # Aumentar ainda mais os valores para analyzeduration e probesize
            decoder_cmd = decoder_cmd.replace(" -i \"", " -analyzeduration 200M -probesize 200M -i \"")
        
        try:
            # Iniciar o decoder
            print("Executando comando decoder:")
            print(decoder_cmd)
            
            decoder_launch_start = time.time()
            
            # Iniciar o decoder com captura de saída para depuração
            self.decoder_process = subprocess.Popen(
                decoder_cmd,
                shell=True,
                stdin=subprocess.PIPE,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
                bufsize=1,
                universal_newlines=True
            )
            print(f"Processo Decoder iniciado com PID: {self.decoder_process.pid}")
            
            # Marcar o início do processamento efetivo
            processing_start_time = time.time()
            
            if self.log_timing:
                launch_time = time.time() - decoder_launch_start
                print(f"  [TIMING] Tempo para iniciar o processo decoder: {launch_time:.2f} segundos")
            
            # Iniciar threads para capturar stdout e stderr
            self.start_process_log_threads(self.decoder_process, "Decoder")
            
            # Usar exatamente a duração original como tempo esperado, sem margens
            expected_wait_time = duration
            
            print(f"Tempo esperado de processamento: {duration:.2f}s (sem margem adicional)")
            
            # Registrar quando começamos a esperar
            wait_start = time.time()
            
            # Usar um loop baseado em tempo real em vez de contagem inteira
            expected_end_time = wait_start + expected_wait_time
            check_interval = min(0.1, duration/10)  # Intervalo de verificação adaptativo
            
            # Primeira fase: espera pelo tempo estimado (exatamente a duração)
            while time.time() < expected_end_time:
                # Verificar se o decoder terminou
                if self.decoder_process.poll() is not None:
                    # Processo terminou dentro do tempo esperado
                    termination_time = time.time()
                    natural_termination_time = termination_time - wait_start
                    print(f"Decoder terminou naturalmente após {natural_termination_time:.2f}s (dentro do tempo original)")
                    
                    # Marcar o fim do processamento para processos que terminaram naturalmente
                    processing_end_time = termination_time
                    break
                
                # Verificar se o encoder ainda está rodando
                if self.encoder_process.poll() is not None:
                    print("\033[91m[ALERTA]\033[0m: Encoder encerrou durante a execução do decoder!")
                    self.check_encoder()
                
                # Dormir por um intervalo curto antes da próxima verificação
                time.sleep(check_interval)
            
            # Se o processo ainda não terminou, apenas continuar aguardando
            if self.decoder_process.poll() is None:
                print(f"\033[93m[AVISO]\033[0m: Decoder está demorando mais que o tempo esperado. Aguardando término natural...")
                extended_wait_start = time.time()
                overtime = 0
                
                # Aguardar indefinidamente pelo término natural
                while self.decoder_process.poll() is None:
                    # Verificar a cada segundo se o processo terminou
                    self.decoder_process.poll()
                    
                    # Saímos do loop quando o processo terminar
                    if self.decoder_process.returncode is not None:
                        # Processo terminou naturalmente
                        termination_time = time.time()
                        total_wait_time = termination_time - wait_start
                        overtime = termination_time - expected_end_time
                        
                        print(f"\033[92m[INFO]\033[0m: Decoder terminou naturalmente após {total_wait_time:.2f}s " + 
                              f"({overtime:.2f}s além do tempo original)")
                        
                        # Marcar o fim do processamento
                        processing_end_time = termination_time
                        break
                    
                    # Atualizar a contagem de tempo extra a cada segundo
                    current_overtime = time.time() - expected_end_time
                    if int(current_overtime) > int(overtime):
                        overtime = current_overtime
                        print(f"Tempo extra de processamento: {overtime:.2f}s")
                    
                    # Verificar encoder a cada 5 segundos durante espera estendida
                    if int(time.time() - extended_wait_start) % 5 == 0:
                        if self.encoder_process.poll() is not None:
                            print("\033[91m[ALERTA]\033[0m: Encoder encerrou durante a espera estendida!")
                            self.check_encoder()
                    
                    time.sleep(0.5)  # Verificação mais frequente durante a espera estendida
            else:
                # Se já terminou durante a primeira fase
                if 'termination_time' not in locals():
                    termination_time = time.time()
                    processing_end_time = termination_time
            
            # Se o decoder não terminou no tempo esperado, apenas mostrar logs e continuar aguardando
            if self.decoder_process.poll() is None:
                print("\033[93m[AVISO]\033[0m: Decoder ainda não terminou mesmo após espera estendida. Continuando a aguardar...")
                
                # Mostrar os logs do decoder para ajudar no diagnóstico
                self.print_log_buffers("Decoder", True)
                
                # Continuamos esperando indefinidamente pelo término natural
                waiting_start = time.time()
                reported_minutes = 0
                
                while self.decoder_process.poll() is None:
                    # Verificar o encoder a cada 30 segundos
                    current_wait = time.time() - waiting_start
                    if int(current_wait) % 30 == 0:
                        if self.encoder_process.poll() is not None:
                            print("\033[91m[ALERTA]\033[0m: Encoder encerrou durante a espera prolongada!")
                            self.check_encoder()
                    
                    # Relatar a cada minuto para indicar que ainda estamos aguardando
                    wait_minutes = int(current_wait / 60)
                    if wait_minutes > reported_minutes:
                        reported_minutes = wait_minutes
                        print(f"Ainda aguardando término natural do decoder... ({reported_minutes} minutos)")
                    
                    time.sleep(1)
                
                # Se chegamos aqui, o decoder terminou naturalmente
                print(f"Decoder finalmente terminou naturalmente com código: {self.decoder_process.returncode}")
                processing_end_time = time.time()
            else:
                # Desativar as threads de log do decoder
                self.decoder_log_threads_active[0] = False
                
                # Registrar tempo total para processos que terminaram naturalmente
                if self.log_total_timing:
                    total_time_natural = time.time() - total_start_time
                    print(f"  [TOTAL TIMING] Tempo total do decoder com encerramento natural: {total_time_natural:.2f} segundos")
                
                print(f"Decoder terminou naturalmente com código: {self.decoder_process.returncode}")
                
                # Verificar se houve erro
                if self.decoder_process.returncode != 0:
                    # Mostrar logs armazenados para diagnóstico
                    self.print_log_buffers("Decoder", True)
            
            # Verificar encoder após decoder terminar
            self.check_encoder()
                
        except Exception as e:
            print(f"Erro ao executar decoder: {e}")
            import traceback
            traceback.print_exc()
            
            if self.log_timing:
                error_time = time.time() - process_start_time
                print(f"  [TIMING] Tempo até erro: {error_time:.2f} segundos")
        
        # Registrar tempo total
        total_process_time = time.time() - process_start_time
        
        # Registrar tempo apenas de processamento se solicitado
        if self.log_process_timing and processing_start_time and processing_end_time:
            pure_processing_time = processing_end_time - processing_start_time
            print(f"[PROCESS TIMING] Tempo de processamento puro do decoder: {pure_processing_time:.2f} segundos")
            if duration > 0:
                ratio = pure_processing_time / duration
                print(f"[PROCESS TIMING] Proporção tempo real/duração solicitada: {ratio:.2f}x")
                if ratio < 1.0:
                    print(f"[PROCESS TIMING] Processamento mais rápido que tempo real (good!)")
                else:
                    print(f"[PROCESS TIMING] Processamento mais lento que tempo real")
            
            # Comparar tempo de processamento puro com tempo total
            if self.log_total_timing:
                total_final_time = time.time() - total_start_time
                overhead_time = total_final_time - pure_processing_time
                overhead_percent = (overhead_time / total_final_time) * 100
                print(f"[TOTAL vs PROCESS] Overhead de {overhead_time:.2f} segundos ({overhead_percent:.1f}%) utilizado em operações não-processamento")
        
        if self.log_timing:
            print(f"[TIMING] Resumo de tempo para o decoder:")
            print(f"  Tempo planejado de processamento: {duration} segundos")
            if 'termination_time' in locals():
                actual_process_time = termination_time - process_start_time
                print(f"  Tempo real de processamento: {actual_process_time:.2f} segundos")
                print(f"  Diferença: {actual_process_time - duration:.2f} segundos")
            else:
                print(f"  Processo não terminou naturalmente dentro do tempo esperado")
                
            print(f"  Tempo total de execução: {total_process_time:.2f} segundos")
            
            # Informações do encoder se solicitado
            if self.log_encoder_timing and self.encoder_start_time:
                encoder_runtime = time.time() - self.encoder_start_time
                print(f"  [ENCODER TIMING] Tempo de execução do encoder até agora: {encoder_runtime:.2f} segundos")
        
        print("Pronto para o próximo decoder.")
        return True

    def run_sequence(self):
        """Executa a sequência completa de decoders"""
        sequence_start_time = time.time()
        
        try:
            # Iniciar o encoder persistente
            self.start_encoder()
            
            print("Encoder iniciado. Iniciando sequência de decoders...")
            
            # Loop principal
            cycle_count = 0
            while True:
                cycle_start_time = time.time()
                cycle_count += 1
                print(f"\n===== CICLO {cycle_count} =====\n")
                
                # Caso 1: Arquivo 1 com parâmetros completos (5 segundos a partir do início)
                self.run_decoder(self.FILE1, self.PARAMS_FULL, 5, 0)
                self.check_encoder()
                    
                # Caso 2: Arquivo 2 com parâmetros completos (3 segundos a partir do início)
                self.run_decoder(self.FILE2, self.PARAMS_FULL, 3, 0)
                self.check_encoder()
                
                # Caso 3: Arquivo 2 com parâmetros básicos (7 segundos a partir do início)
                self.run_decoder(self.FILE2, self.PARAMS_BASIC, 7, 0)
                self.check_encoder()
                
                # Caso 4: Arquivo 1 com parâmetros completos (0.5 segundos a partir do início)
                self.run_decoder(self.FILE1, self.PARAMS_FULL, 0.5, 0)
                self.check_encoder()
                
                # Caso 5: Arquivo 2 com parâmetros básicos (0.2 segundos a partir do início)
                self.run_decoder(self.FILE2, self.PARAMS_BASIC, 0.2, 0)
                self.check_encoder()
                
                # Caso original 4: Arquivo 1 com parâmetros completos (8 segundos a partir do início)
                self.run_decoder(self.FILE1, self.PARAMS_FULL, 8, 0)
                self.check_encoder()
                
                # Caso original 5: Arquivo 2 com parâmetros básicos (9 segundos a partir do início)
                self.run_decoder(self.FILE2, self.PARAMS_BASIC, 9, 0)
                self.check_encoder()
                
                cycle_time = time.time() - cycle_start_time
                print(f"\n===== FIM DO CICLO {cycle_count} =====")
                if self.log_timing:
                    print(f"[TIMING] Tempo total do ciclo {cycle_count}: {cycle_time:.2f} segundos")
                print("Ciclo completo, reiniciando a sequência...\n")
                
        except KeyboardInterrupt:
            print("Interrompido pelo usuário. Encerrando...")
        except Exception as e:
            print(f"Erro não tratado: {e}")
            import traceback
            traceback.print_exc()
        finally:
            self.cleanup()
            if self.log_timing:
                total_run_time = time.time() - sequence_start_time
                print(f"[TIMING] Tempo total de execução da sequência: {total_run_time:.2f} segundos")

def parse_arguments():
    parser = argparse.ArgumentParser(description='Pipeline FFmpeg para processamento de vídeo com CUDA')
    parser.add_argument('--log-timing', action='store_true', help='Habilita o registro de tempo de execução e encerramento dos decoders')
    parser.add_argument('--termination-wait', type=float, default=1.0, help='Tempo de espera (em segundos) após terminate() antes de tentar kill()')
    parser.add_argument('--log-encoder-timing', action='store_true', help='Habilita o registro de tempo de execução do encoder')
    parser.add_argument('--log-kill-timing', action='store_true', help='Habilita o registro de tempo entre o envio de kill() e o encerramento efetivo do processo')
    parser.add_argument('--log-terminate-timing', action='store_true', help='Habilita o registro de tempo entre o envio de terminate() e o encerramento efetivo do processo')
    parser.add_argument('--log-process-timing', action='store_true', help='Habilita o registro do tempo puro de processamento, sem incluir o tempo de terminate() ou kill()')
    parser.add_argument('--log-total-timing', action='store_true', help='Habilita o registro do tempo total de processamento, incluindo o tempo de terminate() e kill()')
    parser.add_argument('--debug-decoder', type=str, default='warning', help='Nível de log para o FFmpeg decoder (quiet, panic, fatal, error, warning, info, verbose, debug, trace)')
    parser.add_argument('--debug-encoder', type=str, default='warning', help='Nível de log para o FFmpeg encoder (quiet, panic, fatal, error, warning, info, verbose, debug, trace)')
    
    # Novas opções para logs de stdout
    parser.add_argument('--log-encoder-stdout', action='store_true', 
                        help='Habilita o registro em tempo real do stdout do encoder')
    parser.add_argument('--log-decoder-stdout', action='store_true', 
                        help='Habilita o registro em tempo real do stdout do decoder')
    parser.add_argument('--encoder-stdout-buffer', type=int, default=20, 
                        help='Número de linhas a manter no buffer de stdout do encoder')
    parser.add_argument('--decoder-stdout-buffer', type=int, default=20, 
                        help='Número de linhas a manter no buffer de stdout do decoder')
    parser.add_argument('--encoder-stderr-buffer', type=int, default=20, 
                        help='Número de linhas a manter no buffer de stderr do encoder')
    parser.add_argument('--decoder-stderr-buffer', type=int, default=20, 
                        help='Número de linhas a manter no buffer de stderr do decoder')
    
    return parser.parse_args()

if __name__ == "__main__":
    args = parse_arguments()
    pipeline = FFmpegPipeline(
        log_timing=args.log_timing,
        termination_wait=args.termination_wait,
        log_encoder_timing=args.log_encoder_timing,
        log_kill_timing=args.log_kill_timing,
        log_terminate_timing=args.log_terminate_timing,
        log_process_timing=args.log_process_timing,
        log_total_timing=args.log_total_timing,
        debug_decoder=args.debug_decoder,
        debug_encoder=args.debug_encoder,
        log_encoder_stdout=args.log_encoder_stdout,
        log_decoder_stdout=args.log_decoder_stdout,
        encoder_stdout_buffer_size=args.encoder_stdout_buffer,
        decoder_stdout_buffer_size=args.decoder_stdout_buffer,
        encoder_stderr_buffer_size=args.encoder_stderr_buffer,
        decoder_stderr_buffer_size=args.decoder_stderr_buffer
    )
    pipeline.run_sequence()