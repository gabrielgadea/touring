"""Mutantes do P0B-d ("tipo fechado" corroborado pelas duas camadas) e a prova de VERMELHO.

Roda DENTRO de uma reserva do hw-slot (rust). Cada mutante e (arquivo relativo a crates/, antigo, novo) sobre o
texto; a ancora tem de existir EXATAMENTE uma vez. Restauracao = reescrever o conteudo original (mtime = AGORA,
nunca o original); o .sh confere o fonte por cmp contra a copia feita antes.

Uso: python3 mut_p0bd.py <worktree> <saida.json> [--checar] [ID ...]    (sem ID = todos, na ordem de MUTANTES)
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
ENV = dict(os.environ, CARGO_TARGET_DIR="/home/gabrielgadea/projects/touring/target/wt-p0bd")

SOMBRA = "touring-code/src/vgp/sombra.rs"
POLITICA = "touring-code/src/vgp/politica.rs"
PY = "touring-code/src/vgp/lang/python.rs"
STDLIB = "touring-code/src/vgp/lang/python/stdlib_model.rs"
TESTS = "touring-code/src/vgp/lang/python/tests.rs"
PYR = "touring-pyright/src/pyright.rs"
L2 = "touring-hook-handlers/src/hooks/vgp_l2.rs"
CLI = "touring-server/src/cli/vgp.rs"

# pacote -> comando de teste (so o que cobre o mutante; o gate completo e o das suites inteiras)
CMDS = {
    "touring-code": ["cargo", "test", "-p", "touring-code", "--lib", "--no-fail-fast", "vgp::"],
    "touring-pyright": ["cargo", "test", "-p", "touring-pyright", "--lib", "--no-fail-fast"],
    "touring-hook-handlers": [
        "cargo", "test", "-p", "touring-hook-handlers", "--features", "pre-hooks,post-hooks",
        "--lib", "--no-fail-fast", "vgp_l2",
    ],
    "touring-server": ["cargo", "test", "-p", "touring-server", "--lib", "--no-fail-fast", "vgp"],
}
CODE = ["touring-code"]

# id -> (pacotes, [(arquivo, antigo, novo), ...])
MUTANTES = {
    # M1: a fusao ignora a igualdade de classe (qualquer receptor das duas camadas corrobora)
    "M1-FUSAO-IGNORA-A-CLASSE": (CODE, [(SOMBRA, "            (Some(a), Some(b)) => mesmo_tipo(a, b),", "            (Some(_), Some(_)) => true,")]),
    # M2: `str` marcado fechado no modelo
    "M2-STR-MARCADO-FECHADO": (CODE, [(STDLIB, 'pub const TIPOS_FECHADOS: &[(&str, &str)] = &[("stat_result", "os.stat_result")];', 'pub const TIPOS_FECHADOS: &[(&str, &str)] = &[("stat_result", "os.stat_result"), ("str", "builtins.str")];')]),
    # M3: `Fechado` tratado como `Incerto` — (a) na fusao, nunca escala; (b) no adaptador, nasce Incerto
    "M3a-FECHADO-NUNCA-ESCALA-NA-FUSAO": (CODE, [(SOMBRA, "    camada1.alvo == Alvo::Fechado\n", "    false\n")]),
    "M3b-FECHADO-NASCE-INCERTO-NO-ADAPTADOR": (CODE, [(PY, 'out_find(out, "atributo", nome, linha, &motivo, reais, Alvo::Fechado);', 'out_find(out, "atributo", nome, linha, &motivo, reais, Alvo::Incerto);')]),
    # M4: `Fechado` SOZINHO nega
    "M4-FECHADO-SOZINHO-NEGA": (CODE, [(POLITICA, '        Alvo::Projeto | Alvo::Incerto | Alvo::Fechado => "warn",', '        Alvo::Projeto | Alvo::Incerto => "warn",\n        Alvo::Fechado => "deny",')]),
    # M5: o criterio de "fechado" sem a condicao de `__dict__` (o oraculo do teste 7 tem de rejeitar os controles)
    "M5-CRITERIO-SEM-A-CONDICAO-DE-DICT": (CODE, [(TESTS, "sem_dict = t.__dictoffset__ == 0", "sem_dict = True")]),
    # M6: aceitar so o nome exato (sem o ultimo segmento): `os.stat_result` deixa de corroborar
    "M6-SO-O-NOME-EXATO": (CODE, [(SOMBRA, "!ultimo(a).is_empty() && ultimo(a) == ultimo(b)", "a == b")]),
    # M7: `ends_with` em vez de igualdade de segmento: `meu_stat_result` corroboraria
    "M7-ENDS-WITH-EM-VEZ-DE-SEGMENTO": (CODE, [(SOMBRA, "!ultimo(a).is_empty() && ultimo(a) == ultimo(b)", "a.ends_with(b) || b.ends_with(a)")]),
    # M8: o adaptador nao marca o receptor no achado Fechado
    "M8-ADAPTADOR-SEM-RECEPTOR": (CODE, [(PY, "        achado.receptor = Some(tipo.to_string());", "        achado.receptor = None;")]),
    # M9: a fusao so promove o que ainda nao e deny... e tambem promove quando a camada 1 NAO e Fechado
    "M9-PROMOVE-QUALQUER-ALVO": (CODE, [(SOMBRA, "    camada1.alvo == Alvo::Fechado\n", "    camada1.alvo != Alvo::Externo\n")]),
    # M10: o mapeador do hook da camada 2 nao carrega a classe receptora
    "M10-HOOK-SEM-RECEPTOR": (["touring-hook-handlers"], [(L2, ".com_receptor(classe_receptora(&a.mensagem))", ".com_receptor(None)")]),
    # M11: o mapeador do CLI (--camadas 1,2) nao carrega a classe receptora
    "M11-CLI-SEM-RECEPTOR": (["touring-server"], [(CLI, ".com_receptor(touring_pyright::pyright::classe_receptora(&a.mensagem))", ".com_receptor(None)")]),
    # M13: o conserto da corrida de /tmp (dois testes P0B no mesmo diretorio fixo) revertido — o guarda tem de morrer
    "M13-DIRETORIO-TEMPORARIO-COMPARTILHADO": (CODE, [(TESTS, 'fn p0b_c_base_irresolvida_new_metaclasse_init_subclass_e_signature_abstem_em_todo_balde() {\n    let tmp = std::env::temp_dir().join("vgp-t-p0b-c3-abstem");', 'fn p0b_c_base_irresolvida_new_metaclasse_init_subclass_e_signature_abstem_em_todo_balde() {\n    let tmp = std::env::temp_dir().join("vgp-t-p0b-c3");')]),
    # M12: `classe_receptora` aceita receptor nao-simples (`X | None`, `dict[...]`)
    "M12-RECEPTORA-ACEITA-UNIAO": (["touring-pyright", "touring-code"], [(PYR, "        .filter(|c| is_simple_name(c))\n", "")]),
}

# vermelho: reverte o CONSERTO inteiro de uma vez (os testes novos precisam falhar, os antigos nao)
RED = (
    ["touring-code", "touring-pyright", "touring-hook-handlers"],
    [
        (STDLIB, 'pub const TIPOS_FECHADOS: &[(&str, &str)] = &[("stat_result", "os.stat_result")];', "pub const TIPOS_FECHADOS: &[(&str, &str)] = &[];"),
        (SOMBRA, "    camada1.alvo == Alvo::Fechado\n", "    false\n"),
        (PYR, "        .filter(|c| is_simple_name(c))\n", "        .filter(|_| false)\n"),
    ],
)


def rodar(pacotes):
    """Roda os testes dos pacotes; devolve (rc, falhos, resumo, cauda_se_nao_compilou)."""
    falhos, resumo, rc_total, cauda = set(), [], 0, ""
    for pkg in pacotes:
        r = subprocess.run(CMDS[pkg], cwd=WT, env=ENV, capture_output=True, text=True)
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
