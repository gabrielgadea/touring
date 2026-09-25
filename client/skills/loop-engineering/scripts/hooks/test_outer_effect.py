#!/usr/bin/env python3
"""The OUTER's EFFECT half — fires once, at the decision point, and hands over the payload.

Opção C (Gabriel, 03/09/2026). These tests pin the three properties that make this a
gate rather than a toll booth, and the fail-open invariant that keeps a broken gate
from ever costing a turn:

  1. it fires on the first MUTATING action (Edit/Write), never on inspection;
  2. it fires AT MOST ONCE per armed cycle;
  3. it DELIVERS the recall in the reason — it does not order the model to go fetch it
     (lesson `nudge-entrega-o-programa`);
  4. every failure path is fail-open: no marker, no cwd, kill switch, broken payload,
     unreadable marker → exit 0, empty stdout, the edit proceeds.

`touring` is deliberately absent from the subprocess PATH, so the recall calls take
their fail-open branch: these assert the HOOK's contract, not the daemon's.
"""
from __future__ import annotations

import json
import os
import shutil
import subprocess
import sys
from pathlib import Path

import pytest

HOOKS = Path(__file__).resolve().parent
EFFECT = HOOKS / "loop_outer_effect.py"
MARKER_CLI = HOOKS / "loop_marker.py"


def _env(home, **extra):
    env = {"PATH": "/usr/bin:/bin", "HOME": str(home),
           "LOOP_ENGINEERING_HOME": str(home)}
    env.update(extra)
    return env


def _arm(home, cwd, session="sess", flow="work-outer"):
    subprocess.run(
        [sys.executable, str(MARKER_CLI), "write", "--task", "OUTER", "--status", "outer",
         "--scope", str(cwd), "--cwd", str(cwd), "--session-id", session, "--flow", flow],
        check=True, capture_output=True, env=_env(home),
    )
    # `loop_marker.py write` does not set the sentinel — loop_outer_arm.arm() does.
    # Set it directly so the unit under test is the effect hook, not the arm hook.
    f = next(x for x in home.iterdir() if x.suffix == ".json")
    d = json.loads(f.read_text())
    d["effect_pending"] = True
    d["topic"] = "assunto do turno"
    f.write_text(json.dumps(d))
    return f


def _fire(home, payload, **envextra):
    proc = subprocess.run(
        [sys.executable, str(EFFECT)], input=json.dumps(payload),
        capture_output=True, text=True, env=_env(home, **envextra), timeout=60,
    )
    assert proc.returncode == 0, f"hook must always exit 0, got {proc.returncode}: {proc.stderr}"
    return proc.stdout.strip()


def _is_deny(out: str) -> bool:
    if not out:
        return False
    hso = json.loads(out).get("hookSpecificOutput", {})
    return hso.get("permissionDecision") == "deny"


def test_blocks_the_first_mutating_action_and_delivers_the_payload(tmp_path):
    cwd = tmp_path / "proj"; cwd.mkdir()
    home = tmp_path / "home"; home.mkdir()
    _arm(home, cwd)
    out = _fire(home, {"tool_name": "Edit", "cwd": str(cwd), "session_id": "sess",
                       "tool_input": {"file_path": str(cwd / "a.rs")}})
    assert _is_deny(out)
    reason = json.loads(out)["hookSpecificOutput"]["permissionDecisionReason"]
    # The nudge must CARRY content, not order a fetch.
    assert "assunto do turno" in reason
    # Concrete executor state, in either of its two honest forms ("n for this cycle" or
    # "nothing yet") — never a placeholder, and never a count that silently folds in
    # artifacts from other cycles.
    assert "deste ciclo" in reason
    assert "<" not in reason.split("Tópico")[0]  # no unresolved placeholder before it


def test_fires_at_most_once_per_cycle(tmp_path):
    cwd = tmp_path / "proj"; cwd.mkdir()
    home = tmp_path / "home"; home.mkdir()
    _arm(home, cwd)
    payload = {"tool_name": "Write", "cwd": str(cwd), "session_id": "sess",
               "tool_input": {"file_path": str(cwd / "a.rs")}}
    assert _is_deny(_fire(home, payload)), "first mutating action must be gated"
    assert not _is_deny(_fire(home, payload)), "second must pass — never interrupt twice"


@pytest.mark.parametrize("tool", ["Read", "Bash", "Grep", "Glob", "Task"])
def test_never_fires_on_non_mutating_tools(tmp_path, tool):
    """Investigation must stay free — the OUTER wants MORE looking, not less."""
    cwd = tmp_path / "proj"; cwd.mkdir()
    home = tmp_path / "home"; home.mkdir()
    _arm(home, cwd)
    assert not _is_deny(_fire(home, {"tool_name": tool, "cwd": str(cwd),
                                     "session_id": "sess", "tool_input": {}}))


def test_no_marker_means_no_gate(tmp_path):
    cwd = tmp_path / "proj"; cwd.mkdir()
    home = tmp_path / "home"; home.mkdir()
    assert not _is_deny(_fire(home, {"tool_name": "Edit", "cwd": str(cwd),
                                     "session_id": "sess", "tool_input": {}}))


def test_kill_switch_disarms(tmp_path):
    cwd = tmp_path / "proj"; cwd.mkdir()
    home = tmp_path / "home"; home.mkdir()
    _arm(home, cwd)
    assert not _is_deny(_fire(home, {"tool_name": "Edit", "cwd": str(cwd),
                                     "session_id": "sess", "tool_input": {}},
                              TOURING_WORK_OUTER_DISABLED="1"))


def test_missing_cwd_never_guesses(tmp_path):
    """A payload without cwd must not fall back to the environment (finding F5, 23/07)."""
    cwd = tmp_path / "proj"; cwd.mkdir()
    home = tmp_path / "home"; home.mkdir()
    _arm(home, cwd)
    assert not _is_deny(_fire(home, {"tool_name": "Edit", "session_id": "sess",
                                     "tool_input": {}}))


def test_malformed_payload_is_fail_open(tmp_path):
    home = tmp_path / "home"; home.mkdir()
    proc = subprocess.run([sys.executable, str(EFFECT)], input="not json",
                          capture_output=True, text=True, env=_env(home), timeout=60)
    assert proc.returncode == 0
    assert proc.stdout.strip() == ""


def test_recall_parses_the_real_envelope_and_labels_the_buckets(monkeypatch):
    """The payload IS this gate's value, so an empty recall makes it hollow.

    Caught live 03/09/2026: the hook reported "no memory matched" for a topic that had
    two memories stored the same hour. Cause — `touring memory recall` emits JSON with
    NO `-j` flag, so the `-j` we passed became a QUERY WORD and matched nothing. A flag
    that degrades into a search term fails silently, which is why this asserts the
    parse POSITIVELY against the real envelope shape instead of checking for absence.
    """
    sys.path.insert(0, str(HOOKS))
    import loop_outer_effect as eff

    envelope = json.dumps({"ann_results": 2, "cases": {
        "positive": [{"key": "k-pos", "value": "abordagem que funcionou"}],
        "negative": [{"key": "k-neg", "value": "padrão a evitar"}],
        "unobserved": [{"key": "k-unk", "value": "sem veredito"}]}})
    seen = {}

    def fake_run(cmd, timeout=None):
        seen["cmd"] = cmd
        return envelope

    monkeypatch.setattr(eff, "_run", fake_run)
    lines = eff._recall("assunto")
    assert "-j" not in seen["cmd"], "recall takes no -j — it would become a query word"
    assert seen["cmd"][:3] == ["touring", "memory", "recall"]
    joined = "\n".join(lines)
    assert "k-pos" in joined and "k-neg" in joined
    # The envelope ships guidance that negatives are patterns to AVOID, never guidance.
    # Flattening the buckets would invert half the payload's meaning.
    assert "reusar" in joined.split("k-pos")[0].split("[")[-1]
    assert "evitar" in joined.split("k-neg")[0].split("[")[-1]


def test_artifact_state_can_come_back_empty(tmp_path, monkeypatch):
    """A status line that cannot report zero is not a status line.

    First live firing said `explore-ledgers=152` — every ledger the project had ever
    accumulated — which reads as "richly done" while nothing existed for this cycle.
    Counted against `flow_armed_at`, the same mtime floor the artifact gate uses.
    """
    sys.path.insert(0, str(HOOKS))
    import loop_outer_effect as eff

    scope = tmp_path / "p"
    (scope / ".touring-explore").mkdir(parents=True)
    old = scope / ".touring-explore" / "antigo.ledger.json"
    old.write_text("{}")
    import os
    os.utime(old, (1000, 1000))  # long before this cycle

    marker = {"scope": str(scope), "bundle": str(tmp_path / "b"),
              "flow_armed_at": 2000.0}
    state = eff._artifact_state(marker)
    assert "152" not in state and "=1" not in state
    assert "nada ainda deste ciclo" in state

    fresh = scope / ".touring-explore" / "novo.ledger.json"
    fresh.write_text("{}")
    os.utime(fresh, (3000, 3000))  # after the floor
    assert "explore-ledgers=1" in eff._artifact_state(marker)


def test_stop_guard_no_longer_blocks_the_outer():
    """Opção C: the OUTER half of the Stop guard must not emit a block decision.

    Guard against silent reversal — the whole point of the redesign is that `Stop`
    became a ruler (it still records to compliance.jsonl via loop_outer_gate) instead
    of a toll booth. The INNER path (loop_converged, Lei L2) keeps its `_block`.
    """
    src = (HOOKS / "loop_stop_guard.py").read_text()
    outer = src.split("def outer_phase_gate")[1].split("\ndef ")[0]
    incomplete = outer.split("report.get(\"complete\")")[-1]
    assert "OUTER incomplete" in incomplete, "the ruler line must survive"
    assert '"decision": "block"' not in incomplete.split("OUTER incomplete")[1], (
        "the OUTER incomplete path must not block — that is the toll booth"
    )


# ── O CONTROLE POSITIVO (04/09/2026) ────────────────────────────────────────────
#
# Os 12 testes acima passaram enquanto o gate marcava 0 de 188 turnos em produção.
# Nenhum deles é falso; eles cobrem etapas VIZINHAS e deixam descoberto o único
# caminho que produção percorre — a extensão indevida de uma garantia real, a
# sétima família da taxonomia cross-session (analise-c1, 03/09/2026):
#
#   test_task_signal.*      exercita assess() com caminho RELATIVO — produção
#                           entrega ABSOLUTO, e `ast blast` responde 0 em silêncio
#                           para absoluto enquanto `ast meta` responde certo;
#   os testes com file_path SEMPRE pré-armam um marker, então o hook entra pelo
#                           ramo "marker existe → deny" e assess() nunca roda;
#   test_no_marker_means_no_gate  passa tool_input {} — sem arquivo. assess("")
#                           retorna None na 1ª linha: o teste passa por CEGUEIRA;
#   o guard estrutural      asserta a string "assess(" no fonte — controle TEXTUAL
#                           lido como COMPORTAMENTAL.
#
# E a cegueira está no próprio fixture: `_env` esconde `touring` do PATH de
# propósito, de modo que NENHUM teste deste arquivo poderia armar.
#
# A regra que sai disso: toda asserção NEGATIVA precisa de um irmão POSITIVO no
# MESMO caminho, com a MESMA forma de entrada. `not deny` casa igualmente com
# "funcionou certo", "não foi chamado" e "foi chamado com o instrumento mudo" —
# indistinguíveis no verde.

REPO = Path("/home/gabrielgadea/projects/touring")
_VIVO = REPO.exists() and shutil.which("touring") is not None


def _env_vivo(home, **extra):
    """O PATH REAL. `_env` omite `touring` de propósito — e é exatamente por isso
    que o caminho de armar nunca foi exercitado. `TOURING_OUTER_NO_SPAWN` mantém o
    executor de fundo fora do teste sem tirar `assess` do circuito."""
    env = {"PATH": os.environ.get("PATH", "/usr/bin:/bin"), "HOME": str(home),
           "LOOP_ENGINEERING_HOME": str(home), "TOURING_OUTER_NO_SPAWN": "1"}
    env.update(extra)
    return env


@pytest.mark.skipif(not _VIVO, reason="exige o workspace touring e o CLI no PATH")
@pytest.mark.parametrize("rel,deve_negar,porque", [
    ("crates/touring-quality/src/verifications/mod.rs", True,
     "144 consumidores — mudança que um recall deve preceder"),
    ("crates/touring-server/src/cli/route.rs", False,
     "1 consumidor — folha, o gate não pode custar nada aqui"),
])
def test_o_caminho_de_armar_pelo_payload_real(tmp_path, rel, deve_negar, porque):
    """SEM marker, `file_path` ABSOLUTO, cwd real: a forma exata que o PreToolUse entrega.

    O par é indivisível de propósito. O caso que NEGA falha quando o caminho quebra
    (foi o que aconteceu: absoluto → blast 0 → L0 → nunca arma). O caso que SILENCIA
    falha quando o sinal satura para cima — o modo pelo qual `cognitive_score` teria
    armado o gate com mais força justamente sobre markdown. Um só dos dois não separa
    "gate correto" de "gate cego".
    """
    home = tmp_path / "home"; home.mkdir()
    payload = {"tool_name": "Edit", "cwd": str(REPO), "session_id": "controle-positivo",
               "tool_input": {"file_path": str(REPO / rel)}}  # ABSOLUTO, como chega de fato
    proc = subprocess.run([sys.executable, str(EFFECT)], input=json.dumps(payload),
                          capture_output=True, text=True, env=_env_vivo(home), timeout=180)
    assert proc.returncode == 0, f"fail-open: exit {proc.returncode} — {proc.stderr}"
    assert _is_deny(proc.stdout.strip()) is deve_negar, porque


# ── O loop de 24/09: uma vez por (task, arquivo), lápide sobrevive ao refresh ──

def _write_fake_touring(bin_dir: Path) -> None:
    """Um `touring` que sempre ARMA: blast 200, route L3, recall vazio."""
    fake = bin_dir / "touring"
    fake.write_text(
        "#!/usr/bin/env python3\n"
        "import json, sys\n"
        "args = sys.argv[1:]\n"
        "if args[:2] == ['ast', 'blast']:\n"
        "    print(json.dumps({'consumers': list(range(200)), 'blast_radius': 200, 'depth': 4, 'symbols': 30}))\n"
        "elif args[:2] == ['ast', 'meta']:\n"
        "    print(json.dumps({'enrichment_source': 'index', 'quality_score': 0.9, 'cognitive_score': 0.9, 'fan_in': 0.9, 'fan_out': 0.8}))\n"
        "elif args and args[0] == 'route':\n"
        "    print(json.dumps({'level': 'L3', 'routing_mode': 'fake', 'composite': 0.9, 'phases': []}))\n"
        "elif args[:2] == ['memory', 'recall']:\n"
        "    print(json.dumps({'cases': {'positive': [], 'negative': [], 'unobserved': []}}))\n"
    )
    fake.chmod(0o755)


def _env_com_fake(home, bin_dir, **extra):
    env = {"PATH": f"{bin_dir}:/usr/bin:/bin", "HOME": str(home),
           "LOOP_ENGINEERING_HOME": str(home), "TOURING_OUTER_NO_SPAWN": "1"}
    env.update(extra)
    return env


def _fire_env(env, payload):
    proc = subprocess.run(
        [sys.executable, str(EFFECT)], input=json.dumps(payload),
        capture_output=True, text=True, env=env, timeout=60,
    )
    assert proc.returncode == 0, f"hook must always exit 0: {proc.stderr}"
    return proc.stdout.strip()


def _marker_file(home):
    return next(x for x in home.iterdir() if x.suffix == ".json")


def _write_marker(home, cwd, task, status, flow="work-outer"):
    subprocess.run(
        [sys.executable, str(MARKER_CLI), "write", "--task", task, "--status", status,
         "--scope", str(cwd), "--cwd", str(cwd), "--session-id", "sess", "--flow", flow],
        check=True, capture_output=True, env=_env(home),
    )
    return _marker_file(home)


def test_o_efeito_dispara_uma_vez_por_task_e_arquivo_mesmo_com_task_real(tmp_path):
    """O loop medido em 24/09 (6 denies seguidos no mesmo rlm.rs): com uma task
    REAL registrada no passo 11, o promote-only preserva task E status="active",
    e o ramo de re-arm (`status != "outer"`) re-armava + re-disparava o efeito
    em CADA edit — o contrato documentado é "não dispara de novo neste ciclo".
    """
    cwd = tmp_path / "proj"; cwd.mkdir()
    home = tmp_path / "home"; home.mkdir()
    fake_bin = tmp_path / "bin"; fake_bin.mkdir()
    _write_fake_touring(fake_bin)
    env = _env_com_fake(home, fake_bin)
    _write_marker(home, cwd, task="task_1790298226705629625", status="active")

    def fire(name):
        return _is_deny(_fire_env(env, {"tool_name": "Edit", "cwd": str(cwd),
                                        "session_id": "sess",
                                        "tool_input": {"file_path": str(cwd / name)}}))

    assert fire("rlm.rs"), "primeiro edit no arquivo dispara"
    assert not fire("rlm.rs"), "segundo edit no MESMO arquivo silencia (o loop de 24/09)"
    assert not fire("rlm.rs"), "e o terceiro"
    assert fire("aco_models_bindings.rs"), "arquivo diferente arma e dispara uma vez"
    assert not fire("aco_models_bindings.rs")
    assert not fire("rlm.rs"), "os tópicos entregues acumulam"


def test_a_lapide_sobrevive_ao_refresh_e_um_ciclo_novo_fala_de_novo(tmp_path):
    """O turn-record regrava o marker via write_marker; se o rebuild derrubar a
    lápide, o gate re-dispara no edit seguinte (foi assim que o loop atravessou
    turns). E um ciclo NOVO (`outer_complete` no anterior) fala de novo."""
    cwd = tmp_path / "proj"; cwd.mkdir()
    home = tmp_path / "home"; home.mkdir()
    fake_bin = tmp_path / "bin"; fake_bin.mkdir()
    _write_fake_touring(fake_bin)
    env = _env_com_fake(home, fake_bin)
    f = _write_marker(home, cwd, task="task_1790298226705629625", status="active")
    payload = {"tool_name": "Edit", "cwd": str(cwd), "session_id": "sess",
               "tool_input": {"file_path": str(cwd / "rlm.rs")}}
    assert _is_deny(_fire_env(env, payload))
    # O turn-record: refresh com a MESMA task/flow — NÃO é ciclo novo.
    _write_marker(home, cwd, task="task_1790298226705629625", status="active")
    assert not _is_deny(_fire_env(env, payload)), "a lápide sobrevive ao refresh"
    # Ciclo NOVO: `outer_complete` no anterior → fala de novo.
    d = json.loads(f.read_text())
    d["outer_complete"] = True
    f.write_text(json.dumps(d))
    _write_marker(home, cwd, task="task_1790298226705629625", status="active")
    assert _is_deny(_fire_env(env, payload)), "ciclo novo dispara de novo"
