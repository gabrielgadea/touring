"""Mutantes do estado Rust no instante (HORUST): cada um e aplicado a uma COPIA de `estado_rust_no_instante.py` (o original nunca e
escrito; sha conferido antes e depois) e `test_estado_rust_no_instante.py` (via $ESTADO_RUST_MODULO) TEM de reprovar. O controle
(copia sem mutacao) tem de passar. Mutante que nao termina (m11) reprova por TIMEOUT.

  m1  «sem overlay de crate»: `overlays_do_crate` devolve []                         (o defeito: overlays vazios para Rust)
  m2  so a raiz lib.rs (sem main/bin/tests/examples/benches/build.rs)
  m3  os filhos de `a.rs` moram ao lado (ignora o stem do arquivo)
  m4  ignora `#[path]`
  m5  ignora o aninhamento de `mod a { mod b; }`
  m6  historia quebrada vira «modulo sem arquivo»
  m7  modulo declarado sem arquivo nao e falta
  m8  entrada fora da arvore nao e falta
  m9  crate sem raiz nao e falta
  m10 o overlay grava o conteudo de HOJE (o arquivo em disco)
  m11 sem guarda de ciclo
  m12 a propria entrada entra nos overlays
  m13 o arquivo carregado por #[path] nao age como mod-rs
  m14 escopo de modulo da classe A ignora o `mod` inline
  m15 let POSTERIOR ao uso conta (classe A)
  m16 comentarios/strings definem o nome (sem `limpar`)
  m17 let de bloco ja fechado conta
  m18 `impl`/`trait` contam como escopo lexico
  m19 a raiz do arquivo e visivel de dentro de um `mod` inline (pai sem glob)
  m20 o provedor ignora o que a sessao registrou (dobra quebrada vira sem_origem)
  m21 o provedor lista so o que o git tem (ignora o que a sessao criou)
  m22 o provedor lista o HEAD de hoje (nao o commit anterior ao instante)
  m23 o predicado de Bash e ignorado
  m24 o `[lib] path` do manifesto e ignorado
  m25 `extras_raiz` ignorado
  m26 prioridade invertida entre os motivos
  m27 `mod x { }` inline tratado como externo
  m28 char literal nao e limpo (a chave de '{' abre bloco)
  m29 o uso qualificado por caminho (`crate::m::nome`) conta como uso no escopo (classe A)

Uso: python3 -I mutantes_estado_rust.py <estado_rust_no_instante.py> <test_estado_rust_no_instante.py> <saida.json>
Exit 0 so se todos morreram E o controle passou E o original nao mudou.
"""

import hashlib
import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path

ESTADO, TESTE, SAIDA = Path(sys.argv[1]).resolve(), Path(sys.argv[2]).resolve(), Path(sys.argv[3])

MUTANTES = (
    ("m1", "sem overlay de crate", "    return [(a.path, a.conteudo) for a in estado.arquivos if str(a.path) != str(proprio)]\n", "    return []\n"),
    ("m2", "so lib.rs", 'cand = [crate_dir / "src/lib.rs", crate_dir / "src/main.rs", crate_dir / "build.rs", *map(Path, extras)]', 'cand = [crate_dir / "src/lib.rs"]'),
    ("m3", "filhos ao lado (ignora o stem)", "base = arquivo.parent if rootlike else arquivo.parent / arquivo.stem", "base = arquivo.parent"),
    ("m4", "ignora #[path]", "    if d.path_attr:\n        alvo =", "    if False:\n        alvo ="),
    ("m5", "ignora o aninhamento inline", "    for n in d.aninhamento:\n        base = base / n\n", "    for n in ():\n        base = base / n\n"),
    ("m6", "quebrada vira mod_sem_arquivo", '"dobra_quebrada" if pior[1] else "mod_sem_arquivo"', '"mod_sem_arquivo"'),
    (
        "m7",
        "mod sem arquivo nao e falta",
        '                faltas.append(Falta(str(pior[1] or cands[0][0]), "dobra_quebrada" if pior[1] else "mod_sem_arquivo", f"declarado em {arq}"))\n',
        "                pass\n",
    ),
    ("m8", "entrada fora da arvore nao e falta", '        faltas.append(Falta(str(proprio), "entrada_fora_da_arvore"))\n', "        pass\n"),
    ("m9", "sem raiz nao e falta", '        faltas.append(Falta(str(crate_dir), "sem_raiz"))\n', "        pass\n"),
    ("m10", "overlay grava o disco de hoje", '        arq.write_text(conteudo, encoding="utf-8")\n', '        arq.write_text(Path(p).read_text(), encoding="utf-8")\n'),
    ("m11", "sem guarda de ciclo", "                if caminho in agendado:\n", "                if False:\n"),
    ("m12", "a propria entrada entra nos overlays", "if str(a.path) != str(proprio)]", "if True]"),
    ("m13", "carregado por #[path] nao age como mod-rs", "return [(Path(os.path.normpath(alvo)), True)]", "return [(Path(os.path.normpath(alvo)), False)]"),
    ("m14", "escopo de modulo ignora o mod inline", '    modulo = next((k for k in reversed(cadeia) if _tipo(blocos[k].cabeca) == "mod"), -1)\n', "    modulo = -1\n"),
    ("m15", "let posterior conta", " and _fim_do_let(limpo, m.end()) < off:", ":"),
    ("m16", "comentarios definem o nome (sem limpar)", "    limpo = limpar(texto)\n    blocos = _blocos(limpo)\n", "    limpo = texto\n    blocos = _blocos(limpo)\n"),
    ("m17", "let de bloco fechado conta", "_dono(blocos, m.start()) in (set(corrente) | {modulo})", "True"),
    (
        "m18",
        "impl/trait contam como escopo lexico",
        '    if re.match(r"(?:unsafe\\s+)?(?:impl|trait)\\b", c):\n        return "impl"\n',
        '    if re.match(r"(?:unsafe\\s+)?(?:impl|trait)\\b", c):\n        return "outro"\n',
    ),
    ("m19", "raiz visivel dentro de mod inline", "visiveis = {modulo} | {k for k in corrente", "visiveis = {modulo, -1} | {k for k in corrente"),
    ("m20", "provedor ignora o registrado da sessao", '        registrado = any(r["timestamp"] <= self.ts for r in self.rec.idx.get(str(p), []))\n', "        registrado = False\n"),
    ("m21", "provedor ignora o que a sessao criou", "        for k, regs in self.rec.idx.items():\n", "        for k, regs in {}.items():\n"),
    ("m22", "provedor lista o HEAD de hoje", 'f"--before={self.ts}", "HEAD"', '"--before=2099-01-01", "HEAD"'),
    ("m23", "predicado de Bash ignorado", "        if self.suspeito is not None and self.suspeito(str(p), lt):\n", "        if False:\n"),
    ("m24", "[lib] path ignorado", '    if isinstance(m.get("lib"), dict) and m["lib"].get("path"):\n', "    if False:\n"),
    ("m25", "extras_raiz ignorado", ', *map(Path, extras)]\n', "]\n"),
    ("m26", "prioridade invertida", "    for chave in PRIORIDADE:\n", "    for chave in reversed(PRIORIDADE):\n"),
    ("m27", "mod inline vira externo", 'saida.append(\n                DeclMod(m.group("nome"), m.group("fim") == ";",', 'saida.append(\n                DeclMod(m.group("nome"), True,'),
    ("m29", "uso qualificado conta como classe A", 'if off is None or texto[:off].rstrip().endswith("::"):', "if off is None:"),
    ("m28", "char literal nao e limpo", "        elif c == \"'\":\n            if texto.startswith(\"'\\\\\", i):", "        elif False:\n            if texto.startswith(\"'\\\\\", i):"),
)


def sha(p: Path) -> str:
    return hashlib.sha256(p.read_bytes()).hexdigest()


def rodar(texto: str) -> dict:
    with tempfile.TemporaryDirectory() as tmp:
        copia = Path(tmp) / "estado_rust_mut.py"
        copia.write_text(texto, encoding="utf-8")
        env = {**os.environ, "ESTADO_RUST_MODULO": str(copia), "PYTHONDONTWRITEBYTECODE": "1"}
        try:
            r = subprocess.run([sys.executable, "-m", "pytest", "-q", "-p", "no:cacheprovider", str(TESTE)], capture_output=True, text=True, env=env, cwd="/tmp", timeout=90)
        except subprocess.TimeoutExpired:  # o mutante nao termina: o teste NAO passa
            return {"rc": 124, "falhas": ["TIMEOUT"], "resumo": "timeout de 90 s: o mutante nao termina"}
    falhas = [ln.split("::")[-1] for ln in r.stdout.splitlines() if ln.startswith("FAILED")]
    return {"rc": r.returncode, "falhas": falhas, "resumo": (r.stdout.strip().splitlines() or [""])[-1]}


def main() -> int:
    antes, texto = sha(ESTADO), ESTADO.read_text(encoding="utf-8")
    controle = rodar(texto)
    resultados = []
    for nome, descricao, velho, novo in MUTANTES:
        if texto.count(velho) != 1:
            resultados.append({"mutante": nome, "descricao": descricao, "erro": f"trecho aparece {texto.count(velho)}x (esperado 1x): nao aplicou"})
            continue
        r = rodar(texto.replace(velho, novo, 1))
        resultados.append({"mutante": nome, "descricao": descricao, **r, "morto": r["rc"] != 0 and bool(r["falhas"])})
    mortos = sum(1 for r in resultados if r.get("morto"))
    saida = {"estado": str(ESTADO), "sha_antes": antes, "sha_depois": sha(ESTADO), "controle_sem_mutacao": controle, "mutantes_mortos": f"{mortos}/{len(resultados)}", "mutantes": resultados}
    SAIDA.write_text(json.dumps(saida, ensure_ascii=False, indent=1), encoding="utf-8")
    for r in resultados:
        print(f"  {'MORTO ' if r.get('morto') else 'VIVO  '} {r['mutante']} {r['descricao']}: {r.get('erro') or r['falhas'][:3]}")
    ok_ctl = controle["rc"] == 0
    print(f"controle sem mutacao: {'passou' if ok_ctl else 'REPROVOU'} ({controle['resumo']}) | sha do original igual antes/depois: {saida['sha_antes'] == saida['sha_depois']}")
    print(f"{mortos}/{len(resultados)} mutantes mortos")
    return 0 if mortos == len(resultados) and ok_ctl and saida["sha_antes"] == saida["sha_depois"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
