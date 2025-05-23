#!/usr/bin/env python3

import os
import sys
import shutil
import argparse
import subprocess
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


def get_git_files(modified=False, staged=False, untracked=False):
    """Obtém arquivos do repositório git com base nos critérios especificados."""
    files = []
    
    if modified:
        modified_files = run_command("git diff --name-only").splitlines()
        files.extend(modified_files)
    
    if staged:
        staged_files = run_command("git diff --name-only --cached").splitlines()
        files.extend(staged_files)
    
    if untracked:
        untracked_files = run_command("git ls-files --others --exclude-standard").splitlines()
        files.extend(untracked_files)
    
    # Remover duplicatas mantendo a ordem
    unique_files = []
    for file in files:
        if file and file not in unique_files:
            unique_files.append(file)
    
    return unique_files


def copy_files(files, target_dir, preserve_structure=True):
    """Copia os arquivos para o diretório de destino."""
    if not files:
        print("Nenhum arquivo para copiar.")
        return
    
    # Obter o diretório raiz do repositório Git
    repo_root = run_command("git rev-parse --show-toplevel")
    
    # Criar o diretório de destino se não existir
    os.makedirs(target_dir, exist_ok=True)
    
    print(f"Copiando {len(files)} arquivos para {target_dir}...")
    
    for file_path in files:
        # Verificar se o arquivo existe
        abs_file_path = os.path.join(repo_root, file_path)
        if not os.path.isfile(abs_file_path):
            print(f"Arquivo não encontrado: {file_path}")
            continue
        
        if preserve_structure:
            # Criar subdiretórios se necessário
            target_file = os.path.join(target_dir, file_path)
            os.makedirs(os.path.dirname(target_file), exist_ok=True)
        else:
            # Colocar todos os arquivos diretamente no diretório de destino
            file_name = os.path.basename(file_path)
            target_file = os.path.join(target_dir, file_name)
            
            # Lidar com nomes de arquivo duplicados
            if os.path.exists(target_file):
                base, ext = os.path.splitext(file_name)
                counter = 1
                while True:
                    new_name = f"{base}_{counter}{ext}"
                    target_file = os.path.join(target_dir, new_name)
                    if not os.path.exists(target_file):
                        break
                    counter += 1
        
        # Copiar o arquivo
        try:
            shutil.copy2(abs_file_path, target_file)
            print(f"Copiado: {file_path} -> {target_file}")
        except Exception as e:
            print(f"Erro ao copiar {file_path}: {e}")
    
    print(f"Operação concluída. {len(files)} arquivos foram copiados para {target_dir}.")


def main():
    """Função principal."""
    parser = argparse.ArgumentParser(
        description="Copia arquivos modificados, em staging, ou não rastreados de um repositório Git para uma pasta específica."
    )
    
    parser.add_argument(
        "target_dir",
        type=str,
        help="Diretório de destino para onde os arquivos serão copiados"
    )
    
    # Grupo exclusivo para forçar seleção explícita de tipos de arquivo
    group = parser.add_argument_group('tipos de arquivo')
    group.add_argument(
        "--modified",
        action="store_true",
        help="Incluir arquivos modificados"
    )
    
    group.add_argument(
        "--staged",
        action="store_true",
        help="Incluir arquivos em staging"
    )
    
    group.add_argument(
        "--untracked",
        action="store_true",
        help="Incluir arquivos não rastreados"
    )
    
    group.add_argument(
        "--all",
        action="store_true",
        help="Incluir todos os tipos de arquivos (modificados, staged e untracked)"
    )
    
    parser.add_argument(
        "--flat",
        action="store_false",
        dest="preserve_structure",
        help="Não preservar a estrutura de diretórios (todos os arquivos serão colocados diretamente no diretório de destino)"
    )
    
    args = parser.parse_args()
    
    if not is_git_repo():
        print("Erro: Este diretório não é um repositório Git.")
        return 1
    
    # Definir flags com base nos argumentos
    include_modified = args.modified or args.all
    include_staged = args.staged or args.all
    include_untracked = args.untracked or args.all
    
    # Se nenhuma opção foi selecionada, mostrar ajuda e sair
    if not (include_modified or include_staged or include_untracked):
        print("Erro: Você deve especificar pelo menos um tipo de arquivo para copiar.")
        print("Use --modified, --staged, --untracked ou --all.\n")
        parser.print_help()
        return 1
    
    # Obter arquivos com base nos critérios
    files = get_git_files(
        modified=include_modified,
        staged=include_staged,
        untracked=include_untracked
    )
    
    # Copiar os arquivos
    copy_files(files, args.target_dir, args.preserve_structure)
    
    return 0


if __name__ == "__main__":
    sys.exit(main())
