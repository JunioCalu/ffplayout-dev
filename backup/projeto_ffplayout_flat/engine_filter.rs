// INÍCIO METADADOS CLAUDE
// Esse é o arquivo '/projeto_ffplayout_flat/engine_filter.rs que eu estou numerando como arquivo número 6'
// Informações adicionais:
// - Tamanho sem o cabeçalho Claude: 1619 bytes
// - Número de linhas sem o cabeçalho Claude: 50
// - Status Git: Untracked (novo arquivo não rastreado)
// - Branch atual: skip_clip_on_cuda_decoder_error
// - Última modificação: Novo arquivo
// - Possível propósito: Configuração, Acesso a dados, Processamento de mídia
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
// - async fn get_config() -> (PlayoutConfig, ChannelManager) {
// - async fn simple_filtering() {
//
// Dependências (imports completos):
// - use std::fs;
// - use sqlx::sqlite::SqlitePoolOptions;
// - use ffplayout::db::handles;
// - use ffplayout::player::{controller::ChannelManager, utils::Media};
// - use ffplayout::utils::config::{OutputMode::*, PlayoutConfig};
// --------------------------------------------------
//
// Este comentário foi adicionado automaticamente para facilitar 
// o entendimento do contexto do projeto por sistemas de IA como o Claude.
// FIM METADADOS CLAUDE
//

use std::fs;

use sqlx::sqlite::SqlitePoolOptions;

use ffplayout::db::handles;
use ffplayout::player::{controller::ChannelManager, utils::Media};
use ffplayout::utils::config::{OutputMode::*, PlayoutConfig};

async fn get_config() -> (PlayoutConfig, ChannelManager) {
    let pool = SqlitePoolOptions::new()
        .connect("sqlite::memory:")
        .await
        .unwrap();
    handles::db_migrate(&pool).await.unwrap();

    sqlx::query(
        r#"
        UPDATE global SET public = "assets/hls", logs = "assets/log", playlists = "assets/playlists", storage = "assets/storage";
        UPDATE channels SET public = "assets/hls", playlists = "assets/playlists", storage = "assets/storage";
        UPDATE configurations SET processing_width = 1024, processing_height = 576, processing_volume = 0.05;
        "#,
    )
    .execute(&pool)
    .await
    .unwrap();

    let config = PlayoutConfig::new(&pool, 1).await.unwrap();
    let channel = handles::select_channel(&pool, &1).await.unwrap();
    let manager = ChannelManager::new(pool, channel, config.clone()).await;

    (config, manager)
}

#[tokio::test]
async fn simple_filtering() {
    let (mut config, _) = get_config().await;

    config.output.mode = Stream;
    config.processing.add_logo = true;
    let logo_path = fs::canonicalize("./assets/logo.png").unwrap();
    config.processing.logo_path = logo_path.to_string_lossy().to_string();

    let mut media = Media::new(0, "./assets/media_mix/with_audio.mp4", true, None).await;
    media.add_filter(&config, &None).await;

    let _f = media.filter.unwrap().cmd();

    // println!("{f:?}");
}
