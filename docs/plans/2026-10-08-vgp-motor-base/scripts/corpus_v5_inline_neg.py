"""V5 — os negativos INLINE do corpus: execuções inline que DERAM CERTO.

Por que: os 300 negativos do corpus V0d sao todos de arquivo (`atribuido`). Sem execucoes inline
bem-sucedidas, o falso deny do motor sobre codigo inline NAO E MEDIVEL: um motor que nega todo
inline "passa" no corpus sem nunca ter visto um inline legitimo.

O programa e o estrato vem do `shell_inline` (o lexer de shell, fonte unica — 09/10/2026), nao de regex
sobre o texto do comando. O que isso corrige, medido:

1. **O texto do `-c "…"` nao era o que o Python recebeu.** O `desscape_duplas` antigo transformava `\\n`,
   `\\t`… em caracteres reais — o bash NAO faz isso dentro de aspas duplas (so `\\$`, `` \\` ``, `\\"`, `\\\\`
   e barra+quebra). O programa "corrigido" compilava e nao era o que rodou. Agora o texto e o do bash.
2. **O consumidor do heredoc era adivinhado por regex.** `cat > x <<EOF` grava um arquivo; nao roda python.
   O lexer sabe QUEM consome cada heredoc, `bash -c "python3 -c …"` e `touring run --lang bash` tambem.
3. **`touring run` sem `--lang` entrava como Python.** A CLI exige `--lang` (`Usage: run [OPTIONS] --lang
   <LANG>`): o comando nunca rodou.
4. **Dois programas no mesmo comando.** Um negativo e UM programa que deu certo: com 0 ou mais de 1
   candidato o comando fica de fora (o motivo e contado), nunca entra por escolha arbitraria.

O estrato `tmp` (`cat > /tmp/x.py <<EOF` + `python3 /tmp/x.py` no MESMO comando) mede um arquivo, nao um
inline: o predicado continua por regex, e o texto vem do corpo do heredoc que ESSE `cat` consome.

NAO TOCA em `entradas.jsonl` nem em `snap/`. `--saida <dir>` grava em outro diretorio (staging);
`--reextrair` refaz SO os ids que ja estao em `entradas_inline_neg.jsonl` (sem sorteio novo), para medir o
efeito do extrator sobre o MESMO conjunto.

Uso: python3 corpus_v5_inline_neg.py [--selftest] [--n 300] [--saida DIR [--reextrair]]
"""

import argparse
import json
import random
import re
import sys
from collections import Counter
from datetime import datetime, timezone
from pathlib import Path

AQUI = Path(__file__).resolve().parent
sys.path.insert(0, str(AQUI))
import replay  # noqa: E402  # pyright: ignore[reportMissingImports]
import shell_inline  # noqa: E402  # pyright: ignore[reportMissingImports]
from corpus_v0d import ident, sha  # noqa: E402  # pyright: ignore

VIVO = replay.VGP / "corpus_v0d"  # o corpus vivo: o default (o juiz mede contra ele)
C = VIVO
SAIDA = C / "entradas_inline_neg.jsonl"
SNAP_DIR = "snap_inline_neg"
SEED, N_NEG, N_TMP = 20261009, 300, 100
ANALISE = Path.home() / "projects/analise"
PROJ_TRANSCRIPTS = Path.home() / ".claude/projects/-home-gabrielgadea-projects-analise"

ENVELOPE_OK = re.compile(r'"success"\s*:\s*true')
EXIT_0 = re.compile(r'"exit_code"\s*:\s*0')
TRACEBACK = re.compile(r"Traceback \(most recent call last\)")
SHELL_MISTURADA = re.compile(
    r"^\s*(?:echo|grep|ls|cat|wc|sed|awk|printf|find|rg|jq|pip|npm|curl|git|cd|export)\b",
    re.M,
)

# A VIA que o shell_inline nomeia -> o estrato do corpus. `herestring` (python3 <<< '…') alimenta o stdin
# como o heredoc. `touring_run_file` nao esta aqui: o programa e o arquivo, nao o comando.
ESTRATO_DA_VIA = {
    "-c": "c",
    "touring_run_code": "touring_run",
    "heredoc": "heredoc",
    "herestring": "heredoc",
}

# O bonus tmp: cat > /tmp/x.py <<EOF seguido, no MESMO comando, de python3 /tmp/x.py.
# O `cat` pode vir depois de `&&` (`mkdir -p … && cat > /tmp/x.py <<EOF`) — medido num comando real —
# e o `python3` pode vir na linha seguinte, entao o separador tambem inclui a quebra de linha.
TMP_ESCREVE = re.compile(
    r"(?:^|&&|\|\||;|\n)\s*(?:cat|tee)\s+>??\s*(?P<alvo>/tmp/\S+?\.py)\s*<<-?\s*['\"]?[A-Za-z_]\w*",
    re.M,
)
TMP_EXECUTA = re.compile(
    r"(?:^|&&|\|\||;|\n)\s*(?:[\w./-]*\.venv/bin/)?python3?\s+(?P<alvo>/tmp/\S+?\.py)\b",
    re.M,
)


def _corpo_do_cat(comando: str, alvo: str) -> str | None:
    """O texto que `cat/tee > ALVO <<EOF` gravou: o corpo do heredoc que ESSE comando consome, lido pelo
    lexer (aspas e escapes do bash) — nao o primeiro `<<` do texto. None: nao ha tal comando, ou o shell
    nao se deixa ler, ou o heredoc sem aspas foi expandido."""
    try:
        simples = shell_inline.comandos_simples(comando)
    except shell_inline.ShellIlegivel:
        return None
    for s in simples:
        argv = shell_inline._efetivas(s.palavras)  # noqa: SLF001 — o mesmo diretorio, a mesma regra de prefixo
        r = shell_inline._redir_stdin(s)  # noqa: SLF001
        if not argv or r is None or r.op not in ("<<", "<<-"):
            continue
        if shell_inline._base(argv[0].valor) not in ("cat", "tee"):  # noqa: SLF001
            continue
        grava = [x.alvo.valor for x in s.redirs if x.op in (">", ">>") and x.alvo]
        grava += [w.valor for w in argv[1:] if not w.valor.startswith("-")]
        if alvo in grava:
            return shell_inline._corpo_heredoc(r)[0]  # noqa: SLF001
    return None


def resolve(comando: str) -> tuple[str | None, str | None, str | None]:
    """(estrato, caminho quando tmp | motivo da exclusao, programa). Estrato None = fora.

    O estrato e decidido pelo programa que DE FATO rodou, nao por um `<<` em qualquer posicao: o
    `shell_inline.candidatos` lista cada programa Python do comando (heredoc, `-c`, `touring run --lang
    python --code`, `bash -c`, `touring run --lang bash`, `$(…)`). Exatamente UM com texto -> o estrato da
    via dele; zero ou mais de um -> fora, com o motivo nomeado. Casos medidos que fecham a regra:
      · `python3 -c "… <<chave ausente>> …"`: `<<` no CORPO, e um `c` (e7c55c79a1bc);
      · `cat >> ARQUIVO <<EOF` depois de `&&`: o consumidor e o `cat` (bbb874e32783);
      · DOIS `cat > x.json <<EOF` e nenhum python: nenhum candidato (a6b86f55b722);
      · `cat > x.json <<EOF` + `python3 -c` DEPOIS: o programa e o `-c` — entra como `c`;
      · `touring run --lang python --code` | `python3 -c`: dois programas, ambiguo.

    O bonus tmp vem antes de tudo: `cat > /tmp/x.py` + `python3 /tmp/x.py` e o que o estrato tmp mede.
    """
    esc, exe = TMP_ESCREVE.search(comando), TMP_EXECUTA.search(comando)
    if esc and exe and esc.group("alvo") == exe.group("alvo"):
        alvo = esc.group("alvo")
        return "tmp_escrito_e_executado", alvo, _corpo_do_cat(comando, alvo)
    cands = shell_inline.candidatos(comando)
    if len(cands) > 1:
        return None, f"ambiguo: {len(cands)} programas Python no mesmo comando", None
    if not cands:
        return None, shell_inline.analisar(comando, True).motivo, None
    c = cands[0]
    if c.texto is None:
        return None, "touring run --file: o programa e o arquivo, nao o comando", None
    return ESTRATO_DA_VIA[c.via], None, c.texto


def extrator_de(comando: str, programa: str | None = None) -> tuple:
    """(estrato, caminho quando tmp | motivo). `programa` so existe por compatibilidade de assinatura."""
    estrato, alvo, _ = resolve(comando)
    return estrato, alvo


def programa_corrigido(comando: str) -> str | None:
    """O programa Python que o comando rodou — o texto que o bash entregou ao interpretador —, ou None."""
    return resolve(comando)[2]


def compila(texto: str) -> bool:
    """O snapshot e python que compila? Um programa que RODOU compila — e o teste de aceite (a)."""
    try:
        compile(texto, "<snap>", "exec")
    except (SyntaxError, ValueError):
        return False
    return True


def texto_result(c: dict) -> str:
    b = c.get("content")
    if isinstance(b, str):
        return b
    if isinstance(b, list):
        return " ".join(str(x.get("text", "")) for x in b if isinstance(x, dict))
    return ""


def envelope_run(texto: str) -> dict | None:
    i = texto.find("{")
    if i < 0 or '"duration_ms"' not in texto:
        return None
    try:
        env, _ = json.JSONDecoder().raw_decode(texto[i:])
    except json.JSONDecodeError:
        return None
    return env if isinstance(env, dict) else None


def eh_negativo(texto: str, comando: str) -> tuple[bool, str]:
    """(e negativo, razao). Ramos PREDICADOS POSITIVOS; nada entra por eliminacao."""
    if TRACEBACK.search(texto):
        return False, "traceback"
    if "touring run" in comando:
        env = envelope_run(texto)
        if env is None:
            return False, "touring run sem envelope legivel"
        if not ENVELOPE_OK.search(texto):
            return False, 'envelope sem "success": true'
        if not EXIT_0.search(texto):
            return False, "envelope sem exit_code 0"
        return True, "envelope: success=true e exit_code=0"
    return True, "tool_result sem is_error e sem traceback"


def universo(so_ids: set[str] | None = None) -> list[dict]:
    """Todo Bash cujo programa python e extraivel, com o tool_result do MESMO tool_use.

    `rglob`: um subagente mora em `<transcript>/subagents/` (medido na V0e-fora-de-gate — `glob`
    produz 0/44 "faltantes" que sao subagentes legitimos). `so_ids` restringe aos tool_use_id dados
    (a re-extracao do MESMO conjunto, sem a deriva de um universo que cresce).
    """
    out = []
    desde = datetime(2026, 1, 1, tzinfo=timezone.utc).timestamp()
    for f in PROJ_TRANSCRIPTS.rglob("*.jsonl"):
        if f.stat().st_mtime < desde:
            continue
        cmds: dict[str, str] = {}
        for ts, c in _blocos(f):
            if c.get("type") == "tool_use" and c.get("name") == "Bash":
                cmds[str(c.get("id"))] = str((c.get("input") or {}).get("command", ""))
                continue
            r = _resultado_do_comando(c, cmds, so_ids)
            if r is not None:
                out.append({"session": f.stem, "ts": ts, **r})
    return out


def _blocos(f: Path):
    """(timestamp, bloco) de cada tool_use/tool_result do transcript, na ordem em que foram gravados."""
    for raw in f.open(encoding="utf-8", errors="replace"):
        if '"tool_use"' not in raw and '"tool_result"' not in raw:
            continue
        try:
            rec = json.loads(raw)
        except json.JSONDecodeError:
            continue
        for c in (rec.get("message") or {}).get("content") or []:
            if isinstance(c, dict):
                yield str(rec.get("timestamp", "")), c


def _resultado_do_comando(
    c: dict, cmds: dict[str, str], so_ids: set[str] | None
) -> dict | None:
    """O registro de um tool_result cujo comando tem programa Python extraivel, ou None."""
    uid = str(c.get("tool_use_id"))
    if c.get("type") != "tool_result" or uid not in cmds:
        return None
    if so_ids is not None and uid not in so_ids:
        return None
    cmd = cmds[uid]
    if not programa_corrigido(cmd):
        return None
    return {
        "tool_use_id": uid,
        "command": cmd,
        "resultado": texto_result(c),
        "is_error": bool(c.get("is_error")),
    }


def monta(u: list[dict]):
    """(os negativos de arquivo, os de tmp, os excluidos e por que)."""
    dentro, tmp, fora = [], [], Counter()
    for r in u:
        if r["is_error"]:
            fora["is_error"] += 1
            continue
        ok, razao = eh_negativo(r["resultado"], r["command"])
        if not ok:
            fora[razao] += 1
            continue
        ext, alvo, prog = resolve(r["command"])
        if ext is None:
            fora[str(alvo)] += 1
            continue
        if not prog or not prog.strip():
            fora["programa vazio"] += 1
            continue
        if not compila(prog):
            fora[f"{ext}: programa nao compila"] += 1
            continue
        if ext != "tmp_escrito_e_executado" and SHELL_MISTURADA.search(prog):
            fora["shell misturada no programa"] += 1
            continue
        (tmp if ext == "tmp_escrito_e_executado" else dentro).append(
            {
                **r,
                "extrator": ext,
                "path": alvo
                if ext == "tmp_escrito_e_executado"
                else ("<string>" if ext == "c" else "<stdin>"),
                "programa": prog,
            }
        )
    return dentro, tmp, fora


def classe_positivo(e: dict) -> tuple[str, str]:
    """(classe, motivo) de uma entrada inline POSITIVA do corpus em `C`, frente ao extrator de hoje:
    `igual` (o snap gravado e o programa extraido), `mudou` (extrai outro texto) ou `removida` (nao extrai
    mais — `motivo` diz por que). `ts` e o `first` do V0a: o instante em que o arquivo da sessao e dobrado."""
    from corpus_v0d import programa_inline_motivo  # noqa: PLC0415 — o mesmo modulo do topo, no uso

    nome, linha = (
        e.get("nome_esperado"),
        e.get("linha"),
    )  # o traceback que desempata os candidatos
    novo, motivo = programa_inline_motivo(
        e.get("command_original") or e.get("command", ""),
        e.get("session", ""),
        e.get("ts", "") or "",
        (nome, linha) if nome and isinstance(linha, int) else None,
    )
    if novo is None:
        return "removida", motivo
    snap = (C / e["snap"]).read_text(encoding="utf-8")
    return ("igual" if novo.strip() == snap.strip() else "mudou"), ""


def definir_saida(saida: str | Path | None) -> Path:
    """Aponta o builder para OUTRO diretorio (staging). Sem `saida` nada muda: o corpus vivo e o default."""
    global C, SAIDA
    if saida:
        C = Path(saida)
        SAIDA = C / "entradas_inline_neg.jsonl"
    return C


def _checks_positivos_do_corpus() -> dict:
    """(e) controle positivo do extrator sobre as entradas inline POSITIVAS do corpus em `C`.

    Num corpus REGENERADO (staging) toda entrada extrai igual ao snap dela. No corpus VIVO os snaps sao
    anteriores ao extrator novo: o selftest mede e DECLARA quantas mudam ou saem (nunca finge que e zero),
    e exige so o controle de que o casamento funciona (alguma igual) e que toda saida diz o motivo."""
    entrada = C / "entradas.jsonl"
    if not entrada.exists():
        return {"entradas.jsonl do V0d ausente": False}
    linhas = [
        json.loads(ln)
        for ln in entrada.read_text(encoding="utf-8").splitlines()
        if ln.strip()
    ]
    pos = [
        e
        for e in linhas
        if e.get("origem") == "inline" and e.get("rotulo") == "positivo"
    ]
    classes = [classe_positivo(e) for e in pos]
    cont = Counter(c for c, _ in classes)
    rotulo = f"{cont['igual']} iguais · {cont['mudou']} mudam · {cont['removida']} saem de {len(pos)}"
    explicadas = all(m for c, m in classes if c == "removida")
    if C == VIVO:
        return {
            f"(e) corpus vivo (anterior ao extrator): {rotulo}": cont["igual"] > 0
            and explicadas
        }
    return {
        f"(e) corpus regenerado: {rotulo}": len(pos) > 0 and cont["igual"] == len(pos)
    }


# (comando, estrato esperado ou "" = fora) — cada linha fixa um caso medido ou uma regra do lexer
CASOS_ESTRATO = [
    ("python3 - <<'EOF'\nprint(1)\nEOF", "heredoc"),
    # um `-c` cujo PROGRAMA contem `<<chave ausente>>` e um `-c`: o `<<` do corpo nao abre heredoc
    ('python3 -c "d = {\x27chave ausente\x27: 1}\ns = \x27a\x27 << b"', "c"),
    # `cat >> ARQUIVO <<'PYEOF'` depois de `&&`: o consumidor e o cat (bbb874e32783) — nao e inline
    (
        "S=/tmp/x && cat >> scripts/lexhub/tests/test_o3_medicao.py <<'PYEOF'\n"
        "def test_sem_cabecalho():\n    assert _d\nPYEOF",
        "",
    ),
    ("S=/tmp/x && python3 - <<'PYEOF'\nprint(1)\nPYEOF", "heredoc"),
    # TROCADO (09/10): era "" porque o extrator antigo pegava o 1o heredoc (o JSON do cat) como programa.
    # O lexer separa os consumidores: o programa que rodou e o `python3 -c`, e entra como `c`.
    ("cat > x.json <<'EOF'\n{}\nEOF\npython3 -c 'print(1)'", "c"),
    # idem para `touring run --lang python --code` depois do cat (dc62f0cdeb78)
    (
        "cat >> t.py <<'EOF'\nx = 1\nEOF\ntouring run --lang python --code 'print(1)'",
        "touring_run",
    ),
    ("python3 -c 'print(1)'\ncat > x.json <<'EOF'\n{}\nEOF", "c"),
    ("echo cat <<'PYEOF'\nprint(1)\nPYEOF\npython3 -c 'print(2)'", "c"),
    # `head -30 /home/...` sob `--lang bash` COMPILA como python (d7351813b6c0) e nao e Python
    ("touring run --lang bash --code 'head -30 /home/x/test_pit_rds.py'", ""),
    ("touring run --lang python --code 'print(1)'", "touring_run"),
    # TROCADO (09/10): era "touring_run". `--lang` e OBRIGATORIO (`Usage: run [OPTIONS] --lang <LANG>`):
    # sem ele a CLI recusa e nenhum programa rodou — nao e um negativo "que deu certo".
    ("touring run --code 'print(1)'", ""),
    ("cat > /tmp/x.py <<EOF\nprint(1)\nEOF", ""),
    ("tee /tmp/x.py <<EOF\nprint(1)\nEOF", ""),
    ("cat >> /tmp/x.py <<EOF\nprint(1)\nEOF", ""),
    # shells aninhados: o Python de dentro tambem rodou
    ("bash -c \"python3 -c 'print(1)'\"", "c"),
    ("touring run --lang bash --code 'python3 - <<EOF\nprint(1)\nEOF'", "heredoc"),
    # dois programas: ambiguo, fora
    ("touring run --lang python --code 'print(1)' | python3 -c 'print(2)'", ""),
    # `--file`: o programa e o arquivo, nao o comando
    ("touring run --lang python --file /tmp/a.py", ""),
]


def _checks_estrato() -> dict:
    checks = {}
    for cmd, esp in CASOS_ESTRATO:
        got, _ = extrator_de(cmd)
        checks[f"(b) {esp or 'fora'}: {cmd.splitlines()[0][:34]!r}"] = got == (
            esp or None
        )
    return checks


def _checks_programa() -> dict:
    """(a) o programa e o texto que o bash entregou; (d) o bonus tmp."""
    checks = {}
    checks["(a) programa que rodou compila"] = compila("print(1)\nx = {'a': 1}\n")
    checks["(a) mutante: string nao terminada e pego"] = not compila('x = "aberta\n')
    checks["(a) mutante: escape de duplas quebrado e pego"] = not compila(
        'print("a\\")'
    )
    prog = programa_corrigido('python3 -c "print(\\"ok\\")"')
    checks['(a) bash: \\" dentro de duplas vira " e compila'] = prog == 'print("ok")\n'
    # TROCADO (09/10): o desscape antigo virava `\n` em quebra real; o bash NAO faz isso dentro de duplas,
    # entao o Python recebeu `x = 1\nprint(2)` (barra-n literal) e NAO compila — fiel ao que rodou.
    prog_n = programa_corrigido('python3 -c "x = 1\\nprint(2)"')
    checks[
        "(a) bash: \\n dentro de duplas fica \\n literal (o Python recebeu isso)"
    ] = prog_n == "x = 1\\nprint(2)\n" and not compila(prog_n or "")
    got, alvo = extrator_de("cat > /tmp/a.py <<'EOF'\nprint(1)\nEOF\npython3 /tmp/a.py")
    checks["(d) tmp: escrita+execucao do MESMO arquivo entra"] = (
        got == "tmp_escrito_e_executado" and alvo == "/tmp/a.py"
    )
    checks["(d) tmp: o programa e o corpo do heredoc do cat"] = (
        programa_corrigido("cat > /tmp/a.py <<'EOF'\nprint(1)\nEOF\npython3 /tmp/a.py")
        == "print(1)\n"
    )
    checks["(d) tmp: arquivos diferentes NAO entra"] = (
        extrator_de("cat > /tmp/a.py <<'EOF'\nprint(1)\nEOF\npython3 /tmp/b.py")[0]
        is None
    )
    return checks


def _checks_monta() -> dict:
    base = {"ts": "2026-01-01T00:00:00Z", "session": "x", "tool_use_id": "t"}
    _, _, fora_err = monta(
        [
            {
                **base,
                "is_error": True,
                "resultado": "ok",
                "command": "python3 -c 'print(1)'",
            }
        ]
    )
    _, _, fora_ok = monta(
        [
            {
                **base,
                "is_error": False,
                "resultado": "1",
                "command": "python3 -c 'print(1)'",
            }
        ]
    )
    _, _, fora_amb = monta(
        [
            {
                **base,
                "is_error": False,
                "resultado": "1",
                "command": "python3 -c 'print(1)'; python3 -c 'print(2)'",
            }
        ]
    )
    _, _, fora_nc = monta(
        [
            {
                **base,
                "is_error": False,
                "resultado": "1",
                "command": 'python3 -c "x = 1\\nprint(2)"',
            }
        ]
    )
    return {
        "negativo: is_error excluido": fora_err["is_error"] == 1,
        "controle: is_error=False limpo entra": not fora_ok,
        "negativo: dois programas = ambiguo, fora": any(
            "ambiguo" in k for k in fora_amb
        ),
        "negativo: programa que nao compila, fora": any(
            "nao compila" in k for k in fora_nc
        ),
    }


def selftest() -> int:
    checks = {
        **_checks_positivos_do_corpus(),
        **_checks_programa(),
        **_checks_estrato(),
        **_checks_monta(),
    }
    for k, v in checks.items():
        print(f"  {'ok ' if v else 'FALHOU'} {k}")
    return 0 if all(checks.values()) else 1


def _amostra(pool: list[dict], n: int, rng: random.Random) -> list[dict]:
    """Amostra por estrato, ordenada por `ts` (a ordem do transcript): a semente decide o corte."""
    ordem = sorted(pool, key=lambda r: (r["ts"], r["tool_use_id"]))
    if len(ordem) <= n:
        return ordem
    return [ordem[i] for i in sorted(rng.sample(range(len(ordem)), n))]


def _sorteia(dentro: list[dict], tmp: list[dict], n_alvo: int) -> list[dict]:
    """O sorteio por estrato (cotas iguais) + ate N_TMP do estrato tmp."""
    estratos = Counter(r["extrator"] for r in dentro)
    rng = random.Random(SEED)
    cotas = {e: n_alvo // len(estratos) for e in estratos}
    escolhidos = []
    for ext in sorted(estratos):
        escolhidos += _amostra(
            [r for r in dentro if r["extrator"] == ext], cotas[ext], rng
        )
    if len(escolhidos) < n_alvo:
        escolhidos += _amostra(
            [r for r in dentro if r not in escolhidos], n_alvo - len(escolhidos), rng
        )
    return escolhidos[:n_alvo] + _amostra(tmp, min(N_TMP, len(tmp)), rng)


def _grava(escolhidos: list[dict]) -> int:
    """Escreve os snaps e o jsonl em `C`; devolve quantos snaps NAO compilam."""
    snap_dir = C / SNAP_DIR
    snap_dir.mkdir(parents=True, exist_ok=True)
    for f in snap_dir.glob("*.py"):
        f.unlink()
    linhas = []
    for r in sorted(escolhidos, key=lambda r: (r["ts"], r["tool_use_id"])):
        eid = ident("negativo", r["session"], r["path"], r["ts"])[:12]
        (snap_dir / f"{eid}.py").write_text(r["programa"], encoding="utf-8")
        linhas.append(
            {
                "id": eid,
                "rotulo": "negativo",
                "origem": "tmp"
                if r["extrator"] == "tmp_escrito_e_executado"
                else "inline",
                "path": r["path"],
                "project_root": str(ANALISE),
                "snap": f"{SNAP_DIR}/{eid}.py",
                "snap_sha256": sha(r["programa"]),
                "lang": "python",
                "ts": r["ts"],
                "session": r["session"],
                "tool_use_id": r["tool_use_id"],
                "command": r["command"][:2000],
                "cwd": str(ANALISE),
                "extrator": r["extrator"],
            }
        )
    SAIDA.write_text(
        "".join(json.dumps(x, ensure_ascii=False) + "\n" for x in linhas),
        encoding="utf-8",
    )
    ruins = [e["id"] for e in linhas if not compila((C / e["snap"]).read_text())]
    print(f"-> {SAIDA} ({len(linhas)} linhas) · snaps em {snap_dir}")
    print(f"   snaps que NAO compilam: {len(ruins)} {ruins[:5]}")
    return len(ruins)


def _pares_gravados() -> set[tuple[str, str]]:
    """(tool_use_id, session) de cada negativo ja gravado em `C` — o conjunto que `--reextrair` refaz.

    O mesmo `tool_use_id` pode morar em DOIS transcripts (o do pai e o do subagente); o par com a sessao
    mantem exatamente a linha que o corpus tinha, sem trazer a copia (medido: 377 linhas para 374 ids).
    O corpus vivo e so leitura aqui: sem `--saida` a re-extracao recusa."""
    if C == VIVO:
        raise SystemExit(
            "--reextrair grava no corpus: exige --saida (o vivo e so leitura aqui)"
        )
    linhas = [ln for ln in SAIDA.read_text(encoding="utf-8").splitlines() if ln.strip()]
    return {(e["tool_use_id"], e["session"]) for e in map(json.loads, linhas)}


def main() -> int:
    ap = argparse.ArgumentParser(description="V5 — negativos inline do corpus V0d")
    ap.add_argument("--selftest", action="store_true")
    ap.add_argument("--n", type=int, default=N_NEG)
    ap.add_argument(
        "--saida",
        help="diretorio do corpus a (re)gravar; sem ele, o corpus VIVO (default)",
    )
    ap.add_argument(
        "--reextrair",
        action="store_true",
        help="refaz so os ids ja presentes (exige --saida)",
    )
    a = ap.parse_args()
    definir_saida(a.saida)
    if a.selftest:
        return selftest()
    pares = _pares_gravados() if a.reextrair else set()
    u = universo({t for t, _ in pares} if a.reextrair else None)
    dentro, tmp, fora = monta(u)
    estratos = Counter(r["extrator"] for r in dentro)
    print(f"universo (programa extraivel): {len(u)}")
    print(
        f"  dentro: {len(dentro)} inline + {len(tmp)} tmp  · por estrato: {dict(estratos)}"
    )
    print(f"  excluidos: {dict(fora)}")
    if a.reextrair:
        escolhidos = [
            r for r in dentro + tmp if (r["tool_use_id"], r["session"]) in pares
        ]
    else:
        escolhidos = _sorteia(dentro, tmp, a.n)
    print(
        f"amostra: {len(escolhidos)} · por estrato: "
        f"{dict(Counter(r['extrator'] for r in escolhidos))}"
    )
    return 0 if _grava(escolhidos) == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
