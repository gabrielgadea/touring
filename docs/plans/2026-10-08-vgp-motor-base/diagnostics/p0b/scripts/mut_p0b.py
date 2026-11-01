"""Mutantes do P0B (a atributos implicitos, b camada 2 so nega inexistencia definitiva, c __init__ herdado) e a
prova de VERMELHO (reverter as decisoes e ver os testes novos falharem).

Roda DENTRO de uma reserva do hw-slot (rust). Cada mutante e (arquivo relativo a crates/, antigo, novo) sobre o texto;
a ancora tem de existir EXATAMENTE uma vez. Restauracao = reescrever o conteudo original (mtime = AGORA, nunca o
original: um cp -p deixou build velho no P0). Marcador .TROCA-EM-CURSO fica a cargo do mut_p0b.sh.

Uso: python3 mut_p0b.py <worktree> <saida.json> [--checar] [ID ...]    (sem ID = todos, na ordem de MUTANTES)
"""
import json
import os
import re
import subprocess
import sys

WT = sys.argv[1]
SAIDA = sys.argv[2]
ARGS = sys.argv[3:]
CHECAR = "--checar" in ARGS
SO = [a for a in ARGS if not a.startswith("--")]
CR = f"{WT}/crates"
ENV = dict(os.environ, CARGO_TARGET_DIR="/home/gabrielgadea/projects/touring/target/wt-p0b")

PY = "touring-code/src/vgp/lang/python.rs"
MOD = "touring-code/src/vgp/lang/python/modules.rs"
SOMBRA = "touring-code/src/vgp/sombra.rs"
PYR = "touring-pyright/src/pyright.rs"
ACH = "touring-pyright/src/achado.rs"

CODE = ["touring-code"]
PYRIGHT = ["touring-pyright"]

DFS = (
    "        let mut vistos: Vec<ClasseNo> = Vec::new();\n"
    "        for no in listas.into_iter().flatten() {\n"
    "            if !vistos.iter().any(|v| mesmo_no(v, &no)) {\n"
    "                vistos.push(no);\n"
    "            }\n"
    "        }\n"
    "        out.extend(vistos);"
)

# id -> (pacotes, [(arquivo, antigo, novo), ...])
MUTANTES = {
    # ---- (a) atributos implicitos de modulo
    "A1-MODULE_HAS-SEM-IMPLICITOS": (CODE, [(PY, "        if modulo.atributo_implicito(nome) {", "        if false && modulo.atributo_implicito(nome) {")]),
    "A2-FILE-SEMPRE": (CODE, [(MOD, '            "__file__" => self.file.is_some(),', '            "__file__" => true,')]),
    "A3-PATH-EM-MODULO-COMUM": (CODE, [(MOD, '                .is_some_and(|f| f == "__init__.py"),', "                .is_some_and(|_| true),")]),
    "A4a-SEM-NAME": (CODE, [(MOD, '    "__name__",\n', "")]),
    "A4b-SEM-BUILTINS": (CODE, [(MOD, '    "__builtins__",\n', "")]),
    "A5-STDLIB-FECHADA-NAO-ABSTEM": (CODE, [(PY, "                    && !implicito_de_modulo_opaco(&nome)\n", "                    && true\n")]),
    "A6-STDLIB-FILE-NEGA": (CODE, [(MOD, '    IMPLICITOS_DE_TODO_MODULO.contains(&nome) || matches!(nome, "__file__" | "__path__")', "    IMPLICITOS_DE_TODO_MODULO.contains(&nome)")]),
    "A7-ARQUIVO-DO-MODULO-NAO-REGISTRADO": (CODE, [(MOD, "            info.file = Some(path.to_path_buf());", "            info.file = None;")]),
    # ---- (c) __init__ herdado (P0BMRO)
    "C1-SO-INIT-DA-CLASSE": (CODE, [(PY, "            return self.init_herdado(cls, modulo);", "            return Some(ci.init.clone());")]),
    "C2-PRIMEIRA-BASE": (CODE, [(PY, "        for (nome, m, ci) in elos {", "        for (nome, m, ci) in elos.into_iter().take(2) {")]),
    "C3-C3-VIRA-DFS": (CODE, [(PY, "        out.extend(c3_merge(listas)?);", DFS)]),
    "C4-IDENTIDADE-SO-PELO-NOME": (CODE, [(PY, "    a.0 == b.0 && a.1 == b.1", "    a.1 == b.1")]),
    "C5-BASE-IRRESOLVIDA-NAO-ABSTEM": (CODE, [(PY, "                Base::Desconhecida => return None,", "                Base::Desconhecida => {}")]),
    "C6-NEW-NAO-ABSTEM": (CODE, [(PY, '    const DESVIADORES: [&str; 3] = ["__new__", "__init_subclass__", "__signature__"];', '    const DESVIADORES: [&str; 2] = ["__init_subclass__", "__signature__"];')]),
    "C7-INIT_SUBCLASS-NAO-ABSTEM": (CODE, [(PY, '    const DESVIADORES: [&str; 3] = ["__new__", "__init_subclass__", "__signature__"];', '    const DESVIADORES: [&str; 2] = ["__new__", "__signature__"];')]),
    "C8-SIGNATURE-NAO-ABSTEM": (CODE, [(PY, '    const DESVIADORES: [&str; 3] = ["__new__", "__init_subclass__", "__signature__"];', '    const DESVIADORES: [&str; 2] = ["__new__", "__init_subclass__"];')]),
    "C9-METACLASSE-NAO-ABSTEM": (CODE, [(PY, "    ci.has_class_kwargs\n        || init_por_atribuicao", "    false\n        || init_por_atribuicao")]),
    "C10-BASE-NAMEDTUPLE-NAO-ABSTEM": (CODE, [(PY, "        || base_com_construtor_proprio\n", "        || false\n")]),
    "C11-INIT-POR-ATRIBUICAO-NAO-ABSTEM": (CODE, [(PY, "        || init_por_atribuicao\n", "        || false\n")]),
    "C12-EXCEPTION-NAO-E-NEUTRA": (CODE, [(PY, '"object" | "ABC" | "Generic" | "Protocol" | "Exception" | "BaseException"', '"object" | "ABC" | "Generic" | "Protocol" | "BaseException"')]),
    "C13-DATACLASS-GERADO-IGNORADO": (CODE, [(PY, "            if ci.is_dataclass && !ci.dc_init_false {\n                return self.parametros_do_dataclass(nome, m);\n            }\n", "")]),
    "C14-SEM-OBJECT-INIT": (CODE, [(PY, "        Some(self::modules::FuncSig::default())", "        None")]),
    "C15-KWARG-DE-CLASSE-NAO-REGISTRADO": (CODE, [(MOD, "                    ci.has_class_kwargs = true;\n", "")]),
    # o 1º parâmetro de um `def __init__` é a instância, não kwarg (conserto de 17/10: `sem_instancia`)
    "C16-SELF-NO-INIT-PROPRIO": (CODE, [(PY, '        if ci.methods.contains("__init__") {\n            return Some(sem_instancia(ci.init.clone()));', '        if ci.methods.contains("__init__") {\n            return Some(ci.init.clone());')]),
    "C17-SELF-NO-INIT-HERDADO": (CODE, [(PY, "                return Some(sem_instancia(ci.init.clone()));", "                return Some(ci.init.clone());")]),
    # ---- (b) camada 2 so nega inexistencia definitiva
    "B1-TIPO-VIRA-PROJETO": (PYRIGHT, [(PYR, "    } else {\n        Escopo::Tipo\n    }\n}", "    } else {\n        Escopo::Projeto\n    }\n}")]),
    "B2-TIPO-NEGA": (PYRIGHT, [(ACH, "        (_, Escopo::Forma | Escopo::Tipo | Escopo::Dinamico) => Severidade::Warn,", "        (_, Escopo::Forma | Escopo::Dinamico) => Severidade::Warn,")]),
    "B3-ABSTENCAO-EXTERNA-DESLIGADA": (PYRIGHT, [(PYR, "            if membro_nao_publico_de_modulo_externo(d, &nome, escopo) {", "            if false && membro_nao_publico_de_modulo_externo(d, &nome, escopo) {")]),
    "B4-ABSTENCAO-EM-QUALQUER-ESCOPO": (PYRIGHT, [(PYR, "    escopo == Escopo::Externo\n        && nome.starts_with('_')", "    nome.starts_with('_')")]),
    "B5-ABSTENCAO-SO-PRIVADO": (PYRIGHT, [(PYR, "        && nome.starts_with('_')\n", '        && nome.starts_with(\'_\') && !nome.starts_with("__")\n')]),
    "B6-ABSTENCAO-SO-DUNDER": (PYRIGHT, [(PYR, "        && nome.starts_with('_')\n", '        && nome.starts_with("__")\n')]),
    "B7-ABSTENCAO-SO-ATRIBUTO-NAO-IMPORT": (PYRIGHT, [(PYR, "        && (msg.contains(MODULE_ATTRIBUTE) || msg.contains(UNKNOWN_IMPORT))", "        && msg.contains(MODULE_ATTRIBUTE)")]),
    "B8-ABSTENCAO-TAMBEM-DO-PUBLICO": (PYRIGHT, [(PYR, "        && nome.starts_with('_')\n        && (msg", "        && (msg")]),
    "B9-FUSAO-NAO-VETA-INCERTO": (CODE, [(SOMBRA, '                    && f.severidade == "deny"\n                    && existente.alvo != Alvo::Incerto\n', '                    && f.severidade == "deny"\n')]),
    "B10-FUSAO-NUNCA-ESCALA": (CODE, [(SOMBRA, "&& existente.alvo != Alvo::Incerto", "&& false")]),
}

# vermelho: reverte as DECISOES do P0B de uma vez so (os testes novos precisam falhar, os antigos nao)
RED = (
    CODE + PYRIGHT,
    [
        (PY, "        if modulo.atributo_implicito(nome) {", "        if false && modulo.atributo_implicito(nome) {"),
        (PY, "                    && !implicito_de_modulo_opaco(&nome)\n", "                    && true\n"),
        (PY, "            return self.init_herdado(cls, modulo);", "            return Some(ci.init.clone());"),
        (SOMBRA, '                    && f.severidade == "deny"\n                    && existente.alvo != Alvo::Incerto\n', '                    && f.severidade == "deny"\n'),
        (PYR, "    } else {\n        Escopo::Tipo\n    }\n}", "    } else {\n        Escopo::Projeto\n    }\n}"),
        (ACH, "        (_, Escopo::Forma | Escopo::Tipo | Escopo::Dinamico) => Severidade::Warn,", "        (_, Escopo::Forma | Escopo::Dinamico) => Severidade::Warn,"),
        (PYR, "            if membro_nao_publico_de_modulo_externo(d, &nome, escopo) {", "            if false && membro_nao_publico_de_modulo_externo(d, &nome, escopo) {"),
    ],
)


def rodar(pacotes):
    """Roda os testes dos pacotes; devolve (rc, falhos, resumo, cauda_se_nao_compilou)."""
    falhos, resumo, rc_total, cauda = set(), [], 0, ""
    for pkg in pacotes:
        filtro = ["vgp::"] if pkg == "touring-code" else []
        r = subprocess.run(
            ["cargo", "test", "-p", pkg, "--lib", "--"] + filtro,
            cwd=WT, env=ENV, capture_output=True, text=True)
        txt = r.stdout + r.stderr
        achados = set(re.findall(r"^test (\S+) \.\.\. FAILED", txt, re.M))
        falhos |= {f"{pkg}:{f}" for f in achados}
        resumo += [f"{pkg}: {m}" for m in re.findall(r"test result: .*", txt)]
        rc_total = rc_total or r.returncode
        if r.returncode and not achados:
            cauda = txt[-1500:]
    return rc_total, sorted(falhos), resumo, cauda


def aplicar(edits):
    """Aplica todas as edicoes (ancora unica); devolve {arquivo: conteudo_original}."""
    originais = {}
    for arq, antigo, novo in edits:
        p = f"{CR}/{arq}"
        txt = originais.get(arq) or open(p, encoding="utf-8").read()
        n = txt.count(antigo)
        if n != 1:
            raise SystemExit(f"ANCORA {n}x (esperado 1) em {arq}: {antigo[:80]!r}")
        originais.setdefault(arq, txt)
        atual = open(p, encoding="utf-8").read()
        open(p, "w", encoding="utf-8").write(atual.replace(antigo, novo))
    return originais


def restaurar(originais):
    for arq, txt in originais.items():
        # mtime = AGORA (nunca o original): escrever o conteudo, nao copiar com -p
        open(f"{CR}/{arq}", "w", encoding="utf-8").write(txt)


def checar():
    ruim = 0
    for nome, (_, edits) in list(MUTANTES.items()) + [("RED", RED)]:
        for arq, antigo, _ in edits:
            n = open(f"{CR}/{arq}", encoding="utf-8").read().count(antigo)
            if n != 1:
                print("ANCORA", n, nome, arq, repr(antigo[:70]))
                ruim += 1
    print("ancoras:", "ok" if not ruim else f"{ruim} fora")
    return ruim


def um(nome, pacotes, edits):
    originais = {}
    try:
        originais = aplicar(edits)
        rc, falhos, resumo, cauda = rodar(pacotes)
    finally:
        restaurar(originais)
    estado = "MORTO" if falhos else ("ERRO-DE-COMPILACAO" if rc else "SOBREVIVEU")
    print(f"{nome}: {estado} rc={rc} falhos={len(falhos)}", flush=True)
    return {"estado": estado, "rc": rc, "falhos": falhos, "resumo": resumo, "cauda": cauda}


if __name__ == "__main__":
    if CHECAR:
        sys.exit(1 if checar() else 0)
    if checar():
        sys.exit(2)
    saida = {}
    if not SO or "RED" in SO:
        saida["RED"] = um("RED", *RED)
    for nome, (pacotes, edits) in MUTANTES.items():
        if SO and nome not in SO:
            continue
        saida[nome] = um(nome, pacotes, edits)
        json.dump(saida, open(SAIDA, "w"), indent=1, ensure_ascii=False)
    json.dump(saida, open(SAIDA, "w"), indent=1, ensure_ascii=False)
