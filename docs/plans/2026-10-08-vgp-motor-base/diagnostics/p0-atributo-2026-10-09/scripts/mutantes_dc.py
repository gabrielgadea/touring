"""Mutantes do DETERMINISMO + SEMANTICA DE DATACLASS (pedidos 2 e 3 da coordenadora): aplica cada mutação, roda os testes vgp::, registra rc + testes que falharam,
restaura o arquivo (cp do backup, nunca git). Uso: python3 mutantes.py <worktree> <saida.json> [M...]
Cada mutante e (arquivo, antigo, novo[, antigo2, novo2...]) sobre o texto do arquivo."""
import json
import os
import re
import shutil
import subprocess
import sys

WT = sys.argv[1]
SAIDA = sys.argv[2]
SO = {a for a in sys.argv[3:] if not a.startswith('--')}
V = f"{WT}/crates/touring-code/src/vgp"
ENV = dict(os.environ, CARGO_TARGET_DIR="/home/gabrielgadea/projects/touring/target/vgp-test")

MUTANTES = {'D1-SEM-ORDENACAO': [('checks.rs',
                       '    v.sort_by(|a, b| {\n'
                       "        a.starts_with('_')\n"
                       "            .cmp(&b.starts_with('_'))\n"
                       '            .then_with(|| a.cmp(b))\n'
                       '    });\n',
                       '')],
 'D2-CORTA-ANTES-DE-ORDENAR': [('checks.rs',
                                '    let mut v: Vec<&String> = membros.iter().collect();',
                                '    let mut v: Vec<&String> = membros.iter().take(REAIS_MAX).collect();')],
 'D3-SUGESTAO-DESEMPATE-PELA-ENTRADA': [('checks.rs',
                                         '.min_by(|(da, ra), (db, rb)| da.cmp(db).then_with(|| ra.cmp(rb)))',
                                         '.min_by_key(|(d, _)| *d)')],
 'D4-SUGESTAO-NOME-ANTES-DA-DISTANCIA': [('checks.rs',
                                          '.min_by(|(da, ra), (db, rb)| da.cmp(db).then_with(|| ra.cmp(rb)))',
                                          '.min_by(|(da, ra), (db, rb)| ra.cmp(rb).then_with(|| da.cmp(db)))')],
 'D5-SEM-PUBLICOS-PRIMEIRO': [('checks.rs',
                               "        a.starts_with('_')\n            .cmp(&b.starts_with('_'))\n            .then_with(|| a.cmp(b))\n",
                               '        a.cmp(b)\n')],
 'D6-ORDEM-INVERTIDA': [('checks.rs',
                         '            .then_with(|| a.cmp(b))\n    });\n    v.into_iter()',
                         '            .then_with(|| b.cmp(a))\n    });\n    v.into_iter()')],
 'D7-SEM-CORTE': [('checks.rs', '    v.into_iter().take(REAIS_MAX).cloned().collect()', '    v.into_iter().cloned().collect()')],
 'D10-REALS-DE-MODULO-SEM-ORDEM': [('checks.rs', '    v.sort();\n    v.truncate(REAIS_MAX);', '    v.truncate(REAIS_MAX);')],
 'N1-C3-VIRA-DFS': [('checks.rs',
                     '        out.extend(c3_merge(listas)?);',
                     '        let mut vistos: Vec<ClasseNo> = Vec::new();\n'
                     '        for no in listas.into_iter().flatten() {\n'
                     '            if !vistos.iter().any(|v| mesmo_no(v, &no)) {\n'
                     '                vistos.push(no);\n'
                     '            }\n'
                     '        }\n'
                     '        out.extend(vistos);')],
 'N2-MRO-NAO-REVERSO': [('checks.rs', '        for (_, nome, m) in linear.iter().rev() {', '        for (_, nome, m) in linear.iter() {')],
 'N3-REDECLARADO-VAI-PARA-O-FIM': [('checks.rs',
                                    '                match campos.iter_mut().find(|c| c.name == f.name) {\n'
                                    '                    Some(slot) => *slot = f.clone(),\n'
                                    '                    None => campos.push(f.clone()),\n'
                                    '                }',
                                    '                campos.retain(|c| c.name != f.name);\n                campos.push(f.clone());')],
 'N4-CLASSVAR-VIRA-CAMPO': [('modules.rs', '    if head == "ClassVar" || head.ends_with(".ClassVar") {\n        return;\n    }\n', '')],
 'N5-INIT-FALSE-IGNORADO': [('modules.rs',
                             '        .and_then(|c| field_call_flag(source, c, "init"))\n        .unwrap_or(true);',
                             '        .and_then(|_| None::<bool>)\n        .unwrap_or(true);')],
 'N6-KW-ONLY-NAO-VAI-PARA-O-FIM': [('modules.rs',
                                    '        .filter(|f| f.init && !f.kw_only)\n'
                                    '        .chain(campos.iter().filter(|f| f.init && f.kw_only))',
                                    '        .filter(|f| f.init)\n        .chain(campos.iter().filter(|_| false))')],
 'N7-SENTINELA-KW-ONLY-IGNORADA': [('modules.rs', '        ci.dc_after_kw_marker = true;\n        return;', '        return;')],
 'N8-KW-ONLY-DA-CLASSE-IGNORADO': [('modules.rs',
                                    '        .any(|d| d.replace(\' \', "").contains("kw_only=True"));',
                                    '        .any(|_| false);')],
 'N9-FIELD-KW-ONLY-IGNORADO': [('modules.rs',
                                '        .and_then(|c| field_call_flag(source, c, "kw_only"))\n'
                                '        .unwrap_or(ci.dc_after_kw_marker || ci.dc_kw_only);',
                                '        .and_then(|_| None::<bool>)\n        .unwrap_or(ci.dc_after_kw_marker || ci.dc_kw_only);')],
 'N10-BASE-IRRESOLVIDA-NAO-ABSTEM': [('checks.rs',
                                      '                Base::Desconhecida => return None,',
                                      '                Base::Desconhecida => {}')],
 'N11-BASE-OPACA-NAO-ABSTEM': [('checks.rs',
                                '        if ci.bases_opaque {\n            return None;\n        }',
                                '        if false {\n            return None;\n        }')],
 'N12-BASES-IGNORADAS': [('checks.rs',
                          '        let linear = self.mro(cls, modulo, 0)?;',
                          '        let linear: Vec<ClasseNo> = vec![(PathBuf::new(), cls.to_string(), modulo.clone())];')],
 'N13-INIT-EXPLICITO-SOBRESCRITO': [('checks.rs',
                                     '        if ci.methods.contains("__init__") || !ci.is_dataclass {',
                                     '        if !ci.is_dataclass {')],
 'N14-INIT-FALSE-NAO-ABSTEM': [('checks.rs',
                                '        if ci.dc_init_false {\n'
                                '            return None; // no generated `__init__`: inherited, so not known here\n'
                                '        }\n',
                                '')],
 'N15-BASE-SUBSCRITA-VIRA-OPACA': [('modules.rs',
                                    '                    Some(v) if matches!(v.kind(), "identifier" | "attribute") => {',
                                    '                    Some(v) if false && matches!(v.kind(), "identifier" | "attribute") => {')],
 'N16-IDENTIDADE-SO-PELO-NOME': [('checks.rs', '    a.0 == b.0 && a.1 == b.1', '    a.1 == b.1')],
 'N17-ARQUIVO-DO-MODULO-NAO-REGISTRADO': [('modules.rs',
                                           '            info.file = Some(path.to_path_buf());',
                                           '            info.file = None;')],
 'N18-BASE-BUILTIN-NAO-E-INOFENSIVA': [('checks.rs',
                                        '        if is_builtin_base(nome) {\n            return Base::Inofensiva;\n        }\n',
                                        '')]}


def rodar():
    r = subprocess.run(
        ["cargo", "test", "-p", "touring-code", "--lib", "vgp::"],
        cwd=WT, env=ENV, capture_output=True, text=True)
    txt = r.stdout + r.stderr
    falhos = sorted(set(re.findall(r"^test (\S+) \.\.\. FAILED", txt, re.M)))
    resumo = re.findall(r"test result: .*", txt)
    return r.returncode, falhos, resumo, txt[-1500:] if r.returncode and not falhos else ""


def checar():
    """Valida que toda ancora existe no codigo atual (sem compilar)."""
    ruim = 0
    for nome, edits in MUTANTES.items():
        for arq, antigo, _ in edits:
            if antigo not in open(f"{V}/{arq}").read():
                print("ANCORA AUSENTE", nome, arq, repr(antigo[:70]))
                ruim += 1
    print("ancoras:", "ok" if not ruim else f"{ruim} ausentes")


def main():
    res = {}
    if os.path.exists(SAIDA):
        res = json.load(open(SAIDA))
    for nome, edits in MUTANTES.items():
        if SO and nome not in SO:
            continue
        arqs = {}
        try:
            for arq, antigo, novo in edits:
                p = f"{V}/{arq}"
                if p not in arqs:
                    arqs[p] = open(p).read()
                    shutil.copy(p, p + ".orig_mut")
                txt = open(p).read()
                if antigo not in txt:
                    raise RuntimeError(f"ancora ausente em {arq}: {antigo[:60]!r}")
                open(p, "w").write(txt.replace(antigo, novo, 1))
            rc, falhos, resumo, cauda = rodar()
            res[nome] = {"rc": rc, "morto": rc != 0, "falhos": falhos, "resumo": resumo, "cauda": cauda}
        except Exception as e:  # noqa: BLE001
            res[nome] = {"erro": str(e)}
        finally:
            for p, orig in arqs.items():
                open(p, "w").write(orig)
                os.remove(p + ".orig_mut")
        json.dump(res, open(SAIDA, "w"), indent=1, ensure_ascii=False)
        print(nome, "->", {k: v for k, v in res[nome].items() if k != "cauda"}, flush=True)


if __name__ == "__main__":
    (checar if "--checar" in sys.argv else main)()
