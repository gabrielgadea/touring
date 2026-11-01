#!/usr/bin/env python3
"""Generates `tabelas.rs` — the closed tables the TS/TSX/JS adapters (VTS) consult.

Run:  python3 gerar_tabelas.py <node_modules dir> > tabelas.rs

Sources, all READ from the declaration files of a real project (never typed by hand):
  * `typescript/lib/lib.*.d.ts`  → global value names (ES + DOM) and the members of the builtin types;
  * `@types/node/*.d.ts`         → Node globals (`process`, `Buffer`, timers…);
  * `@types/react/index.d.ts`    → the union of every JSX attribute name React types declare.

The tables are OVER-approximations on purpose: a name that is in a table is never reported missing, so an
extra name costs a missed detection, never a false deny.
"""

import re
import sys
from pathlib import Path

NM = Path(sys.argv[1])
LIB = NM / "typescript" / "lib"

# --- global value names -------------------------------------------------------------------------------------
DECL = re.compile(r"^declare\s+(?:var|let|const|function|class|namespace|enum)\s+([A-Za-z_$][\w$]*)", re.M)
INDENTED_VAR = re.compile(r"^\s+(?:var|let|const|function)\s+([A-Za-z_$][\w$]*)", re.M)

globais: set[str] = set()
for f in sorted(LIB.glob("lib.*.d.ts")):
    if "webworker" in f.name or "scripthost" in f.name:
        continue
    globais |= set(DECL.findall(f.read_text(errors="ignore")))

for nome in ("globals.d.ts", "process.d.ts", "console.d.ts", "timers.d.ts", "buffer.d.ts", "module.d.ts"):
    p = NM / "@types" / "node" / nome
    if p.is_file():
        t = p.read_text(errors="ignore")
        globais |= set(DECL.findall(t))
        # `declare global { var process: … }` blocks are indented
        for bloco in re.finditer(r"declare\s+global\s*\{", t):
            globais |= set(INDENTED_VAR.findall(t[bloco.end(): bloco.end() + 40000]))
web = NM / "@types" / "node" / "web-globals"
if web.is_dir():
    for p in web.glob("*.d.ts"):
        t = p.read_text(errors="ignore")
        for bloco in re.finditer(r"declare\s+global\s*\{", t):
            globais |= set(INDENTED_VAR.findall(t[bloco.end(): bloco.end() + 40000]))

# Names that no declaration file lists but every JS runtime resolves, and the test-runner globals
# (jest / vitest / mocha / jasmine / cypress) that `@types/*` provide only when the project asks for them.
globais |= {
    "undefined", "NaN", "Infinity", "globalThis", "arguments", "eval", "require", "module", "exports",
    "__dirname", "__filename", "process", "Buffer", "global", "console",
    "Deno", "Bun",
    "describe", "it", "test", "expect", "jest", "vi", "beforeEach", "afterEach", "beforeAll", "afterAll",
    "fdescribe", "fit", "xdescribe", "xit", "xtest", "pending", "fail", "spyOn", "suite", "before", "after",
    "context", "specify", "setup", "teardown", "suiteSetup", "suiteTeardown", "run", "cy", "Cypress",
    "expectTypeOf", "assertType", "chrome", "browser",
}
globais.discard("")

# --- members of builtin types -------------------------------------------------------------------------------
TIPOS = [
    "Date", "Map", "Set", "WeakMap", "WeakSet", "WeakRef", "RegExp", "Promise", "Array", "ReadonlyArray",
    "String", "Number", "Boolean", "BigInt", "Symbol", "Error", "TypeError", "RangeError", "SyntaxError",
    "ReferenceError", "EvalError", "URIError", "URL", "URLSearchParams", "AbortController", "AbortSignal",
    "Headers", "FormData", "Blob", "ArrayBuffer", "DataView", "TextEncoder", "TextDecoder",
    "Uint8Array", "Int8Array", "Uint16Array", "Int16Array", "Uint32Array", "Int32Array", "Float32Array",
    "Float64Array", "Uint8ClampedArray", "BigInt64Array", "BigUint64Array",
]
OBJETO = ["constructor", "toString", "toLocaleString", "valueOf", "hasOwnProperty", "isPrototypeOf",
          "propertyIsEnumerable", "__proto__", "__defineGetter__", "__defineSetter__", "__lookupGetter__",
          "__lookupSetter__"]

HEAD = re.compile(r"^interface\s+([A-Za-z_$][\w$]*)\s*(?:<[^{]*?>)?\s*(?:extends\s+([^{]+?))?\s*\{", re.M)
MEMBRO = re.compile(r"^ {4}(?:readonly\s+)?(?:\"([^\"]+)\"|'([^']+)'|([A-Za-z_$][\w$]*))\s*(?:\?|\(|<|:)", re.M)

interfaces: dict[str, dict] = {}
for f in sorted(LIB.glob("lib.*.d.ts")):
    if "webworker" in f.name or "scripthost" in f.name:
        continue
    t = f.read_text(errors="ignore")
    for m in HEAD.finditer(t):
        nome, ext = m.group(1), m.group(2)
        fim = t.find("\n}", m.end())
        if fim < 0:
            continue
        corpo = t[m.end(): fim]
        alvo = interfaces.setdefault(nome, {"membros": set(), "extends": set()})
        for mm in MEMBRO.finditer(corpo):
            alvo["membros"].add(mm.group(1) or mm.group(2) or mm.group(3))
        if ext:
            for e in re.split(r",", re.sub(r"<[^<>]*(?:<[^<>]*>[^<>]*)*>", "", ext)):
                e = e.strip()
                if e:
                    alvo["extends"].add(e)


def fecho(nome: str, visto: set[str]) -> set[str]:
    if nome in visto or nome not in interfaces:
        return set()
    visto.add(nome)
    out = set(interfaces[nome]["membros"])
    for e in interfaces[nome]["extends"]:
        out |= fecho(e, visto)
    return out


# ECMA-262 Annex B members every engine has and the TS lib does not type (legacy, but real at runtime).
ANEXO_B = {
    "Date": ["getYear", "setYear", "toGMTString"],
    "String": ["anchor", "big", "blink", "bold", "fixed", "fontcolor", "fontsize", "italics", "link", "small",
               "strike", "sub", "sup", "trimLeft", "trimRight", "substr"],
    "RegExp": ["compile"],
}

tipos_membros: dict[str, list[str]] = {}
for t in TIPOS:
    ms = fecho(t, set()) | set(OBJETO) | set(ANEXO_B.get(t, []))
    if t in ("Array", "ReadonlyArray"):
        ms |= {"length"}
    if t not in interfaces:
        sys.exit(f"tipo `{t}` não declarado nos lib.*.d.ts — o diretório lib está errado?")
    tipos_membros[t] = sorted(ms)

# --- React attribute names ---------------------------------------------------------------------------------
rt = (NM / "@types" / "react" / "index.d.ts").read_text(errors="ignore")
atributos: set[str] = set()
for m in re.finditer(r"^\s*interface\s+(\w*(?:Attributes|Props))\b[^{]*\{", rt, re.M):
    if not re.search(r"Attributes|Props", m.group(1)):
        continue
    fim = rt.find("\n    }", m.end())
    corpo = rt[m.end(): fim if fim > 0 else m.end() + 20000]
    for mm in re.finditer(r"^\s{8}(?:readonly\s+)?(?:\"([^\"]+)\"|'([^']+)'|([A-Za-z_$][\w$-]*))\s*\??\s*[:(<]", corpo, re.M):
        atributos.add(mm.group(1) or mm.group(2) or mm.group(3))
atributos |= {"key", "ref", "children", "dangerouslySetInnerHTML", "suppressContentEditableWarning",
              "suppressHydrationWarning"}
if len(atributos) < 300:
    sys.exit(f"só {len(atributos)} atributos React lidos — @types/react ausente ou em outro formato")


def rust_lista(nome: str, itens: list[str], doc: str) -> str:
    corpo = ",\n    ".join(f'"{i}"' for i in itens)
    return f"/// {doc}\npub(super) static {nome}: &[&str] = &[\n    {corpo},\n];\n"


print("//! GENERATED by `gerar_tabelas.py` from a real project's declaration files — do not edit by hand.")
print("//!")
print("//! Closed tables the TS/TSX/JS adapters consult. They are over-approximations: a name in a table is")
print("//! never reported missing, so an extra name costs a missed detection, never a false deny.")
print()
print(rust_lista("GLOBAIS", sorted(globais), f"{len(globais)} global value names (ES + DOM + Node + test runners)."))
print(rust_lista("ATRIBUTOS_REACT", sorted(atributos), f"{len(atributos)} JSX attribute names the React types declare (union over every tag)."))
print("/// Members of the builtin types a receiver can be proved to have (own + inherited + `Object`).")
print("pub(super) static TIPOS_BUILTIN: &[(&str, &[&str])] = &[")
for nome in sorted(tipos_membros):
    itens = ", ".join(f'"{m}"' for m in tipos_membros[nome])
    print(f'    ("{nome}", &[{itens}]),')
print("];")
