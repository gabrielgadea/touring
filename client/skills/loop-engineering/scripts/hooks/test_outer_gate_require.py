#!/usr/bin/env python3
"""Guards for the gate's artifact predicate (`require_json`).

The regression: on 2026-08-25 the `explore-ledger` artifact was satisfied by
ANY fresh `*.ledger.json` in scope. A ledger for a different question, whose
exploration had NOT converged, reported `present` while the converged ledger
for the real topic sat one filename away — the gate would have let the turn end
with the exploration still open. Freshness and shape are not a verdict.
"""
from __future__ import annotations

import json
import sys
import time
import unittest
from pathlib import Path
from tempfile import TemporaryDirectory

sys.path.insert(0, str(Path(__file__).parent))
import loop_outer_gate as gate  # noqa: E402

import loop_turn_record as record  # noqa: E402

CONVERGED = {"verdict.converged": True}


def _transcript(dirpath: Path, turns: list[dict]) -> Path:
    """Um transcript no formato que o Claude Code escreve, linha a linha."""
    p = dirpath / "t.jsonl"
    p.write_text("\n".join(json.dumps(t) for t in turns), encoding="utf-8")
    return p


def _human(text):
    return {"type": "user", "message": {"content": [{"type": "text", "text": text}]}}


def _says(text):
    return {"type": "assistant", "message": {"content": [{"type": "text", "text": text}]}}


def _acts(tool, inp):
    return {"type": "assistant", "message": {"content": [
        {"type": "tool_use", "id": "x1", "name": tool, "input": inp}]}}


def _result(text):
    return {"type": "user", "message": {"content": [
        {"type": "tool_result", "tool_use_id": "x1", "content": text}]}}


class ClassBTurnRecordTest(unittest.TestCase):
    """P4/S-4.3: a interação como gate, com o artefato escrito por CÓDIGO.

    A Lei L3 (veredito por artefato, jamais narrativa) fica intacta: o arquivo
    continua obrigatório. O que muda é o AUTOR — o executor lê o turno já
    concluído em vez de o modelo parar no meio do raciocínio para escrever.
    """

    def test_the_turn_record_exists_on_disk_with_no_mid_reasoning_write(self):
        with TemporaryDirectory() as tmp:
            t = _transcript(Path(tmp), [
                _human("tarefa antiga"),
                _says("resposta antiga que NAO deve entrar"),
                _human("a tarefa deste turno"),
                _acts("Bash", {"command": "cargo test -p touring-cli"}),
                _result("ok. 13 passed"),
                _says("O resultado e **13 verdes**.\nprosa de ligacao sem estrutura."),
            ])
            out = Path(tmp) / "turns" / "turn.md"
            res = record.materialize({"cwd": tmp, "scope": tmp, "topic": "t"}, out, t)
            self.assertTrue(res["written"], res)
            self.assertTrue(out.exists(), "o artefato TEM de existir em disco")
            body = out.read_text()
            self.assertIn("13 verdes", body, "a espinha do turno entra")
            self.assertIn("cargo test -p touring-cli", body, "o que o turno FEZ entra")
            self.assertNotIn("resposta antiga", body,
                             "a fronteira e o turno humano — nao o transcript inteiro")
            self.assertNotIn("prosa de ligacao", body,
                             "prosa sem estrutura nem enfase fica de fora")

    def test_a_working_turn_is_not_an_empty_record(self):
        """Um turno que decide AGINDO: sem as ações o registro seria decoração.

        Medido ao vivo em 04/09/2026 antes da correção: 2 linhas de espinha para
        um turno inteiro de trabalho, porque a regra só via negrito no INÍCIO da
        linha e o turno carregava a decisão em negrito no meio da frase.
        """
        with TemporaryDirectory() as tmp:
            t = _transcript(Path(tmp), [
                _human("faca"),
                _acts("Edit", {"file_path": "/a/b.rs"}),
                _result("ok"),
                _says("segui adiante."),
            ])
            out = Path(tmp) / "turn.md"
            res = record.materialize({"cwd": tmp, "scope": tmp}, out, t)
            self.assertTrue(res["written"])
            self.assertIn("/a/b.rs", out.read_text())

    def test_class_b_never_becomes_a_manual_write_when_the_executor_fails(self):
        """Fail-open é a razão de ser da classe B: se o executor falha, o gate
        segue em frente e REGISTRA a falha — nunca a converte na escrita manual
        que a classe existe para eliminar."""
        art = {"id": "turn-record", "class": "B", "materializer": "inexistente"}
        manifest = {"f": {"artifacts": [art], "enforced_classes": ["A", "B"]}}
        rep = gate.evaluate({"flow": "f", "scope": "/tmp", "cwd": "/tmp",
                             "flow_armed_at": time.time()}, manifest)
        self.assertEqual(rep["missing"], [], "classe B jamais vira cobranca")
        self.assertEqual(rep["class_b_unmaterialized"], ["turn-record"],
                         "…mas a falha aparece: ausencia exibida, nunca sumida")
        self.assertTrue(rep["complete"])

    def test_o_registro_e_idempotente_por_armacao_do_flow(self):
        """Uma armação, um arquivo — e a segunda avaliação NÃO reescreve.

        O gate reavalia várias vezes por run. Com o nome derivado do relógio,
        cada avaliação criava um arquivo novo: o diretório crescia sem limite e
        a cláusula ficava vacuamente verdadeira (o artefato "aparecia" só por
        ter acabado de ser criado). Identidade derivada de critério, nunca de
        relógio — a mesma disciplina da REGRA #17.
        """
        m1 = {"session_id": "abc123def456", "flow_armed_at": 1_700_000_000.7}
        m2 = {"session_id": "abc123def456", "flow_armed_at": 1_700_000_000.9}
        m3 = {"session_id": "abc123def456", "flow_armed_at": 1_700_009_999.0}
        self.assertEqual(record.record_id(m1), record.record_id(m2),
                         "a MESMA armação tem de dar o MESMO id")
        self.assertNotEqual(record.record_id(m1), record.record_id(m3),
                            "armações distintas têm ids distintos")
        with TemporaryDirectory() as tmp:
            a = record.default_out(tmp, None, m1)
            b = record.default_out(tmp, None, m2)
            self.assertEqual(a, b)

    def test_a_segunda_avaliacao_reaproveita_o_registro_da_primeira(self):
        """Prova do caminho: duas avaliações, UM arquivo, conteúdo preservado."""
        with TemporaryDirectory() as tmp:
            t = _transcript(Path(tmp), [_human("faca"), _says("**pronto**")])
            marker = {"flow": "f", "scope": tmp, "cwd": tmp, "bundle": tmp,
                      "session_id": "sess01", "flow_armed_at": time.time(),
                      # O harness NOMEIA o transcript da sessão; adivinhar "o
                      # mais recente do projeto" pega o de outra sessão CC.
                      "transcript_path": str(t)}
            art = {"id": "turn-record", "class": "B", "materializer": "loop_turn_record"}
            manifest = {"f": {"artifacts": [art], "enforced_classes": ["A", "B"]}}
            # 1ª avaliação: materializa
            r1 = gate.evaluate(marker, manifest)
            p1 = [f for p in r1["present"] if p["id"] == "turn-record" for f in p["files"]]
            self.assertTrue(p1, r1)
            Path(p1[0]).write_text("MARCA-DA-PRIMEIRA", encoding="utf-8")
            # 2ª avaliação: encontra a mesma, sem reescrever
            r2 = gate.evaluate(marker, manifest)
            p2 = [f for p in r2["present"] if p["id"] == "turn-record" for f in p["files"]]
            self.assertEqual(p1, p2, "mesma armação -> mesmo arquivo")
            self.assertEqual(Path(p1[0]).read_text(encoding="utf-8"), "MARCA-DA-PRIMEIRA",
                             "a 2a avaliacao reescreveu — nao e' idempotente")
            self.assertEqual(len(list(Path(tmp).glob("turns/*.md"))), 1,
                             "um arquivo por armacao, nao um por avaliacao")
            _ = t  # o transcript existe; a fixture o mantém vivo

    def test_a_transcript_that_cannot_be_read_is_reported_not_guessed(self):
        with TemporaryDirectory() as tmp:
            res = record.materialize({"cwd": tmp, "scope": tmp}, Path(tmp) / "x.md")
            self.assertFalse(res["written"])
            self.assertIn("reason", res)

    def test_the_transcript_directory_rule_matches_the_rulers(self):
        """Uma regra, duas implementações (aqui e em `context_budget.rs`): o
        teste é o que impede as duas de divergirem — o defeito D2 deste plano."""
        got = record.transcript_dir_for(Path("/home/x"), Path("/home/x/projects/touring"))
        self.assertEqual(got.name, "-home-x-projects-touring")


class RequireJsonTest(unittest.TestCase):
    def _ledger(self, tmp, name, converged):
        p = Path(tmp) / name
        p.write_text(json.dumps({"topic": "t", "verdict": {"converged": converged}}))
        return str(p)

    def test_converged_ledger_satisfies(self):
        with TemporaryDirectory() as tmp:
            self.assertTrue(gate._satisfies(self._ledger(tmp, "a.json", True), CONVERGED))

    def test_unconverged_ledger_does_not_satisfy(self):
        """The measured bug: a fresh but open exploration counted as done."""
        with TemporaryDirectory() as tmp:
            self.assertFalse(gate._satisfies(self._ledger(tmp, "b.json", False), CONVERGED))

    def test_missing_pointer_does_not_satisfy(self):
        """A ledger with no verdict has not stated one — absence is not success."""
        with TemporaryDirectory() as tmp:
            p = Path(tmp) / "c.json"
            p.write_text(json.dumps({"topic": "t"}))
            self.assertFalse(gate._satisfies(str(p), CONVERGED))

    def test_unparseable_file_fails_closed(self):
        with TemporaryDirectory() as tmp:
            p = Path(tmp) / "d.json"
            p.write_text("{not json")
            self.assertFalse(gate._satisfies(str(p), CONVERGED))

    def test_no_requirement_keeps_the_old_behaviour(self):
        """Artifacts without a predicate must not become harder to satisfy."""
        with TemporaryDirectory() as tmp:
            self.assertTrue(gate._satisfies(self._ledger(tmp, "e.json", False), None))

    def test_evaluate_rejects_an_unconverged_ledger_end_to_end(self):
        """The predicate must actually reach `evaluate`, not just exist."""
        with TemporaryDirectory() as tmp:
            explore = Path(tmp) / ".touring-explore"
            explore.mkdir()
            self._ledger(str(explore), "open.ledger.json", False)
            marker = {"flow": "f", "scope": tmp, "cwd": tmp,
                      "flow_armed_at": time.time() - 5}
            manifests = {"f": {"artifacts": [{
                "id": "explore-ledger",
                "glob": "{scope}/.touring-explore/*.ledger.json",
                "min": 1, "next_action": "run explore",
                "require_json": {"verdict.converged": True},
            }]}}
            report = gate.evaluate(marker, manifests)
            self.assertFalse(report["complete"])
            self.assertEqual([m["id"] for m in report["missing"]], ["explore-ledger"])

            self._ledger(str(explore), "done.ledger.json", True)
            self.assertTrue(gate.evaluate(marker, manifests)["complete"])


if __name__ == "__main__":
    unittest.main(verbosity=2)

# ── P4/S-4.1 (2026-09-04): o contrato da CLASSE de custo ─────────────────────

def _manifests():
    import json, pathlib
    return json.loads((pathlib.Path(__file__).parent / "flow_manifests.json").read_text())


def test_every_artifact_declares_its_cost_class():
    """Sem `class`, o gate assume A e cobra — mas um artefato caro nao declarado
    seria cobrado do default, que e' exatamente o que P4 desfaz. A declaracao e'
    obrigatoria, e este teste e' quem a torna obrigatoria."""
    for flow, m in _manifests().items():
        if not isinstance(m, dict) or "artifacts" not in m:
            continue
        for art in m["artifacts"]:
            assert art.get("class") in {"A", "B", "C"}, (
                f"{flow}/{art['id']} sem classe de custo declarada"
            )


def test_the_default_flow_never_charges_a_class_c_artifact():
    """O achado que originou P4: 965 avaliacoes, 74% dos runs nunca completaram,
    e o que faltava era sempre classe C — artefato que exige parar o raciocinio.
    O flow DEFAULT (`work-outer`, 392 das 965) cobra so' o que codigo produz."""
    m = _manifests()["work-outer"]
    # A asserção segue a INTENÇÃO do nome, não uma lista literal. A forma
    # anterior (`== ["A"]`) era mais estreita que o próprio enunciado e reprovou
    # a classe B em 04/09 — que custa zero por construção e portanto não viola
    # nada do que este teste protege. Sobre-especificar uma asserção transforma
    # o guard num obstáculo à correção que ele deveria permitir.
    assert "C" not in m["enforced_classes"], m["enforced_classes"]
    # …e o que sustenta isso: uma classe B só é cobrável porque um EXECUTOR a
    # produz. Sem materializador declarado, "B" seria classe C com outro nome.
    for art in m["artifacts"]:
        if art["class"] == "B" and art["class"] in m["enforced_classes"]:
            assert art.get("materializer"), (
                f"{art['id']} e' classe B cobrada sem executor que a escreva"
            )
    cobrados = [a["id"] for a in m["artifacts"] if a["class"] in m["enforced_classes"]]
    assert cobrados, "o default ainda precisa cobrar ALGO — senao nao e' gate"
    assert "explore-ledger" not in cobrados, (
        "explore-ledger faltou 418x e 56 de 79 bundles nunca o tiveram"
    )


def test_an_explicitly_invoked_flow_still_charges_everything():
    """Quem digitou /loop-engineering PEDIU a exploracao: ali o documento e' o
    entregavel, nao um desvio. O alivio e' do default, nunca do pedido."""
    m = _manifests()["strategy-outer"]
    assert set(m["enforced_classes"]) == {"A", "B", "C"}
    assert any(a["id"] == "explore-ledger" for a in m["artifacts"])
