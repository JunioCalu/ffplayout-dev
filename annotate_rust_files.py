#!/usr/bin/env python3

import os
import re
import sys
import subprocess
import argparse
from pathlib import Path


def run_command(command):
    """Executa um comando de shell e retorna a saída."""
    process = subprocess.run(command, shell=True, capture_output=True, text=True)
    if process.returncode != 0:
        return ""
    return process.stdout.strip()


def is_git_repo():
    """Verifica se o diretório atual é um repositório git."""
    return run_command("git rev-parse --is-inside-work-tree") == "true"


def get_rust_files():
    """Obtém arquivos Rust modificados, no staging e untracked."""
    staged_files = run_command("git diff --name-only --cached | grep '\\.rs$' || true").splitlines()
    modified_files = run_command("git diff --name-only | grep '\\.rs$' || true").splitlines()
    untracked_files = run_command("git ls-files --others --exclude-standard | grep '\\.rs$' || true").splitlines()
    
    # Combine all files, removing duplicates
    all_files = list(set(staged_files + modified_files + untracked_files))
    
    # Create status mapping
    file_status = {}
    for file in all_files:
        if file in staged_files:
            file_status[file] = "Staged (modificado e adicionado ao staging)"
        elif file in modified_files:
            file_status[file] = "Modified (modificado mas não adicionado ao staging)"
        else:
            file_status[file] = "Untracked (novo arquivo não rastreado)"
    
    return all_files, file_status


def get_file_info(file_path):
    """Obtém informações sobre o arquivo."""
    abs_path = os.path.abspath(file_path)
    repo_root = run_command("git rev-parse --show-toplevel")
    rel_path = abs_path.replace(repo_root, "", 1)
    if not rel_path.startswith("/"):
        rel_path = "/" + rel_path
    
    try:
        with open(file_path, 'r', encoding='utf-8', errors='replace') as f:
            content = f.read()
    except Exception as e:
        print(f"Erro ao ler o arquivo {file_path}: {e}")
        content = ""  # Fallback para conteúdo vazio em caso de erro
    
    file_size = os.path.getsize(file_path)
    line_count = content.count('\n') + 1  # +1 para a última linha se não terminar com quebra
    last_modified = run_command(f"git log -1 --format=\"%ad\" -- {file_path}") or "Novo arquivo"
    git_branch = run_command("git branch --show-current")
    
    # Extract Rust structures and imports more thoroughly
    structs = re.findall(r'^(?:pub)?\s*struct\s+([A-Za-z0-9_]+)\s*(?:\{|$)', content, re.MULTILINE)
    structs_lines = [line.strip() for line in re.findall(r'^(?:pub)?\s*struct\s+[A-Za-z0-9_]+\s*(?:\{|$).*', content, re.MULTILINE)]
    
    enums = re.findall(r'^(?:pub)?\s*enum\s+([A-Za-z0-9_]+)\s*(?:\{|$)', content, re.MULTILINE)
    enums_lines = [line.strip() for line in re.findall(r'^(?:pub)?\s*enum\s+[A-Za-z0-9_]+\s*(?:\{|$).*', content, re.MULTILINE)]
    
    traits = re.findall(r'^(?:pub)?\s*trait\s+([A-Za-z0-9_]+)\s*(?:\{|<|$)', content, re.MULTILINE)
    traits_lines = [line.strip() for line in re.findall(r'^(?:pub)?\s*trait\s+[A-Za-z0-9_]+\s*(?:\{|<|$).*', content, re.MULTILINE)]
    
    # Melhoria: Capturar todas as funções, mesmo em diferentes blocos impl
    functions_full = []
    
    # Primeiro, capture todas as funções de alto nível (não dentro de impl)
    top_level_funcs = re.findall(r'^(?:\s*pub)?\s*(?:async\s+)?fn\s+[A-Za-z0-9_]+.*?(?:\{|$)', content, re.MULTILINE)
    for func in top_level_funcs:
        func_clean = func.strip()
        if not func_clean.startswith('//') and func_clean not in functions_full:
            functions_full.append(func_clean)
    
    # Em seguida, analise todos os blocos impl para encontrar métodos
    # Não podemos usar uma regex simples para capturar todos os blocos impl devido a aninhamentos
    # Então vamos pegar todas as declarações de função dentro do arquivo
    all_funcs = re.findall(r'(?:^|\n)\s*(?:pub\s+)?(?:async\s+)?fn\s+[A-Za-z0-9_]+.*?(?:\{|$)', content, re.MULTILINE)
    for func in all_funcs:
        func_clean = func.strip()
        if not func_clean.startswith('//') and func_clean not in functions_full:
            functions_full.append(func_clean)
    
    # Método simplificado para captura de imports
    # Primeiro encontramos todos os blocos de uso, seja de linha única ou multi-linha
    import_blocks = []
    lines = content.split('\n')
    i = 0
    while i < len(lines):
        line = lines[i].strip()
        
        # Detecta linha de import condicional (cfg)
        if line.startswith('#[cfg') and i + 1 < len(lines) and lines[i+1].strip().startswith('use '):
            block = [line, lines[i+1].strip()]
            import_blocks.append('\n'.join(block))
            i += 2
            continue
            
        # Detecta início de um bloco use
        if line.startswith('use '):
            # Se for um import de uma linha
            if line.endswith(';'):
                import_blocks.append(line)
                i += 1
                continue
                
            # É um import multi-linha
            block = [line]
            j = i + 1
            # Continue adicionando linhas até encontrar o ponto e vírgula final
            while j < len(lines) and ';' not in lines[j]:
                block.append(lines[j].strip())
                j += 1
                
            # Adicione a linha final com ponto e vírgula
            if j < len(lines):
                block.append(lines[j].strip())
                
            import_blocks.append('\n'.join(block))
            i = j + 1
            continue
            
        # Detecta comentário de import
        if line.startswith('// use '):
            import_blocks.append(line)
            
        i += 1
    
    # Determine possible purpose
    purpose = []
    if re.search(r'mod test|#\[test\]', content):
        purpose.append("Testes")
    if re.search(r'struct.*Config|fn.*config', content):
        purpose.append("Configuração")
    if re.search(r'impl.*Error|#\[derive\(Error\)', content):
        purpose.append("Tratamento de erros")
    if re.search(r'Database|db|query|sql|table', content):
        purpose.append("Acesso a dados")
    if re.search(r'Router|route|endpoint|handler|Response', content):
        purpose.append("API/Web")
    if re.search(r'main\(\)', content):
        purpose.append("Ponto de entrada")
    
    # Verificar padrões adicionais para melhorar a detecção de propósito
    if re.search(r'Iterator|next\(\)', content):
        purpose.append("Iterador")
    if re.search(r'player|media|clip|playlist', content, re.IGNORECASE):
        purpose.append("Processamento de mídia")
    
    purpose_str = ", ".join(purpose) if purpose else "Indeterminado"
    
    return {
        "rel_path": rel_path,
        "file_size": file_size,
        "line_count": line_count,
        "last_modified": last_modified,
        "git_branch": git_branch,
        "purpose": purpose_str,
        "structs": structs_lines or ["Nenhuma struct definido neste arquivo"],
        "enums": enums_lines or ["Nenhuma enum definido neste arquivo"],
        "traits": traits_lines or ["Nenhuma trait definida neste arquivo"],
        "functions": functions_full or ["Nenhuma função encontrada"],
        "imports": import_blocks or ["Nenhum import encontrado"]
    }


def add_comment(file_path, file_number, total_files, status):
    """Adiciona comentário ao início do arquivo."""
    if not os.path.isfile(file_path):
        print(f"Arquivo não encontrado: {file_path}")
        return False
    
    # Se o arquivo já contém um cabeçalho, remova-o primeiro
    if has_header(file_path):
        remove_comment(file_path)
    
    print(f"Processando arquivo {file_number}/{total_files}: {file_path}")
    
    info = get_file_info(file_path)
    
    # Extrair documentação da struct e funções principais
    struct_doc = ""
    if info['structs'][0] != "Nenhuma struct definido neste arquivo":
        with open(file_path, 'r', encoding='utf-8', errors='replace') as f:
            content = f.read()
            struct_match = re.search(r'///.*?\n(?:///.*?\n)*\s*pub struct', content, re.DOTALL)
            if struct_match:
                doc_lines = struct_match.group(0).split('\n')
                # Remover o 'pub struct' da última linha
                doc_lines = [line for line in doc_lines if not line.strip().startswith('pub struct')]
                # Formatar corretamente como comentários
                formatted_doc = "\n".join(['// ' + line[3:].strip() if line.startswith('///') else '// ' + line.strip() for line in doc_lines if line.strip()])
                if formatted_doc:
                    struct_doc = "// Documentação da struct:\n" + formatted_doc + "\n//\n"
    
    # Agrupar funções por categoria para melhor entendimento
    categorized_functions = {
        "Funções de inicialização": [],
        "Funções de gerenciamento de tempo": [],
        "Funções de gerenciamento de mídia": [],
        "Funções de iteração/controle": [],
        "Outras funções": []
    }
    
    for func in info['functions']:
        func_name = re.search(r'fn\s+([A-Za-z0-9_]+)', func)
        if func_name:
            name = func_name.group(1)
            if name in ['new', 'init_clip', 'get_current_clip']:
                categorized_functions["Funções de inicialização"].append(func)
            elif name in ['get_current_time', 'set_status', 'recalculate_begin']:
                categorized_functions["Funções de gerenciamento de tempo"].append(func)
            elif name in ['gen_source', 'duplicate_for_seek_and_loop', 'fill_end']:
                categorized_functions["Funções de gerenciamento de mídia"].append(func)
            elif name in ['next', 'check_for_playlist', 'load_or_update_playlist']:
                categorized_functions["Funções de iteração/controle"].append(func)
            else:
                categorized_functions["Outras funções"].append(func)
    
    # Criar a seção de cabeçalho básica
    header = f"""// INÍCIO METADADOS CLAUDE
// Esse é o arquivo '{info['rel_path']} que eu estou numerando como arquivo número {file_number}'
// Informações adicionais:
// - Tamanho sem o cabeçalho Claude: {info['file_size']} bytes
// - Número de linhas sem o cabeçalho Claude: {info['line_count']}
// - Status Git: {status}
// - Branch atual: {info['git_branch']}
// - Última modificação: {info['last_modified']}
// - Possível propósito: {info['purpose']}
//
{struct_doc}// RESUMO ESTRUTURAL:
// --------------------------------------------------
// Estruturas (structs):
{os.linesep.join(['// - ' + s for s in info['structs']])}
//
// Enumerações (enums):
{os.linesep.join(['// - ' + e for e in info['enums']])}
//
// Traits:
{os.linesep.join(['// - ' + t for t in info['traits']])}
//
// Funções por categoria:
"""
    
    # Adicionar funções agrupadas por categoria
    functions_text = ""
    for category, funcs in categorized_functions.items():
        if funcs:
            functions_text += f"// {category}:\n"
            for func in funcs:
                functions_text += f"// - {func}\n"
            functions_text += "//\n"
    
    # Gerar seção de imports como uma string separada
    imports_text = "// Dependências (imports completos):\n"
    for imp in info['imports']:
        lines = imp.split('\n')
        for i, line in enumerate(lines):
            if i == 0:
                imports_text += f"// - {line}\n"
            else:
                imports_text += f"//   {line}\n"
    
    # Gerar o rodapé
    footer = """// --------------------------------------------------
//
// Este comentário foi adicionado automaticamente para facilitar 
// o entendimento do contexto do projeto por sistemas de IA como o Claude.
// FIM METADADOS CLAUDE
//

"""
    
    # Montar o comentário completo
    comment = header + functions_text + imports_text + footer
    
    # Read original file
    with open(file_path, 'r', encoding='utf-8', errors='replace') as f:
        original_content = f.read()
    
    # Write directly to a new file
    new_content = comment + original_content
    
    # Usar método direto para reescrever o arquivo
    with open(file_path, 'w', encoding='utf-8') as f:
        f.write(new_content)
    
    print(f"✓ Comentário adicionado a {file_path}")
    return True


def has_header(file_path):
    """Verifica se o arquivo já possui um cabeçalho de metadados Claude."""
    try:
        with open(file_path, 'r', encoding='utf-8', errors='replace') as f:
            content = f.read(1000)  # Lê apenas os primeiros 1000 caracteres para eficiência
            return "// INÍCIO METADADOS CLAUDE" in content
    except Exception as e:
        print(f"Erro ao verificar cabeçalho em {file_path}: {e}")
        return False


def remove_comment(file_path):
    """Remove o comentário do início do arquivo."""
    if not os.path.isfile(file_path):
        print(f"Arquivo não encontrado: {file_path}")
        return False
    
    try:
        with open(file_path, 'r', encoding='utf-8', errors='replace') as f:
            content = f.read()
        
        # Procura pelo início e fim do cabeçalho
        start_pattern = "// INÍCIO METADADOS CLAUDE"
        end_pattern = "// FIM METADADOS CLAUDE\n//\n\n"
        
        start_index = content.find(start_pattern)
        if start_index == -1:
            print(f"Cabeçalho não encontrado em {file_path}")
            return False
        
        end_index = content.find(end_pattern)
        if end_index == -1:
            # Tenta encontrar um padrão alternativo de fim
            end_pattern = "// FIM METADADOS CLAUDE"
            end_index = content.find(end_pattern)
            if end_index == -1:
                print(f"Fim do cabeçalho não encontrado em {file_path}")
                return False
            end_index = content.find("\n", end_index) + 1
        else:
            end_index += len(end_pattern)
        
        # Remove o cabeçalho
        new_content = content[:start_index] + content[end_index:]
        
        # Escreve o arquivo sem o cabeçalho
        with open(file_path, 'w', encoding='utf-8') as f:
            f.write(new_content)
        
        print(f"✓ Cabeçalho removido de {file_path}")
        return True
    
    except Exception as e:
        print(f"Erro ao remover cabeçalho de {file_path}: {e}")
        return False


def main():
    """Função principal."""
    # Configurar argparser para melhor interface de linha de comando
    parser = argparse.ArgumentParser(
        description="Adiciona ou remove cabeçalhos com metadados em arquivos Rust para melhorar o entendimento por IAs como Claude."
    )
    
    group = parser.add_mutually_exclusive_group()
    group.add_argument(
        '--modificados', 
        action='store_true', 
        help='Processa todos os arquivos Rust modificados, em staging ou untracked (comportamento padrão)'
    )
    group.add_argument(
        '--arquivo', 
        type=str, 
        help='Caminho para um arquivo específico a ser processado'
    )
    group.add_argument(
        '--pasta', 
        type=str, 
        help='Processa todos os arquivos .rs em uma pasta específica e suas subpastas'
    )
    
    # Adiciona argumento para remover cabeçalhos em vez de adicioná-los
    parser.add_argument(
        '--remover', 
        action='store_true', 
        help='Remove cabeçalhos existentes em vez de adicioná-los'
    )
    
    args = parser.parse_args()
    
    # Processa arquivo específico
    if args.arquivo:
        if not args.arquivo.endswith('.rs'):
            print("Erro: Apenas arquivos Rust (.rs) são suportados.")
            return 1
        
        if not os.path.isfile(args.arquivo):
            print(f"Erro: Arquivo {args.arquivo} não encontrado.")
            return 1
        
        if args.remover:
            success = remove_comment(args.arquivo)
        else:
            if is_git_repo():
                status = "Arquivo específico"
                if args.arquivo in run_command(f"git diff --name-only -- {args.arquivo}").splitlines():
                    status = "Modified (modificado mas não adicionado ao staging)"
                elif args.arquivo in run_command(f"git diff --name-only --cached -- {args.arquivo}").splitlines():
                    status = "Staged (modificado e adicionado ao staging)"
                elif args.arquivo in run_command(f"git ls-files --others --exclude-standard -- {args.arquivo}").splitlines():
                    status = "Untracked (novo arquivo não rastreado)"
            else:
                status = "Não rastreado por Git"
            
            success = add_comment(args.arquivo, 1, 1, status)
        
        return 0 if success else 1
    
    # Processa pasta específica
    elif args.pasta:
        if not os.path.isdir(args.pasta):
            print(f"Erro: Pasta {args.pasta} não encontrada.")
            return 1
        
        # Encontra todos os arquivos .rs na pasta e subpastas
        rust_files = []
        for root, _, files in os.walk(args.pasta):
            for file in files:
                if file.endswith('.rs'):
                    rust_files.append(os.path.join(root, file))
        
        if not rust_files:
            print(f"Nenhum arquivo Rust (.rs) encontrado na pasta {args.pasta}.")
            return 0
        
        print(f"Encontrados {len(rust_files)} arquivos Rust na pasta {args.pasta}.")
        
        for idx, file in enumerate(rust_files, 1):
            if args.remover:
                remove_comment(file)
            else:
                status = "Arquivo em pasta específica"
                if is_git_repo():
                    if file in run_command(f"git diff --name-only -- {file}").splitlines():
                        status = "Modified (modificado mas não adicionado ao staging)"
                    elif file in run_command(f"git diff --name-only --cached -- {file}").splitlines():
                        status = "Staged (modificado e adicionado ao staging)"
                    elif file in run_command(f"git ls-files --others --exclude-standard -- {file}").splitlines():
                        status = "Untracked (novo arquivo não rastreado)"
                
                add_comment(file, idx, len(rust_files), status)
        
        print("Processamento concluído!")
        return 0
    
    # Comportamento padrão - processa arquivos modificados
    else:
        if not is_git_repo():
            print("Erro: Este diretório não é um repositório Git.")
            return 1
        
        print("Buscando arquivos Rust (.rs) modificados, no staging e untracked...")
        
        rust_files, file_status = get_rust_files()
        
        if not rust_files:
            print("Nenhum arquivo Rust (.rs) encontrado modificado, no staging ou untracked.")
            return 0
        
        print(f"Encontrados {len(rust_files)} arquivos Rust para processar.")
        
        for idx, file in enumerate(rust_files, 1):
            if args.remover:
                remove_comment(file)
            else:
                add_comment(file, idx, len(rust_files), file_status[file])
        
        print("Processamento concluído!")
        print("Nota: Os arquivos foram modificados localmente. Execute 'git add' novamente se desejar incluir estas mudanças no próximo commit.")
        return 0


if __name__ == "__main__":
    sys.exit(main())
