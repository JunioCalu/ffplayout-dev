// INÍCIO METADADOS CLAUDE
// Esse é o arquivo '/projeto_ffplayout_flat/recovery.rs que eu estou numerando como arquivo número 13'
// Informações adicionais:
// - Tamanho sem o cabeçalho Claude: 1240 bytes
// - Número de linhas sem o cabeçalho Claude: 45
// - Status Git: Untracked (novo arquivo não rastreado)
// - Branch atual: skip_clip_on_cuda_decoder_error
// - Última modificação: Novo arquivo
// - Possível propósito: Indeterminado
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
// - pub fn enter_recovery_mode() {
// - pub fn exit_recovery_mode() {
// - pub fn is_in_recovery_mode() -> bool {
// - pub fn mark_file_incompatible(file_path: String) {
// - pub fn is_file_incompatible(file_path: &str) -> bool {
// - pub fn clear_incompatible_files() {
//
// Dependências (imports completos):
// - use std::cell::RefCell;
// - use std::collections::HashSet;
// - use std::sync::Mutex;
// - use once_cell::sync::Lazy;
// --------------------------------------------------
//
// Este comentário foi adicionado automaticamente para facilitar 
// o entendimento do contexto do projeto por sistemas de IA como o Claude.
// FIM METADADOS CLAUDE
//

use std::cell::RefCell;
use std::collections::HashSet;
use std::sync::Mutex;
use once_cell::sync::Lazy;

// Estado de recuperação thread-local (como você escolheu)
thread_local! {
    pub static RECOVERY_MODE: RefCell<bool> = RefCell::new(false);
}

// HashSet global para arquivos incompatíveis
// Usamos Lazy para inicialização sob demanda
pub static INCOMPATIBLE_FILES: Lazy<Mutex<HashSet<String>>> = 
    Lazy::new(|| Mutex::new(HashSet::new()));

// Funções para modo de recuperação
pub fn enter_recovery_mode() {
    RECOVERY_MODE.with(|f| *f.borrow_mut() = true);
}

pub fn exit_recovery_mode() {
    RECOVERY_MODE.with(|f| *f.borrow_mut() = false);
}

pub fn is_in_recovery_mode() -> bool {
    RECOVERY_MODE.with(|f| *f.borrow())
}

// Funções para gerenciar arquivos incompatíveis
pub fn mark_file_incompatible(file_path: String) {
    let mut files = INCOMPATIBLE_FILES.lock().unwrap();
    files.insert(file_path);
}

pub fn is_file_incompatible(file_path: &str) -> bool {
    let files = INCOMPATIBLE_FILES.lock().unwrap();
    files.contains(file_path)
}

// Opcional: limpar arquivos incompatíveis
pub fn clear_incompatible_files() {
    let mut files = INCOMPATIBLE_FILES.lock().unwrap();
    files.clear();
}
