#!/usr/bin/env python3
"""Testes do S-4: o congelamento e a partição que os critérios exigem.

Os três testes nomeados no plano (plan.md:94):
  · test_check_detects_a_single_changed_byte
  · test_no_session_crosses_partitions
  · test_audit_sample_is_balanced_by_b0

## HERMÉTICO — e por que isso é uma propriedade testada, não uma promessa

A 1ª versão destes testes rodava `r3_freeze_dataset.py` por `subprocess` contra o
**corpus real** (`~/.claude/touring/r3-data/`, `docs/plans/…/data/`), e o `finally`
regravava tudo a cada execução. Dois defeitos, ambos medidos pelo orquestrador:

1. **Renumeração silenciosa.** Os `item_id` são posicionais (`a2-00000`…), então a
   primeira oferta nova — e esta sessão gera denies o tempo todo — renumera tudo e
   embaralha rótulo→item no meio da adjudicação. O conteúdo era idêntico por hash, o
   que prova que o estrago é de IDENTIDADE, não de conteúdo.
2. **CI quebrado.** No CI não existe `~/.claude/touring/r3-data/`.

Por isso o fixture `sandbox` redireciona `REPO`/`DATA_DIR`/`EXTERNAL` para um tmp, e
`test_o_teste_nao_toca_o_corpus_real` prova isso por mtime+hash antes e depois.
"""
from __future__ import annotations

import hashlib
import json
import os
import shutil
import subprocess
import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).parent))

import r3_freeze_dataset as fz  # noqa: E402

SCRIPT = Path(__file__).parent / "r3_freeze_dataset.py"
# o nome do campo que o freeze grava, para o corpus real na prova de não-toque
AUDIT_REAL = Path("~/.claude/touring/r3-data/audit_sample.jsonl").expanduser()


def _sha(p: Path) -> str:
    return hashlib.sha256(p.read_bytes()).hexdigest() if p.exists() else "<ausente>"


def _st_mtime(p: Path) -> float:
    return p.stat().st_mtime if p.exists() else -1.0


@pytest.fixture
def sandbox(tmp_path: Path, monkeypatch: pytest.MonkeyPatch):
    """Um CONGELAMENTO INTEIRO dentro de um tmp: repo, data_dir e corpus externos.

    Nada aqui toca `~/.claude/touring/r3-data/` nem `docs/plans/…/data/`. O
    `r3_freeze_dataset` lê os caminhos de CONSTANTES DE MÓDULO, então redirecioná-las
    é o que isola — e é por isso que o teste de não-toque existe: uma constante nova
    lida direto do disco passaria despercebida.
    """
    repo = tmp_path / "repo"
    data = repo / "docs/plans/2026-10-02-jev-harness-r3/data"
    data.mkdir(parents=True)
    scripts = repo / "scripts"
    scripts.mkdir()
    for nome in ("r3_use_cases.py", "r3_b0.py", "r3_extract_a1_blocks.py"):
        shutil.copy(Path(__file__).parent / nome, scripts / nome)
    shutil.copy(Path(__file__).parent.parent / "docs/plans/2026-10-02-jev-harness-r3"
                / "CRITERIOS-ACEITACAO.md",
                repo / "docs/plans/2026-10-02-jev-harness-r3/CRITERIOS-ACEITACAO.md")

    externo = tmp_path / "externo"
    externo.mkdir()
    # um corpus mínimo, com a MESMA FORMA que o extrator produz
    _escrever_corpus_minimo(externo)

    monkeypatch.setattr(fz, "REPO", repo)
    monkeypatch.setattr(fz, "DATA_DIR", data)
    monkeypatch.setattr(fz, "EXTERNAL", externo)
    monkeypatch.setattr(fz, "CRITERIOS", repo / "docs/plans/2026-10-02-jev-harness-r3/CRITERIOS-ACEITACAO.md")
    monkeypatch.setattr(fz, "USE_CASES", scripts / "r3_use_cases.py")
    monkeypatch.setattr(fz, "R3", scripts)
    monkeypatch.setattr(fz, "MANIFEST", data / "MANIFEST.sha256")
    monkeypatch.setattr(fz, "SPLITS", data / "splits.json")
    monkeypatch.setattr(fz, "AUDIT", externo / "audit_sample.jsonl")
    monkeypatch.setattr(fz, "EXEMPT_MARKERS_OUT", data / "exempt_markers.json")
    return {"repo": repo, "data": data, "externo": externo}


def _escrever_corpus_minimo(externo: Path) -> None:
    """Duas ofertas de A2 e um punhado de A1, com os campos que o freeze lê."""
    # Cada sessão carrega OS DOIS estratos de B0. Uma sessão de estrato único cai
    # inteira numa metade (a sessão é indivisível, §1) e o guard `not_followed`
    # reprovaria o congelamento — o que é o comportamento correto, e o que
    # torna este corpus um caso de teste legítimo e não um atalho.
    ofertas = []
    for i, sess in enumerate(["aaaa1111", "bbbb2222", "cccc3333"]):
        for j in range(4):
            dec = "not_followed" if j % 2 == 0 else "followed"
            ofertas.append({
                "session": sess, "ts": f"2026-09-0{i + 1}T10:00:0{j}.000Z",
                "lang": "bash", "rota_proposta": "cat /a/x.rs /b/y.rs",
                "janela_k": 3, "cmds": ["touring run --lang bash --code 'cat /a/x.rs /b/y.rs'"],
                "n_cmds": 1, "cortada_em_nova_oferta": False,
                "b0_decisao": dec, "b0_valor": 0.0 if dec == "not_followed" else 1.0,
            })
    (externo / "a2_offers.jsonl").write_text(
        "\n".join(json.dumps(o, ensure_ascii=False) for o in ofertas) + "\n", encoding="utf-8")
    blocos = [{"session": "aaaa1111", "ts": "2026-09-01T10:00:00.000Z", "turno": i,
               "attachment_type": "hook_success", "family": "touring-suggest",
               "is_touring_family": True, "is_exempt": False, "is_duplicate": False,
               "bytes": 100, "propositions": 1, "head": "x"} for i in range(4)]
    (externo / "a1_blocks.jsonl").write_text(
        "\n".join(json.dumps(b, ensure_ascii=False) for b in blocos) + "\n", encoding="utf-8")


def _rodar(sandbox, *args) -> subprocess.CompletedProcess:
    """Roda o freeze como SUBPROCESSO, para provar o caminho de `main()` também.

    O `R3_FAKE_REPO` é OBRIGATÓRIO aqui: o monkeypatch do pytest vale só neste
    processo, e o `--check` é um processo novo — sem a variável ele cairia no
    default de produção e reescreveria o corpus vivo. Foi assim que a 1ª versão
    do teste passou "verde" tocando o arquivo real.
    """
    env = dict(os.environ)
    env["R3_FAKE_REPO"] = str(sandbox["repo"])
    env["R3_FAKE_EXTERNAL"] = str(sandbox["externo"])
    return subprocess.run([sys.executable, str(SCRIPT), *args],
                          capture_output=True, text=True, cwd=str(sandbox["repo"]), env=env)


def _ofertas(n_sessoes: int = 6, por_sessao: int = 10, seed: int = 7) -> list[dict]:
    out = []
    for s in range(n_sessoes):
        for i in range(por_sessao):
            nf = (s % 2 == 0 and i % 3 == 0)
            out.append({
                "session": f"s{s:04d}",
                "ts": f"2026-09-{(s % 28) + 1:02d}T{i:02d}:00:00.000Z",
                "b0_decisao": "not_followed" if nf else "followed",
                "b0_valor": 0.0 if nf else 1.0,
                "n_cmds": 3, "cmds": ["touring run --lang bash --code 'cat /a/x.rs /b/y.rs'"],
                "rota_proposta": "cat /a/x.rs /b/y.rs",
            })
    return out


# ---------------------------------------------------------------- partições

def test_no_session_crosses_partitions() -> None:
    """§1 dos critérios: "nenhum evento de uma sessão cruza partições".

    A 1ª versão estratifikava por B0 DENTRO da sessão e partia 12 sessões; este
    teste é o que impede que isso volte.
    """
    val, test, diag = fz.dividir_audit(_ofertas())
    assert not ({o["session"] for o in val} & {o["session"] for o in test}), \
        f"sessão nas duas metades: {diag['sessoes_cruzadas']}"
    assert diag["sessoes_cruzadas"] == []
    assert len(val) + len(test) == diag["n_total"], "nenhuma oferta pode ser perdida"


def test_audit_sample_is_balanced_by_b0() -> None:
    """E3.1: as DUAS metades carregam os DOIS estratos de B0."""
    ofertas = _ofertas(n_sessoes=8, por_sessao=12)
    val, test, _ = fz.dividir_audit(ofertas)
    for metade, nome in ((val, "audit-val"), (test, "audit-test")):
        decisoes = {o["b0_decisao"] for o in metade}
        assert "not_followed" in decisoes, f"{nome} sem not_followed (E3.1)"
        assert "followed" in decisoes, f"{nome} sem followed (E3.1)"


def test_split_is_deterministic_under_the_seed() -> None:
    ofertas = _ofertas()
    a = fz.dividir_audit(ofertas, seed=fz.SEED)[2]
    b = fz.dividir_audit(ofertas, seed=fz.SEED)[2]
    assert a["sessoes_val"] == b["sessoes_val"]
    assert a["sessoes_test"] == b["sessoes_test"]


def test_proportion_is_forty_sixty_not_half_half() -> None:
    """A proporção da E3.1 é 40/60. Um 50/50 passaria em quase todo teste de
    sanidade — o que o distingue é a razão das metades."""
    _, _, diag = fz.dividir_audit(_ofertas(n_sessoes=10, por_sessao=20))
    razao = diag["n_val"] / max(1, diag["n_val"] + diag["n_test"])
    assert fz.SPLIT_AUDIT == (0.4, 0.6), f"a constante da E3.1 mudou: {fz.SPLIT_AUDIT}"
    assert 0.30 <= razao <= 0.50, f"audit-val fora da banda 40/60: {razao:.3f}"


def test_seed_desempata_somente_sessoes_iguais() -> None:
    """A seed tem ESCOPO estreito, e este teste fixa esse escopo.

    A divisão é determinística pelo tamanho e pela composição de B0; a seed só
    entra no desempate de sessões de mesmo tamanho e mesma composição. Num corpus
    onde nenhuma sessão é indistinguível, mudar a seed NÃO muda o split — e isso é
    o comportamento CORRETO (reprodutibilidade), não cobertura faltando.
    """
    ofertas = _ofertas(n_sessoes=6, por_sessao=10)
    for seed in (1, 2, fz.SEED):
        diag = fz.dividir_audit(ofertas, seed=seed)[2]
        assert diag["n_val"] > 0 and diag["n_test"] > 0, f"split degenerado com seed={seed}"
        assert diag["sessoes_cruzadas"] == [], f"sessão cruzada com seed={seed}"
    a = fz.dividir_audit(ofertas, seed=fz.SEED)[2]
    b = fz.dividir_audit(ofertas, seed=fz.SEED)[2]
    assert a["sessoes_val"] == b["sessoes_val"] and a["sessoes_test"] == b["sessoes_test"]


# ------------------------------------------------------- congelamento real

def test_check_detects_a_single_changed_byte(sandbox) -> None:
    """A lição 4 do §0: GRAVAR NÃO É CONGELAR.

    Roda o congelamento no SANDBOX (o fixture redireciona tudo), mexe UM BYTE de
    um artefato e exige que o `--check` reprove nomeando o arquivo.
    """
    r = _rodar(sandbox)
    assert r.returncode == 0, f"congelamento falhou: {r.stdout}{r.stderr}"
    assert fz.MANIFEST.exists(), "sem manifesto após congelar"

    ok = _rodar(sandbox, "--check")
    assert ok.returncode == 0, f"um byte em um arquivo íntegro não pode reprovar: {ok.stdout}"

    artefato = fz.SPLITS
    backup = artefato.read_bytes()
    artefato.write_bytes(backup[:-1] + b" ")   # UM byte
    try:
        ruim = _rodar(sandbox, "--check")
        assert ruim.returncode == 1, "1 byte alterado tem de reprovar o --check"
        assert "MUDOU" in ruim.stdout and "splits.json" in ruim.stdout, (
            f"o --check tem que NOMEAR o arquivo: {ruim.stdout}"
        )
    finally:
        artefato.write_bytes(backup)


def test_o_teste_nao_toca_o_corpus_real(sandbox) -> None:
    """A propriedade que o orquestrador mediu: rodar esta suíte NÃO pode reescrever
    o corpus vivo.

    A 1ª versão rodava o freeze contra `~/.claude/touring/r3-data/` e o `finally`
    regravava tudo — conteúdo idêntico por hash (o estrago é de IDENTIDADE, porque
    os `item_id` são posicionais), mas o teste ainda era o agente de escrita.
    """
    antes_sha, antes_mtime = _sha(AUDIT_REAL), _st_mtime(AUDIT_REAL)
    antes_manifest = _sha(fz.MANIFEST) if fz.MANIFEST.exists() else None
    antes_splits = _sha(fz.SPLITS) if fz.SPLITS.exists() else None

    for args in ([], ["--check"]):
        _rodar(sandbox, *args)

    assert _sha(AUDIT_REAL) == antes_sha, (
        "o teste REESCREVEU o audit_sample real — os item_id são posicionais e uma "
        "oferta nova embaralha rótulo→item no meio da adjudicação"
    )
    assert _st_mtime(AUDIT_REAL) == antes_mtime, "o teste TOCOU o mtime do corpus real"
    if antes_manifest is not None:
        assert _sha(fz.MANIFEST) == antes_manifest, "o teste reescreveu o MANIFEST real"
    if antes_splits is not None:
        assert _sha(fz.SPLITS) == antes_splits, "o teste reescreveu o splits.json real"


def test_manifest_covers_data_and_the_code_that_defines_it(sandbox) -> None:
    """Congelar o dado sem congelar a REGRA congela o número, não o significado."""
    r = _rodar(sandbox)
    assert r.returncode == 0, r.stdout + r.stderr
    caminhos = [l.split("  ", 1)[1] for l in fz.MANIFEST.read_text().splitlines() if l.strip()]
    assert any("a2_offers.jsonl" in c for c in caminhos), "o dataset A2 tem de estar"
    assert any("a1_blocks.jsonl" in c for c in caminhos), "o dataset A1 tem de estar"
    assert any("r3_use_cases.py" in c for c in caminhos), "a regra tem de estar"
    assert any("CRITERIOS-ACEITACAO.md" in c for c in caminhos), "os critérios têm de estar"


def test_audit_sample_carries_item_id_for_the_pack(sandbox) -> None:
    """O `pack` do S-5 consome `item_id` — sem ele o pacote cego não embaralha."""
    r = _rodar(sandbox)
    assert r.returncode == 0, r.stdout + r.stderr
    linhas = [json.loads(l) for l in fz.AUDIT.read_text().splitlines() if l.strip()]
    assert linhas, "audit_sample vazio"
    assert all("item_id" in r for r in linhas), "todo item precisa de item_id"
    ids = [r["item_id"] for r in linhas]
    assert len(set(ids)) == len(ids), "item_id tem de ser único"
    assert all(r["split"] in ("audit-val", "audit-test") for r in linhas)
