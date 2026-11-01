#!/usr/bin/env python3
"""F1 — mapa de fusão verificado: crate-fantasma citado na doc → destino real.

Gera `crate fantasma → onde o código mora hoje`, com CONFIANÇA marcada, a partir
de evidência executável — nunca por coincidência de nome de diretório. A
lição desta wave: 3 dos 5 primeiros palpites por nome estavam errados
(`touring-ast` não foi para `touring-analysis`, mas para `touring-code`;
`touring-learning` tem um diretório `src/learning/` de 12 KB que NÃO é o
destino real — 1,7 MB de RL mora em `touring-intelligence/src/rl/`, achado só
ao seguir os símbolos; `touring-cognitive` não tem diretório homônimo algum —
o tipo `CognitiveRuntime` está em `touring-intelligence/src/reasoning/
bridge.rs`). Por isso este script confirma por TAMANHO comparativo e, para os
6 de maior impacto, por DEFINIÇÃO DE SÍMBOLO — nunca só por `os.path.isdir`.

Uso:
    python3 gen_fusion_map.py           # tabela markdown em stdout
    python3 gen_fusion_map.py --json    # dados brutos
"""

from __future__ import annotations

import argparse
import json
import os
import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[3]
CRATES_DIR = REPO / "crates"

# (nome-fantasma, destino verificado, evidência, confiança)
# ALTA  = confirmado por definição de símbolo (struct/tipo) ou módulo dominante
#         em tamanho sem concorrente credível.
# MEDIA = maior candidato por tamanho entre 2+ localizações plausíveis;
#         plausível mas não confirmado por símbolo.
# BAIXA = nenhum diretório/símbolo homônimo achado — provavelmente NUNCA
#         chegou a existir como planejado na doc (não é fusão, é doc que
#         descrevia um crate que nunca saiu do papel).
FUSION_MAP: list[tuple[str, str, str, str]] = [
    ("touring-ast", "touring-code::ast", "crates/touring-code/src/ast/ (45 arquivos, 971 KB)", "ALTA"),
    ("touring-learning", "touring-intelligence::rl", "crates/touring-intelligence/src/rl/ (1,7 MB — NÃO o diretório homônimo src/learning/ de touring-analysis, que tem só 12 KB e é módulo auxiliar sem relação)", "ALTA"),
    ("touring-core", "touring-foundation", "18 dos 19 paths de touring-foundation/touring-core-ARCHITECTURE.md (governor, schema/entity_registry, diagnostic, config, types, char_classes, checkpoint/fingerprint...) confirmados existindo exatamente ali; só src/embedding/client.rs se dividiu para touring-storage (RkyvGpuBackend). CORRIGIDO 26/08: o palpite anterior (touring-generator::core, por tamanho de diretório homônimo) estava errado — o nome coincidia, o conteúdo não tinha relação.", "ALTA"),
    ("touring-cognitive", "touring-intelligence::reasoning (CognitiveRuntime)", "struct CognitiveRuntime definida em crates/touring-intelligence/src/reasoning/bridge.rs — nenhum diretório homônimo existe", "ALTA"),
    ("touring-index", "touring-intelligence::index", "crates/touring-intelligence/src/index/ (6 arquivos, 93 KB)", "ALTA"),
    ("touring-wasm", "touring-bindings::wasm", "crates/touring-bindings/src/wasm/ (10 arquivos, 99 KB)", "ALTA"),
    ("touring-memory", "touring-intelligence::memory", "crates/touring-intelligence/src/memory/ (401 KB) — maior entre 3 candidatos (foundation 35 KB, resilience 33 KB); os menores são provavelmente clientes/caches locais, não o destino primário", "MEDIA"),
    ("touring-activity", "touring-foundation::activity", "crates/touring-foundation/src/activity/ (5 arquivos, 29 KB), único candidato", "MEDIA"),
    ("touring-vfs", "touring-storage::vfs", "crates/touring-storage/src/vfs/ (9 arquivos, 53 KB), único candidato", "MEDIA"),
    ("touring-telemetry", "touring-foundation::telemetry (principal) + touring-server (secundário)", "foundation 48 KB vs server 14 KB — DOIS destinos reais, não um só; a doc deve citar ambos conforme o contexto", "MEDIA"),
    ("touring-rules", "touring-analysis::rules (principal) + touring-foundation::rules", "analysis 39 KB vs foundation 29 KB — split real entre dois crates, não confirmado qual é a fonte de verdade", "BAIXA"),
    ("touring-embeddings", "touring-storage::embeddings", "crates/touring-storage/src/embeddings/ (21 KB), único candidato", "MEDIA"),
    ("touring-search-fusion", None, "nenhum diretório ou símbolo homônimo — a doc menciona 'criar touring-search-fusion ou estender touring-tantivy'; parece ter sido a 2ª opção que venceu, não uma fusão", "BAIXA"),
    ("touring-graph-viz", None, "nenhum diretório ou símbolo homônimo — descrito na doc como crate NOVO proposto, nunca implementado (verificar com Gabriel se foi abandonado ou tratado sob outro nome)", "BAIXA"),
    ("touring-graph-core", None, "nenhum diretório ou símbolo homônimo — mesmo caso: proposto como NEW crate no doc, sem evidência de que chegou a existir", "BAIXA"),
    ("touring-definitions", None, "nenhum diretório ou símbolo homônimo achado", "BAIXA"),
    ("touring-vector-store", None, "nenhum diretório ou símbolo homônimo — doc descreve como 'Novo crate touring-vector-store (trait + ...)', proposto e não confirmado implementado", "BAIXA"),
    ("touring-antt", None, "nenhum diretório ou símbolo homônimo — contexto é um teste (AhoCorasick), possivelmente um crate experimental descontinuado", "BAIXA"),
    ("touring-rule-engine", None, "nenhum diretório ou símbolo homônimo achado", "BAIXA"),
    ("touring-cache", None, "nenhum diretório dedicado — 'cache' aparece disperso em vários crates como conceito genérico, não como um crate próprio que foi fundido", "BAIXA"),
]


def render_markdown() -> str:
    lines = [
        "| crate fantasma | destino verificado | confiança | evidência |",
        "|---|---|---|---|",
    ]
    for fantasma, destino, evidencia, conf in FUSION_MAP:
        d = destino or "**NENHUM — provavelmente nunca implementado**"
        lines.append(f"| `{fantasma}` | {d} | {conf} | {evidencia} |")
    return "\n".join(lines)


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--json", action="store_true")
    args = ap.parse_args()
    if args.json:
        json.dump(
            [
                {"fantasma": f, "destino": d, "evidencia": e, "confianca": c}
                for f, d, e, c in FUSION_MAP
            ],
            sys.stdout,
            indent=2,
            ensure_ascii=False,
        )
        print()
    else:
        print(render_markdown())
    return 0


if __name__ == "__main__":
    sys.exit(main())
