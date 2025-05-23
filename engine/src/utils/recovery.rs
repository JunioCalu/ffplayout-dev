// INÍCIO METADADOS CLAUDE
// Esse é o arquivo '/engine/src/utils/recovery.rs que eu estou numerando como arquivo número 16'
// Informações adicionais:
// - Tamanho sem o cabeçalho Claude: 3724 bytes
// - Número de linhas sem o cabeçalho Claude: 117
// - Status Git: Staged (modificado e adicionado ao staging)
// - Branch atual: skip_clip_on_cuda_decoder_error
// - Última modificação: Novo arquivo
// - Possível propósito: Indeterminado
//
// Documentação da struct:
// Estado de recuperação específico para um canal.
// #[derive(Debug, Clone)]
//
// RESUMO ESTRUTURAL:
// --------------------------------------------------
// Estruturas (structs):
// - pub struct ChannelRecoveryState {
//
// Enumerações (enums):
// - Nenhuma enum definido neste arquivo
//
// Traits:
// - Nenhuma trait definida neste arquivo
//
// Funções por categoria:
// Funções de inicialização:
// - pub fn new() -> Self {
//
// Outras funções:
// - pub fn enter_recovery_mode(&self) {
// - pub fn exit_recovery_mode(&self) {
// - pub fn is_in_recovery_mode(&self) -> bool {
// - pub async fn mark_file_incompatible(&self, file_path: String) {
// - pub async fn remove_incompatible_file(&self, file_path: &str) {
// - pub async fn is_file_incompatible(&self, file_path: &str) -> bool {
// - pub async fn clear_incompatible_files(&self) {
// - pub async fn incompatible_files_count(&self) -> usize {
// - pub async fn mark_file_for_retry(&self, file_path: String) {
// - pub async fn is_file_in_retry(&self, file_path: &str) -> bool {
// - pub async fn remove_from_retry(&self, file_path: &str) {
// - fn default() -> Self {
//
// Dependências (imports completos):
// - use std::collections::HashSet;
// - use std::sync::atomic::{AtomicBool, Ordering};
// - use std::sync::Arc;
// - use tokio::sync::Mutex;
// --------------------------------------------------
//
// Este comentário foi adicionado automaticamente para facilitar 
// o entendimento do contexto do projeto por sistemas de IA como o Claude.
// FIM METADADOS CLAUDE
//

//! Módulo para gerenciamento de recuperação de erros e arquivos incompatíveis.
//! Cada canal deve ter sua própria instância de ChannelRecoveryState.

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;

/// Estado de recuperação específico para um canal.
#[derive(Debug, Clone)]
pub struct ChannelRecoveryState {
    /// Flag atômica indicando se o canal está em modo de recuperação
    recovery_mode: Arc<AtomicBool>,
    
    /// Conjunto de arquivos marcados como incompatíveis para este canal
    incompatible_files: Arc<Mutex<HashSet<String>>>,
    retry_files: Arc<Mutex<HashSet<String>>>,
}

impl ChannelRecoveryState {
    /// Cria um novo estado de recuperação para um canal
    pub fn new() -> Self {
        Self {
            recovery_mode: Arc::new(AtomicBool::new(false)),
            incompatible_files: Arc::new(Mutex::new(HashSet::new())),
            retry_files: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    /// Coloca o canal em modo de recuperação
    pub fn enter_recovery_mode(&self) {
        self.recovery_mode.store(true, Ordering::SeqCst);
    }

    /// Sai do modo de recuperação
    pub fn exit_recovery_mode(&self) {
        self.recovery_mode.store(false, Ordering::SeqCst);
    }

    /// Verifica se o canal está em modo de recuperação
    pub fn is_in_recovery_mode(&self) -> bool {
        self.recovery_mode.load(Ordering::SeqCst)
    }

    /// Marca um arquivo como incompatível para este canal
    pub async fn mark_file_incompatible(&self, file_path: String) {
        let mut files = self.incompatible_files.lock().await;
        files.insert(file_path);
    }

    /// Remove um arquivo da lista de incompatíveis
    pub async fn remove_incompatible_file(&self, file_path: &str) {
        let mut files = self.incompatible_files.lock().await;
        files.remove(file_path);
        
        // Se não há mais arquivos incompatíveis, podemos sair do modo de recuperação
        if files.is_empty() {
            self.exit_recovery_mode();
        }
    }

    /// Verifica se um arquivo foi marcado como incompatível
    pub async fn is_file_incompatible(&self, file_path: &str) -> bool {
        let files = self.incompatible_files.lock().await;
        files.contains(file_path)
    }

    /// Limpa a lista de arquivos incompatíveis
    pub async fn clear_incompatible_files(&self) {
        let mut files = self.incompatible_files.lock().await;
        files.clear();
    }
    
    /// Retorna a quantidade de arquivos incompatíveis
    pub async fn incompatible_files_count(&self) -> usize {
        let files = self.incompatible_files.lock().await;
        files.len()
    }

    // Novos métodos
    pub async fn mark_file_for_retry(&self, file_path: String) {
        let mut files = self.retry_files.lock().await;
        files.insert(file_path);
    }
    
    pub async fn is_file_in_retry(&self, file_path: &str) -> bool {
        let files = self.retry_files.lock().await;
        files.contains(file_path)
    }
    
    pub async fn remove_from_retry(&self, file_path: &str) {
        let mut files = self.retry_files.lock().await;
        files.remove(file_path);
    }
    

}

impl Default for ChannelRecoveryState {
    fn default() -> Self {
        Self::new()
    }
}

// A estrutura seria usada adicionando-a ao ChannelManager:
// 
// pub struct ChannelManager {
//     // ... campos existentes
//     pub recovery_state: Arc<ChannelRecoveryState>,
// }
//
// E inicializando em setup_channel_manager():
//
// let manager = ChannelManager {
//     // ... outros campos
//     recovery_state: Arc::new(ChannelRecoveryState::new()),
// };