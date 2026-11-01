"""HORUST-REAL — mutantes do gerador: cada mutante troca UM trecho de lógica numa CÓPIA dos módulos (o original nunca é tocado) e roda os
testes; o mutante tem de ser MORTO (testes reprovam). Sobrevivente = teste que não prova a cláusula. Saída: `mutantes_horust.json`.

Uso: python3 -I -B mutantes_horust.py
"""

import json
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent

# (id, arquivo, trecho original, trecho mutado)
MUTANTES = [
    ("m01 sem corte em cfg(test)", "operadores_rust.py", 'ln.strip().startswith("#[cfg(test)]")', "False"),
    ("m02 macro_rules vivo", "operadores_rust.py", 'mortas.update(range(mascarado.count("\\n", 0, m.start()), mascarado.count("\\n", 0, j) + 1))', "pass"),
    ("m03 atributo vivo", "operadores_rust.py", 'if s.startswith("#[") or s.startswith("#!["):', "if False:"),
    ("m04 chamada livre aceita método", "operadores_rust.py", "(?<![\\w.:$'])([a-z_][a-z0-9_]{2,})(?=\\s*\\()", "(?<![\\w:$'])([a-z_][a-z0-9_]{2,})(?=\\s*\\()"),
    ("m05 nome curto vira sítio", "operadores_rust.py", "_VALOR = re.compile(r\"(?<![\\w.:$'])([a-z_][a-z0-9_]{2,})", "_VALOR = re.compile(r\"(?<![\\w.:$'])([a-z_][a-z0-9_]{0,})"),
    ("m06 palavra reservada vira sítio", "operadores_rust.py", "if g.group(1) in KEYWORDS or re.search", "if re.search"),
    ("m07 definição `fn x(` vira sítio", "operadores_rust.py", r'(?:fn|macro_rules!)\s*$', r'(?:fnx|macro_rules!)\s*$'),
    ("m08 prelude vira tipo-sítio", "operadores_rust.py", "if nome not in PRELUDE_TIPOS and not nome.isupper():", "if not nome.isupper():"),
    ("m09 raiz do caminho vira sítio de import", "operadores_rust.py", "if len(segs) >= 3:", "if len(segs) >= 2:"),
    ("m10 `pub use` vira sítio", "operadores_rust.py", 'r"^(\\s*)use\\s+((?:crate|super|self|touring_[a-z0-9_]+)(?:::[A-Za-z_][A-Za-z0-9_]*)+)', 'r"^(\\s*)(?:pub\\s+)?use\\s+((?:crate|super|self|touring_[a-z0-9_]+)(?:::[A-Za-z_][A-Za-z0-9_]*)+)'),
    ("m11 std vira sítio de E0433", "operadores_rust.py", "if g.group(1) not in STD_TIPOS_CAMINHO and not g.group(1).isupper():", "if True:"),
    ("m12 campo aceita chamada", "operadores_rust.py", "\\b(?!\\s*(?:\\(|::))", "\\b(?!::)"),
    ("m13 cadeia com ponto no início ignorada", "operadores_rust.py", "*([_METODO_LINHA.match(ln)] if _METODO_LINHA.match(ln) else [])", "*[]"),
    ("m14 `} == x` vira padrão", "operadores_rust.py", '(r.startswith("=") and not r.startswith("=="))', 'r.startswith("=")'),
    ("m15 definição de struct vira literal", "operadores_rust.py", "struct|enum|impl|trait", "enum|impl|trait"),
    ("m15b guarda `} if` vira literal", "operadores_rust.py", 're.match(r"if\\b", r) is not None', "False"),
    ("m16 remoção com 1 campo só", "operadores_rust.py", "len(campos) >= 2 and not update", "len(campos) >= 1 and not update"),
    ("m17 remoção com ..base", "operadores_rust.py", "len(campos) >= 2 and not update", "len(campos) >= 2"),
    ("m18 Self vira tipo do literal", "operadores_rust.py", 'or m.group(1) == "Self"', "or False"),
    ("m19 mutar não confere o texto", "operadores_rust.py", "!= sitio.nome:\n        raise ValueError", "== '\\x00':\n        raise ValueError"),
    ("m20 remoção não tira a linha", "operadores_rust.py", "del linhas[sitio.linha - 1]", "pass"),
    ("m21 distância errada", "operadores_rust.py", "(x != y)", "(x == y)"),
    ("m22 proporção 70/30 vira 90/10", "operadores_rust.py", "% 10 < 7", "% 10 < 9"),
    ("m23 edição mexe no primeiro caractere", "operadores_rust.py", "pos = rng.randrange(1, n)", "pos = rng.randrange(0, n)"),
    ("m24 plausível ignora existentes", "operadores_rust.py", "if v not in existentes and v not in KEYWORDS:", "if v not in KEYWORDS:"),
    ("m25 comentário de linha só de documentação", "operadores_rust.py", 'texto.startswith("//", i)', 'texto.startswith("///", i)'),
    ("m26 comentário aninhado", "operadores_rust.py", "prof, j = prof + 1, j + 2", "prof, j = prof, j + 2"),
    ("m27 char literal vira lifetime", "operadores_rust.py", "if i + 2 < n and texto[i + 2] == \"'\":", "if False:"),
    ("m28 escape de string", "operadores_rust.py", 'j += 2 if texto[j] == "\\\\" else 1', "j += 1"),
    ("m29 round_robin sem ordenar os itens", "selecao_horust.py", "ordenados = {g: ordem(itens, semente) for g, itens in grupos.items()}", "ordenados = {g: list(itens) for g, itens in grupos.items()}"),
    ("m30 teto final folgado", "selecao_horust.py", "cap = int(teto * melhor)\n    todos", "cap = int(teto * melhor) + 1\n    todos"),
    ("m31 piso de linhas", "selecao_horust.py", "linhas >= LINHAS_MIN", "linhas > LINHAS_MIN"),
    ("m32 ambíguo só com 3 alvos", "selecao_horust.py", "if n > 1}", "if n > 2}"),
    ("m33 estilo 1/3", "selecao_horust.py", 'indice % 2 == 0', 'indice % 3 == 0'),
    ("m34 fixture VRS liberada", "selecao_horust.py", '"crates/touring-code/src/vgp/",', '"crates/touring-code/src/vgp-x/",'),
    ("m35 dedupe de diagnóstico", "oraculo_cargo_json.py", "if d not in vistos:", "if True:"),
    ("m36 aviso conta como erro", "oraculo_cargo_json.py", 'if m.get("level") != "error" or not primarios:', "if not primarios:"),
    ("m37 vários erros no arquivo passam", "oraculo_cargo_json.py", "if len(diags_do_arquivo) > 1:", "if len(diags_do_arquivo) > 2:"),
    ("m38 janela do E0063 maior", "oraculo_cargo_json.py", "inj.linha - 60 <= d.linha", "inj.linha - 61 <= d.linha"),
    ("m39 nome pedaço de palavra", "oraculo_cargo_json.py", r'r"(?<![\w])" + re.escape(nome) + r"(?![\w])"', 're.escape(nome)'),
    ("m40 erro em arquivo sem injeção tem dono", "oraculo_cargo_json.py", "[d for d in diags if d.arq not in donos]", "[]"),
    ("m41 cascata de uma só não descarta", "oraculo_cargo_json.py", "if len(injecoes) == 1:", "if len(injecoes) == 0:"),
    ("m42 não confere o original antes de aplicar", "oraculo_cargo_json.py", "if _sha(atual) != _sha(i.texto_original.encode(\"utf-8\")):", "if False:"),
    ("m43 contrato aceita qualquer saída", "oraculo_cargo_json.py", 'sinal_re=r\'"reason"\\s*:\\s*"build-finished"\'', "sinal_re=r'.'"),
    ("m44 sem --locked", "oraculo_cargo_json.py", '"--message-format=json", "--locked",', '"--message-format=json",'),
    ("m45 pacote com erro conta como limpo", "construtor_horust.py", "if n in com_erro:", "if False:"),
    ("m46 alvo faltando conta como limpo", "construtor_horust.py", "not esperado <= compilados[n]", "False"),
    ("m47 arquivo reaproveitado entre classes", "construtor_horust.py", "usados.add(rel)", "pass"),
    ("m48 E0063 nomeia o campo e não o tipo", "construtor_horust.py", 'novo, nome_msg = "", s.tipo', 'novo, nome_msg = "", s.nome'),
    ("m49 corpus sobrescreve", "construtor_horust.py", "if saida.exists() and any(saida.iterdir()):", "if False:"),
    ("m50 positivo vira reproduz=false", "construtor_horust.py", '"rotulo": "positivo", "reproduz": "true"', '"rotulo": "positivo", "reproduz": "false"'),
    ("m51 diags_out vazio", "construtor_horust.py", "if diags_out is not None:", "if False:"),
    ("m53 amostra sem piso de 3", "construir_horust_real.py", "n = max(3, math.ceil(0.10 * len(validos)))", "n = max(1, math.ceil(0.10 * len(validos)))"),
    ("m54 amostra de 20%", "construir_horust_real.py", "math.ceil(0.10 * len(validos))", "math.ceil(0.20 * len(validos))"),
    ("m55 rodada seguinte retenta classe com exatamente 20", "construir_horust_real.py", "if validos[c] < MIN_POR_CLASSE}", "if validos[c] <= MIN_POR_CLASSE}"),
    ("m56 semente da rodada 2 igual à da 1", "construir_horust_real.py", 'return SEMENTE if k == 1 else f"{SEMENTE}-r{k}"', "return SEMENTE"),
    ("m57 negativo tentado entra", "construir_horust_real.py", "if rel not in tentados:", "if True:"),
    ("m58 teto dos positivos fora", "construir_horust_real.py", "sel.selecao_com_teto(por_crate, TETO,", "sel.selecao_com_teto(por_crate, 1.0,"),
    ("m60 integridade ignora arquivo do dev", "integridade_horust.py", "if m := sel.motivo_exclusao(rel, dev, controle):", "if False:"),
    ("m61 integridade ignora worktree alterado", "integridade_horust.py", '_sha(disco.read_bytes()) != e.get("sha_original")', "False"),
    ("m62 integridade ignora snap adulterado", "integridade_horust.py", 'if not snap.is_file() or _sha(snap.read_bytes()) != e.get("snap_sha256"):', "if False:"),
    ("m63 integridade ignora base/ trocada", "integridade_horust.py", '_sha(base.read_bytes()) != e.get("sha_original")', "False"),
    ("m64 integridade ignora código fora da classe", "integridade_horust.py", 'if e.get("codigo_esperado") not in CODIGOS_DA_CLASSE.get(e.get("classe", ""), set()):', "if False:"),
    ("m65 integridade ignora mensagem sem código", "integridade_horust.py", 'if not e.get("mensagem", "").startswith(', "if False and not e.get(\"mensagem\", \"\").startswith("),
    ("m66 integridade ignora negativo diferente", "integridade_horust.py", "snap.read_bytes() != disco.read_bytes()", "False"),
    ("m67 integridade sem teto", "integridade_horust.py", "if n > int(teto * total)]", "if n > total]"),
    ("m68 integridade ignora id duplicado", "integridade_horust.py", "for i, n in ids.items() if n > 1]", "for i, n in ids.items() if n > 99]"),
    ("m69 integridade ignora arquivo repetido", "integridade_horust.py", "for p, n in paths.items() if n > 1]", "for p, n in paths.items() if n > 99]"),
    ("m70 integridade ignora positivo=negativo", "integridade_horust.py", "sorted(pos & neg)", "[]"),
    ("m71 integridade piso de classe 19", "integridade_horust.py", "if por_classe[c] < minimo_por_classe]", "if por_classe[c] < minimo_por_classe - 1]"),
    ("m72 integridade piso de negativos", "integridade_horust.py", "if len(neg) < minimo_negativos:", "if len(neg) < minimo_negativos - 1:"),
    ("m73 selo sem o pré-registro", "integridade_horust.py", '"pre_registro_sha256": _sha(pre_registro.read_bytes())', '"pre_registro_sha256": ""'),
    ("m75 antecadeias ignora transitividade", "selecao_horust.py", "pilha.extend(deps.get(x, ()))", "pass"),
    ("m76 antecadeias junta tudo", "selecao_horust.py", "if not any(m in alcance[n] or n in alcance[m] for m in g):", "if True:"),
    ("m77 camadas dos bins separadas", "construtor_horust.py", "saida.update({(p, a): 0 for p, a in nos if a != \"lib\"})", "saida.update({(p, a): i for i, (p, a) in enumerate(sorted(n for n in nos if n[1] != \"lib\"))})"),
    ("m78 dependência de dev conta", "construtor_horust.py", 'if d.get("kind") != "dev"}', "}"),
    ("m79 grupo ignora camadas", "construtor_horust.py", "grupo = camadas[(t.pkg, t.alvo)] if camadas else t.pkg", "grupo = t.pkg"),
    ("m80 comando só com o primeiro pacote", "oraculo_cargo_json.py", "*[x for p in lista for x in (\"-p\", p)]", "\"-p\", lista[0]"),
    ("m81 runner sem exigir pacote", "oraculo_cargo_json.py", 'if not pacotes or "" in pacotes:', "if False:"),
    ("m82 plano da rodada exclui arquivos da própria rodada", "construir_horust_real.py", 'usados = {r["rel"] for r in feitos if r["rodada"] < k}', 'usados = {r["rel"] for r in feitos if r["rodada"] <= k}'),
    ("m83 retomada repete o que já foi feito", "construir_horust_real.py", "return [t for t in plano if t.inj.id not in ja]", "return plano"),
    ("m84 pendentes olham a própria rodada", "construir_horust_real.py", 'if r["motivo"] == "" and r["rodada"] < k)', 'if r["motivo"] == "" and r["rodada"] <= k)'),
    ("m52 dep-info sem fallback de prefixo lib", "construtor_horust.py", 'rmeta.stem.removeprefix("lib") + ".d"', 'rmeta.stem + ".d"'),
    # rodada 2 (reparo D1/D2) e revalidação
    ("m85 nome_tipo só aceita E0425", "operadores_rust.py", 'ACEITOS = {"nome_tipo": ("E0425", "E0412")}', 'ACEITOS = {"nome_tipo": ("E0425",)}'),
    ("m86 validar ignora os códigos aceitos", "oraculo_cargo_json.py", "if d.codigo not in (inj.aceitos or (inj.codigo,)):", "if d.codigo != inj.codigo:"),
    ("m87 reconstruir perde os códigos aceitos", "construir_horust_real.py", 'op.ACEITOS.get(r["operador"], ()))', "())"),
    ("m88 registro mais antigo vence", "construir_horust_real.py", 'list({r["id"]: r for r in todos}.values())', 'list({r["id"]: r for r in reversed(todos)}.values())'),
    ("m89 reparo vale a partir da rodada 2", "construir_horust_real.py", "return REPARO_R2 if k == 2 else None", "return REPARO_R2 if k >= 2 else None"),
    ("m90 filtro da rodada 2 deixa o último segmento", "construir_horust_real.py", 's.posicao == "penultimo") if k == 2 else None', 's.posicao != "") if k == 2 else None'),
    ("m91 filtro de segmento vale na rodada 1", "construir_horust_real.py", 's.posicao == "penultimo") if k == 2 else None', 's.posicao == "penultimo") if k >= 1 else None'),
    ("m92 posição do segmento trocada", "operadores_rust.py", 'match(ln, pos[-1]), posicao="ultimo")', 'match(ln, pos[-1]), posicao="penultimo")'),
    ("m93 revalidar pega todo descarte de nome_tipo", "construir_horust_real.py", 'r["operador"] == "nome_tipo" and r["motivo"] == "codigo_inesperado"', 'r["operador"] == "nome_tipo" and r["motivo"] != ""'),
    ("m94 revalidar pega qualquer operador", "construir_horust_real.py", 'r["operador"] == "nome_tipo" and r["motivo"] == "codigo_inesperado"', 'r["motivo"] == "codigo_inesperado"'),
    ("m95 reparo planeja também as classes pendentes", "construir_horust_real.py", "totais = {} if reparo_da_rodada(k) else classes_pendentes(k, feitos)", "totais = classes_pendentes(k, feitos)"),
    # estrato cfg: operador
    ("c01 erro do cfg sem a linha a mais", "operador_cfg.py", "return s.linha_uso + 1 if s.linha_uso > s.linha_def else s.linha_uso", "return s.linha_uso"),
    ("c02 erro do cfg sempre deslocado", "operador_cfg.py", "return s.linha_uso + 1 if s.linha_uso > s.linha_def else s.linha_uso", "return s.linha_uso + 1"),
    ("c03 linha do uso não desfaz o deslocamento", "operador_cfg.py", "return linha_erro - 1 if linha_erro > linha_def else linha_erro", "return linha_erro"),
    ("c04 `test` vira atributo inofensivo", "operador_cfg.py", "(?:inline|must_use", "(?:test|inline|must_use"),
    ("c05 comentário esconde o atributo", "operador_cfg.py", "if originais[j].strip() and not linhas[j].strip():", "if False:"),
    ("c06 exige só duas ocorrências ou mais", "operador_cfg.py", "len(ocorrencias) != 2", "len(ocorrencias) < 2"),
    ("c07 recursão vira sítio", "operador_cfg.py", "i <= j <= fim", "j == i"),
    ("c08 método vira chamada livre", "operador_cfg.py", "(?<![\\w.:$']){re.escape(nome)}\\s*\\(", "(?<![\\w:$']){re.escape(nome)}\\s*\\("),
    ("c09 uso na zona de teste vale", "operador_cfg.py", "if j in mortas or", "if"),
    ("c10 definição na zona morta vale", "operador_cfg.py", "or i in mortas or", "or"),
    ("c11 nome de outro arquivo não exclui", "operador_cfg.py", "or m.group(1) in em_outros_arquivos", ""),
    ("c12 mutar não confere o nome", "operador_cfg.py", "m.group(1) != s.nome", "False"),
    ("c13 cfg inserido uma linha abaixo", "operador_cfg.py", "LINHA_CFG, *linhas[s.linha_def - 1 :]", "LINHA_CFG, *linhas[s.linha_def :]"),
    ("c14 sem o modificador unsafe", "operador_cfg.py", "(?:unsafe\\s+)?", ""),
    # estrato cfg: seleção e driver
    ("k01 selecao_ate sem o máximo", "selecao_horust.py", "range(1, min(total, maximo) + 1)", "range(1, total + 1)"),
    ("k02 plano ignora os excluídos", "construir_cfg.py", "if rel in excluidos:", "if False:"),
    ("k03 palavra em 2 arquivos não exclui", "construir_cfg.py", "cont[a.pkg][w] > 1", "cont[a.pkg][w] > 2"),
    ("k04 plano sem limite de tentativas", "construir_cfg.py", "sel.round_robin(por_pacote, n,", "sel.round_robin(por_pacote, 10**6,"),
    ("k05 registro perde a linha da definição", "construir_cfg.py", '"linha_def": t.coluna,', '"linha_def": 0,'),
    ("k06 descarte entra na escolha", "construir_cfg.py", 'if r["motivo"] == "":\n            por_crate', "if True:\n            por_crate"),
    ("k07 máximo do estrato maior", "construir_cfg.py", "SEMENTE_CFG + \"|cfg\", MAX_CFG)", "SEMENTE_CFG + \"|cfg\", MAX_CFG + 10)"),
    ("k08 negativo é o texto mutado", "construir_cfg.py", "p.rel, p.pkg, p.texto_original, p.id)", "p.rel, p.pkg, p.texto_novo, p.id)"),
    ("k09 positivos fora dos ids pedidos", "construir_cfg.py", 'if r["id"] in ids:\n            texto = ler', "if True:\n            texto = ler"),
    ("k10 palavras sem mascarar", "construir_cfg.py", "re.findall(r\"[A-Za-z_][A-Za-z0-9_]*\", op.mascarar(texto))", "re.findall(r\"[A-Za-z_][A-Za-z0-9_]*\", texto)"),
    # estrato cfg: integridade
    ("i01 inserção aceita qualquer tamanho", "integridade_cfg.py", "if len(novo) != len(velho) + 1:", "if len(novo) < len(velho):"),
    ("i02 inserção aceita qualquer linha", "integridade_cfg.py", "novo[k] == oc_.LINHA_CFG and ", ""),
    ("i03 cfg acima de qualquer função", "integridade_cfg.py", 'm.group(1) != e.get("nome_esperado")', "False"),
    ("i04 arquivo do principal passa", "integridade_cfg.py", "if rel in principal:", "if False:"),
    ("i05 teto exclusivo", "integridade_cfg.py", "n > int(teto * len(positivos))", "n >= int(teto * len(positivos))"),
    ("i06 máximo exclusivo", "integridade_cfg.py", "len(positivos) > maximo", "len(positivos) >= maximo"),
    ("i07 positivo sem par passa", "integridade_cfg.py", "if pareados[i] == 0]", "if False]"),
    ("i08 estrato qualquer", "integridade_cfg.py", 'e.get("estrato") != ESTRATO', "False"),
    ("i09 linha do erro qualquer", "integridade_cfg.py", 'f"{e.get(\'nome_esperado\')}(" not in novo[linha - 1]', "False"),
    ("i10 par inexistente passa", "integridade_cfg.py", "if par is None:", "if False:"),
    # adendos: prefixo do pré-registro e regra do registro
    ("r01 prefixo compara o arquivo inteiro", "integridade_horust.py", "dados[:tamanho_original]", "dados"),
    ("r02 tudo é regra original", "construir_horust_real.py", 'return "original" if r["rodada"] == 1 and "codigo_rustc" not in r else "adendo1"', 'return "original"'),
    ("r03 rodada 2 sem codigo_rustc seria original", "construir_horust_real.py", 'if r["rodada"] == 1 and "codigo_rustc" not in r else', 'if "codigo_rustc" not in r else'),
    ("r04 revalidado conta como original", "construir_horust_real.py", 'if r["rodada"] == 1 and "codigo_rustc" not in r else', 'if r["rodada"] == 1 else'),
    ("v01 negativo tentado passa", "verificar_negativos_tentados.py", "            if rel in tentados:", "            if False:"),
    ("v02 negativo ignora o rótulo", "verificar_negativos_tentados.py", 'if e and e["rotulo"] == "negativo":', "if e:"),
    ("v03 resumo não conta as violações", "verificar_negativos_tentados.py", '"violacoes": len(v),', '"violacoes": 0,'),
    ("r05 ids fora do corpus entram na lista", "construir_horust_real.py", "        if r[\"id\"] in ids:\n            saida[regra_do_registro(r)]", "        if True:\n            saida[regra_do_registro(r)]"),
]


# Mutantes EQUIVALENTES (sobrevivem por construção, não por teste fraco). Não entram na contagem de mortos; ficam registrados no JSON.
EQUIVALENTES = [
    ("e1 antecadeias só vê uma direção", "selecao_horust.py", "if not any(m in alcance[n] or n in alcance[m] for m in g):", "if not any(m in alcance[n] for m in g):",
     "a ordem (menor alcance primeiro) garante que m, já no grupo, nunca depende de n: a direção `n in alcance[m]` é redundante"),
]


def roda(dir_copia: Path, testes: list[str]) -> tuple[int, str]:
    env = {**os.environ, "HORUST_SRC_DIR": str(dir_copia), "HORUST_SCRIPTS_DIR": str(HERE.parents[1] / "scripts")}
    r = subprocess.run(
        [sys.executable, "-I", "-B", "-m", "pytest", *testes, "-q", "-x", "-p", "no:cacheprovider"],
        cwd=dir_copia, env=env, capture_output=True, text=True, timeout=600, check=False,
    )
    return r.returncode, (r.stdout + r.stderr)[-300:]


def main() -> int:
    fontes = sorted(p for p in HERE.glob("*.py") if p.name not in ("mutantes_horust.py",))
    testes = sorted(p.name for p in fontes if p.name.startswith("test_"))
    resultado = []
    with tempfile.TemporaryDirectory() as tmp:
        base = Path(tmp) / "limpo"
        base.mkdir()
        for p in fontes:
            shutil.copy(p, base / p.name)
        rc, saida = roda(base, testes)
        print(f"controle (sem mutante): rc={rc}")
        if rc != 0:
            print(saida)
            return 2
        for i, (nome, arq, antes, depois) in enumerate(MUTANTES):
            d = Path(tmp) / f"m{i}"
            shutil.copytree(base, d)
            alvo = d / arq
            txt = alvo.read_text()
            if txt.count(antes) != 1:
                resultado.append({"mutante": nome, "estado": f"TRECHO_NAO_UNICO ({txt.count(antes)})"})
                print(f"?? {nome}: trecho aparece {txt.count(antes)}x")
                continue
            alvo.write_text(txt.replace(antes, depois))
            rc, saida = roda(d, testes)
            estado = "morto" if rc != 0 else "SOBREVIVEU"
            resultado.append({"mutante": nome, "estado": estado})
            print(f"{estado:10} {nome}", flush=True)
    mortos = sum(r["estado"] == "morto" for r in resultado)
    (HERE / "mutantes_horust.json").write_text(json.dumps({"total": len(resultado), "mortos": mortos, "itens": resultado,
                                                          "equivalentes": [{"mutante": n, "motivo": porque} for n, _a, _b, _c, porque in EQUIVALENTES]}, ensure_ascii=False, indent=1))
    print(f"mortos {mortos}/{len(resultado)}")
    return 0 if mortos == len(resultado) else 1


if __name__ == "__main__":
    raise SystemExit(main())
