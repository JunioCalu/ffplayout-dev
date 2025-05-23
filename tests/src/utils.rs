// INÍCIO METADADOS CLAUDE
// Esse é o arquivo '/tests/src/utils.rs que eu estou numerando como arquivo número 10'
// Informações adicionais:
// - Tamanho sem o cabeçalho Claude: 2586 bytes
// - Número de linhas sem o cabeçalho Claude: 92
// - Status Git: Modified (modificado mas não adicionado ao staging)
// - Branch atual: skip_clip_on_cuda_decoder_error
// - Última modificação: Thu Feb 13 08:48:15 2025 +0100
// - Possível propósito: Testes, Configuração, Acesso a dados, Processamento de mídia
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
// - async fn prepare_config() -> (PlayoutConfig, ChannelManager) {
// - fn mock_date_time() {
// - fn get_date_yesterday() {
// - fn get_date_tomorrow() {
// - async fn test_delta() {
//
// Dependências (imports completos):
// - use sqlx::sqlite::SqlitePoolOptions;
// - use chrono::prelude::*;
// - use serial_test::serial;
// - use ffplayout::db::handles;
// - use ffplayout::player::{controller::ChannelManager, utils::*};
// - use ffplayout::utils::{
//   config::{PlayoutConfig, ProcessMode::Playlist},
//   time_machine::{set_mock_time, time_now},
//   };
// --------------------------------------------------
//
// Este comentário foi adicionado automaticamente para facilitar 
// o entendimento do contexto do projeto por sistemas de IA como o Claude.
// FIM METADADOS CLAUDE
//

use sqlx::sqlite::SqlitePoolOptions;

use chrono::prelude::*;
use serial_test::serial;

use ffplayout::db::handles;
use ffplayout::player::{controller::ChannelManager, utils::*};
use ffplayout::utils::{
    config::{PlayoutConfig, ProcessMode::Playlist},
    time_machine::{set_mock_time, time_now},
};

async fn prepare_config() -> (PlayoutConfig, ChannelManager) {
    let pool = SqlitePoolOptions::new()
        .connect("sqlite::memory:")
        .await
        .unwrap();
    handles::db_migrate(&pool).await.unwrap();

    sqlx::query(
        r#"
        UPDATE global SET public = "assets/hls", logs = "assets/log", playlists = "assets/playlists", storage = "assets/storage";
        UPDATE channels SET public = "assets/hls", playlists = "assets/playlists", storage = "assets/storage";
        UPDATE configurations SET processing_width = 1024, processing_height = 576;
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

#[test]
#[serial]
#[ignore]
fn mock_date_time() {
    let time_str = "2022-05-20T06:00:00+02:00";
    let date_obj = NaiveDateTime::parse_from_str(time_str, "%Y-%m-%dT%H:%M:%S%z");
    let time = Local.from_local_datetime(&date_obj.unwrap()).unwrap();

    set_mock_time(&Some(time_str.to_string())).unwrap();

    assert_eq!(
        time.format("%Y-%m-%dT%H:%M:%S.2f").to_string(),
        time_now(&None).format("%Y-%m-%dT%H:%M:%S.2f").to_string()
    );
}

#[test]
#[serial]
#[ignore]
fn get_date_yesterday() {
    set_mock_time(&Some("2022-05-20T05:59:24+02:00".to_string())).unwrap();

    let date = get_date(true, 21600.0, false, &None);

    assert_eq!("2022-05-19".to_string(), date);
}

#[test]
#[serial]
#[ignore]
fn get_date_tomorrow() {
    set_mock_time(&Some("2022-05-20T23:59:58+02:00".to_string())).unwrap();

    let date = get_date(false, 0.0, true, &None);

    assert_eq!("2022-05-21".to_string(), date);
}

#[actix_web::test]
#[serial]
#[ignore]
async fn test_delta() {
    let (mut config, manager) = prepare_config().await;

    config.mail.recipient = "".into();
    config.processing.mode = Playlist;
    config.playlist.day_start = "00:00:00".into();
    config.playlist.length = "24:00:00".into();

    set_mock_time(&Some("2022-05-09T23:59:59+02:00".to_string())).unwrap();
    let (delta, _) = get_delta(&config, &manager.recovery_state, &86401.0);

    assert!(delta < 2.0);
}
