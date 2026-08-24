"""Varredura das 2 fontes de code mode — mecanismos com endereço file:line.

Pergunta: que práticas EXISTEM nos repos e ainda NÃO estão no plano?
"""
import re, pathlib, collections
RAIZ = pathlib.Path("/home/gabrielgadea/references/code-mode-2026-08-23")

MECANISMOS = [
 ("sdk_instructions",   r"SDK_INSTRUCTIONS"),
 ("code_only_collapse", r"CODE_ONLY_INSTRUCTION|codeOnly|code_only"),
 ("two_arg_contract",   r"description.*required|INVALID_ARGS"),
 ("typings_no_prompt",  r"generateTypes|typings|\.d\.ts.*prompt|toTypeScript"),
 ("needs_approval",     r"needsApproval|needs_approval"),
 ("skills_serialize",   r"skill|serializ.*code|savedCode|codeToTool"),
 ("self_healing",       r"self.?heal|retry.*error|error.*feedback"),
 ("kv_cache",           r"kv.?cache|prefix.*stable|cache.*invalidat"),
 ("size_overlay",       r"MessageSizeOverlay|external_call|external_result"),
 ("efficiency_measure", r"efficiency|roundTrips|tokensSaved"),
 ("abort_signal",       r"abortSignal|AbortController"),
 ("watchers_budget",    r"watcher|budget.*phase|two.?phase"),
 ("isolate_drivers",    r"QuickJS|isolate|worker.*driver|newContext"),
 ("harvest_hint",       r"harvest|reoffer|amortiz"),
 ("pure_function_log",  r"pure function of the log|replay.*log|event.?sourc"),
]

hits = collections.defaultdict(list)
n_arq = 0
for repo in ("deepseek-harness", "tanstack-ai"):
    base = RAIZ/repo
    for f in base.rglob("*"):
        if f.suffix not in (".ts",".tsx",".md",".mts") or "node_modules" in str(f) or "/.git/" in str(f): continue
        n_arq += 1
        try: txt = f.read_text(errors="ignore")
        except OSError: continue
        for nome, padrao in MECANISMOS:
            for i, l in enumerate(txt.splitlines()):
                if re.search(padrao, l, re.I):
                    rel = f"{repo}/{f.relative_to(base)}"
                    if len(hits[nome]) < 4 and rel not in [h.split(":")[0] for h in hits[nome]]:
                        hits[nome].append(f"{rel}:{i+1}")
                    break
print(f"ARQUIVOS_VARRIDOS={n_arq}")
for nome, _ in MECANISMOS:
    h = hits.get(nome, [])
    print(f"{'OK ' if h else '-- '} {nome:<20} {len(h)} :: {'; '.join(h[:3])}")
