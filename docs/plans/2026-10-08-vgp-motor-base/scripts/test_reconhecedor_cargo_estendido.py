"""Reconhecedor de `cargo` ESTENDIDO (HORUST, 10/10/2026) — TDD. O do gerador (`analisar_cargo`) so entende prefixos simples
(`VAR=x`, `timeout N`, `time`, `nice -n N`, `env`, `hw-slot --class X --`); desde a W38 ele deixa de ver ~48% dos comandos `cargo`
dos transcripts: `hw-slot --class rust --wait N --`, `env -u VAR`, `taskset -c`, `bash -c '...'`. Fixtures SINTETICAS com os prefixos reais.
O modulo sob teste vem de `$RECONHECEDOR_MODULO` (o mutante roda sobre uma COPIA).
"""

import importlib.util
import os
import sys
from pathlib import Path

import pytest

SCRIPTS = Path(__file__).resolve().parent
sys.path.insert(0, str(SCRIPTS))
import corpus_rust_natural as crn  # noqa: E402

CAMINHO = Path(os.environ.get("RECONHECEDOR_MODULO", SCRIPTS / "reconhecedor_cargo_estendido.py"))
_spec = importlib.util.spec_from_file_location("reconhecedor_sob_teste", CAMINHO)
rec = importlib.util.module_from_spec(_spec)
sys.modules["reconhecedor_sob_teste"] = rec
_spec.loader.exec_module(rec)


def subs(cmd: str) -> list[str]:
    return [i.sub for i in rec.analisar_cargo_estendido(cmd)]


@pytest.mark.parametrize(
    "cmd",
    [
        "scripts/hw-slot --class rust --wait 3600 -- cargo test -p touring-code",
        "cd ~/projects/touring && ./scripts/hw-slot --class rust -- cargo check -p touring-code",
        "hw-slot --class rust --label r3-eng --wait 5 -- cargo clippy --workspace",
        "env -u TOURING_CODE_MODE timeout 600 cargo clippy --workspace",
        "env -u A -u B VAR=1 cargo build --release",
        "CARGO_BUILD_JOBS=2 taskset -c 0-3 cargo build --release",
        "nohup nice -n 19 cargo test -p x",
        "timeout -k 5 600 cargo nextest run",
        "time cargo check",
    ],
)
def test_reconhece_o_cargo_atras_dos_envoltorios(cmd):
    assert len(subs(cmd)) == 1, cmd


def test_o_original_nao_reconhece_o_que_o_estendido_reconhece():
    cmd = "scripts/hw-slot --class rust --wait 3600 -- cargo test -p touring-code"
    assert crn.analisar_cargo(cmd) == () and subs(cmd) == ["test"]


def test_bash_c_abre_o_script_e_preserva_o_cd_e_os_argumentos():
    inv = rec.analisar_cargo_estendido("cd ~/projects/touring && ./scripts/hw-slot --class rust -- bash -c 'cargo check -p touring-code 2>&1 | tail -3'")
    assert [(i.sub, i.pacotes, i.base) for i in inv] == [("check", ("touring-code",), "~/projects/touring")]
    dentro = rec.analisar_cargo_estendido("hw-slot --class rust --label a --wait 5 -- env -u X bash -c 'cd /p && cargo check -p y'")
    assert [(i.sub, i.pacotes, i.base) for i in dentro] == [("check", ("y",), "/p")]


def test_bash_c_com_varios_cargos_e_separadores():
    assert subs("bash -c 'cargo check -p a && cargo test -p b'") == ["check", "test"]
    assert subs("bash -lc \"set -o pipefail; cargo clippy --workspace | tail -5\"") == ["clippy"]


def test_o_que_o_original_ja_reconhecia_continua_igual():
    cmd = "cd /p && cargo check -p a && cargo test -p b"
    assert rec.analisar_cargo_estendido(cmd) == crn.analisar_cargo(cmd)
    cmd2 = "RUSTFLAGS=-Dwarnings timeout 5 cargo check"
    assert rec.analisar_cargo_estendido(cmd2) == crn.analisar_cargo(cmd2)


@pytest.mark.parametrize(
    "cmd",
    ["echo 'cargo check'", "grep -r 'cargo check' .", "git commit -m 'cargo test fix'", "bash -c 'echo cargo test'", "cat Cargo.toml", "hw-slot --status", "echo cargo check | wc -l"],
)
def test_mencao_a_cargo_nao_e_cargo(cmd):
    assert subs(cmd) == [], cmd


def test_aspas_desbalanceadas_nao_levantam_e_caem_no_original():
    cmd = "cargo check -p a && echo 'sem fechar"
    assert rec.analisar_cargo_estendido(cmd) == crn.analisar_cargo(cmd)
    assert rec.analisar_cargo_estendido("hw-slot --class rust -- bash -c 'cargo check") == ()


def test_nunca_devolve_menos_que_o_analisador_original(monkeypatch):
    """A rede de seguranca: se o desembrulhado achar MENOS invocacoes que o texto original, vale o original."""
    chamadas = []

    def falso(c):
        chamadas.append(c)
        return ("A", "B") if len(chamadas) == 1 else ("C",)

    monkeypatch.setattr(rec, "_ORIGINAL", falso)
    assert rec.analisar_cargo_estendido("x") == ("A", "B")


def test_instala_e_restaura_no_gerador():
    original = crn.analisar_cargo
    rec.instalar()
    try:
        assert crn.analisar_cargo is rec.analisar_cargo_estendido
        assert len(crn.analisar_cargo("hw-slot --class rust --wait 9 -- cargo check")) == 1
    finally:
        rec.restaurar()
    assert crn.analisar_cargo is original
