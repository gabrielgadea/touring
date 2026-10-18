"""VRS — testes do construtor do corpus natural de Rust (09/10/2026).

Fixtures sintéticas: repo git em tmp + transcript escrito à mão, com rustc e cargo imitando a saída real.
Nada depende de ~/.claude/projects nem do corpus vivo (~/.claude/touring/vgp/corpus_rust_natural).
"""

import hashlib
import json
import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).parent))
import corpus_rust_natural as cr  # pyright: ignore[reportMissingImports]

# --------------------------------------------------------------------------- amostras no formato do rustc real

E0063 = """error[E0063]: missing fields `orphan_symbols` and `total_pub_symbols` in initializer of `models::wiring::WiringModule`
   --> crates/touring-bindings/src/desktop/components/wiring_graph_viewer.rs:372:13
    |
372 |             WiringModule {
    |             ^^^^^^^^^^^^ missing `orphan_symbols` and `total_pub_symbols`

For more information about this error, try `rustc --explain E0063`.
"""

E0061 = """error[E0061]: this function takes 2 arguments but 1 argument was supplied
  --> src/lib.rs:12:5
   |
12 |     compute(a)
   |     ^^^^^^^ - argument #2 of type `u32` is missing
"""

E0425_DUAS_LINHAS = """error[E0425]: cannot find function `cli_index_ingest` in this scope
    --> crates/touring-cli/src/cli/handlers/index.rs:3167:23
     |
 206 | pub fn cli_index_find(rt: &mut HookRuntime) -> String {
     | ---------------------------------------------------- similarly named function `cli_index_find` defined here
...
3167 |         let r = cli_index_ingest(rt);
     |                 ^^^^^^^^^^^^^^^^ help: a function with a similar name exists: `cli_index_find`
"""


def diag(texto: str) -> cr.Diag:
    achados = cr.parse_erros(texto)
    assert len(achados) == 1
    return achados[0]


# --------------------------------------------------------------------------- parser do rustc


@pytest.mark.parametrize(
    ("cabeca", "codigo", "esperado"),
    [
        (
            "error[E0425]: cannot find value `MAX_DEPTH` in this scope",
            "E0425",
            "MAX_DEPTH",
        ),
        ("error[E0412]: cannot find type `Tri` in this scope", "E0412", "Tri"),
        ("error[E0433]: cannot find `bridges` in `crate`", "E0433", "bridges"),
        (
            "error[E0433]: failed to resolve: use of undeclared type `Tri`",
            "E0433",
            "Tri",
        ),
        (
            "error[E0432]: unresolved import `super::super::gates_limpo`",
            "E0432",
            "gates_limpo",
        ),
        (
            "error[E0599]: no method named `try_build` found for struct `Idx` in the current scope",
            "E0599",
            "try_build",
        ),
        (
            "error[E0599]: no variant, associated function, or constant named `Semantic` found for enum `rlm::MemoryTier` in the current scope",
            "E0599",
            "Semantic",
        ),
        (
            "error[E0609]: no field `knowledge` on type `HookRuntime`",
            "E0609",
            "knowledge",
        ),
        ("error[E0560]: struct `S` has no field named `extra`", "E0560", "extra"),
        ("error[E0559]: variant `E::V` has no field named `extra`", "E0559", "extra"),
        (
            "error[E0026]: struct `S` does not have a field named `extra`",
            "E0026",
            "extra",
        ),
    ],
)
def test_nome_esperado_por_codigo(cabeca: str, codigo: str, esperado: str):
    d = diag(cabeca + "\n --> src/x.rs:1:1\n")
    assert d.codigo == codigo
    assert cr.nome_esperado(d) == (esperado, "")


def test_e0599_que_nao_nomeia_um_simbolo_inventado_sai():
    d = diag(
        "error[E0599]: the method `iter` exists for struct `Foo`, but its trait bounds were not satisfied\n --> a.rs:1:1\n"
    )
    nome, motivo = cr.nome_esperado(d)
    assert nome is None
    assert "E0599" in motivo


def test_e0063_o_nome_e_o_tipo_do_literal_que_esta_na_linha():
    d = diag(E0063)
    assert (d.caminho, d.linha, d.coluna) == (
        "crates/touring-bindings/src/desktop/components/wiring_graph_viewer.rs",
        372,
        13,
    )
    assert cr.nome_esperado(d) == ("WiringModule", "")


def test_e0061_o_nome_e_o_chamado_sublinhado_pelo_rustc():
    assert cr.nome_esperado(diag(E0061)) == ("compute", "")


def test_e0061_sem_trecho_impresso_nao_tem_nome():
    d = diag(
        "error[E0061]: this function takes 2 arguments but 1 argument was supplied\n --> src/lib.rs:12:5\n"
    )
    assert cr.nome_esperado(d)[0] is None


def test_parse_aponta_a_linha_numerada_certa_entre_varias_no_trecho():
    d = diag(E0425_DUAS_LINHAS)
    assert d.linha == 3167
    assert cr.linha_do_snippet(d.bloco, 3167) == "        let r = cli_index_ingest(rt);"
    assert cr.linha_do_snippet(d.bloco, 206) is not None
    assert cr.linha_do_snippet(d.bloco, 7) is None


def test_parse_ignora_codigos_fora_da_lista_e_um_bloco_sem_localizacao_fica_sem_caminho():
    texto = "error[E0308]: mismatched types\n --> a.rs:1:1\n\nerror[E0425]: cannot find value `x` in this scope\n"
    achados = cr.parse_erros(texto)
    assert [a.codigo for a in achados] == ["E0425"]
    assert achados[0].caminho == ""


def test_parse_tira_as_cores_ansi():
    d = diag(
        "\x1b[31merror[E0425]\x1b[0m: cannot find value `x` in this scope\n --> a.rs:2:3\n"
    )
    assert (d.codigo, d.caminho, d.linha) == ("E0425", "a.rs", 2)


# --------------------------------------------------------------------------- o oráculo textual


def test_confirmar_aceita_quando_o_trecho_do_rustc_e_o_nome_batem():
    d = diag(E0061)
    texto = "\n" * 11 + "    compute(a)\n"
    assert cr.confirmar(texto, d, "compute")[0] == ""


def test_confirmar_recusa_quando_o_trecho_impresso_difere_do_reconstruido():
    d = diag(E0061)
    texto = "\n" * 11 + "    outra_coisa(a)\n"
    assert cr.confirmar(texto, d, "compute")[0].startswith("linha não bate")


def test_confirmar_recusa_linha_fora_do_arquivo():
    d = diag(E0061)
    assert "fora do arquivo" in cr.confirmar("curto\n", d, "compute")[0]


def test_confirmar_sem_trecho_exige_o_nome_na_linha():
    d = diag("error[E0425]: cannot find value `x` in this scope\n --> a.rs:2:3\n")
    assert cr.confirmar("a\nlet y = 1;\n", d, "x")[0] == "nome ausente na linha"
    assert cr.confirmar("a\nlet y = x + 1;\n", d, "x")[0] == ""


def test_confirmar_tolera_a_linha_cortada_com_reticencias():
    d = diag(
        "error[E0425]: cannot find value `x` in this scope\n --> a.rs:1:1\n  |\n1 | ...let y = x + 1;...\n"
    )
    assert cr.confirmar("let y = x + 1; // um comentario enorme\n", d, "x")[0] == ""


def test_no_trecho_respeita_a_fronteira_do_identificador():
    assert cr.no_trecho("x", "let y = x + 1;")
    assert not cr.no_trecho("x", "let xs = 1;")
    assert not cr.no_trecho("x", "let _x = 1;")


# --------------------------------------------------------------------------- onde o erro foi introduzido


def test_introducao_acha_o_estado_anterior_a_edicao_que_trouxe_a_linha():
    estados = ("fn a() {}\n", "fn a() {}\nfoo();\n", "fn a() {}\nfoo();\n// c\n")
    assert cr.introducao(estados, "foo();") == 0


def test_introducao_linha_ja_presente_na_base_nao_e_introduzida():
    estados = ("foo();\n", "foo();\n// c\n")
    assert cr.introducao(estados, "foo();") == -1


def test_introducao_estado_anterior_irreconstruivel_devolve_none():
    assert cr.introducao((None, "x\nfoo();\n"), "foo();") is None


def test_introducao_conta_copias_da_linha_repetida():
    estados = ("foo();\n", "foo();\nfoo();\n")
    assert cr.introducao(estados, "foo();") == 0


# --------------------------------------------------------------------------- o que o cargo prova


def inv(comando: str) -> cr.Invocacao:
    achadas = cr.analisar_cargo(comando)
    assert len(achadas) == 1, comando
    return achadas[0]


def test_cargo_so_vale_quando_o_comando_o_executa():
    assert cr.analisar_cargo('echo "rode cargo check depois"') == ()
    assert cr.analisar_cargo("grep -n 'cargo test' Makefile") == ()
    assert (
        len(
            cr.analisar_cargo(
                "cd /p && CARGO_TARGET_DIR=/t timeout 600 cargo test -p a --lib"
            )
        )
        == 1
    )


def test_cargo_barra_dentro_de_aspas_nao_separa_o_pipeline():
    i = inv(
        "cargo test -p x 2>&1 | grep -E 'test result|^error|FAILED' | head -6; echo fim"
    )
    assert i.forma == "grep-erro"
    assert not i.saida_valida  # `; echo fim` mascara o exit code


def test_cargo_base_pacotes_e_escopo():
    i = inv(
        "cd /home/g/projects/touring && cargo check -p touring-code -p touring-cli --workspace"
    )
    assert i.base == "/home/g/projects/touring"
    assert i.pacotes == ("touring-code", "touring-cli")
    assert i.workspace


def test_cargo_encadeado_com_e_comercial_mantem_o_exit_code_valido():
    achadas = cr.analisar_cargo("cargo check && cargo test -p z --lib")
    assert [a.sub for a in achadas] == ["check", "test"]
    assert all(a.saida_valida for a in achadas)
    assert achadas[1].seletores == (("lib", ""),)


def test_cargo_em_segundo_plano_e_pipefail():
    assert inv("cargo test &").segundo_plano
    seguro = inv("set -o pipefail; cargo build 2>&1 | tail -2")
    assert seguro.pipefail_ok
    assert not inv("set -o pipefail; cargo build 2>&1 | grep -v warning").pipefail_ok


def test_cargo_seletores_e_features():
    i = inv("cargo test -p x --test dag_e2e --features a,b --no-default-features")
    assert i.seletores == (("test", "dag_e2e"),)
    assert i.features == ("a", "b")
    assert i.sem_padrao
    assert i.testes
    assert not inv("cargo check -p x").testes
    assert inv("cargo check --all-targets").testes


LIMPO = "   Compiling cratex v0.1.0 (/p/cratex)\n    Finished `dev` profile in 0.4s\n"


def test_resumo_le_o_que_o_cargo_provou():
    r = cr.resumir_cargo(LIMPO)
    assert r.finished and not r.erro and r.compilou
    assert ("cratex", "/p/cratex") in r.compilados
    assert cr.resumir_cargo("running 3 tests\ntest a ... ok\n").compilou
    assert cr.resumir_cargo("error[E0425]: cannot find value `x`\n").erro
    assert not cr.resumir_cargo(
        "error: test failed, to rerun pass `-p x`\n"
    ).erro  # falha de teste ≠ de compilação
    assert cr.resumir_cargo("Command running in background with ID: bx\n").segundo_plano


@pytest.mark.parametrize(
    ("comando", "texto", "is_error", "esperado"),
    [
        ("cargo check", LIMPO, False, "visto"),
        (
            "cargo check 2>&1 | tail -2",
            "    Finished `dev` profile in 0.4s\n",
            False,
            "finished",
        ),
        (
            "cargo test -p x 2>&1 | grep 'test result'",
            "test result: ok. 3 passed\n",
            False,
            "finished",
        ),
        ("cargo check -q", "", False, "rc0"),
        ("cargo check -q; echo $?", "0\n", False, ""),  # `;` mascara o exit code
        ("cargo check 2>&1 | head -3", "", False, ""),  # `head` esconde tudo
        ("cargo check 2>&1 | grep -E '^error' | head", "", False, "grep-erro"),
        ("cargo check", "error[E0425]: cannot find value `x`\n", True, "erro"),
        ("cargo check", "timeout\n", True, "erro"),
        ("cargo test -p x", "Command running in background with ID: b\n", False, ""),
        ("set -o pipefail; cargo build 2>&1 | tail -1", "ok\n", False, "rc0"),
    ],
)
def test_evidencia_por_forma_do_comando(
    comando: str, texto: str, is_error: bool, esperado: str
):
    assert cr.evidencia(inv(comando), cr.resumir_cargo(texto), is_error) == esperado


def test_evidencia_com_varios_cargo_so_atribui_o_que_e_atribuivel():
    achadas = cr.analisar_cargo(
        "cargo check -p a 2>&1 | tail -1; cargo test -p b 2>&1 | tail -3"
    )
    r_finished = cr.resumir_cargo("    Finished `dev` profile in 0.4s\n")
    r_teste = cr.resumir_cargo("test result: ok. 3 passed\n")
    assert [cr.evidencia(a, r_finished, False, unico=False) for a in achadas] == [
        "",
        "",
    ]
    assert (
        cr.evidencia(achadas[0], r_teste, False, unico=False) == ""
    )  # só o `cargo test` imprime `test result`
    assert cr.evidencia(achadas[1], r_teste, False, unico=False) == "finished"


# --------------------------------------------------------------------------- escopo, módulos e Bash que reescreve


def _crate(base: Path, nome: str = "a", ws: bool = False) -> cr.Crate:
    (base / nome).mkdir(parents=True, exist_ok=True)
    (base / nome / "Cargo.toml").write_text(
        f'[package]\nname = "{nome}"\n[features]\ndefault = ["d"]\nd = ["e"]\ne = []\n'
    )
    if ws:
        (base / "Cargo.toml").write_text(f'[workspace]\nmembers = ["{nome}"]\n')
    return cr.Crate(nome, str(base / nome))


def test_escopo_de_um_workspace_virtual_cobre_todo_membro(tmp_path: Path):
    crate = _crate(tmp_path, "a", ws=True)
    cache: cr.Manifestos = {}
    assert cr.cobre_por_escopo(inv("cargo check"), tmp_path, crate, cache)
    assert cr.cobre_por_escopo(inv("cargo check -p a"), tmp_path, crate, cache)
    assert not cr.cobre_por_escopo(inv("cargo check -p b"), tmp_path, crate, cache)
    assert cr.cobre_por_escopo(
        inv("cargo check --workspace"), tmp_path / "a", crate, cache
    )


def test_escopo_dentro_do_crate_so_cobre_o_proprio(tmp_path: Path):
    a = _crate(tmp_path, "a", ws=True)
    b = _crate(tmp_path, "b")
    cache: cr.Manifestos = {}
    assert cr.cobre_por_escopo(inv("cargo check"), tmp_path / "a", a, cache)
    assert not cr.cobre_por_escopo(inv("cargo check"), tmp_path / "a", b, cache)


def test_features_ativas_seguem_o_default_e_o_fechamento(tmp_path: Path):
    crate = _crate(tmp_path)
    cache: cr.Manifestos = {}
    assert {"d", "e"} <= cr.features_ativas(crate, inv("cargo check"), cache)
    assert "d" not in cr.features_ativas(
        crate, inv("cargo check --no-default-features"), cache
    )
    assert "e" in cr.features_ativas(
        crate, inv("cargo check --no-default-features --features e"), cache
    )
    assert "*" in cr.features_ativas(crate, inv("cargo check --all-features"), cache)


def test_alvo_coberto_respeita_lib_bin_test(tmp_path: Path):
    lib = str(tmp_path / "a/src/x.rs")
    teste = str(tmp_path / "a/tests/dag_e2e.rs")
    assert cr.alvo_coberto(inv("cargo test -p a"), lib, None, False)
    assert cr.alvo_coberto(inv("cargo test -p a --lib"), lib, None, True)
    assert not cr.alvo_coberto(inv("cargo test -p a --lib"), teste, None, True)
    assert cr.alvo_coberto(inv("cargo test -p a --test dag_e2e"), teste, None, True)
    assert not cr.alvo_coberto(inv("cargo test -p a --test outro"), teste, None, True)
    assert not cr.alvo_coberto(
        inv("cargo test -p a --test dag_e2e"), lib, None, True
    )  # lib só em modo normal
    assert cr.alvo_coberto(inv("cargo test -p a --test dag_e2e"), lib, None, False)
    assert not cr.alvo_coberto(
        inv("cargo test -p a --lib"), str(tmp_path / "a/src/main.rs"), None, False
    )


def test_declaracoes_de_modulo_com_pilha_de_atributos_e_varias_por_linha():
    texto = (
        '#[cfg(test)]\n#[path = "wiring_tests.rs"]\nmod tests;\n'
        "pub mod a; pub(crate) mod b;\n"
        '#[cfg(feature = "knowledge")]\npub mod c;\n'
        "mod inline { }\n// mod comentado;\n"
    )
    achadas = {m.group(2): m.group(1) for m in cr.DECL_MOD.finditer(texto)}
    assert set(achadas) == {"tests", "a", "b", "c"}
    assert (
        'path = "wiring_tests.rs"' in achadas["tests"]
        and "cfg(test)" in achadas["tests"]
    )
    assert 'feature = "knowledge"' in achadas["c"]
    assert achadas["a"] == "" and achadas["b"] == ""


def test_mutados_por_bash_precisa_do_verbo_e_do_arquivo_na_mesma_unidade():
    assert (
        cr.mutados_por_bash("cp /tmp/a.txt /tmp/b.txt && cargo test lib.rs", "/p")
        == set()
    )
    patch = 'python3 - <<EOF\nfrom pathlib import Path\np = Path("crates/x/src/y.rs")\np.write_text(p.read_text())\nEOF'
    assert ("crates/x/src/y.rs", "/p") in cr.mutados_por_bash(patch, "/p")
    assert ("src/viva.rs", "/p") in cr.mutados_por_bash(
        "sed -i 's/a/b/' src/viva.rs", "/p"
    )
    heredoc_rastreado = "cat > /p/src/novo.rs <<'EOF'\nfn a() {}\nEOF"
    assert (
        cr.mutados_por_bash(heredoc_rastreado, "/p") == set()
    )  # a dobra já reproduz o heredoc


def test_mutacao_por_bash_so_vale_dentro_da_janela_e_aponta_para_o_arquivo(
    tmp_path: Path,
):
    (tmp_path / ".git").mkdir()
    uni = cr.Universo("9999")
    uni.bash_muta["viva.rs"].append(
        ("2099-01-02T00:00:00.000Z", "src/viva.rs", str(tmp_path))
    )
    uni.bash_muta["lib.rs"].append(
        ("2099-01-02T00:00:00.000Z", "lib.rs", str(tmp_path / "outra"))
    )
    alvo = str(tmp_path / "src/viva.rs")
    assert cr.reescrito_por_bash(
        alvo, "2099-01-01T00:00:00.000Z", "2099-01-03T00:00:00.000Z", uni
    )
    assert not cr.reescrito_por_bash(
        alvo, "2099-01-02T00:00:01.000Z", "2099-01-03T00:00:00.000Z", uni
    )
    # nome comum (lib.rs) só conta quando o comando rodou NA pasta do arquivo
    assert not cr.reescrito_por_bash(
        str(tmp_path / "src/lib.rs"),
        "2099-01-01T00:00:00.000Z",
        "2099-01-03T00:00:00.000Z",
        uni,
    )


# --------------------------------------------------------------------------- o construtor inteiro


def _linhas(saida: Path) -> list[dict]:
    return [
        json.loads(ln)
        for ln in (saida / "entradas.jsonl").read_text(encoding="utf-8").splitlines()
    ]


def test_selftest_do_modulo_passa():
    assert cr.selftest() == 0


def test_positivo_sintetico_vira_positivo_com_o_arquivo_de_antes_e_negativos_certos(
    tmp_path: Path,
):
    saida = tmp_path / "saida"
    rel = cr.cenario(tmp_path, saida, True)
    assert cr.falhas_do_cenario(saida, rel) == []
    pos = [e for e in _linhas(saida) if e["rotulo"] == "positivo"]
    assert len(pos) == 1
    p = pos[0]
    assert (p["classe"], p["nome_esperado"], p["reproduz"], p["lang"], p["origem"]) == (
        "nome",
        "inventada",
        "true",
        "rust",
        "atribuido",
    )
    assert p["mensagem"].startswith("error[E0425]: cannot find function `inventada`")
    assert p["contexto"] == [] and p["snap"] == f"snap/{p['id']}.rs"
    assert p["snippet_confere"] is True
    manifesto = json.loads(
        (saida / "estado_no_instante/manifesto.json").read_text(encoding="utf-8")
    )
    assert manifesto[p["id"]]["overlays"] == []
    assert (
        saida / manifesto[p["id"]]["disk_file"]
    ).read_text() == cr.BASE_LIB + "pub mod viva;\n"


def test_mutante_sem_confirmacao_textual_deixa_as_iscas_entrarem(tmp_path: Path):
    saida = tmp_path / "saida"
    rel = cr.cenario(tmp_path, saida, False)
    falhas = cr.falhas_do_cenario(saida, rel)
    assert falhas, "desligar o oráculo textual tem de reprovar o selftest"
    assert any("positivos esperados" in f for f in falhas)


def test_cada_isca_de_negativo_sai_pelo_motivo_certo(tmp_path: Path):
    saida = tmp_path / "saida"
    rel = cr.cenario(tmp_path, saida, True)
    neg = rel["negativos"]
    assert neg["n"] == 3
    assert neg["por_evidencia"] == {"rc0": 1, "visto": 2}
    barato, caro = (
        neg["recusados_nos_filtros_baratos"],
        neg["recusados_na_materializacao"],
    )
    assert (
        barato["o cargo seguinte terminou com erro"] == 2
    )  # e1 (o próprio positivo) e n2
    assert barato["outra edição do mesmo arquivo antes do cargo"] == 1  # n4a
    assert (
        barato["nenhum cargo verificável, em 30 min, compilou o crate da edição"] >= 3
    )  # n3 (45 min), n5 (-p outro), n9 (head)
    assert caro["arquivo fora da árvore de módulos (nenhum `mod` o declara)"] == 1  # n6
    assert (
        caro[
            "arquivo reescrito por Bash (patch que a dobra não reproduz) antes do cargo"
        ]
        == 1
    )  # n8


def test_descartados_do_positivo_sao_contados_por_motivo(tmp_path: Path):
    rel = cr.cenario(tmp_path, tmp_path / "saida", True)
    motivos = rel["positivos"]["descartados_por_motivo"]
    assert any(
        m.startswith("linha não bate") for m in motivos
    )  # isca com trecho do rustc diferente
    assert (
        motivos.get("nome ausente na linha") == 1
    )  # isca sem trecho e sem o nome na linha
    assert rel["positivos"]["n"] == 1


def test_duas_rodadas_sobre_a_mesma_entrada_dao_o_mesmo_sha256(tmp_path: Path):
    primeira, segunda = tmp_path / "um", tmp_path / "dois"
    cfg = cr.config_de_teste(tmp_path, primeira)
    cr.construir(cfg)
    cr.construir(
        cr.Config(
            cfg.transcripts,
            segunda,
            cfg.raizes,
            corte=cfg.corte,
            n_min=cfg.n_min,
            cap=cfg.cap,
        )
    )
    sha = [
        hashlib.sha256((d / "entradas.jsonl").read_bytes()).hexdigest()
        for d in (primeira, segunda)
    ]
    assert sha[0] == sha[1]
    manifestos = [
        (d / "estado_no_instante/manifesto.json").read_bytes()
        for d in (primeira, segunda)
    ]
    assert manifestos[0] == manifestos[1]


def test_universo_vazio_aborta_em_vez_de_devolver_zero(tmp_path: Path):
    vazio = tmp_path / "t"
    vazio.mkdir()
    cfg = cr.Config((("x", vazio),), tmp_path / "saida", (tmp_path,), corte="9999")
    with pytest.raises(SystemExit, match="universo vazio"):
        cr.construir(cfg)
    with pytest.raises(SystemExit, match="universo vazio"):
        cr.construir(
            cr.Config((("x", tmp_path / "nao-existe"),), tmp_path / "s", (tmp_path,))
        )


def test_corte_congela_o_universo(tmp_path: Path):
    cfg = cr.config_de_teste(tmp_path, tmp_path / "saida")
    antes_de_tudo = cr.Config(
        cfg.transcripts, cfg.saida, cfg.raizes, corte="2098-12-31T00:00:00.000Z"
    )
    with pytest.raises(
        SystemExit, match="universo vazio"
    ):  # nenhum registro anterior ao corte
        cr.construir(antes_de_tudo)


def test_gravar_remove_os_arquivos_de_uma_rodada_anterior_um_a_um(tmp_path: Path):
    saida = tmp_path / "saida"
    cfg = cr.config_de_teste(tmp_path, saida)
    cr.construir(cfg)
    snaps = sorted((saida / "snap").iterdir())
    assert snaps
    cr.gravar(saida, [])  # uma rodada que não produz nada
    assert not list((saida / "snap").iterdir())
    assert not list((saida / "estado_no_instante/antes").iterdir())
    assert (saida / "entradas.jsonl").read_text() == ""


@pytest.mark.skipif(cr.shutil.which("rustfmt") is None, reason="rustfmt ausente")
def test_snapshot_que_nao_parseia_nao_e_o_arquivo_real():
    assert cr.sintaxe_invalida("pub mod x;\nfn a() { let y = 1; }\n") == ""
    assert cr.sintaxe_invalida("fn a( {\n").startswith("snapshot não é Rust válido")
