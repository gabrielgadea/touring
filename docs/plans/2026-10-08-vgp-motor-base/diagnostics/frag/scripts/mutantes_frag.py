#!/usr/bin/env python3
"""Mutantes do FRAG sobre o worktree touring-wt-frag (UMA vaga rust: o chamador envolve no hw-slot).

Regras (memória harness-de-mutacao-e-asercao-vacua):
  * o baseline de cada comando é IMPRESSO primeiro e o harness sai se ele não for verde;
  * âncora ausente (ou repetida) no arquivo = mutante-fantasma: aborta, não "sobrevive";
  * restaurar escreve o texto original E faz `os.utime(path, None)` — mtime = AGORA, nunca o original (o cargo
    reaproveitaria o build do mutante);
  * "morto" exige falha de TESTE (`test result: FAILED`), nunca erro de compilação — esse vira COMPILA_MAL e conta
    como não-provado;
  * depois do último mutante, TODOS os comandos rodam de novo (a 1a execução pós-restauro é a que o cargo contamina).

Uso: mutantes_frag.py <dir-de-saida> [--so M01,M02] [--lista]
"""

import json
import os
import re
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path("/home/gabrielgadea/projects/touring-wt-frag")
CRATES = ROOT / "crates"
TARGET = "/home/gabrielgadea/projects/touring/target/wt-frag"

T = ["cargo", "test", "--locked", "--no-fail-fast"]
CMDS = {
    "pyright": [*T, "-p", "touring-pyright", "--lib"],
    "code": [*T, "-p", "touring-code", "--lib", "vgp::"],
    "server": [*T, "-p", "touring-server", "--lib", "cli::vgp"],
    "handlers": [*T, "-p", "touring-hook-handlers", "--features", "pre-hooks,post-hooks", "--lib", "vgp"],
}

H = "touring-hook-handlers/src/hooks"
# (id, descrição, arquivo relativo a crates/, âncora, substituto, comandos em ordem barato -> caro)
M = [
    ("M01", "o ramo de execução não checa (a ordem do pedido: «exec não checa»)", f"{H}/pre_bash.rs",
     "crate::vgp_execucao::vgp_de_execucao(runtime, input, command),", "None::<HookResponse>,", ["handlers"]),
    ("M02", "/tmp e .cache ainda rodam agora na ESCRITA (o comportamento de antes; «/tmp ainda roda agora»)",
     "touring-code/src/vgp/politica.rs",
     "    if contexto == Contexto::Escrita {\n        return false;\n    }\n", "", ["code", "handlers", "server"]),
    ("M03", "só o /tmp ainda roda agora na escrita (o .cache já não)", "touring-code/src/vgp/politica.rs",
     "    if contexto == Contexto::Escrita {", '    if contexto == Contexto::Escrita && !path.starts_with("/tmp/") {', ["code"]),
    ("M04", "o predicado aceita `python3 -m` (o -m deixa de encerrar a busca)", f"{H}/vgp_execucao.rs",
     "'c' | 'm' | 'V' | 'h' | '?' => return Opcao::SemArquivo,", "'c' | 'V' | 'h' | '?' => return Opcao::SemArquivo,", ["handlers"]),
    ("M05", "check_em ignora o contexto (sempre Escrita)", "touring-code/src/vgp/mod.rs",
     "            &lang::transversais_de(&input.lang),\n            contexto,\n",
     "            &lang::transversais_de(&input.lang),\n            Contexto::Escrita,\n", ["code", "server", "handlers"]),
    ("M06", "check (a escrita) vira execução", "touring-code/src/vgp/mod.rs",
     "    check_em(input, Contexto::Escrita)\n", "    check_em(input, Contexto::Execucao)\n", ["code", "server", "handlers"]),
    ("M07", "sombra_check (a escrita) vira execução", "touring-code/src/vgp/sombra.rs",
     "        extra,\n        Contexto::Escrita,\n    )", "        extra,\n        Contexto::Execucao,\n    )", ["code", "handlers"]),
    ("M08", "camada 2: a condição do rebaixamento invertida", "touring-pyright/src/achado.rs",
     "if Balde::from_path(path, raiz) != Balde::Tmp {", "if Balde::from_path(path, raiz) == Balde::Tmp {", ["pyright", "handlers"]),
    ("M09", "camada 2: o rebaixamento leva o alvo EXTERNO junto", "touring-pyright/src/achado.rs",
     "achado.severidade = severidade(Balde::Arquivo, achado.escopo);", "achado.severidade = Severidade::Warn;", ["pyright"]),
    ("M10", "o corpo do heredoc vira comando da linha", f"{H}/vgp_execucao.rs",
     "shell_pipelines(&sem_corpos_de_heredoc(command))", "shell_pipelines(command)", ["handlers"]),
    ("M11", "o marcador `<<-` deixa de aceitar o recuo", f"{H}/vgp_execucao.rs",
     ".trim_start_matches('-')", "", ["handlers"]),
    ("M12", "o marcador entre aspas deixa de ser reconhecido", f"{H}/vgp_execucao.rs",
     ".trim_matches(['\\'', '\"'])", "", ["handlers"]),
    ("M13", "combina: o aviso vence o deny", f"{H}/vgp_execucao.rs",
     "        (Some(deny @ HookResponse::Deny { .. }), _)\n        | (_, Some(deny @ HookResponse::Deny { .. })) => Some(deny),",
     "        (Some(deny @ HookResponse::Deny { .. }), _) => Some(deny),", ["handlers"]),
    ("M14", "o `cd` da linha é ignorado", f"{H}/vgp_execucao.rs",
     "cwd = cwd.join(dir);", "let _ = dir;", ["handlers"]),
    ("M15", "o filtro de scratch some (script do repo passa a ser julgado)", f"{H}/vgp_execucao.rs",
     "if !roda_agora(&path, &runtime.project_root, Contexto::Execucao) {", "if false {", ["handlers"]),
    ("M16", "o log da execução sai com o nome do hook de escrita", f"{H}/vgp_execucao.rs",
     'const HOOK: &str = "pre-bash-exec";', 'const HOOK: &str = "pre-bash";', ["handlers"]),
    ("M17", "o CLI ignora --contexto", "touring-server/src/cli/vgp.rs",
     "let mut out = check_em(&input, contexto);", "let mut out = check_em(&input, Contexto::Escrita);", ["server"]),
    ("M18", "o CLI: camada 2 com o momento invertido", "touring-server/src/cli/vgp.rs",
     "    if contexto == Contexto::Escrita {", "    if contexto == Contexto::Execucao {", ["server"]),
    ("M19", "o hook: camada 2 com o momento invertido", f"{H}/vgp_l2.rs",
     "    if contexto == Contexto::Escrita {", "    if contexto == Contexto::Execucao {", ["handlers"]),
    ("M20", "o deny da execução não diz por quê", f"{H}/vgp_execucao.rs",
     "if execucao {", "if false {", ["handlers"]),
    ("M21", "-W/-X deixam de consumir o valor da palavra seguinte", f"{H}/vgp_execucao.rs",
     "                    Opcao::ValorNaPalavraSeguinte\n                } else {", "                    Opcao::Segue\n                } else {", ["handlers"]),
    ("M22", "node -r deixa de consumir o valor", f"{H}/vgp_execucao.rs",
     '        "-r",\n', "", ["handlers"]),
    ("M23", "uv run --with deixa de consumir o valor", f"{H}/vgp_execucao.rs",
     '    "--with",\n', "", ["handlers"]),
    ("M24", "o teto de scripts por linha sobe para 5", f"{H}/vgp_execucao.rs",
     "const MAX_SCRIPTS: usize = 4;", "const MAX_SCRIPTS: usize = 5;", ["handlers"]),
    ("M25", "o teto é ultrapassável (== vira >)", f"{H}/vgp_execucao.rs",
     "if achados.len() == MAX_SCRIPTS {", "if achados.len() > MAX_SCRIPTS {", ["handlers"]),
    ("M26", "o mesmo script na linha é julgado duas vezes", f"{H}/vgp_execucao.rs",
     "&& !achados.contains(&candidato)", "&& true", ["handlers"]),
    ("M27", "arquivo inexistente também conta", f"{H}/vgp_execucao.rs",
     "candidato.caminho.is_file()", "true", ["handlers"]),
    ("M28", "o teto de leitura de 1 MiB some (2x)", f"{H}/vgp_execucao.rs",
     "meta.len() > TETO_BYTES", "meta.len() > TETO_BYTES * 2", ["handlers"]),
    ("M29", "em sombra a resposta passa a seguir o motor", f"{H}/vgp_execucao.rs",
     "        if mode != VgpMode::Ativo {\n            continue;\n        }", "        if false {\n            continue;\n        }", ["handlers"]),
    ("M30", "modo off ainda mede e grava", f"{H}/vgp_execucao.rs",
     "    if mode == VgpMode::Off {\n        return None;\n    }\n    let cwd", "    let cwd", ["handlers"]),
    ("M31", "Contexto::parse troca execucao por escrita", "touring-code/src/vgp/politica.rs",
     '"execucao" | "execução" => Some(Self::Execucao),', '"execucao" | "execução" => Some(Self::Escrita),', ["code", "server"]),
]


def rodar(chave: str) -> dict:
    t0 = time.time()
    r = subprocess.run(CMDS[chave], cwd=ROOT, env={**os.environ, "CARGO_TARGET_DIR": TARGET},
                       capture_output=True, text=True, check=False)
    txt = r.stdout + "\n" + r.stderr
    falhas = sorted(set(re.findall(r"^test (\S+) \.\.\. FAILED$", txt, re.M)))
    return {"cmd": chave, "rc": r.returncode, "s": round(time.time() - t0, 1),
            "compila_mal": ("could not compile" in txt) or bool(re.search(r"^error(\[E\d+\])?:", r.stderr, re.M)) and not falhas,
            "falhas": falhas, "resumo": [l for l in txt.splitlines() if l.startswith("test result:")][-3:]}


def baseline(usados: list[str], out: Path) -> dict | None:
    """Roda e IMPRIME o baseline de cada comando; `None` se algum não for verde."""
    base = {}
    for c in usados:
        base[c] = rodar(c)
        print(f"[baseline] {c}: rc={base[c]['rc']} {base[c]['s']}s {base[c]['resumo']}", flush=True)
        if base[c]["rc"] != 0:
            (out / "baseline_vermelho.json").write_text(json.dumps(base[c], indent=1))
            return None
    return base


def mutar(ident: str, desc: str, rel: str, ancora: str, novo: str, cmds: list[str]) -> dict:
    """Aplica UM mutante, roda os comandos (barato -> caro) até um matar, e restaura com mtime = AGORA."""
    p = CRATES / rel
    original = p.read_text(encoding="utf-8")
    if original.count(ancora) != 1:
        return {"id": ident, "desc": desc, "status": "ANCORA_INVALIDA", "ocorrencias": original.count(ancora)}
    reg = {"id": ident, "desc": desc, "arquivo": rel, "rodadas": [], "status": "SOBREVIVEU"}
    try:
        p.write_text(original.replace(ancora, novo), encoding="utf-8")
        for c in cmds:
            r = rodar(c)
            reg["rodadas"].append(r)
            if r["rc"] != 0:
                reg["status"] = "MORTO" if r["falhas"] else "COMPILA_MAL"
                if r["falhas"]:
                    reg["morto_por"] = {"cmd": c, "testes": r["falhas"][:6]}
                break
    finally:
        p.write_text(original, encoding="utf-8")
        os.utime(p, None)  # mtime = AGORA
        assert p.read_text(encoding="utf-8") == original, f"restauro divergiu em {rel}"
    return reg


def veredito(res: list[dict], fim: dict) -> int:
    contagem: dict[str, int] = {}
    for r in res:
        contagem[r["status"]] = contagem.get(r["status"], 0) + 1
    print("RESUMO", contagem, flush=True)
    limpo = all(v["rc"] == 0 for v in fim.values())
    so_mortos = set(contagem) <= {"MORTO"}
    return 0 if limpo and so_mortos else 1


def main(argv: list[str]) -> int:
    if "--lista" in argv:
        for m in M:
            print(m[0], m[1], "->", m[5])
        return 0
    out = Path(argv[0])
    out.mkdir(parents=True, exist_ok=True)
    so = set(argv[argv.index("--so") + 1].split(",")) if "--so" in argv else None
    sel = [m for m in M if so is None or m[0] in so]
    usados = [c for c in CMDS if any(c in m[5] for m in sel)]
    print(f"[baseline] comandos: {usados}", flush=True)
    base = baseline(usados, out)
    if base is None:
        print("BASELINE VERMELHO: nada a mutar", flush=True)
        return 3
    res = []
    for m in sel:
        reg = mutar(*m)
        print(f"[{reg['id']}] {reg['status']} {reg.get('morto_por', '')} — {reg['desc']}", flush=True)
        res.append(reg)
        (out / "mutantes.json").write_text(json.dumps(res, ensure_ascii=False, indent=1))
    # a 1a execução depois do último restauro é a que o cargo contamina: roda TUDO de novo, com mtime já novo
    fim = {}
    for c in usados:
        fim[c] = rodar(c)
        print(f"[pos-restauro] {c}: rc={fim[c]['rc']} {fim[c]['s']}s {fim[c]['resumo']}", flush=True)
    (out / "mutantes.json").write_text(
        json.dumps({"baseline": base, "mutantes": res, "pos_restauro": fim}, ensure_ascii=False, indent=1)
    )
    return veredito(res, fim)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
