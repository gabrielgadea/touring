#!/usr/bin/env python3
"""okf_emit.py — o emissor único de documentos OKF (executor, não convenção).

Origem (30/08/2026): o report do cross-audit saiu SEM frontmatter OKF e Gabriel
pegou; o censo achou outros 5 iguais (o mais antigo de junho) e um JSON
truncado no meio da string com a fence quebrada (violação A6). Cada produtor
fabricava frontmatter à mão — printf em bash no nó `report` do ADW, f-strings
em `loop_phase_close.py` (2 sítios) e `loop_diagnose.py` (2 sítios) — a mesma
classe de defeito de "cinco sítios gravam a mesma aresta": consertar um
mascara os outros. Este módulo é o sítio único, e o enforcement mora AQUI, no
executor (D8): por este caminho é impossível emitir documento sem os campos.

O que o emissor garante por construção:
  1. VALIDAÇÃO antes de escrever — type/title/description obrigatórios; o erro
     ENSINA o formato (A5), nunca só recusa.
  2. Teto por seção com elisão DECLARADA (A6) — a íntegra vai para o sidecar
     `.full/` e a nota diz quantos chars ficaram de fora; uma fence ```json
     jamais quebra (a truncagem acontece DENTRO dela).
  3. Proveniência blake2b opcional — hash do report anterior + hash do próprio
     corpo no frontmatter (a cadeia que `judge_attest` pratica nos graders).
  4. Registro do artefato no grafo de memória com facetas `#artifact:…` +
     `memory link` para a lesson que o gerou — fecha o furo medido em
     30/08/2026: `memory query "#artifact:report"` devolvia 0 reports reais;
     o phase-close guardava o resumo, nunca o documento.

Essência importada de `analise/scripts/pln2_generator` (models congelados +
validator + part_generator com teto de tokens + toon_checkpoint blake2b),
reescrita stdlib-only (política MVP dos scripts do loop — zero deps externas;
o dimension_analyzer NÃO veio: o juiz de qualidade aqui é o touring-quality
50-dim). O guard de CI (`scripts/test_okf_audit_reports.py` no workspace
touring) segue como rede para o legado e para qualquer rota que fuja deste
executor.

Consumo:
  import — loop_phase_close.py, loop_diagnose.py (render_frontmatter/emit/
           register_artifact);
  CLI    — nós `report` de ADW:
           python3 okf_emit.py --out <f.md> --type AuditReport --title "…" \
             --description "…" --section "NOME=texto" --section-json "NOME=json"
           (imprime `REPORT=<path>` no sucesso — o contrato que o runner lê.)
"""
from __future__ import annotations

import argparse
import datetime
import hashlib
import json
import os
import re
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path

DEFAULT_SECTION_CAP = 20000
_SIMPLE_TOKEN = re.compile(r"[A-Za-z0-9_./:+\-]+")


class OkfValidationError(ValueError):
    """Frontmatter inválido — a mensagem carrega o formato correto (A5)."""


@dataclass(frozen=True)
class Section:
    """Uma seção `## nome` do corpo; kind="json" ganha fence ```json fechada."""

    name: str
    content: str
    kind: str = "text"  # "text" | "json"


def _slug(text: str) -> str:
    return re.sub(r"[^A-Za-z0-9._-]+", "-", str(text)).strip("-") or "doc"


def _yaml_value(value) -> str:
    """JSON string é YAML válido — aspas/quebras escapam por construção."""
    s = str(value)
    if _SIMPLE_TOKEN.fullmatch(s):
        return s
    return json.dumps(s, ensure_ascii=False)


def render_frontmatter(doc_type, title, description, *, timestamp=None,
                       tags=(), fields=None) -> str:
    """O frontmatter OKF canônico — ou uma recusa que ensina o formato.

    Os 4 campos que o guard de CI exige (`type/title/description/timestamp`)
    saem daqui; `timestamp` é derivado quando omitido. `fields` entra entre a
    description e as tags (plan_id, flow, okf_version, proveniência…).
    """
    faltam = [k for k, v in (("type", doc_type), ("title", title),
                             ("description", description))
              if not (v and str(v).strip())]
    if faltam:
        raise OkfValidationError(
            "frontmatter OKF incompleto — faltam: " + ", ".join(faltam)
            + ". Todo documento exige type + title + description (timestamp é "
            "derivado). Exemplo: render_frontmatter(\"AuditReport\", "
            "\"cross-audit <alvo>\", \"o que este documento prova\")."
        )
    ts = timestamp or datetime.datetime.now().astimezone().isoformat()
    lines = [
        "---",
        f"type: {_yaml_value(doc_type)}",
        f"title: {json.dumps(str(title), ensure_ascii=False)}",
        f"description: {json.dumps(str(description), ensure_ascii=False)}",
    ]
    for k, v in (fields or {}).items():
        lines.append(f"{k}: {_yaml_value(v)}")
    if tags:
        lines.append("tags: [" + ", ".join(_yaml_value(t) for t in tags) + "]")
    lines.append(f"timestamp: {ts}")
    lines.append("---")
    return "\n".join(lines) + "\n\n"


def _render_section(sec: Section, cap: int, sidecar_dir: Path,
                    out_stem: str) -> tuple[str, int]:
    """Renderiza uma seção respeitando o teto — elisão sempre DECLARADA (A6).

    A truncagem acontece DENTRO da fence (ela nunca quebra — o defeito real do
    report de 28/08: `"text": "/// TODO: real im` cortado no meio da string,
    fence aberta, zero declaração) e a íntegra vai para `.full/` ao lado.
    """
    content = sec.content or ""
    total = len(content)
    note = ""
    elided = 0
    if total > cap:
        sidecar_dir.mkdir(parents=True, exist_ok=True)
        full = sidecar_dir / f"{_slug(out_stem)}--{_slug(sec.name)}.txt"
        full.write_text(content, encoding="utf-8")
        content = content[:cap]
        elided = total - cap
        note = (
            f"\n\n_[A6: seção elidida — exibidos {cap} de {total} chars; "
            f"íntegra em `.full/{full.name}`]_"
        )
    if sec.kind == "json":
        body = f"```json\n{content}\n```"
    else:
        body = content
    return f"## {sec.name}\n\n{body}{note}\n", elided


def _run(cmd, timeout=20):
    try:
        p = subprocess.run(cmd, capture_output=True, text=True, timeout=timeout)
        return p.returncode, p.stdout + p.stderr
    except Exception as exc:  # noqa: BLE001 — daemon mudo nunca gata a emissão
        return 127, str(exc)


def _default_key(out: Path) -> str:
    """Chave determinística do caminho canônico (REGRA #17) — dois reports no
    mesmo dia em árvores distintas nunca colidem."""
    h = hashlib.sha1(str(out.resolve()).encode()).hexdigest()[:8]
    return f"report:{h}:{out.stem}"


def register_artifact(path, key, title, *, facets=(), link_to=None) -> dict:
    """O ponteiro do artefato entra no grafo de memória com facetas.

    Sem isso, recall/moc/portfolio nunca chegam ao documento — só ao resumo
    (medido 30/08/2026: `memory query "#artifact:report"` → 4 entradas, nenhuma
    um report real). `link_to` liga o report à lesson que o gerou (rel
    `documents`). Fail-open em toda ponta; `OKF_EMIT_NO_MEMORY=1` desliga
    (testes / ambientes sem daemon).
    """
    if os.environ.get("OKF_EMIT_NO_MEMORY") == "1":
        return {"memory_key": key, "memory_stored": False, "memory_linked": 0,
                "memory_skipped": "env"}
    cmd = ["touring", "memory", "store", key, f"OKF: {title} — {path}",
           "--tier", "semantic", "--type", "reference"]
    for t in facets:
        cmd += ["--tag", t]
    rc, out_s = _run(cmd)
    stored = '"status":"stored"' in out_s or rc == 0
    linked = 0
    if link_to:
        rc2, _ = _run(["touring", "memory", "link", key, str(link_to),
                       "--rel", "documents"])
        linked = 1 if rc2 == 0 else 0
    return {"memory_key": key, "memory_stored": stored, "memory_linked": linked}


def emit(out, doc_type, title, description, *, sections=(), tags=(),
         fields=None, body_prefix="", cap=DEFAULT_SECTION_CAP, prev=None,
         timestamp=None, memory_key=None, facets=("#artifact:report",),
         process=None, link_to=None, no_memory=False) -> dict:
    """Emite um documento OKF completo: valida → renderiza → escreve → registra.

    Devolve `{path, elided:{seção:chars}, memory_*}`. Levanta
    `OkfValidationError` (com o formato na mensagem) antes de tocar o disco.
    """
    out = Path(out)
    fields = dict(fields or {})
    if prev:
        prevp = Path(prev)
        if prevp.is_file():
            fields["provenance_prev"] = prevp.name
            fields["provenance_prev_blake2b"] = hashlib.blake2b(
                prevp.read_bytes(), digest_size=16).hexdigest()
    rendered, elided = [], {}
    for sec in sections:
        body, e = _render_section(sec, cap, out.parent / ".full", out.stem)
        rendered.append(body)
        if e:
            elided[sec.name] = e
    body_txt = (body_prefix + "\n\n" if body_prefix else "") + "\n".join(rendered)
    fields["content_blake2b"] = hashlib.blake2b(
        body_txt.encode(), digest_size=16).hexdigest()
    fm = render_frontmatter(doc_type, title, description, timestamp=timestamp,
                            tags=tags, fields=fields)
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(fm + body_txt.rstrip() + "\n", encoding="utf-8")
    result = {"path": str(out), "elided": elided}
    all_facets = list(facets) + ([f"#process:{process}"] if process else [])
    if no_memory:
        result.update({"memory_key": None, "memory_stored": False,
                       "memory_linked": 0, "memory_skipped": "flag"})
    else:
        result.update(register_artifact(out, memory_key or _default_key(out),
                                        title, facets=all_facets,
                                        link_to=link_to))
    return result


# ── CLI (nós `report` de ADW e qualquer caller bash) ─────────────────────────
def _parse_kv(raw: str, flag: str) -> tuple[str, str]:
    """Split no PRIMEIRO '=' — o contrato exige NOME sem '='.

    O smoke de estreia (30/08/2026) provou o modo de falha: a seção
    "PURPOSE AUDIT (FACT= por alvo)" foi mutilada em silêncio no '=' interno
    do nome — os specs foram renomeados para títulos sem '='. Não há
    heurística de detecção aqui: um adivinhador de mis-split erra nos dois
    sentidos; o contrato determinístico fica declarado no --help.
    """
    if "=" not in raw:
        raise OkfValidationError(
            f"{flag} exige a forma NOME=CONTEUDO (recebi {raw[:60]!r}). "
            f"Exemplo: {flag} \"DEBT (scan_debt)=$4\""
        )
    name, content = raw.split("=", 1)
    if not name.strip():
        raise OkfValidationError(f"{flag}: o NOME antes de '=' não pode ser vazio.")
    return name.strip(), content


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(
        description="Emissor único de documentos OKF (valida antes de escrever; "
                    "elisão declarada; registro no grafo de memória).")
    ap.add_argument("--out", required=True, help="caminho do .md a emitir")
    ap.add_argument("--type", required=True, dest="doc_type")
    ap.add_argument("--title", required=True)
    ap.add_argument("--description", required=True)
    ap.add_argument("--tag", action="append", default=[],
                    help="tag do frontmatter (repetível)")
    ap.add_argument("--field", action="append", default=[],
                    help="campo extra k=v do frontmatter (repetível)")
    ap.add_argument("--section", action="append", default=[],
                    help="seção texto NOME=CONTEUDO (repetível; o 1º '=' separa "
                         "— o NOME não pode conter '=')")
    ap.add_argument("--section-json", action="append", default=[],
                    help="seção JSON NOME=CONTEUDO — fence fechada garantida")
    ap.add_argument("--section-file", action="append", default=[],
                    help="seção texto NOME=@arquivo")
    ap.add_argument("--body-prefix", default="", help="markdown antes das seções")
    ap.add_argument("--cap", type=int, default=DEFAULT_SECTION_CAP,
                    help=f"teto de chars por seção (default {DEFAULT_SECTION_CAP}; "
                         "excedente vai DECLARADO para .full/)")
    ap.add_argument("--prev", default=None,
                    help="report anterior — hash blake2b entra na proveniência")
    ap.add_argument("--process", default=None,
                    help="faceta #process:<v> no registro de memória")
    ap.add_argument("--memory-key", default=None)
    ap.add_argument("--link-lesson", default=None,
                    help="chave de memória da lesson que este report documenta")
    ap.add_argument("--no-memory", action="store_true")
    ap.add_argument("--json", action="store_true", dest="as_json")
    args = ap.parse_args(argv)

    try:
        sections = []
        for raw in args.section:
            n, c = _parse_kv(raw, "--section")
            sections.append(Section(n, c))
        for raw in args.section_json:
            n, c = _parse_kv(raw, "--section-json")
            sections.append(Section(n, c, kind="json"))
        for raw in args.section_file:
            n, c = _parse_kv(raw, "--section-file")
            if not c.startswith("@"):
                raise OkfValidationError(
                    "--section-file exige NOME=@arquivo (o @ marca o caminho).")
            sections.append(Section(n, Path(c[1:]).read_text(encoding="utf-8",
                                                             errors="ignore")))
        fields = dict(_parse_kv(raw, "--field") for raw in args.field)
        result = emit(args.out, args.doc_type, args.title, args.description,
                      sections=sections, tags=args.tag, fields=fields,
                      body_prefix=args.body_prefix, cap=args.cap,
                      prev=args.prev, memory_key=args.memory_key,
                      process=args.process, link_to=args.link_lesson,
                      no_memory=args.no_memory)
    except OkfValidationError as exc:
        print(f"okf_emit: RECUSADO — {exc}", file=sys.stderr)
        return 2

    if args.as_json:
        print(json.dumps(result, ensure_ascii=False, indent=2))
    else:
        for name, chars in result["elided"].items():
            print(f"ELIDED {name}={chars} chars (íntegra em .full/)")
        print(f"REPORT={result['path']}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
