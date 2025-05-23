// INÍCIO METADADOS CLAUDE
// Esse é o arquivo '/projeto_ffplayout_flat/playlist.rs que eu estou numerando como arquivo número 1'
// Informações adicionais:
// - Tamanho sem o cabeçalho Claude: 35904 bytes
// - Número de linhas sem o cabeçalho Claude: 904
// - Status Git: Untracked (novo arquivo não rastreado)
// - Branch atual: skip_clip_on_cuda_decoder_error
// - Última modificação: Novo arquivo
// - Possível propósito: Acesso a dados, Processamento de mídia
//
// Documentação da struct:
// Struct for current playlist.
// 
// Here we prepare the init clip and build a iterator where we pull our clips.
// #[derive(Debug)]
//
// RESUMO ESTRUTURAL:
// --------------------------------------------------
// Estruturas (structs):
// - pub struct CurrentProgram {
//
// Enumerações (enums):
// - Nenhuma enum definido neste arquivo
//
// Traits:
// - Nenhuma trait definida neste arquivo
//
// Funções por categoria:
// Funções de inicialização:
// - pub async fn new(manager: ChannelManager) -> Self {
// - async fn get_current_clip(&mut self) {
// - async fn init_clip(&mut self) -> bool {
//
// Funções de gerenciamento de tempo:
// - async fn set_status(&mut self, date: &Option<String>, shift: f64) {
// - fn get_current_time(&mut self) -> f64 {
// - async fn recalculate_begin(&mut self, extend: bool) {
//
// Funções de gerenciamento de mídia:
// - async fn fill_end(&mut self, total_delta: f64) {
// - pub async fn gen_source(&mut self, mut node: Media, last_index: usize) {
// - async fn duplicate_for_seek_and_loop(&mut self, node: &mut Media) {
//
// Funções de iteração/controle:
// - async fn load_or_update_playlist(&mut self, seek: bool) {
// - async fn check_for_playlist(&mut self, seek: bool) -> bool {
// - pub async fn next(&mut self) -> Option<Media> {
//
// Outras funções:
// - async fn last_next_ad(&mut self, node: &mut Media) {
// - async fn handle_list_init(&mut self, mut node: Media, last_index: usize) {
// - async fn handle_list_end(&mut self, mut node: Media, total_delta: f64, last_index: usize) {
// - async fn timed_source(&mut self, mut node: Media, last: bool, last_index: usize) -> Option<()> {
//
// Dependências (imports completos):
// - use std::{
//   path::Path,
//   sync::{
//   atomic::{AtomicBool, Ordering},
//   Arc,
//   },
//   };
// - use log::*;
// - use crate::db::handles;
// - use crate::player::{
//   controller::ChannelManager,
//   utils::{
//   gen_dummy, get_delta, is_close, is_remote,
//   json_serializer::{read_json, set_defaults},
//   loop_filler, loop_image, modified_time,
//   probe::MediaProbe,
//   seek_and_length, time_in_seconds, JsonPlaylist, Media,
//   },
//   };
// - use crate::utils::{
//   config::{PlayoutConfig, IMAGE_FORMAT},
//   logging::Target,
//   recovery::{
//   exit_recovery_mode,
//   is_in_recovery_mode,
//   },
//   };
// --------------------------------------------------
//
// Este comentário foi adicionado automaticamente para facilitar 
// o entendimento do contexto do projeto por sistemas de IA como o Claude.
// FIM METADADOS CLAUDE
//

use std::{
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use log::*;

use crate::db::handles;
use crate::player::{
    controller::ChannelManager,
    utils::{
        gen_dummy, get_delta, is_close, is_remote,
        json_serializer::{read_json, set_defaults},
        loop_filler, loop_image, modified_time,
        probe::MediaProbe,
        seek_and_length, time_in_seconds, JsonPlaylist, Media,
    },
};
use crate::utils::{
    config::{PlayoutConfig, IMAGE_FORMAT},
    logging::Target,
    recovery::{
        exit_recovery_mode, 
        is_in_recovery_mode,
    },
};


/// Struct for current playlist.
///
/// Here we prepare the init clip and build a iterator where we pull our clips.
#[derive(Debug)]
pub struct CurrentProgram {
    channel_id: i32,
    config: PlayoutConfig,
    manager: ChannelManager,
    start_sec: f64,
    length_sec: f64,
    json_playlist: JsonPlaylist,
    current_node: Media,
    is_alive: Arc<AtomicBool>,
    last_json_path: Option<String>,
    last_node_ad: bool,
}

/// Prepare a playlist iterator.
impl CurrentProgram {
    pub async fn new(manager: ChannelManager) -> Self {
        let config = manager.config.lock().await.clone();
        let is_alive = manager.is_alive.clone();

        Self {
            channel_id: config.general.channel_id,
            config: config.clone(),
            manager,
            start_sec: config.playlist.start_sec.unwrap(),
            length_sec: config.playlist.length_sec.unwrap(),
            json_playlist: JsonPlaylist::new(
                "1970-01-01".to_string(),
                config.playlist.start_sec.unwrap(),
            ),
            current_node: Media::default(),
            is_alive,
            last_json_path: None,
            last_node_ad: false,
        }
    }

    // Check if there is no current playlist or file got updated,
    // and when is so load/reload it.
    async fn load_or_update_playlist(&mut self, seek: bool) {
        let mut get_current = false;
        let mut reload = false;

        if let Some(path) = self.json_playlist.path.clone() {
            if (Path::new(&path).is_file() || is_remote(&path))
                && self.json_playlist.modified != modified_time(&path).await
            {
                info!(target: Target::file_mail(), channel = self.channel_id; "Reload playlist <b><magenta>{path}</></b>");
                self.manager.list_init.store(true, Ordering::SeqCst);
                get_current = true;
                reload = true;
            }
        } else {
            get_current = true;
        }

        if get_current {
            self.json_playlist = read_json(
                &mut self.config,
                self.manager.current_list.clone(),
                self.json_playlist.path.clone(),
                self.is_alive.clone(),
                seek,
                false,
            )
            .await;

            if !reload {
                if let Some(file) = &self.json_playlist.path {
                    info!(target: Target::file_mail(), channel = self.channel_id; "Read playlist: <b><magenta>{file}</></b>");
                }

                if *self
                    .manager
                    .channel
                    .lock()
                    .await
                    .last_date
                    .clone()
                    .unwrap_or_default()
                    != self.json_playlist.date
                {
                    self.set_status(&Some(self.json_playlist.date.clone()), 0.0)
                        .await;
                }

                self.manager
                    .current_date
                    .lock()
                    .await
                    .clone_from(&self.json_playlist.date);
            }

            self.manager
                .current_list
                .lock()
                .await
                .clone_from(&self.json_playlist.program);

            if self.json_playlist.path.is_none() {
                trace!("missing playlist");

                self.current_node = Media::default();
                self.manager.list_init.store(true, Ordering::SeqCst);
                self.manager.current_index.store(0, Ordering::SeqCst);
            }
        }
    }

    // Check if day is past and it is time for a new playlist.
    async fn check_for_playlist(&mut self, seek: bool) -> bool {
        let (delta, total_delta) = get_delta(
            &self.config,
            &time_in_seconds(&self.config.channel.timezone),
        );
        let mut next = false;

        let mut duration = self.current_node.out;

        let node_index = self.current_node.index.unwrap_or_default();

        let mut next_start = self.current_node.begin.unwrap_or_default() - self.start_sec + delta;
        let last_index = self.manager.current_list.lock().await.len() - 1;

        if node_index > 0 && node_index == last_index {
            if self.current_node.duration >= self.current_node.out {
                duration = self.current_node.duration;
            }

            next_start += self.config.general.stop_threshold;
        }

        next_start += duration;

        trace!(
            "delta: {delta} | total_delta: {total_delta}, index: {node_index}, last index: {last_index}, init: {} \n        next_start: {next_start} | length_sec: {} | source {}",
            self.manager.list_init.load(Ordering::SeqCst),
            self.length_sec,
            self.current_node.source
        );

        // Check if we over the target length or we are close to it, if so we load the next playlist.
        if !self.config.playlist.infinit
            && (next_start >= self.length_sec
                || is_close(total_delta, 0.0, 2.0)
                || is_close(total_delta, self.length_sec, 2.0))
        {
            trace!("get next day");
            next = true;

            self.json_playlist = read_json(
                &mut self.config,
                self.manager.current_list.clone(),
                None,
                self.is_alive.clone(),
                false,
                true,
            )
            .await;

            if let Some(file) = &self.json_playlist.path {
                info!(target: Target::file_mail(), channel = self.channel_id; "Read next playlist: <b><magenta>{file}</></b>");
            }

            self.manager.list_init.store(false, Ordering::SeqCst);
            self.set_status(&Some(self.json_playlist.date.clone()), 0.0)
                .await;

            self.manager
                .current_list
                .lock()
                .await
                .clone_from(&self.json_playlist.program);
            self.manager.current_index.store(0, Ordering::SeqCst);
        } else {
            self.load_or_update_playlist(seek).await;
        }

        next
    }

    async fn set_status(&mut self, date: &Option<String>, shift: f64) {
        let time_shift = if is_in_recovery_mode() {
            // Preferimos NÃO IGNORAR completamente o time_shift durante recuperação,
            // mas SIM evitar ADICIONAR NOVOS ajustes durante esse período.
            // Isso manterá a sincronização atual sem pioras.
            self.manager.channel.lock().await.time_shift  // Manter o time_shift atual
        } else {
            // Processamento normal, permitindo ajustes
            shift
        };
        
        // Nota: não precisamos de time_shift mutável agora, pois seu valor já foi determinado acima
        let final_shift = if self.manager.channel.lock().await.last_date != *date
            && self.manager.channel.lock().await.time_shift != 0.0
        {
            info!(target: Target::file_mail(), channel = self.channel_id; "Reset playout status");
            0.0
        } else {
            time_shift
        };
    
        if let Some(d) = date {
            self.manager.current_date.lock().await.clone_from(d);
            self.manager.channel.lock().await.last_date.clone_from(date);
        }
    
        self.manager.channel.lock().await.time_shift = final_shift;
    
        if let Err(e) =
            handles::update_stat(&self.manager.db_pool, self.channel_id, date, final_shift).await
        {
            error!(target: Target::file_mail(), channel = self.channel_id; "Unable to write status: {e}");
        };
    }

    // Check if last and/or next clip is a advertisement.
    async fn last_next_ad(&mut self, node: &mut Media) {
        let index = self.manager.current_index.load(Ordering::SeqCst);
        let current_list = self.manager.current_list.lock().await;

        if index + 1 < current_list.len() && &current_list[index + 1].category == "advertisement" {
            node.next_ad = true;
        }

        if index > 0
            && index < current_list.len()
            && &current_list[index - 1].category == "advertisement"
        {
            node.last_ad = true;
        }
    }

    // Get current time and when we are before start time,
    // we add full seconds of a day to it.
    fn get_current_time(&mut self) -> f64 {
        let mut time_sec = time_in_seconds(&self.config.channel.timezone);

        if time_sec < self.start_sec {
            time_sec += 86400.0; // self.config.playlist.length_sec.unwrap();
        }

        time_sec
    }

    // On init or reload we need to seek for the current clip.
    async fn get_current_clip(&mut self) {
        let mut time_sec = self.get_current_time();
        
        // Aplicando a verificação de modo de recuperação aqui
        let shift = self.manager.channel.lock().await.time_shift;
    
        if shift != 0.0 {
            info!(target: Target::file_mail(), channel = self.channel_id; "Shift playlist start for <yellow>{shift:.3}</> seconds");
            time_sec += shift;
        }
    
        if self.config.playlist.infinit
            && self.json_playlist.length.unwrap() < 86400.0
            && time_sec > self.json_playlist.length.unwrap() + self.start_sec
        {
            self.recalculate_begin(true).await;
        }
    
        for (i, item) in self.manager.current_list.lock().await.iter().enumerate() {
            if item.begin.unwrap() + item.out - item.seek > time_sec {
                self.manager.list_init.store(false, Ordering::SeqCst);
                self.manager.current_index.store(i, Ordering::SeqCst);
    
                break;
            }
        }
    }

    // Prepare init clip.
    async fn init_clip(&mut self) -> bool {
        trace!("init_clip");
        // debug!(target: Target::file_mail(), channel = id;
        // "[play] Retomando operação após pausa - arquivo será substituído por filler");
        self.get_current_clip().await;
        let mut is_filler = false;
    
        if !self.manager.list_init.load(Ordering::SeqCst) {
            let time_sec_opt = self.get_current_time();
            let index = self.manager.current_index.load(Ordering::SeqCst);
    
            debug!(target: Target::file_mail(), channel = self.channel_id;
                "[init_clip] Processando index: {} no tempo: {}", index, time_sec_opt);
    
            let nodes = self.manager.current_list.lock().await;
            let last_index = nodes.len() - 1;
    
            // de-instance node to preserve original values in list
            let mut node_clone = nodes[index].clone();
    
            // Important! When no manual drop is happen here, lock is still active in handle_list_init
            drop(nodes);
    
            trace!("Clip from init: {}", node_clone.source);
    
            if is_in_recovery_mode() {
                debug!(target: Target::file_mail(), channel = self.channel_id;
                    "[init_clip] Substituindo arquivo incompatível: {} por filler", 
                    node_clone.source);

                let mut media = Media::new(index, "", false, Some(self.manager.clone().into())).await;
                            
                // Copiar EXATAMENTE todos os atributos temporais
                media.probe = None;
                media.begin = node_clone.begin;
                media.seek = node_clone.seek;
                media.out = node_clone.out;
                media.duration = node_clone.duration;

                debug!(target: Target::file_mail(), channel = self.channel_id;
                    "[init_clip] Atributos temporais do original: begin={}, seek={}, duration={}, out={}",
                    node_clone.begin.unwrap_or_default(), node_clone.seek, node_clone.duration, node_clone.out);
                
                debug!(target: Target::file_mail(), channel = self.channel_id;
                    "[init_clip] Criando filler com duração exata de {} segundos e begin: {}", 
                    media.out - media.seek, media.begin.unwrap_or_default());
                
                // Garantir que list_init permaneça false
                self.manager.list_init.store(false, Ordering::SeqCst);
                
                let last_index = self.manager.current_list.lock().await.len() - 1;
                self.gen_source(media, last_index).await;

                exit_recovery_mode();
                
                return true;
            }
    

            let time_value = time_sec_opt;
            node_clone.seek += time_value - (node_clone.begin.unwrap() - self.manager.channel.lock().await.time_shift);
            
            debug!(target: Target::file_mail(), channel = self.channel_id;
                "[init_clip] Seek ajustado para: {}", node_clone.seek);
                
            self.last_next_ad(&mut node_clone).await;
            self.manager.current_index.fetch_add(1, Ordering::SeqCst);
            self.handle_list_init(node_clone, last_index).await;
    
            // Em vez da condição atual, usar algo como:
            if self.current_node.source.is_empty() && self.current_node.duration > 0.0  // Filler de substituição
                || self.current_node.source.contains("color=c=#121212")  // Filler de cor
                || self.current_node.source == self.config.storage.filler_path.to_string_lossy()  // Filler específico
                || self.current_node.source.contains("/filler/") // Diretório de fillers
                {
                    debug!(target: Target::file_mail(), channel = self.channel_id;
                    "[init_clip] Detectado filler: {}", self.current_node.source);
                    is_filler = true;
                }
        } 
        is_filler
    }

    async fn fill_end(&mut self, total_delta: f64) {
        // Fill end from playlist
        let index = self.manager.current_index.load(Ordering::SeqCst);
        let mut media = Media::new(index, "", false, Some(self.manager.clone().into())).await;
        media.begin = Some(time_in_seconds(&self.config.channel.timezone));
        media.duration = total_delta;
        media.out = total_delta;

        self.last_next_ad(&mut media).await;
        self.gen_source(media, 0).await;

        self.manager
            .current_list
            .lock()
            .await
            .push(self.current_node.clone());

        self.current_node.last_ad = self.last_node_ad;
        self.current_node
            .add_filter(&self.config, &self.manager.filter_chain)
            .await;

        self.manager.current_index.fetch_add(1, Ordering::SeqCst);
    }

    async fn recalculate_begin(&mut self, extend: bool) {
        debug!(target: Target::file_mail(), channel = self.channel_id; "Infinit playlist reaches end, recalculate clip begins. Extend: <yellow>{extend}</>");

        let mut time_sec = time_in_seconds(&self.config.channel.timezone);

        if extend {
            // Calculate the elapsed time since the playlist start
            let elapsed_sec = if time_sec >= self.start_sec {
                time_sec - self.start_sec
            } else {
                time_sec + 86400.0 - self.start_sec
            };

            // Time passed within the current playlist loop
            let time_in_current_loop = elapsed_sec % self.json_playlist.length.unwrap();

            // Adjust the start time so that the playlist starts at the correct point in time
            time_sec -= time_in_current_loop;
        }

        self.json_playlist.start_sec = Some(time_sec);
        set_defaults(&mut self.json_playlist);
        self.manager
            .current_list
            .lock()
            .await
            .clone_from(&self.json_playlist.program);
    }

    /// Handle init clip, but this clip can be the last one in playlist,
    /// this we have to figure out and calculate the right length.
    async fn handle_list_init(&mut self, mut node: Media, last_index: usize) {
        debug!(target: Target::file_mail(), channel = self.channel_id; "Playlist init");
        let (_, total_delta) = get_delta(&self.config, &node.begin.unwrap());

        if !self.config.playlist.infinit && node.out - node.seek > total_delta {
            node.out = total_delta + node.seek;
        }

        self.gen_source(node, last_index).await;
    }

    /// when we come to last clip in playlist,
    /// or when we reached total playtime,
    /// we end up here
    async fn handle_list_end(&mut self, mut node: Media, total_delta: f64, last_index: usize) {
        debug!(target: Target::file_mail(), channel = self.channel_id; "Handle last clip from day");

        let out = if node.seek > 0.0 {
            node.seek + total_delta
        } else {
            if node.duration > total_delta {
                warn!(target: Target::file_mail(), channel = self.channel_id; "Adjust clip duration to: <yellow>{total_delta:.2}</>");
            }

            total_delta
        };

        if (node.duration > total_delta || node.out > total_delta)
            && (node.duration - node.seek >= total_delta || node.out - node.seek >= total_delta)
            && total_delta > 1.0
        {
            node.out = out;
        } else if total_delta > node.duration {
            warn!(target: Target::file_mail(), channel = self.channel_id; "Playlist is not long enough: <yellow>{total_delta:.2}</> seconds needed");
        }

        node.skip = false;

        self.gen_source(node, last_index).await;
    }

    /// Prepare input clip:
    ///
    /// - check begin and length from clip
    /// - return clip only if we are in 24 hours time range
    async fn timed_source(&mut self, mut node: Media, last: bool, last_index: usize) -> Option<()> {
        let time_shift = self.manager.channel.lock().await.time_shift;
        let current_date = self.manager.current_date.lock().await.clone();
        let last_date = self.manager.channel.lock().await.last_date.clone();
        let (delta, total_delta) = get_delta(&self.config, &node.begin.unwrap());
        let mut shifted_delta= delta;
        
        let mut shifted_msg = String::new();
        trace!(
            "Node - begin: {} | source: {}",
            node.begin.unwrap(),
            node.source
        );
        trace!(
            "timed source is last: {last} | current_date: {current_date} | last_date: {last_date:?} | time_shift: {time_shift}"
        );

        if self.config.playlist.length.contains(':') {
            if Some(current_date) == last_date && time_shift != 0.0 {
                shifted_delta = delta - time_shift;
                shifted_msg = format!("shifted: <yellow>{delta:.3}</>");
            }

            debug!(target: Target::file_mail(), channel = self.channel_id; "Delta: <yellow>{shifted_delta:.3}</> {shifted_msg}");

            if self.config.general.stop_threshold > 0.0
                && shifted_delta.abs() > self.config.general.stop_threshold
            {
                // Handle summer/winter time changes.
                if is_close(
                    shifted_delta.abs(),
                    3600.0,
                    self.config.general.stop_threshold,
                ) {
                    warn!(
                        target: Target::file_mail(), channel = self.channel_id;
                        "A time change seemed to have occurred, apply time shift: <yellow>{shifted_delta:.3}</> seconds."
                    );

                    self.set_status(&None, time_shift + shifted_delta).await;
                } else if self.manager.is_alive.load(Ordering::SeqCst) {
                    error!(target: Target::file_mail(), channel = self.channel_id; "Clip begin out of sync for <yellow>{delta:.3}</> seconds.");

                    node.cmd = None;
                    self.current_node = node;
                    return Some(()); // Modificado para retornar Some(())
                }
            }
        }

        if (total_delta > node.out - node.seek && !last)
            || node.index.unwrap() < 2
            || !self.config.playlist.length.contains(':')
            || self.config.playlist.infinit
        {
            // when we are in the 24 hour range, get the clip
            self.gen_source(node, last_index).await;
            return Some(()); // Modificado para retornar Some(())
        } else if total_delta <= 0.0 {
            info!(target: Target::file_mail(), channel = self.channel_id; "Begin is over play time, skip: {}", node.source);
        } else if total_delta < node.duration - node.seek || last {
            self.handle_list_end(node, total_delta, last_index).await;
            return Some(()); // Modificado para retornar Some(())
        }

        node.skip = true;
        self.current_node = node;
        
        Some(()) // Retorno ao final da função
    }

    /// Generate the source CMD, or when clip not exist, get a dummy.
    pub async fn gen_source(&mut self, mut node: Media, last_index: usize) {
        let node_index = node.index.unwrap_or_default();
        let original_duration = node.out - node.seek;  // Preservar a duração original desejada
    
        if node.duration > 0.0 && original_duration < 1.0 {
            warn!(
                target: Target::file_mail(), channel = self.channel_id;
                "Skip clip that is less then one second long (<yellow>{original_duration:.3}</>)."
            );
    
            // INFO:
            // This part has been changed twice, the last time in January 2024.
            // Better case is that it skips the short clip, especially when reloading a playlist,
            // it prevents the last clip from playing again for 1.2 seconds.
            // But the behavior needs to be observed for a longer time to be sure that it has no side effects.
    
            // duration = 1.2;
    
            // if node.seek > 1.0 {
            //     node.seek -= 1.2;
            // } else {
            //     node.out = 1.2;
            // }
            node.skip = true;
        }
    
        trace!("Clip length: {original_duration}, duration: {}", node.duration);
    
        if node.probe.is_none() && !node.source.is_empty() {
            if let Err(e) = node.add_probe(true).await {
                trace!("{e:?}");
            };
        }
    
        // separate if condition, because of node.add_probe() in last condition
        if node.probe.is_some() {
            if node
                .source
                .rsplit_once('.')
                .map(|(_, e)| e.to_lowercase())
                .filter(|c| IMAGE_FORMAT.contains(&c.as_str()))
                .is_some()
            {
                node.cmd = Some(loop_image(&self.config, &node));
            } else {
                if node.seek > 0.0 && node.out > node.duration {
                    warn!(target: Target::file_mail(), channel = self.channel_id; "Clip loops and has seek value: duplicate clip to separate loop and seek.");
                    self.duplicate_for_seek_and_loop(&mut node).await;
                }
    
                node.cmd = Some(seek_and_length(&self.config, &mut node));
            }
        } else {
            trace!("clip index: {node_index} | last index: {last_index}");
    
            // Last index is the index from the last item from the node list.
            if node_index < last_index {
                error!(target: Target::file_mail(), channel = self.channel_id; "Source not found: <b><magenta>{}</></b>", node.source);
            }
    
            let fillers = self.manager.filler_list.lock().await;
    
            if self.config.storage.filler_path.is_dir() && !fillers.is_empty() {
                let mut index = self.manager.filler_index.fetch_add(1, Ordering::SeqCst);
    
                if index > fillers.len() - 1 {
                    index = 0;
                }
    
                let mut filler_media = fillers[index].clone();
                
                // Log para diagnóstico
                debug!(target: Target::file_mail(), channel = self.channel_id;
                    "[gen_source] Filler selecionado - source: {}, duration: {}, duração necessária: {}", 
                    filler_media.source, filler_media.duration, original_duration);
    
                trace!("take filler: {}", filler_media.source);
    
                if index == fillers.len() - 1 {
                    // reset index for next round
                    self.manager.filler_index.store(0, Ordering::SeqCst);
                }
    
                if filler_media.probe.is_none() {
                    if let Err(e) = filler_media.add_probe(false).await {
                        error!(target: Target::file_mail(), channel = self.channel_id; "{e:?}");
                    };
                }
    
                // CORREÇÃO CRÍTICA: Preservar a duração original ao criar o filler
                node.source = filler_media.source;
                node.seek = 0.0;
                
                // Preservar a duração original no out
                node.out = original_duration;
                
                // Manter a duração real do filler para que o cálculo de loop_count funcione
                node.duration = filler_media.duration;
                
                debug!(target: Target::file_mail(), channel = self.channel_id;
                    "[gen_source] Configurando filler - duração original: {}, duração do filler: {}, loop necessário: {}",
                    original_duration, filler_media.duration, 
                    if filler_media.duration > 0.0 { (original_duration / filler_media.duration).ceil() } else { 1.0 });
                
                node.cmd = Some(loop_filler(&self.config, &node));
                node.probe = filler_media.probe;
            } else {
                match MediaProbe::new(&self.config.storage.filler_path).await {
                    Ok(probe) => {
                        if self
                            .config
                            .storage
                            .filler_path
                            .extension()
                            .map(|e| e.to_string_lossy().to_lowercase())
                            .filter(|c| IMAGE_FORMAT.contains(&c.as_str()))
                            .is_some()
                        {
                            node.source = self
                                .config
                                .storage
                                .filler_path
                                .clone()
                                .to_string_lossy()
                                .to_string();
                            node.cmd = Some(loop_image(&self.config, &node));
                            node.probe = Some(probe);
                        } else if let Some(filler_duration) = probe.clone().format.duration {
                            // CORREÇÃO: Preservar a duração original aqui também
                            node.source = self
                                .config
                                .storage
                                .filler_path
                                .clone()
                                .to_string_lossy()
                                .to_string();
                            node.seek = 0.0;
                            node.out = original_duration;  // Usar a duração original
                            node.duration = filler_duration;  // Manter a duração real do filler
                            node.cmd = Some(loop_filler(&self.config, &node));
                            node.probe = Some(probe);
                        } else {
                            // Create colored placeholder.
                            let (source, cmd) = gen_dummy(&self.config, original_duration);  // Usar duração original
                            node.source = source;
                            node.cmd = Some(cmd);
                        }
                    }
                    Err(e) => {
                        // Create colored placeholder.
                        error!(target: Target::file_mail(), channel = self.channel_id; "Filler error: {e}");
    
                        // Usar a duração original aqui também
                        let (source, cmd) = gen_dummy(&self.config, original_duration);
                        node.seek = 0.0;
                        node.out = original_duration;
                        node.duration = original_duration;  // Para placeholders coloridos, a duração é a mesma
                        node.source = source;
                        node.cmd = Some(cmd);
                    }
                }
            }
    
            warn!(
                target: Target::file_mail(), channel = self.channel_id;
                "Generate filler with <yellow>{:.2}</> seconds length! Duração original: <yellow>{:.2}</>, Loop configurado: {}",
                node.out,
                original_duration,
                node.cmd.as_ref().map_or(false, |cmd| cmd.join(" ").contains("-stream_loop"))
            );
        }
    
        node.add_filter(&self.config, &self.manager.filter_chain.clone())
            .await;
    
        trace!(
            "return gen_source: {}, seek: {}, out: {}, duration: {}",
            node.source,
            node.seek,
            node.out,
            node.duration
        );
    
        self.current_node = node;
    }

    async fn duplicate_for_seek_and_loop(&mut self, node: &mut Media) {
        let mut nodes = self.manager.current_list.lock().await;
        let index = node.index.unwrap_or_default();

        let mut node_duplicate = node.clone();
        node_duplicate.seek = 0.0;
        let orig_seek = node.seek;
        node.out = node.duration;

        if node.seek > node.duration {
            node.seek %= node.duration;

            node_duplicate.out = node_duplicate.out - orig_seek - (node.out - node.seek);
        } else {
            node_duplicate.out -= node_duplicate.duration;
        }

        if node.seek == node.out {
            node.seek = node_duplicate.seek;
            node.out = node_duplicate.out;
        } else if node_duplicate.out - node_duplicate.seek > 1.2 {
            node_duplicate.begin =
                Some(node_duplicate.begin.unwrap_or_default() + (node.out - node.seek));

            nodes.insert(index + 1, node_duplicate);

            for (i, item) in nodes.iter_mut().enumerate() {
                item.index = Some(i);
            }
        }
    }
}

/// Build the playlist iterator
impl CurrentProgram {
    pub async fn next(&mut self) -> Option<Media> {
        self.last_json_path.clone_from(&self.json_playlist.path);
        self.last_node_ad = self.current_node.last_ad;
        self.check_for_playlist(self.manager.list_init.load(Ordering::SeqCst))
            .await;

        if self.manager.list_init.load(Ordering::SeqCst) {
            trace!("Init playlist, from next iterator");
            let init_clip_is_filler = match self.json_playlist.path {
                Some(_) => self.init_clip().await,
                None => false,
            };

            if self.manager.list_init.load(Ordering::SeqCst) && !init_clip_is_filler {
                // On init load, playlist could be not long enough, or clips are not found
                // so we fill the gap with a dummy.
                trace!("Init clip is no filler");

                let mut current_time = time_in_seconds(&self.config.channel.timezone);
                let (_, total_delta) = get_delta(&self.config, &current_time);

                if self.start_sec > current_time {
                    current_time += self.length_sec + 1.0;
                }

                let mut last_index = 0;
                let length = self.manager.current_list.lock().await.len();

                if length > 0 {
                    last_index = length - 1;
                }

                let mut media = Media::new(length, "", false, Some(self.manager.clone().into())).await;
                media.begin = Some(current_time);
                media.duration = total_delta;
                media.out = total_delta;

                self.last_next_ad(&mut media).await;

                self.gen_source(media, last_index).await;
            }
        } else if self.manager.current_index.load(Ordering::SeqCst)
            < self.manager.current_list.lock().await.len()
        {
            // get next clip from current playlist
            debug!(target: Target::file_mail(), channel = self.channel_id;
            "[next] Branch normal de processamento de arquivo");

            let mut is_last = false;
            let index = self.manager.current_index.load(Ordering::SeqCst);

            debug!(target: Target::file_mail(), channel = self.channel_id;
            "[next] Processando index: {}", index);

            let node_list = self.manager.current_list.lock().await;

            let mut node = node_list[index].clone();
            let last_index = node_list.len() - 1;

            drop(node_list);

            if index == last_index {
                is_last = true;
                debug!(target: Target::file_mail(), channel = self.channel_id;
                "[next] Processando último arquivo da lista");
            }

            self.last_next_ad(&mut node).await;
            self.timed_source(node, is_last, last_index).await;

            self.manager.current_index.fetch_add(1, Ordering::SeqCst);
        } else {
            let (_, total_delta) = get_delta(&self.config, &self.start_sec);

            if !self.config.playlist.infinit
                && self.last_json_path == self.json_playlist.path
                && total_delta.abs() > 1.0
            {
                // Playlist is to early finish,
                // and if we have to fill it with a placeholder.
                trace!("Total delta on list end: {total_delta}");
            
                debug!(target: Target::file_mail(), channel = self.channel_id;
                    "[next] Preenchendo fim da playlist com placeholder, total_delta = {}", 
                    total_delta);

                trace!("Total delta on list end: {total_delta}");

                self.fill_end(total_delta).await;

                return Some(self.current_node.clone());
            }
            // Get first clip from next playlist.
            debug!(target: Target::file_mail(), channel = self.channel_id;
            "[next] Obtendo primeiro clip da próxima playlist");

            let c_list = self.manager.current_list.lock().await;
            let mut first_node = c_list[0].clone();

            drop(c_list);

            if self.config.playlist.infinit {
                self.recalculate_begin(false).await;
            }

            self.manager.current_index.store(0, Ordering::SeqCst);
            self.last_next_ad(&mut first_node).await;
            first_node.last_ad = self.last_node_ad;

            self.gen_source(first_node, 0).await;

            self.manager.current_index.store(1, Ordering::SeqCst);
        }

        Some(self.current_node.clone())
    }
}
