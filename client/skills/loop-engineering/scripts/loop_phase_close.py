#!/usr/bin/env python3
"""loop_phase_close.py — close a loop phase and persist its knowledge.

Runs the loop's step-14 phase-close as ONE deterministic operation:
  1. mark the subtask done      touring decompose update <task> <phase>
  2. persist the lesson         touring memory store … --tier semantic
  3. reward the outcome         touring learning reward orchestrate
  4. OKF phase report           bundle/phases/<phase>.md   (frontmatter + gates)
  5. Hyper-Extract abstract     bundle/knowledge/<phase>.json  (typed hypergraph,
                                deterministic entity_id / relation_id — REGRA #17)
  6. append the bundle log      bundle/log.md

Zero external deps. IDs are deterministic (derived from canonical name, not
order) so abstracts diff + merge cleanly across runs (the L4 compounding basis).

Usage:
    loop_phase_close.py --task <task> --phase <Pn> --summary "<text>"
        [--status done] [--bundle <dir>] [--reward 1.0]
        [--gates <json>] [--abstract <json>] [--json] [--quiet]
"""
from __future__ import annotations

import argparse
import datetime
import json
import shlex
import subprocess
import sys
from pathlib import Path


def run(cmd, timeout=120):
    try:
        p = subprocess.run(cmd, capture_output=True, text=True, timeout=timeout)
        return p.returncode, p.stdout, p.stderr
    except Exception as exc:  # noqa: BLE001 — fail-open
        return 127, "", str(exc)


def load_json_file(path):
    if not path:
        return None
    try:
        return json.loads(Path(path).read_text())
    except Exception:  # noqa: BLE001
        return None


def plan_id_from_bundle(bundle: Path):
    """plan_id do bundle: o declarado no index.md; senão, o **nome do diretório**.

    O fallback importa porque a ordem real de trabalho não é a ordem ideal: fases são
    fechadas antes de o `index.md` existir, e aí o relatório nascia com `plan_id: unknown`
    — que o doc-link gate depois acusa como contradição contra o próprio bundle que o
    gerou (4 fases assim em 07/08/2026). O diretório é `docs/plans/<plan_id>/` por
    convenção, então o nome é a resposta determinística que sempre está disponível.
    """
    idx = bundle / "index.md"
    if idx.exists():
        for line in idx.read_text(errors="ignore").splitlines():
            if line.startswith("plan_id:"):
                declarado = line.split(":", 1)[1].strip()
                if declarado and declarado != "unknown":
                    return declarado
    return bundle.resolve().name or None


# ── Touring side effects ─────────────────────────────────────────────────────
def resolve_subtask_id(task, phase):
    """Resolve um phase curto ("P2") para o subtask_id COMPLETO da DAG.

    `decompose update` não casa prefixo: id parcial devolve rc 0 com
    `subtask_updated:false` — o fechamento declara sucesso e a DAG não anda
    (medido 25/08: P2 fechado com dag=False). O match é pelo sufixo após
    `::`: igual ao phase, ou começando com ele seguido de espaço ("P2 ...").
    Sem candidato, devolve o phase como veio (o veredito honesto do
    `subtask_updated` continuará dizendo a verdade).
    """
    rc, out, _err = run(["touring", "decompose", "get", task])
    if rc != 0:
        return phase
    try:
        d = json.loads(out)
    except Exception:
        return phase
    for s in d.get("subtasks", []):
        sid = str(s.get("subtask_id", ""))
        suffix = sid.split("::", 1)[1] if "::" in sid else sid
        if suffix == phase or suffix.startswith(phase + " "):
            return sid
    return phase


def update_dag(task, subtask, status):
    """Marca `subtask` no DAG e devolve se ALGO foi de fato atualizado.

    O veredito é `subtask_updated`, não o exit code. `touring decompose update`
    com um subtask inexistente devolve **rc 0** e o payload
    `{"subtask_updated": false, "updated": true}` — dois campos se contradizendo
    na mesma resposta. A versão anterior testava `rc == 0 or …`, e o
    short-circuit fazia o `or` nunca alcançar o campo honesto: todo fechamento
    reportava `dag_updated: true` mesmo quando o id não casava com nada.
    Descoberto em 25/08/2026, quando `decompose ready` seguia listando uma fase
    fechada com sucesso declarado.
    """
    rc, out, err = run(["touring", "decompose", "update", task, subtask, "--status", status])
    payload = out + err
    if '"subtask_updated":true' in payload.replace(" ", ""):
        return True
    if '"subtask_updated":false' in payload.replace(" ", ""):
        return False
    # Sem o campo (binário antigo, erro de transporte): cai no exit code.
    return rc == 0


def store_memory(task, phase, status, summary, tags=None):
    """Persist the phase lesson as a case, carrying the gate's verdict as its `r`.

    The verdict is the strongest reward signal Touring has — `loop_converged.py`
    plus cargo/clippy, all deterministic, where Memento's own case bank is scored
    by an LLM judge (arXiv 2508.16153, client:565). Storing the lesson without it
    left every curated case unscored, so value-ranked recall had nothing to rank
    them by (04/08/2026).

    `--reward` is a newer flag: on a binary that predates it clap rejects the
    call, so the store is retried without it rather than losing the lesson.
    """
    key = f"loop:{task}:{phase}:{status}"
    body = summary or f"{phase} {status}"
    base = ["touring", "memory", "store", key, body, "--tier", "semantic", "--type", "lesson"]
    # Hashtag library (v30.4): phase lessons land faceted — recall by
    # `#process:<fase>` / the caller's domain tags becomes possible. The
    # store auto-derive already adds kind:lesson + status:stable.
    for tag in tags or []:
        base += ["--tag", tag]
    verdict = "1.0" if status == "done" else "0.0"
    rc, out, _ = run(base + ["--reward", verdict,
                             "--outcome-context", f"loop_phase_close:{phase}:{status}"])
    if rc != 0 and '"status":"stored"' not in out:
        rc, out, _ = run(base)
    return '"status":"stored"' in out or rc == 0


def credit_recalls(task, phase, status, queries):
    """Credit the recalls this phase relied on with the phase's own verdict.

    Closes Memento's Eq. 9 loop end to end: a case served by `memory recall` is
    only known to be useful once the work it informed has been judged. Silent
    no-op when no query was recorded — crediting is best-effort, never a gate.
    """
    verdict = "1.0" if status == "done" else "0.0"
    if not queries:
        # The affordance, not the reminder. A phase knows its VERDICT; it never
        # knows the list of questions it asked along the way. Requiring
        # `--credit-query` meant the loop closed only when a caller remembered
        # to pass it — and measured 2026-08-25, none ever did: the ledger's
        # `credited_count` was 0 for the daemon's whole life with the mechanism
        # fully built. `--all-pending` moves the remembering into the executor.
        # A binary predating the flag rejects the call; crediting stays
        # best-effort and never gates the phase.
        rc, out, _ = run(["touring", "memory", "credit", "--all-pending",
                          "--reward", verdict])
        if rc != 0 or '"credited"' not in out:
            return 0
        try:
            return len(json.loads(out).get("claimed_queries") or [])
        except (ValueError, AttributeError):
            return 0
    credited = 0
    for q in queries:
        rc, out, _ = run(["touring", "memory", "credit", q, "--reward", verdict])
        if rc == 0 and '"credited"' in out:
            credited += 1
    return credited


def reward(phase, value):
    rc, out, _ = run(["touring", "learning", "reward", "orchestrate", str(value),
                      f"loop_phase_close:{phase}"])
    return '"reward_injected"' in out or rc == 0


def link_provenance(task, phase, status):
    """P4 (graph contract, 2026-08-30): the phase memory leaves the close
    ALREADY wired — `generated-by` edges to the loop and DAG anchors, created
    by the executor, never left to whoever remembers (the measured corpus had
    27/31 writers as islands with the whole link surface available). The
    anchors are bridge nodes: the edge is valid before any entry exists at the
    destination key. Best-effort — an edge failure never gates the phase.
    """
    key = f"loop:{task}:{phase}:{status}"
    linked = 0
    for dst in (f"loop:{task}", f"decomp:{task}"):
        rc, _out, _ = run(["touring", "memory", "link", key, dst,
                           "--rel", "generated-by"])
        if rc == 0:
            linked += 1
    return linked


# ── Hyper-Extract typed abstract ─────────────────────────────────────────────
def _rel(source, rtype, target):
    return {"relation_id": f"{source}|{rtype}|{target}", "source": source,
            "type": rtype, "target": target}


def build_abstract(phase, summary, extra, task=None, status="done"):
    """Deterministic typed hypergraph: nodes + typed edges, ids from canonical name."""
    phase_id = f"phase:{phase}"
    entities = [{"entity_id": phase_id, "type": "phase", "description": summary or phase}]
    relations = []
    seen_e = {phase_id}
    seen_r = set()

    def add_entity(name, etype, desc):
        eid = f"{etype}:{name}"
        if eid not in seen_e:
            seen_e.add(eid)
            entities.append({"entity_id": eid, "type": etype, "description": desc})
        return eid

    def add_rel(source, rtype, target):
        r = _rel(source, rtype, target)
        if r["relation_id"] not in seen_r:
            seen_r.add(r["relation_id"])
            relations.append(r)

    for e in (extra or {}).get("entities", []) or []:
        name = e.get("name") or e.get("entity_id")
        if not name:
            continue
        eid = add_entity(name, e.get("type", "entity"), e.get("description", ""))
        add_rel(phase_id, "produces", eid)
    for r in (extra or {}).get("relations", []) or []:
        if r.get("source") and r.get("target"):
            add_rel(r["source"], r.get("type", "relates"), r["target"])

    # P4 (graph contract, 2026-08-30): the abstract is a PROJECTION of the
    # same material the close just wired into memory_links — the phase memory
    # node, the loop/DAG anchors and the generated-by edges — so a run with
    # no --abstract can never again degenerate to 1 node / 0 relations while
    # the real graph has both ("the same graph written twice, one copy
    # hollow": knowledge/P1.json measured 2026-08-29 with 1 entity, 0
    # relations, because nobody passes --abstract).
    if task:
        mem_id = add_entity(f"loop:{task}:{phase}:{status}", "memory",
                            "phase lesson (memory key)")
        loop_id = add_entity(f"loop:{task}", "loop", "loop anchor (bridge node)")
        dag_id = add_entity(f"decomp:{task}", "dag",
                            "decompose task anchor (bridge node)")
        add_rel(phase_id, "produces", mem_id)
        add_rel(mem_id, "generated-by", loop_id)
        add_rel(mem_id, "generated-by", dag_id)

    return {"phase": phase, "entities": entities, "relations": relations}


def run_extractor(cmd, summary):
    """Optional real-tool adapter (ref-b): shell out to an external extractor —
    e.g. a real **Hyper-Extract** — with the phase summary on stdin and parse its
    ``{entities:[], relations:[]}`` JSON. The native deterministic abstract stays
    the default; this only *enriches* it. Absent tool / any error → ``{}`` (native
    fallback), so the loop never hard-depends on the external tool.
    """
    if not cmd:
        return {}
    try:
        proc = subprocess.run(shlex.split(cmd), input=summary or "",
                              capture_output=True, text=True, timeout=120)
        data = json.loads(proc.stdout)
        return data if isinstance(data, dict) else {}
    except Exception:  # noqa: BLE001 — optional adapter, fail-open to native
        return {}


def merge_extra(*extras):
    """Union of several ``{entities, relations}`` sources (native `--abstract`
    file + `--extractor` output). Deterministic dedup happens in `build_abstract`."""
    out = {"entities": [], "relations": []}
    for e in extras:
        if isinstance(e, dict):
            out["entities"] += e.get("entities", []) or []
            out["relations"] += e.get("relations", []) or []
    return out


# ── OKF emission ─────────────────────────────────────────────────────────────
def validate_facts(raw):
    """Parse --facts, recusando a forma errada com a forma CERTA na mensagem.

    O contrato é uma LISTA de objetos `{chave, valor, run_id}`. Passar um dict
    (o erro natural — "fatos" soa como mapa) fazia o relatório estourar com
    `AttributeError: 'str' object has no attribute 'get'` lá dentro de
    `write_phase_report`, DEPOIS de a memória e o reward já terem sido gravados
    — um fechamento meio feito, com o erro apontando para a linha errada.
    Falhar cedo e dizer o formato custa uma linha; adivinhar custou um turno
    (25/08/2026).
    """
    if not raw:
        return None
    dados = json.loads(raw)
    exemplo = '[{"chave":"testes","valor":"546 pass","run_id":"run-123"}]'
    if not isinstance(dados, list):
        raise SystemExit(
            f"--facts precisa ser uma LISTA de objetos, recebi {type(dados).__name__}.\n"
            f"Formato: {exemplo}"
        )
    for i, f in enumerate(dados):
        if not isinstance(f, dict):
            raise SystemExit(
                f"--facts[{i}] precisa ser um objeto, recebi {type(f).__name__}.\n"
                f"Formato: {exemplo}"
            )
    return dados


def write_phase_report(bundle: Path, plan_id, phase, status, summary, gates, ts,
                       facts=None):
    path = bundle / "phases" / f"{phase}.md"
    path.parent.mkdir(parents=True, exist_ok=True)
    fm = (
        "---\n"
        "type: PhaseReport\n"
        f"title: {phase} — phase report\n"
        f"description: {(summary or phase)[:140]}\n"
        f"plan_id: {plan_id or 'unknown'}\n"
        f"tags: [loop, phase, {phase}]\n"
        f"timestamp: {ts}\n"
        'okf_version: "0.1"\n'
        "---\n\n"
    )
    # Link only to bundle docs that actually exist — a report must never emit a
    # broken bundle-relative link (E2E finding 2026-07-02).
    refs = [f"[{label}](/{fn})" for label, fn in
            (("bundle", "index.md"), ("plan", "plan.md"), ("log", "log.md"))
            if (bundle / fn).exists()]
    part_of = ("Part of the " + " · ".join(refs) + ".") if refs else ""
    lines = [f"# {phase} — phase report", "",
             f"**Status**: {status}", "",
             part_of,
             "", "## Summary", "", summary or "_(no summary)_", ""]
    if gates:
        lines += ["## Schema", "", "| Gate clause | Result | Evidence |",
                  "|-------------|--------|----------|"]
        for name, c in (gates.get("clauses", {}) or {}).items():
            lines.append(f"| {name} | {c.get('result')} | {c.get('evidence', '')} |")
        lines.append("")
    # W5 S-5.5 (plano code-mode-total) — claim-ledger: fatos com ENDEREÇO
    # (run_id) entram no relatório; um relatório sem nenhum fato endereçado
    # ganha a seção visível (não bloqueante — a fala não tem executor, mas
    # ganha espelho: o custo da afirmação sem endereço fica legível).
    if facts:
        lines += ["## Fatos com endereço (claim-ledger)", "",
                  "| Fato | Valor | run_id |", "|---|---|---|"]
        for f in facts:
            lines.append(
                f"| {f.get('chave', f.get('key', '?'))} "
                f"| {f.get('valor', f.get('value', '?'))} "
                f"| `{f.get('run_id', '?')}` |")
        lines.append("")
    else:
        lines += ["## Afirmações sem endereço", "",
                  "_Nenhum fato com run_id foi citado neste fechamento "
                  "(`--facts`). As afirmações do Summary acima valem o que "
                  "vale a narrativa — um probe (FACT=) daria endereço a cada "
                  "uma. (claim-ledger, S-5.5)_", ""]
    lines += ["## Knowledge", "", f"Typed abstract: [/knowledge/{phase}.json](/knowledge/{phase}.json)."]
    path.write_text(fm + "\n".join(lines) + "\n")
    return str(path)


def write_abstract(bundle: Path, phase, abstract):
    path = bundle / "knowledge" / f"{phase}.json"
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(abstract, indent=2) + "\n")
    return str(path)


def append_log(bundle: Path, phase, status, summary, ts):
    """Append one phase to the bundle's chronological log.

    The log is an OKF document like every other `.md` the loop writes, so a NEW
    one is created with frontmatter. It was not, until 2026-08-19: this function
    produced a file that `loop_doc_link_gate.py` — the loop's own step 17 — then
    rejected for `missing_type` and `missing_plan_id`. Every fresh bundle started
    its life failing that gate and was repaired by hand, which is why the defect
    survived: the evidence of it was erased each time by the fix.
    """
    log = bundle / "log.md"
    entry = f"\n## {ts} — {phase} {status}\n\n{summary or ''}\n"
    if log.exists():
        log.write_text(log.read_text() + entry)
    else:
        plan_id = plan_id_from_bundle(bundle)
        header = (
            "---\n"
            'okf_version: "1.0"\n'
            "type: Log\n"
            f'title: "Log — {plan_id}"\n'
            f'description: "Chronological history of the phases closed in this bundle."\n'
            f"plan_id: {plan_id}\n"
            'tags: ["#kind:log", "#artifact:log"]\n'
            f"timestamp: {ts}\n"
            "---\n\n"
            f"# Log — {plan_id}\n\n"
            "Cada entrada é um fecho de fase registrado por `loop_phase_close.py`.\n"
            "O plano: [`plan.md`](/plan.md)\n"
        )
        log.write_text(header + entry)
    return str(log)


# ── Orchestration ────────────────────────────────────────────────────────────
def variant_score(gates: dict | None, status: str) -> float:
    """Reduce whatever graded this phase to a score in [0, 1].

    Prefers the convergence report's clause tally over the coarse pass/fail,
    because a phase that met five clauses of six is a materially better stepping
    stone than one that met none — and collapsing both to 0.0 would throw away
    exactly the gradient the archive exists to preserve.
    """
    clauses = ((gates or {}).get("clauses") or {})
    scored = [c for c in clauses.values() if c.get("result") in {"PASS", "FAIL"}]
    if scored:
        return sum(1 for c in scored if c["result"] == "PASS") / len(scored)
    return 1.0 if status in {"done", "completed", "complete"} else 0.0


def main(argv=None):
    ap = argparse.ArgumentParser(description="Close a loop phase and persist its knowledge.")
    ap.add_argument("--task", required=True)
    ap.add_argument("--phase", required=True)
    ap.add_argument(
        "--subtask",
        help="Subtask id no DAG, quando difere de --phase. `--phase` nomeia o "
        "RELATÓRIO (phases/P0.md); o id do subtask é o que `touring decompose "
        "add` recebeu, e nem sempre são o mesmo texto. Conflatá-los fazia o "
        "fechamento atualizar um subtask inexistente (25/08/2026).",
    )
    ap.add_argument("--summary", default="")
    ap.add_argument("--status", default="done")
    ap.add_argument("--bundle", default=None, help="OKF bundle dir (writes report + abstract + log)")
    ap.add_argument("--reward", type=float, default=1.0)
    ap.add_argument("--tag", action="append", default=None,
                    help="Faceted hashtag `#facet:value` (repeatable) for the phase lesson")
    ap.add_argument("--credit-query", action="append", default=None,
                    help="a `memory recall` query this phase relied on; credited with the "
                         "phase verdict (repeatable). Closes the recall->outcome loop.")
    ap.add_argument("--facts", default=None,
                    help="W5 S-5.5 claim-ledger: JSON list de fatos endereçados "
                         '[{"chave","valor","run_id"},…] — entram no relatório OKF; '
                         "sem --facts o relatório ganha a seção 'Afirmações sem endereço'")
    ap.add_argument("--gates", default=None, help="JSON file: loop_converged report to embed")
    ap.add_argument("--abstract", default=None, help="JSON file: {entities:[],relations:[]} to enrich")
    ap.add_argument("--extractor", default=None,
                    help="optional external extractor cmd (real Hyper-Extract adapter): "
                         "reads the summary on stdin, emits {entities,relations} JSON")
    ap.add_argument("--variant", default=None,
                    help="archive this phase as a scored variant of --variant-target "
                         "(a stepping stone, kept whether or not it passed)")
    ap.add_argument("--variant-target", default=None,
                    help="what the variant is an attempt AT (defaults to the task id)")
    ap.add_argument("--variant-parent", default=None,
                    help="variant_id this attempt was branched from")
    ap.add_argument("--json", action="store_true")
    ap.add_argument("--quiet", action="store_true")
    args = ap.parse_args(argv)

    result = {
        "task": args.task, "phase": args.phase, "status": args.status,
        "dag_updated": update_dag(args.task, args.subtask or resolve_subtask_id(args.task, args.phase), args.status),
        "memory_stored": store_memory(args.task, args.phase, args.status, args.summary, tags=args.tag),
        # P4: provenance is created BY the executor at close time — clause 5
        # of the graph contract stops being a convention nobody follows.
        "provenance_links": link_provenance(args.task, args.phase, args.status),
        "rewarded": reward(args.phase, args.reward),
        "recalls_credited": credit_recalls(
            args.task, args.phase, args.status, args.credit_query
        ),
    }

    if args.bundle:
        bundle = Path(args.bundle)
        plan_id = plan_id_from_bundle(bundle)
        ts = datetime.datetime.now().astimezone().isoformat()
        gates = load_json_file(args.gates)
        extra = merge_extra(load_json_file(args.abstract),
                            run_extractor(args.extractor, args.summary))
        abstract = build_abstract(args.phase, args.summary, extra,
                                  task=args.task, status=args.status)
        facts = validate_facts(args.facts)
        result["phase_report"] = write_phase_report(bundle, plan_id, args.phase, args.status,
                                                    args.summary, gates, ts, facts=facts)
        result["abstract"] = write_abstract(bundle, args.phase, abstract)
        result["log"] = append_log(bundle, args.phase, args.status, args.summary, ts)
        result["entities"] = len(abstract["entities"])
        result["relations"] = len(abstract["relations"])

    # The stepping-stone archive (T6.1). A phase close is the one moment that
    # holds BOTH the attempt and the score that graded it, so it is where a
    # variant can be recorded honestly. Recorded whether or not the phase passed:
    # keeping only winners is the greedy baseline arXiv:2505.22954 beats by 10
    # points, because the path to a good answer runs through worse ones.
    if args.variant:
        try:
            sys.path.insert(0, str(Path(__file__).resolve().parent))
            import variant_archive

            score = variant_score(load_json_file(args.gates), args.status)
            result["variant"] = variant_archive.record(
                target=args.variant_target or args.task,
                variant=args.variant,
                score=score,
                verdict=args.status,
                parent=args.variant_parent,
            )
        except Exception as exc:  # noqa: BLE001 — fail-open, like every hook here
            result["variant_error"] = f"{exc.__class__.__name__}: {exc}"

    # O exit code sempre foi honesto; o CABEÇALHO não era. Ele anunciava
    # "→ done" mesmo com `dag=False`, e um leitor que não conferisse os flags
    # (ou que canalizasse a saída por um `tail`) levava embora a impressão de
    # que a fase fechou. Um cabeçalho que afirma o desfecho contradito pelos
    # próprios flags logo abaixo é a classe `sinais-de-progresso-que-mentem`.
    ok = result["dag_updated"] and result["memory_stored"]
    if args.json:
        print(json.dumps(result, indent=2))
    elif not args.quiet:
        estado = args.status if ok else f"{args.status} — NÃO PERSISTIDO (exit 1)"
        print(f"phase-close · {args.phase} → {estado}")
        print(f"  dag={result['dag_updated']} memory={result['memory_stored']} reward={result['rewarded']}")
        if not ok:
            print("  ⚠ o relatório OKF foi escrito, mas a DAG/memória NÃO avançaram "
                  "— reexecute quando o daemon responder")
        if args.bundle:
            print(f"  OKF report: {result['phase_report']}")
            print(f"  abstract:   {result['abstract']} ({result['entities']} entities, {result['relations']} relations)")

    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
