"""Register the VGP plan subtasks on its DAG, each add verified by its own verdict field (leitura_fiel).

Usage (cwd ~/projects/analise, whose daemon holds the DAG): .venv/bin/python3 registrar_dag.py
"""

import subprocess
import sys

TASK = "task_1791484992466538442"
SUBTASKS = [
    ("D1", "", "dedupe do ~/.claude/settings.json (pre-read/pre-edit/pre-edit-prevention/pre-write 17x cada), com backup e prova por contagem"),
    ("D2", "D1", "achar o instalador nao idempotente que acrescenta as entradas e consertar com teste de idempotencia (rodar 2x = mesmo arquivo)"),
    ("V0a", "", "inventario de TODOS os erros (todas as classes; py/rs/ts; baldes atribuido/inline/nao atribuido) em analise+touring, com controles"),
    ("V0b", "V0a", "classificar cada erro de arquivo do projeto: ainda vive no codigo atual (reproduz) ou ja foi corrigido, por predicado positivo"),
    ("V0c", "V0b", "corrigir TODO erro vivo (TDD: teste vermelho, conserto, mutacao) — ordem do Gabriel 'precisamos corrigir tudo'"),
    ("V0d", "V0a", "corpus de replay: edicoes que introduziram os erros + >=300 edicoes Python bem-sucedidas sorteadas (semente fixa)"),
    ("V1", "V0d", "indice do touring com parent/membros/assinatura (py, ts, rs) + ast detail com caminho absoluto (DEPOIS do deploy da S8)"),
    ("V2", "V1", "camada 1 nativa em sombra em pre-edit-prevention, pre-write e MultiEdit; aceite recall>=70% falso deny<=1% p95<=150ms"),
    ("V3", "V2", "camada 2: adaptador do verificador do projeto (pyright/ty/mypy, tsc) em sombra; p95<=1s"),
    ("V4", "V3", "deny ligado nas classes de alta precisao; recusa ensina os membros reais; 5 sessoes reais sem falso deny"),
    ("V5", "V2", "codigo inline (pre-bash python3 -/-c, touring run --code) com o mesmo verificador, em aviso"),
    ("V6", "V4,V5", "vgp-protocol.md encolhido para ponteiro agnostico; docs + skill Touring em co-evolucao; juiz de drift verde"),
    ("J", "D2,V0c,V6", "juiz final do plano por exit code + memoria + reward + relatorio ao Gabriel"),
]


def add(sid: str, deps: str, desc: str) -> int:
    cmd = [sys.executable, "-m", "scripts.utils.leitura_fiel", "--exigir", "subtask_added", "--", "touring",
           "decompose", "add", TASK, sid, desc]
    if deps:
        cmd[-1:-1] = [f"--depends-on={deps}"]
    p = subprocess.run(cmd, capture_output=True, text=True)
    print(f"{sid}: rc={p.returncode} {(p.stdout + p.stderr).strip().splitlines()[-1][:160]}")
    return p.returncode


if __name__ == "__main__":
    sys.exit(max(add(*s) for s in SUBTASKS))
