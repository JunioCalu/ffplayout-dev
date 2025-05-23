// INÍCIO METADADOS CLAUDE
// Esse é o arquivo '/engine/src/player/output/mod.rs que eu estou numerando como arquivo número 3'
// Informações adicionais:
// - Tamanho sem o cabeçalho Claude: 14157 bytes
// - Número de linhas sem o cabeçalho Claude: 380
// - Status Git: Modified (modificado mas não adicionado ao staging)
// - Branch atual: skip_clip_on_cuda_decoder_error
// - Última modificação: Wed Jan 29 10:25:15 2025 +0100
// - Possível propósito: Acesso a dados, Iterador, Processamento de mídia
//
// RESUMO ESTRUTURAL:
// --------------------------------------------------
// Estruturas (structs):
// - Nenhuma struct definido neste arquivo
//
// Enumerações (enums):
// - Nenhuma enum definido neste arquivo
//
// Traits:
// - Nenhuma trait definida neste arquivo
//
// Funções por categoria:
// Outras funções:
// - async fn play(
// - pub async fn player(manager: ChannelManager) -> Result<(), ServiceError> {
//
// Dependências (imports completos):
// - use std::{process::Stdio, sync::atomic::Ordering};
// - use log::*;
// - use tokio::{
//   io::{AsyncReadExt, AsyncWriteExt, BufReader, BufWriter},
//   process::{ChildStdin, Command},
//   };
// - use crate::player::{
//   controller::{ChannelManager, ProcessUnit::*},
//   input::{ingest_server, source_generator},
//   utils::{sec_to_time, stderr_reader, Media},
//   };
// - use crate::utils::{
//   config::OutputMode::*,
//   errors::ServiceError,
//   logging::{fmt_cmd, Target},
//   task_runner,
//   };
// - use crate::vec_strings;
// --------------------------------------------------
//
// Este comentário foi adicionado automaticamente para facilitar 
// o entendimento do contexto do projeto por sistemas de IA como o Claude.
// FIM METADADOS CLAUDE
//

use std::{process::Stdio, sync::atomic::Ordering};

use log::*;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt, BufReader, BufWriter},
    process::{ChildStdin, Command},
};

mod desktop;
mod hls;
mod null;
mod stream;

use crate::player::{
    controller::{ChannelManager, ProcessUnit::*},
    input::{ingest_server, source_generator},
    utils::{sec_to_time, stderr_reader, modify_decoder_cmd_for_recovery, Media},
};
use crate::utils::{
    config::OutputMode::*,
    errors::ServiceError,
    logging::{fmt_cmd, Target},
    task_runner,
};
use crate::vec_strings;

async fn play(
    manager: ChannelManager,
    mut enc_writer: BufWriter<ChildStdin>,
    ff_log_format: &str,
) -> Result<(), ServiceError> {
    let config = manager.config.lock().await.clone();
    let id = config.general.channel_id;
    let playlist_init = manager.list_init.clone();
    let is_alive = manager.is_alive.clone();
    let ingest_is_alive = manager.ingest_is_alive.clone();
    let mut buffer = vec![0u8; 64 * 1024];
    let mut live_on = false;

    // get source iterator
    let mut node_sources = source_generator(manager.clone()).await;

    while let Some(mut node) = node_sources.next().await {
        *manager.current_media.lock().await = Some(node.clone());
        let ignore_dec = config.logging.ignore_lines.clone();

        if !is_alive.load(Ordering::SeqCst) {
            debug!(target: Target::file_mail(), channel = id; "Playout is stopped, break out from source loop");
            break;
        }

        trace!("Decoder CMD: {:?}", node.cmd);

        let mut cmd = match node.cmd {
            Some(cmd) => cmd,
            None => break,
        };

        // ADICIONADO: Log para analisar a duração e comando
        debug!(target: Target::file_mail(), channel = id;
            "[play] Analisando comando de decoder - duração calculada: {}, duração do arquivo: {}, out: {}, seek: {}, -t no comando: {}, comando contém stream_loop? {}",
            node.out - node.seek,
            node.duration,
            node.out,
            node.seek,
            cmd.join(" ").contains(" -t "),
            cmd.join(" ").contains("-stream_loop")
        );

        // ADICIONADO: Log para analisar parâmetros exatos de -t
        if let Some(t_pos) = cmd.iter().position(|arg| arg == "-t") {
            if t_pos + 1 < cmd.len() {
                debug!(target: Target::file_mail(), channel = id;
                    "[play] Valor de -t no comando: {}", cmd[t_pos + 1]);
            }
        }

        // ADICIONADO: Log para ver o comando completo
        debug!(target: Target::file_mail(), channel = id;
            "[play] Comando completo: {}", cmd.join(" "));

        if node.skip {
            // skip is different from node.cmd = None.
            // This source is valid, but too short to play,
            // so better skip it and go to the next one.
            continue;
        }

        let c_index = if cfg!(debug_assertions) {
            format!(
                " ({}/{})",
                node.index.unwrap() + 1,
                manager.current_list.lock().await.len()
            )
        } else {
            String::new()
        };

        info!(target: Target::file_mail(), channel = id;
            "Play for <yellow>{}</>{c_index}: <b><magenta>{}  {}</></b>",
            sec_to_time(node.out - node.seek),
            node.source,
            node.audio
        );

        if config.task.enable {
            if config.task.path.is_file() {
                let channel_mgr_3 = manager.clone();

                tokio::spawn(task_runner::run(channel_mgr_3));
            } else {
                error!(target: Target::file_mail(), channel = id;
                    "<bright-blue>{:?}</> executable not exists!",
                    config.task.path
                );
            }
        }

        let mut dec_cmd = vec_strings!["-hide_banner", "-nostats", "-v", &ff_log_format];

        let skip_advanced = manager.recovery_state.is_file_in_retry(&node.source).await;

        // Adicionar parâmetros avançados somente se não for segunda tentativa
        if !skip_advanced {
            if let Some(decoder_input_cmd) = &config.advanced.decoder.input_cmd {
                dec_cmd.append(&mut decoder_input_cmd.clone());
            }
        } else {
            // Log informativo
            info!(target: Target::file_mail(), channel = id;
                "[play] Tentando decodificação sem recursos avançados para: {}", node.source);
            
            // Se estamos em retry, usar decodificação por hardware se possível
            if let Some(mut filter) = node.filter.clone() {
                if let Err(e) = modify_decoder_cmd_for_recovery(&mut dec_cmd, &mut filter, id).await {
                    warn!(target: Target::file_mail(), channel = id;
                        "[play] Erro ao configurar decodificação por hardware para recuperação: {}", e);
                } else {
                    // Atualizar o filtro no nó
                    //manager.stop(Encoder).await;
                    //manager.stop(Decoder).await;
                    node.filter = Some(filter);

                }
            }
        }

        dec_cmd.append(&mut cmd);

        if let Some(mut filter) = node.filter {
            debug!(target: Target::file_mail(), channel = id;
                "[play] Adicionando filtros - Video chain: {}, Audio chain: {}", 
                filter.video_chain, filter.audio_chain);
                
            debug!(target: Target::file_mail(), channel = id;
                "[play] Links de saída de vídeo: {:?}", filter.video_out_link);
                
            debug!(target: Target::file_mail(), channel = id;
                "[play] Comando de filtros completo: {:?}", filter.cmd());
                
            debug!(target: Target::file_mail(), channel = id;
                "[play] Comando de mapeamento: {:?}", filter.map());
                
            dec_cmd.append(&mut filter.cmd());
            dec_cmd.append(&mut filter.map());
        }
        
        if config.processing.vtt_enable && dec_cmd.iter().any(|s| s.ends_with(".vtt")) {
            let i = dec_cmd
                .iter()
                .filter(|&n| n == "-i")
                .count()
                .saturating_sub(1);

            dec_cmd.append(&mut vec_strings!("-map", format!("{i}:s"), "-c:s", "copy"));
        }

        if let Some(cmd) = &config.processing.cmd {
            dec_cmd.extend_from_slice(cmd);
        }

        debug!(target: Target::file_mail(), channel = id;
            "Decoder CMD: <bright-blue>ffmpeg {}</>",
            fmt_cmd(&dec_cmd)
        );

        // create ffmpeg decoder instance, for reading the input files
        let mut dec_proc = Command::new("ffmpeg")
            .args(dec_cmd)
            .kill_on_drop(true)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        let mut decoder_stdout = dec_proc.stdout.take().unwrap();
        let dec_err = BufReader::new(dec_proc.stderr.take().unwrap());

        *manager.clone().decoder.lock().await = Some(dec_proc);

        let error_decoder_task = tokio::spawn(stderr_reader(dec_err, ignore_dec, Decoder, id));

        loop {
            if ingest_is_alive.load(Ordering::SeqCst) {
                // read from ingest server instance
                if !live_on {
                    info!(target: Target::file_mail(), channel = id; "Switch from {} to live ingest", config.processing.mode);
                    playlist_init.store(true, Ordering::SeqCst);

                    manager.stop(Decoder).await;
                    live_on = true;
                }

                let mut ingest_stdout_guard = manager.ingest_stdout.lock().await;
                if let Some(ref mut ingest_stdout) = *ingest_stdout_guard {
                    let num = ingest_stdout.read(&mut buffer[..]).await?;

                    if num == 0 {
                        continue;
                    }

                    enc_writer.write_all(&buffer[..num]).await?;
                }
            } else {
                // read from decoder instance
                if live_on {
                    info!(target: Target::file_mail(), channel = id; "Switch from live ingest to {}", config.processing.mode);

                    live_on = false;
                    break;
                }

                let num = decoder_stdout.read(&mut buffer[..]).await?;

                if num == 0 {
                    break;
                }

                enc_writer.write_all(&buffer[..num]).await?;
            }
        }

        drop(decoder_stdout);

        manager.wait(Decoder).await;
        
        match error_decoder_task.await {
            Ok(Ok(())) => {
                debug!(target: Target::file_mail(), channel = id;
                    "[play] Decodificação bem-sucedida para: {}", node.source);
                    
                // Se era uma segunda tentativa, remover da lista de retry
                if manager.recovery_state.is_file_in_retry(&node.source).await {
                    manager.recovery_state.remove_from_retry(&node.source).await;
                    debug!(target: Target::file_mail(), channel = id;
                        "[play] Sucesso na segunda tentativa sem decodificação avançada: {}", node.source);
                }
                
                // Limpar modo de recuperação se estiver ativo
                if manager.recovery_state.is_in_recovery_mode() {
                    manager.recovery_state.exit_recovery_mode();
                }
            },
            Ok(Err(ServiceError::DecodingError(msg))) => {
                // Parar o processo atual
                //manager.stop(Encoder).await;
                // Verificar se é a primeira falha deste arquivo
                if !manager.recovery_state.is_file_in_retry(&node.source).await {
                    manager.stop(Decoder).await;
                    // Primeira tentativa falhou - tentar novamente sem decodificação avançada
                    debug!(target: Target::file_mail(), channel = id;
                        "[play] Primeiro erro de decodificação - tentando sem recursos avançados: {}: {}", 
                        node.source, msg);

                    // Obter o índice atual
                    let current_index = manager.current_index.load(Ordering::SeqCst);
                    if current_index > 0 {
                        // Decrementar para que o próximo next() retorne o mesmo arquivo
                        manager.current_index.store(current_index - 1, Ordering::SeqCst);
                    }
                    
                    // Marcar para retry e definir modo especial de recuperação
                    manager.recovery_state.mark_file_for_retry(node.source.clone()).await;
                    
                    // Não alterar o índice - mesmo arquivo será tentado novamente
                    // com configurações diferentes
                    continue;
                } 
                // else {
                //     // Segunda tentativa falhou - substituir por filler
                //     manager.stop(Decoder).await;
                //     error!(target: Target::file_mail(), channel = id; 
                //         "[play] Falha na segunda tentativa - arquivo incompatível: {}: {}", 
                //         node.source, msg);
                    
                //     // Entrar em modo de recuperação completo
                //     manager.recovery_state.enter_recovery_mode();
                //     manager.recovery_state.mark_file_incompatible(node.source.clone()).await;
                    
                //     // Criar filler substituto com mesma duração
                //     let mut filler = Media::new(
                //         node.index.unwrap_or(0), 
                //         "", 
                //         false
                //     ).await;
                    
                //     // Preservar parâmetros de tempo do original
                //     filler.begin = node.begin;
                //     filler.seek = node.seek;
                //     filler.out = node.out;
                //     filler.duration = node.duration;
                    
                //     // Substituir o nó atual pelo filler
                //     *manager.current_media.lock().await = Some(filler.clone());
                    
                //     debug!(target: Target::file_mail(), channel = id;
                //         "[play] Substituindo por filler com duração: {} segundos", 
                //         filler.out - filler.seek);
                    
                //     // Continuar com o filler
                //     continue;
                // }
            },
            Ok(Err(e)) => {
                debug!(target: Target::file_mail(), channel = id;
                    "[play] Erro diferente de DecodingError: {:?}", e);
                return Err(e);
            },
            Err(e) => {
                debug!(target: Target::file_mail(), channel = id;
                    "[play] Erro na task de decodificação: {:?}", e);
                return Err(ServiceError::InternalServerError);
            },
        }
    }

    Ok(())
}

/// Player
///
/// Here we create the input file loop, from playlist, or folder source.
/// Then we read the stdout from the reader ffmpeg instance
/// and write it to the stdin from the streamer ffmpeg instance.
/// If it is configured we also fire up a ffmpeg ingest server instance,
/// for getting live feeds.
/// When a live ingest arrive, it stops the current playing and switch to the live source.
/// When ingest stops, it switch back to playlist/folder mode.
pub async fn player(manager: ChannelManager) -> Result<(), ServiceError> {
    let config = manager.config.lock().await.clone();
    let config_clone = config.clone();
    let ff_log_format = format!("level+{}", config.logging.ffmpeg_level.to_lowercase());
    let ignore_enc = config.logging.ignore_lines.clone();
    let channel_id = config.general.channel_id;

    if config.output.mode == HLS {
        hls::writer(&manager, &ff_log_format).await?;
        manager.stop_all(false).await;

        return Ok(());
    }

    // get ffmpeg output instance
    let mut enc_proc = match config.output.mode {
        Desktop => desktop::output(&config, &ff_log_format).await?,
        Null => null::output(&config, &ff_log_format).await?,
        Stream => stream::output(&config, &ff_log_format).await?,
        _ => panic!("Output mode doesn't exists!"),
    };

    let enc_err = BufReader::new(enc_proc.stderr.take().unwrap());
    let enc_writer = BufWriter::new(enc_proc.stdin.take().unwrap());

    *manager.encoder.lock().await = Some(enc_proc);
    let mgr_clone2 = manager.clone();

    // spawn a task to log ffmpeg output error messages
    let handle_enc_stderr = tokio::spawn(stderr_reader(enc_err, ignore_enc, Encoder, channel_id));

    // spawn a task for ffmpeg ingest server and create a channel for package sending
    let handle_ingest = if config.ingest.enable {
        Some(tokio::spawn(ingest_server(config_clone, mgr_clone2)))
    } else {
        None
    };

    tokio::select! {
        result = handle_enc_stderr => {
            result??;
        }

        result = async {
            if let Some(f) = handle_ingest {
                f.await?
            } else {
                Ok(())
            }
        }, if handle_ingest.is_some() => {
            result?;
        }

        result = play(manager.clone(), enc_writer, &ff_log_format) => {
            result?;
        }
    }

    trace!("Out of source loop");

    Ok(())
}
