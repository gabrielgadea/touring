#!/usr/bin/env python3
"""Mutantes do conserto G1 (v2): aplica, roda os testes G1, restaura COM touch e confere o sha256."""
import hashlib
import os
import shutil
import subprocess
import sys
from pathlib import Path

ALVO = Path("/home/gabrielgadea/projects/touring-wt-g1/crates/touring-cli/src/cli_suggester.rs")
COPIA = Path(sys.argv[1]) / "cli_suggester.rs.pristino_v2"
ENV = {**os.environ, "CARGO_TARGET_DIR": os.path.expanduser("~/projects/touring/target")}

INV = "    burst_ledger().invalidate(&key);\n    // S212 no G1"
ENT = (
    "    if !ovr.g1_rewrite_disabled {\n"
    "        record_gate_event(GateId::G1, GateEvent::Emitted);\n"
    "        record_gate_event(GateId::G1, GateEvent::Followed);"
)

# cada mutante é uma lista de (trecho, substituto); cada trecho tem de ser ÚNICO no arquivo
MUTANTES = {
    "M-a remove o invalidate": [(INV, "    // S212 no G1")],
    "M-b volta o texto antigo do advisory": [(
        "{G1_DEMOTE_FLOOR}] {count}ª inspeção `{class}` desde o último disparo. \\\n"
        "                 ESTE comando roda NATIVO e intacto — não o repita. O programa abaixo é \\\n"
        "                 o FORMATO para a próxima rajada: junte as inspeções seguintes num só \\\n"
        "                 `touring run`:\\n  {programa}",
        "{G1_DEMOTE_FLOOR}] {count}ª inspeção `{class}` na janela. A rajada \\\n"
        "                 acumulada já é o programa:\\n  {programa}",
    )],
    "M-c volta o texto antigo da razão por contagem": [(
        "{count}ª inspeção da classe `{class}` desde o último disparo (janela de \\\n"
        "             {CODE_MODE_WINDOW_SECS}s) — 90% das rajadas ≥{G1_DENY_AT} continuavam \\\n"
        "             iguais (51 disparos/55 sessões)",
        "{count}ª inspeção da classe `{class}` em {CODE_MODE_WINDOW_SECS}s — 90% \\\n"
        "             das rajadas ≥{G1_DENY_AT} continuavam iguais (51 disparos/55 sessões)",
    )],
    "M-d invalidate DEPOIS do advisory (posição antiga)": [
        (INV, "    // S212 no G1"),
        (ENT, "    burst_ledger().invalidate(&key);\n" + ENT),
    ],
    "M-e ignora a precisão injetada (lê só o contador global)": [(
        "g1_should_deny(ovr.g1_precision.unwrap_or_else(g1_live_precision))",
        "g1_should_deny(g1_live_precision())",
    )],
}


def sha(p: Path) -> str:
    return hashlib.sha256(p.read_bytes()).hexdigest()


shutil.copy2(ALVO, COPIA)
sha0 = sha(ALVO)
print(f"sha256 inicial : {sha0}")
ok = True
for nome, trocas in MUTANTES.items():
    src = ALVO.read_text(encoding="utf-8")
    contagens = [src.count(v) for v, _ in trocas]
    print(f"\n=== {nome}  (ocorrências de cada trecho: {contagens})")
    if any(c != 1 for c in contagens):
        print("ABORTA: trecho não único — mutante inválido")
        ok = False
        continue
    mut = src
    for velho, novo in trocas:
        mut = mut.replace(velho, novo)
    try:
        ALVO.write_text(mut, encoding="utf-8")
        print(f"sha256 mutado  : {sha(ALVO)}")
        r = subprocess.run(
            ["cargo", "test", "-p", "touring-cli", "--lib", "g1_no_replay_tests"],
            cwd=ALVO.parents[3], env=ENV, capture_output=True, text=True,
        )
        for l in (r.stdout + r.stderr).splitlines():
            if l.startswith("test ") or l.startswith("test result") or "error[" in l:
                print(l)
        print(f"cargo test rc={r.returncode}")
    finally:
        shutil.copyfile(COPIA, ALVO)
        os.utime(ALVO)  # touch: o cargo decide fresco por mtime
        s1 = sha(ALVO)
        print(f"sha256 restaur.: {s1}  {'IGUAL' if s1 == sha0 else 'DIVERGE!!'}")
        ok = ok and s1 == sha0
sys.exit(0 if ok else 1)
