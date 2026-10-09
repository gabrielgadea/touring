"""V0d — testes do construtor do corpus (TDD por lacuna, 09/10/2026).

Cada teste nasceu RED sobre o construtor herdado e fecha num conserto medido; a mutação de cada
um está registrada em RELATORIO-V0d-2026-10-09.md. Fixtures são sintéticas: repos git em tmp,
records de transcript escritos à mão — nada depende do corpus real (~/.claude/touring/vgp).
"""

import subprocess
import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).parent))
import corpus_v0d  # noqa: E402

DATA_BASE = "2026-01-01T00:00:00Z"  # commits do fixture SÃO anteriores às edições (senão rev-list --before não os vê)
GIT_ENV = {"GIT_AUTHOR_DATE": DATA_BASE, "GIT_COMMITTER_DATE": DATA_BASE}


# --------------------------------------------------------------------------- fixtures


def rec(ts: str, *contents: dict) -> dict:
    """Um registro de transcript com os contents dados (tool_use/tool_result)."""
    return {"timestamp": ts, "message": {"content": list(contents)}}


def write_use(arq: str, content: str, uid: str = "w") -> dict:
    return {
        "type": "tool_use",
        "id": uid,
        "name": "Write",
        "input": {"file_path": arq, "content": content},
    }


def edit_use(arq: str, old: str, new: str, uid: str = "e") -> dict:
    return {
        "type": "tool_use",
        "id": uid,
        "name": "Edit",
        "input": {"file_path": arq, "old_string": old, "new_string": new},
    }


def negado(uid: str) -> dict:
    return {
        "type": "tool_result",
        "tool_use_id": uid,
        "is_error": True,
        "content": [{"type": "text", "text": "PreToolUse:Edit hook error: negado"}],
    }


@pytest.fixture
def repo(tmp_path: Path) -> Path:
    import os

    r = tmp_path / "proj"
    r.mkdir()
    subprocess.run(["git", "init", "-q", str(r)], check=True)
    subprocess.run(
        [
            "git",
            "-C",
            str(r),
            "-c",
            "user.email=t@t",
            "-c",
            "user.name=t",
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            "v0",
        ],
        check=True,
        env={**os.environ, **GIT_ENV},
    )
    return r


def commit_file(repo: Path, rel: str, text: str) -> None:
    import os

    p = repo / rel
    p.parent.mkdir(parents=True, exist_ok=True)
    p.write_text(text, encoding="utf-8")
    subprocess.run(["git", "-C", str(repo), "add", rel], check=True)
    subprocess.run(
        [
            "git",
            "-C",
            str(repo),
            "-c",
            "user.email=t@t",
            "-c",
            "user.name=t",
            "commit",
            "-q",
            "-m",
            "x",
        ],
        check=True,
        env={**os.environ, **GIT_ENV},
    )


# --------------------------------------------------------------------------- lacuna 1: negativos sem .git


def test_conteudo_depois_sem_git_com_write(tmp_path: Path):
    """Arquivo fora de repo (ex.: ~/.claude/hooks) com Write ≤ ts na sessão: a dobra de base vazia
    reconstrói — hoje `_conteudo_depois` devolve None porque exige raiz com .git."""
    arq = str(tmp_path / "hooks" / "foo.py")  # tmp_path não é repo
    registros = [
        rec("2026-01-01T10:00:00Z", write_use(arq, "x = 1\n")),
        rec("2026-01-01T11:00:00Z", edit_use(arq, "x = 1", "x = 2")),
    ]
    texto = corpus_v0d._conteudo_depois(arq, "2026-01-01T11:00:00Z", {arq: registros})
    assert texto == "x = 2\n"


def test_conteudo_depois_sem_git_sem_write_e_irredutivel(tmp_path: Path):
    """Sem Write registrado o conteúdo prévio é incognoscível: None, nunca conteúdo inventado."""
    arq = str(tmp_path / "hooks" / "foo.py")
    registros = [rec("2026-01-01T11:00:00Z", edit_use(arq, "x = 1", "x = 2"))]
    assert (
        corpus_v0d._conteudo_depois(arq, "2026-01-01T11:00:00Z", {arq: registros})
        is None
    )


def test_conteudo_depois_corte_em_ts(repo: Path):
    """O snapshot é o estado logo após a edição sorteada (spec) — não o estado final dos transcripts."""
    commit_file(repo, "a.py", "v1\n")
    arq = str(repo / "a.py")
    registros = [
        rec("2026-01-02T00:00:00Z", edit_use(arq, "v1", "v2")),
        rec("2026-01-03T00:00:00Z", edit_use(arq, "v2", "v3")),
    ]
    texto = corpus_v0d._conteudo_depois(arq, "2026-01-02T12:00:00Z", {arq: registros})
    assert texto == "v2\n"


def test_conteudo_depois_ordena_registros(repo: Path):
    """Records de sessões concatenadas chegam fora de ordem temporal; a dobra ordena antes de cortar."""
    commit_file(repo, "a.py", "v1\n")
    arq = str(repo / "a.py")
    registros = [
        rec(
            "2026-01-05T00:00:00Z", edit_use(arq, "v3", "v4")
        ),  # sessão tardia primeiro
        rec("2026-01-02T00:00:00Z", edit_use(arq, "v1", "v2")),
    ]
    texto = corpus_v0d._conteudo_depois(arq, "2026-01-02T12:00:00Z", {arq: registros})
    assert texto == "v2\n"


def test_conteudo_depois_base_vazia_quando_write_criou(repo: Path):
    """Arquivo versionado DEPOIS de first (ou nunca): se a sessão o criou por Write, a base é vazia —
    não é incompleto."""
    arq = str(repo / "novo.py")  # nunca commitado
    registros = [
        rec("2026-01-02T00:00:00Z", write_use(arq, "a = 1\n")),
        rec("2026-01-02T01:00:00Z", edit_use(arq, "a = 1", "a = 2")),
    ]
    texto = corpus_v0d._conteudo_depois(arq, "2026-01-02T02:00:00Z", {arq: registros})
    assert texto == "a = 2\n"


def test_conteudo_depois_edit_sem_write_sem_base_e_incompleto(repo: Path):
    """Arquivo untracked que a sessão só Editou (veio de fora): sem base → None (honesto)."""
    arq = str(repo / "solto.py")
    registros = [rec("2026-01-02T00:00:00Z", edit_use(arq, "a = 1", "a = 2"))]
    assert (
        corpus_v0d._conteudo_depois(arq, "2026-01-02T01:00:00Z", {arq: registros})
        is None
    )


def test_edit_de_old_vazio_nao_valida_base_vazia(tmp_path: Path):
    """Guarda: Edit com old_string='' casa numa base vazia por acidente ('' in ''); sem um Write real
    o candidato NÃO entra."""
    arq = str(tmp_path / "x.py")
    registros = [rec("2026-01-01T10:00:00Z", edit_use(arq, "", "conteudo = 1\n"))]
    assert (
        corpus_v0d._conteudo_depois(arq, "2026-01-01T11:00:00Z", {arq: registros})
        is None
    )


def test_conteudo_depois_heredoc_bash_cria_fora_de_tmp(tmp_path: Path):
    """Sessões criam hooks/scripts com `cat > path <<EOF` fora de /tmp: a dobra livre aplica heredoc
    sem a guarda de session_work do replay."""
    arq = str(tmp_path / "hooks" / "h.py")
    cmd = f"cat > {arq} <<'EOF'\nprint('oi')\nEOF"
    registros = [
        rec(
            "2026-01-01T10:00:00Z",
            {"type": "tool_use", "id": "b", "name": "Bash", "input": {"command": cmd}},
        )
    ]
    texto = corpus_v0d._conteudo_depois(arq, "2026-01-01T11:00:00Z", {arq: registros})
    assert texto == "print('oi')\n"


# --------------------------------------------------------------------------- lacuna 1b: sorteio por ids


def test_sorteio_por_ids_reproduz_o_conjunto():
    """O juiz re-sorteia `random.Random(SEED).sample(pool, n)` e exige os MESMOS ids das entradas:
    o pool registrado é de ids, a amostra final reconstrói inteira, e o re-sorteio casa."""
    ids = [f"id-{i:04d}" for i in range(50)]
    falhos = {
        f"id-{i:04d}" for i in range(0, 50, 5)
    }  # 10/50: toda amostra de 10 pega falho

    def reconhece(eid: str) -> bool:
        return eid not in falhos

    pool, amostra, stats = corpus_v0d._sorteia_negativos(
        ids, seed=7, n=10, reconhece=reconhece
    )
    assert len(amostra) == 10
    assert not falhos & set(amostra)
    # o re-sorteio do juiz sobre o pool FINAL devolve exatamente a amostra final
    import random

    assert set(random.Random(7).sample(pool, min(10, len(pool)))) == set(amostra)
    # e o pool só perdeu o que foi sorteado e falhou de fato
    assert set(ids) - set(pool) <= falhos
    assert stats["podados"] >= 1
    assert stats["rodadas"] >= 2


def test_sorteio_universo_menor_que_n():
    ids = [f"id-{i}" for i in range(4)]
    pool, amostra, _ = corpus_v0d._sorteia_negativos(
        ids, seed=7, n=10, reconhece=lambda i: True
    )
    assert len(amostra) == 4 and set(pool) == set(ids)


# --------------------------------------------------------------------------- lacuna 2: contexto no mesmo rev


def test_contexto_sobrepoe_ref_de_sessao_reconstruida(tmp_path: Path, monkeypatch):
    monkeypatch.setattr(corpus_v0d, "C", tmp_path)
    """O comando nomeia um /tmp/x.py que a sessão gravou e hoje não existe mais: o contexto o
    carrega com o conteúdo RECONSTRUÍDO (hoje: files={} no chamador → contexto vazio)."""
    alvo = str(
        tmp_path / "scr" / "x.py"
    )  # fora de /tmp → _referenced não o vê; uso /tmp real:
    alvo = "/tmp/v0d-test-ctx/x.py"
    sessao = {alvo: "import os\n"}
    ctx = corpus_v0d._contexto(
        f"python3 {alvo}",
        "/tmp",
        "/outro.py",
        "2026-01-02T00:00:00Z",
        files_sessao=sessao,
        snap_text="print(1)\n",
    )
    assert [c["path"] for c in ctx] == [alvo]
    assert ctx[0]["sha256"] == corpus_v0d.sha("import os\n")


def test_contexto_projeto_no_rev_de_first(repo: Path, monkeypatch, tmp_path: Path):
    monkeypatch.setattr(corpus_v0d, "C", tmp_path)
    """Arquivo de PROJETO nomeado no comando e que mudou desde first: o contexto carrega a versão
    de first (base git + dobra), nunca a de hoje. O rc 0 dos 8 casos nasceu do módulo de hoje."""
    commit_file(repo, "mod.py", "QUEBRADO = 1\n")
    (repo / "mod.py").write_text(
        "QUEBRADO = None  # corrigido hoje\n", encoding="utf-8"
    )  # só worktree
    # tmp_path mora em /tmp = session_work: sem este monkeypatch o ref entraria pelo canal de
    # sessão (replay._referenced) e o teste NÃO mediria _refs_projeto — M2b passou sem matá-lo
    monkeypatch.setattr(corpus_v0d.replay, "session_work", lambda _p: False)
    cmd = f"python3 {repo}/mod.py"
    ctx = corpus_v0d._contexto(
        cmd,
        str(repo),
        str(repo / "main.py"),
        "2026-01-02T12:00:00Z",
        files_sessao={},
        snap_text="import mod\n",
    )
    alvos = {c["path"]: c for c in ctx}
    assert str(repo / "mod.py") in alvos
    snap_file = corpus_v0d.C / alvos[str(repo / "mod.py")]["snap"]
    assert snap_file.read_text(encoding="utf-8") == "QUEBRADO = 1\n"


def test_contexto_nao_sobrepoe_o_proprio_nem_igual(
    repo: Path, monkeypatch, tmp_path: Path
):
    monkeypatch.setattr(corpus_v0d, "C", tmp_path)
    """O próprio arquivo nunca é contexto; e o que está igual ao disco de hoje não infla o corpus."""
    commit_file(repo, "mesmo.py", "a = 1\n")
    commit_file(
        repo, "outra.py", "b = 2\n"
    )  # referenciada, inalterada desde first → fora
    cmd = f"python3 {repo}/mesmo.py  # {repo}/outra.py"
    ctx = corpus_v0d._contexto(
        cmd,
        str(repo),
        str(repo / "mesmo.py"),
        "2026-01-02T12:00:00Z",
        files_sessao={},
        snap_text="a = 1\n",
    )
    assert ctx == []


def test_contexto_import_do_snapshot_no_rev(repo: Path, monkeypatch, tmp_path: Path):
    monkeypatch.setattr(corpus_v0d, "C", tmp_path)
    """O snapshot importa um módulo irmão do projeto que mudou desde first: entra no contexto na
    versão de first — o import é uma dependência mesmo quando o comando não o nomeia."""
    commit_file(repo, "main.py", "import irmao\nprint(irmao.X)\n")
    commit_file(repo, "irmao.py", "X = 1\n")
    (repo / "irmao.py").write_text(
        "X = 2\n", encoding="utf-8"
    )  # hoje diverge, sem commit
    ctx = corpus_v0d._contexto(
        f"cd {repo} && python3 main.py",
        str(repo),
        str(repo / "main.py"),
        "2026-01-02T12:00:00Z",
        files_sessao={},
        snap_text="import irmao\nprint(irmao.X)\n",
    )
    alvos = {c["path"]: c for c in ctx}
    assert str(repo / "irmao.py") in alvos
    snap_file = corpus_v0d.C / alvos[str(repo / "irmao.py")]["snap"]
    assert snap_file.read_text(encoding="utf-8") == "X = 1\n"


# --------------------------------------------------------------------------- lacuna 3: touring run


def test_inline_touring_run_code():
    cmd = "touring run --lang python --timeout-ms 30000 --code '\nimport json\nprint(json.loads)' 2>&1 | head"
    assert corpus_v0d.programa_inline(cmd) == "\nimport json\nprint(json.loads)\n"


def test_inline_touring_run_file(tmp_path: Path, monkeypatch):
    """--file aponta um /tmp que a sessão gravou: o programa é o conteúdo reconstruído na dobra."""
    alvo = "/tmp/v0d-test-trf/diag.py"
    monkeypatch.setattr(
        corpus_v0d,
        "_reconstroi_arquivo_sessao",
        lambda path, session, first: "raise NameError('x')\n" if path == alvo else None,
    )
    cmd = f"touring run --lang python --file {alvo} --timeout-ms 120000 --brief 2>&1 | head -80"
    assert (
        corpus_v0d.programa_inline(cmd, session="s1", first="2026-01-02T00:00:00Z")
        == "raise NameError('x')\n"
    )


def test_comando_replay_de_touring_run_e_python_c():
    """O replay roda o programa extraído como `python3 -c` — que nomeia o frame `<string>` exatamente
    como o `touring run` nomeava (medido nos 17 casos: file='<string>')."""
    cmd = corpus_v0d.comando_replay(
        "touring run --lang python --code 'print(1)'", "print(1)\n"
    )
    assert cmd is not None and "python3 -c " in cmd and "touring" not in cmd


def test_comando_replay_preserva_o_timeout_declarado():
    """`replay._declared_timeout` lê o teto do TEXTO do comando — sem ele o replay herdaria 60 s e
    programas pesados morreriam como TIMEOUT falso (o original tinha --timeout-ms 120000)."""
    cmd = corpus_v0d.comando_replay(
        "touring run --timeout-ms 120000 --code 'print(1)'", "print(1)\n"
    )
    assert cmd.startswith("timeout 120 python3 -c ")


def test_comando_replay_preserva_sufixo_de_pipe():
    """O `| head -80` do original NÃO vai para o replay (cortaria o traceback e o frame)."""
    cmd = corpus_v0d.comando_replay(
        "touring run --code 'print(1)' 2>&1 | head -3", "print(1)\n"
    )
    assert "|" not in cmd and "head" not in cmd


# --------------------------------------------------------------------------- lacuna 4: base vazia no positivo


def test_snap_projeto_base_vazia_quando_write_criou(repo: Path):
    """Positivo atribuído cujo arquivo não existia no commit anterior a first: se a sessão o criou
    por Write, a base é vazia e a entrada entra — não é incompleta."""
    arq = str(repo / "pkg" / "novo.py")
    registros = [rec("2026-01-02T10:00:00Z", write_use(arq, "a = 1\n"))]
    origem, texto, motivo = corpus_v0d._snap_projeto(
        registros, arq, "2026-01-02T11:00:00Z"
    )
    assert (origem, texto) == ("atribuido", "a = 1\n") and not motivo


def test_snap_projeto_sem_write_continua_incompleto(repo: Path):
    """Sem Write registrado, o conteúdo prévio é incognoscível: incompleto com motivo, nunca errado."""
    arq = str(repo / "pkg" / "solto.py")
    registros = [rec("2026-01-02T10:00:00Z", edit_use(arq, "a = 1", "a = 2"))]
    _, texto, motivo = corpus_v0d._snap_projeto(registros, arq, "2026-01-02T11:00:00Z")
    assert texto is None and "Write" in motivo


# --------------------------------------------------------------------------- replay: staleness por assinatura


def test_replay_reexecuta_quando_overlay_muda(tmp_path: Path, monkeypatch):
    """O registro de replay é reutilizado só se a assinatura do overlay (paths+shas) é a mesma —
    rebuild com snapshot novo NÃO herda o veredito do antigo."""
    monkeypatch.setattr(corpus_v0d, "C", tmp_path)
    calls = []

    def fake_run(work, command, cwd, files, cap):
        calls.append(dict(files))
        return 1, False, "Traceback\nAttributeError: x\n"

    monkeypatch.setattr(corpus_v0d.replay, "run_isolated", fake_run)
    (tmp_path / "snap").mkdir()
    (tmp_path / "snap" / "e1.py").write_text("x = 1\n", encoding="utf-8")
    item = {
        "id": "e1",
        "rotulo": "positivo",
        "classe": "atributo",
        "origem": "atribuido",
        "path": "/p/a.py",
        "snap": "snap/e1.py",
        "snap_sha256": corpus_v0d.sha("x = 1\n"),
        "command": "python3 /p/a.py",
        "cwd": "/p",
        "linha": 1,
        "kind": "AttributeError",
        "mensagem": "m",
        "contexto": [],
    }
    corpus_v0d.reproduzir(item, 60)
    corpus_v0d.reproduzir(item, 60)
    assert len(calls) == 1  # mesmo overlay: reuso
    (tmp_path / "snap" / "e1.py").write_text("x = 2\n", encoding="utf-8")
    corpus_v0d.reproduzir({**item, "snap_sha256": corpus_v0d.sha("x = 2\n")}, 60)
    assert len(calls) == 2  # overlay mudou: re-executa


# --------------------------------------------------------------------------- tentativa 2 (pós-build medido)


def test_montar_dedup_por_id(monkeypatch):
    """Duas linhas V0a do MESMO evento (session, path, first) geram UM positivo — o id
    determinístico colide por construção e o juiz barra 'ids repetidos'."""
    rows = [
        {"session": "s", "file": "/x.py", "first": "t"},
        {
            "session": "s",
            "file": "/x.py",
            "first": "t",
        },  # o mesmo evento, 2ª linha do inventário
        {"session": "s", "file": "/y.py", "first": "t"},
    ]
    monkeypatch.setattr(corpus_v0d, "candidatos", lambda: rows)
    monkeypatch.setattr(
        corpus_v0d,
        "montar_positivo",
        lambda row: ({"id": "ID-" + row["file"][-4:]}, ""),
    )
    entradas, incompletos, _ = corpus_v0d._montar()
    assert [e["id"] for e in entradas] == ["ID-x.py", "ID-y.py"]
    assert incompletos.get("duplicata de inventario (mesmo evento)") == 1


def test_comando_replay_lang_bash_vira_bash_c():
    """`touring run --lang bash --code '<bash>'` NÃO é python: a reescrita fiel é `bash -c`, que
    preserva os frames internos (o python chamado dentro nomeia o arquivo real ou <stdin>)."""
    cmd = corpus_v0d.comando_replay(
        "touring run --lang bash --code 'echo oi'", "echo oi\n"
    )
    assert cmd == "bash -c 'echo oi\n'"


def test_comando_replay_lang_python_segue_python_c():
    cmd = corpus_v0d.comando_replay(
        "touring run --lang python --code 'print(1)'", "print(1)\n"
    )
    assert "python3 -c " in cmd and "bash" not in cmd


def test_imports_do_com_base_extra(tmp_path: Path):
    """`touring run --file /tmp/dir/x.py`: o programa importa o IRMÃO de x.py — a base de imports é
    o diretório do arquivo-fONte, não o parent de '<string>'."""
    d = tmp_path / "prog"
    d.mkdir()
    (d / "harness.py").write_text("X = 1\n", encoding="utf-8")
    refs = corpus_v0d._imports_do(
        "import harness\n", "<string>", "", extra_bases=[str(d)]
    )
    assert refs == {str(d / "harness.py")}


def test_assinatura_do_overlay_cobre_o_comando(tmp_path: Path, monkeypatch):
    """Rebuild que muda só o `command` (overlay igual) NÃO pode herdar o registro — o veredito
    pertence ao par (comando, overlay)."""
    monkeypatch.setattr(corpus_v0d, "C", tmp_path)
    calls = []

    def fake_run(work, command, cwd, files, cap):
        calls.append(command)
        return 1, False, "AttributeError\n"

    monkeypatch.setattr(corpus_v0d.replay, "run_isolated", fake_run)
    (tmp_path / "snap").mkdir()
    (tmp_path / "snap" / "e2.py").write_text("x = 1\n", encoding="utf-8")
    base = {
        "id": "e2",
        "rotulo": "positivo",
        "classe": "atributo",
        "origem": "atribuido",
        "path": "/p/a.py",
        "snap": "snap/e2.py",
        "snap_sha256": corpus_v0d.sha("x = 1\n"),
        "cwd": "/p",
        "linha": 1,
        "kind": "AttributeError",
        "mensagem": "m",
        "contexto": [],
    }
    corpus_v0d.reproduzir({**base, "command": "python3 /p/a.py"}, 60)
    corpus_v0d.reproduzir({**base, "command": "python3 /p/a.py"}, 60)
    assert len(calls) == 1
    corpus_v0d.reproduzir({**base, "command": "bash -c 'python3 /p/a.py'"}, 60)
    assert len(calls) == 2  # comando mudou → re-executa


# --------------------------------------------------------------------------- tentativa 3 (pós-build 2)


def test_programa_para_replay_nao_inline_extrai_o_code():
    """Para tmp/atribuido o snapshot é o CONTEÚDO DO ARQUIVO (python) — o programa a reexecutar é o
    --code/--file do comando (o wrapper bash que chama python3 /tmp/x.py). Alimentar o bash -c com o
    snapshot python foi o bug medido em 5 entradas (bash: -c: syntax error)."""
    cmd = "touring run --lang bash --timeout-ms 300000 --code 'cd /p && python3 /tmp/x.py'"
    # inline: o snapshot É o programa
    assert (
        corpus_v0d._programa_para_replay(cmd, "inline", "raise KeyError\n", "s", "t")
        == "raise KeyError\n"
    )
    # tmp/atribuido: o programa é o --code extraído do comando, NUNCA o snapshot
    got = corpus_v0d._programa_para_replay(
        cmd, "tmp", "CONTEUDO_DO_ARQUIVO = 1\n", "s", "t"
    )
    assert (
        got == "cd /p && python3 /tmp/x.py\n"
    )  # programa_inline devolve o programa + \n final


def test_contexto_expande_vars_de_atribuicao(tmp_path: Path, monkeypatch):
    """`S=/tmp/dir; python3 $S/a.py` — o replay herdado não expande; sem expansão o arquivo some do
    overlay e o replay morre com 'No such file or directory' (caso 429e18f3d11f)."""
    monkeypatch.setattr(corpus_v0d, "C", tmp_path)
    alvo = "/tmp/v0d-test-var/a.py"
    cmd = "S=/tmp/v0d-test-var\npython3 $S/a.py"
    ctx = corpus_v0d._contexto(
        cmd, "/tmp", "/outro.py", "2026-01-02T00:00:00Z", files_sessao={alvo: "x = 1\n"}
    )
    assert [c["path"] for c in ctx] == [alvo]


def test_resolve_mod_aceita_arquivo_de_sessao_sumido(tmp_path: Path):
    """`import harness` de um --file em /tmp: o módulo NÃO existe hoje (sessão passada) — a base de
    imports aceita candidatos session_work; o conteúdo vem da dobra, não do disco (a0b2f665934b)."""
    d = "/tmp/v0d-test-gone"
    refs = corpus_v0d._imports_do(
        "import harness\n",
        "<string>",
        "",
        extra_bases=[d],
        virtuais={f"{d}/harness.py"},
    )  # o que a dobra da sessão reconstruiu
    assert refs == {f"{d}/harness.py"}
    assert not Path(f"{d}/harness.py").exists()  # controle: de fato sumiu do disco


def test_comando_eh_o_proprio_arquivo_roda_o_path():
    """O inventário às vezes registra `python3 -c '<fonte do script>'` para um erro cujo frame é o
    ARQUIVO (file:97). Sob -c o frame é <string> e __file__ não existe — a reescrita fiel é
    `python3 <path>` (o snapshot está sobreposto lá)."""
    texto = "#!/usr/bin/env python3\nimport sys\nprint(__file__)\n"
    cmd = f"python3 -c '{texto}'"
    got = corpus_v0d._programa_para_replay(cmd, "atribuido", texto, "s", "t")
    assert got is None  # não é touring run
    assert corpus_v0d.comando_de_arquivo(cmd, "/p/a.py", texto) == "python3 /p/a.py"
    assert (
        corpus_v0d.comando_de_arquivo("python3 -c 'print(1)'", "/p/a.py", texto) is None
    )
