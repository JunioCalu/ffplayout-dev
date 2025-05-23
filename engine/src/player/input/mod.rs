// INÍCIO METADADOS CLAUDE
// Esse é o arquivo '/engine/src/player/input/mod.rs que eu estou numerando como arquivo número 1'
// Informações adicionais:
// - Tamanho sem o cabeçalho Claude: 1765 bytes
// - Número de linhas sem o cabeçalho Claude: 57
// - Status Git: Modified (modificado mas não adicionado ao staging)
// - Branch atual: skip_clip_on_cuda_decoder_error
// - Última modificação: Tue Jan 28 17:03:40 2025 +0100
// - Possível propósito: Iterador, Processamento de mídia
//
// RESUMO ESTRUTURAL:
// --------------------------------------------------
// Estruturas (structs):
// - Nenhuma struct definido neste arquivo
//
// Enumerações (enums):
// - pub enum SourceIterator {
//
// Traits:
// - Nenhuma trait definida neste arquivo
//
// Funções por categoria:
// Funções de iteração/controle:
// - pub async fn next(&mut self) -> Option<Media> {
//
// Outras funções:
// - pub async fn source_generator(manager: ChannelManager) -> SourceIterator {
//
// Dependências (imports completos):
// - use log::*;
// - use crate::player::{controller::ChannelManager, input::folder::FolderSource, utils::Media};
// - use crate::utils::{config::ProcessMode::*, logging::Target};
// --------------------------------------------------
//
// Este comentário foi adicionado automaticamente para facilitar 
// o entendimento do contexto do projeto por sistemas de IA como o Claude.
// FIM METADADOS CLAUDE
//

use log::*;

pub mod folder;
pub mod ingest; 
pub mod playlist;

pub use ingest::ingest_server;
pub use playlist::CurrentProgram;

use crate::player::{controller::ChannelManager, input::folder::FolderSource, utils::Media};
use crate::utils::{config::ProcessMode::*, logging::Target};

pub enum SourceIterator {
    Folder(FolderSource),
    Playlist(CurrentProgram),
}

impl SourceIterator {
    pub async fn next(&mut self) -> Option<Media> {
        match self {
            SourceIterator::Folder(folder_source) => folder_source.next().await,
            SourceIterator::Playlist(program) => program.next().await,
        }
    }
}

/// Create a source iterator from playlist, or from folder.
pub async fn source_generator(manager: ChannelManager) -> SourceIterator {
    let config = manager.config.lock().await.clone();
    let id = config.general.channel_id;
    let is_alive = manager.is_alive.clone();
    let current_list = manager.current_list.clone();

    match config.processing.mode {
        Folder => {
            info!(target: Target::file_mail(), channel = id; "Playout in folder mode");
            let config_clone = config.clone();

            // Spawn a task to monitor folder for file changes.
            {
                let mut storage = manager.storage.lock().await;
                storage.watchman(config_clone, is_alive, current_list).await;
            }

            let folder_source = FolderSource::new(&config, manager);

            SourceIterator::Folder(folder_source.await)
        }
        Playlist => {
            info!(target: Target::file_mail(), channel = id; "Playout in playlist mode");
            let program = CurrentProgram::new(manager);

            SourceIterator::Playlist(program.await)
        }
    }
}
