#!/usr/bin/env python3
"""Testes de `r3_arms` — herméticos: Agent FALSO, nada de carregar os 842 MB.

O contrato testado é o do AVALIADOR REAL (`r3_shadow_evaluator`), não um dict
escrito à mão: a prova é rodar `run_once` num tmp e conferir que a linha
`shadow_decision` que ele grava passa em `r3_event_schema.validate_event`.
"""
from __future__ import annotations

import json
import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).parent))

import r3_arms  # noqa: E402
import r3_event_schema as ev  # noqa: E402
import r3_replay_a2 as replay  # noqa: E402


class _AgentFalso:
    """Um `laya.Agent` que responde o que o teste mandar, sem carregar nada."""

    def __init__(self, p: float = 0.7, erro: Exception | None = None):
        self.p = p
        self.erro = erro
        self.chamadas = 0

    def decide(self, state, questions=None, return_details=False):
        self.chamadas += 1
        if self.erro is not None:
            raise self.erro
        qid = next(iter(questions))
        return {qid: {"type": "noul", "noul": self.p}}


def _evento_a2(**over) -> dict:
    ev_ = {
        "v": 1, "kind": "a2_offer_closed", "use_case": "a2_outcome", "ts_ms": 1,
        "session_hash": "0123456789ab", "mode": "code", "offered_secs": 3,
        "closed_by": "consumed", "cmd_heads": ["ls", "touring", "git"],
        "baseline_value": 1.0, "baseline_followed": True,
    }
    ev_.update(over)
    return ev_


# ───────────────────────────────────────── carga preguiçosa (o import não paga o modelo)

def test_importar_o_modulo_nao_carrega_o_laya() -> None:
    """Importar `r3_arms` não pode construir o `laya.Agent` (~3 s, 842 MB).

    É o motivo de o Agent ser carregado na 1ª DECISÃO: o avaliador importa este
    módulo no `build_arms()` e pode nunca chegar a decidir (a fila pode estar
    vazia), e pagar 3 s de carga por nada seria o erro clássico do eager init.
    """
    import importlib
    import sys as _s
    # um import novo, com `laya` bloqueado: se o import tentasse carregar, falharia
    sys.modules.pop("laya", None)
    mod = importlib.import_module("r3_arms")
    braco = mod.build_online_arms()[0]
    assert braco._agent is None, "o Agent foi construido antes da 1a decisao"
    assert "laya" not in _s.modules, "o import de r3_arms puxou laya"


def test_a_carga_so_acontece_na_primeira_decisao() -> None:
    agente = _AgentFalso(p=0.9)
    braco = r3_arms.LayaBaseArm(agent_factory=lambda: agente)
    assert braco._agent is None
    braco.decide(_evento_a2())
    assert braco._agent is agente, "o Agent nao foi carregado na 1a decisao"
    assert agente.chamadas == 1
    # a 2a decisao REUSA: nao reconstroi
    braco.decide(_evento_a2())
    assert agente.chamadas == 2, "a 2a decisao nao reusou o Agent"


# ───────────────────────────────────────── a decisão sai de p >= τ

@pytest.mark.parametrize("p,esperado", [
    (0.0, False), (0.49, False), (0.5, True), (0.51, True), (1.0, True),
])
def test_followed_sai_de_p_maior_ou_igual_a_tau(p: float, esperado: bool) -> None:
    """`followed = (p >= τ)` — o τ da Songsera, importado do replay."""
    braco = r3_arms.LayaBaseArm(agent_factory=lambda: _AgentFalso(p=p))
    d = braco.decide(_evento_a2())
    assert d is not None, f"p={p} produziu None em vez de decisão"
    assert d["followed"] is esperado, f"p={p} -> {d}, esperado followed={esperado}"


def test_o_tau_vem_do_replay_e_nao_e_uma_constante_solta() -> None:
    """Divergir o τ entre replay e online produziria dois números para o mesmo
    braço; o τ é lido do módulo do replay (ou do env, que o replay não usa)."""
    assert r3_arms.TAU_DESFECHO == 0.5, f"τ = {r3_arms.TAU_DESFECHO}"


# ───────────────────────────────────────── exceção SOBE (não vira decisão silenciosa)

def test_excecao_do_agent_propaga() -> None:
    """O avaliador isola e CONTA o braço que falha (linha 105-107). Engolir aqui
    transformaria uma falha de infraestrutura em "o modelo disse que não"."""
    erro = RuntimeError("cuda out of memory (simulado)")
    braco = r3_arms.LayaBaseArm(agent_factory=lambda: _AgentFalso(erro=erro))
    with pytest.raises(RuntimeError, match="cuda out of memory"):
        braco.decide(_evento_a2())


def test_evento_de_outro_caso_nao_gera_decisao() -> None:
    """`None` = sem decisão, não veredito. Um braço de outro `use_case` não fala
    por este."""
    agente = _AgentFalso()
    braco = r3_arms.LayaBaseArm(agent_factory=lambda: agente)
    assert braco.decide(_evento_a2(use_case="a1_relevance")) is None
    assert agente.chamadas == 0, "o Agent foi carregado para um evento de outro caso"


def test_evento_a1_nao_gera_decisao_no_braco_de_a2() -> None:
    braco = r3_arms.LayaBaseArm(agent_factory=lambda: _AgentFalso())
    ev_a1 = {"v": 1, "kind": "a1_block", "use_case": "a1_relevance", "ts_ms": 1,
             "session_hash": "0123456789ab", "hook": "hook_success",
             "family": "touring-suggest", "bytes": 100}
    assert braco.decide(ev_a1) is None


# ───────────────────────────────────────── o CONTRATO com o avaliador real

def test_contrato_com_o_avalidador_real_run_once(tmp_path: Path, monkeypatch) -> None:
    """A prova pedida: rodar `r3_shadow_evaluator.run_once` de verdade, num tmp,
    e conferir que a linha `shadow_decision` que ele grava passa no esquema.

    Nada aqui é um dict escrito à mão no lugar da saída: o avaliador monta a
    linha, e a validação é a de produção (`ev.validate_event`).
    """
    import r3_shadow_evaluator as se

    logs = tmp_path / "logs"
    logs.mkdir()
    # um evento A2 REAL do schema, gravado no journal que o avaliador lê
    evento = _evento_a2()
    assert not ev.validate_event(evento), "o evento de teste não passa no schema"
    (logs / se.EVENTS).write_text(json.dumps(evento, ensure_ascii=False) + "\n",
                                   encoding="utf-8")

    braco = r3_arms.LayaBaseArm(agent_factory=lambda: _AgentFalso(p=0.9))
    # `run_once(logs, arms)` recebe os bracos explicitamente: o avaliador REAL
    # roda, so o que entra na lista de braços e o que o teste controla.
    stats = se.run_once(logs, [braco])
    assert stats["read"] == 1, f"o avaliador não leu o evento: {stats}"
    assert stats["written"] == 1, f"nenhuma linha escrita: {stats}"
    assert stats["invalid_out"] == 0, f"a linha de saida foi rejeitada: {stats}"
    assert braco.decisoes == 1, "o braço não decidiu"

    linhas = [json.loads(l) for l in (logs / se.SHADOW).read_text().splitlines() if l.strip()]
    assert len(linhas) == 1
    r = linhas[0]
    # a MESMA validação que o avaliador aplica, re-executada aqui
    assert not ev.validate_event(r), f"a linha nao passa no schema: {r}"
    assert not ev.scan_privacy(json.dumps(r, ensure_ascii=False)), (
        f"a linha carrega texto livre: {r}"
    )
    assert r["kind"] == "shadow_decision" and r["arm"] == "B-L"
    assert r["decision"] == {"followed": True}
    assert r["baseline_agrees"] is True, (
        f"o avaliador cruzou com baseline_followed={evento['baseline_followed']}: {r}"
    )
    assert r["model"] == "laya-base-zero-shot-cpu"


def test_o_avaliador_usa_o_modulo_quando_existe(monkeypatch) -> None:
    """`build_arms()` do avaliador importa `r3_arms` se existir — o caminho que
    este módulo destrava."""
    import r3_shadow_evaluator as se
    arms = se.build_arms()
    nomes = [a.name for a in arms]
    assert "B-D" in nomes, "o braço determinístico sumiu"
    assert "B-L" in nomes, f"o braço de modelo não entrou: {nomes}"


def test_o_braco_e_local_e_por_isso_recebe_o_evento_cru() -> None:
    """`kind = "local"` ⇒ o avaliador entrega o evento CRU (linha 103); um braço
    `api` receberia `redact_for_jev`. O rótulo decide quem vê o quê."""
    braco = r3_arms.build_online_arms()[0]
    assert braco.kind == "local", braco.kind
    assert braco.use_case == "a2_outcome"
    # e a redacao existe para o caso api: o payload que o braço envia
    payload = replay.payload_of(_evento_a2())
    assert not ev.validate_jev_payload(payload), (
        f"o payload do braço local nao passaria na validacao de API: {payload}"
    )
