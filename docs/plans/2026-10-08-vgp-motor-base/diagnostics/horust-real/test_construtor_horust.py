"""HORUST-REAL — construtor (TDD): leitura da base (artefatos, dep-info, pacotes limpos), inventário, plano determinístico das tentativas,
execução pela rodada e o corpus escrito. A parte de ponta a ponta usa um workspace SINTÉTICO de verdade (cargo real, sem dependências).
O módulo sob teste vem de `$HORUST_SRC_DIR` (os mutantes rodam sobre uma cópia).
"""

import collections
import json
import os
import shutil
import subprocess
import sys
from pathlib import Path

import pytest

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(Path(os.environ.get("HORUST_SRC_DIR", HERE))))
import construtor_horust as ch  # noqa: E402
import operadores_rust as op  # noqa: E402
import oraculo_cargo_json as oc  # noqa: E402

SEMENTE = "horust-real-20261010"


# ------------------------------------------------------------------ dep-info


def test_dep_info_lista_os_arquivos_lidos_em_caminho_relativo(tmp_path):
    raiz = tmp_path / "wt"
    d = (
        f"{raiz}/target/debug/deps/libx-abc.rmeta: crates/x/src/lib.rs {raiz}/crates/x/src/a.rs crates/x/src/b/mod.rs\n"
        "crates/x/src/lib.rs:\n/outro/fora/registry/src/foo.rs:\n"
    )
    assert ch.ler_dep_info(d, raiz) == {"crates/x/src/lib.rs", "crates/x/src/a.rs", "crates/x/src/b/mod.rs"}


def test_dep_info_ignora_o_que_nao_e_fonte_de_crate():
    d = "t/libx.rmeta: crates/x/build.rs crates/x/src/lib.rs vendor/y/src/lib.rs crates/x/tests/t.rs\n"
    assert ch.ler_dep_info(d, Path("/wt")) == {"crates/x/src/lib.rs"}


# ------------------------------------------------------------------ base


def artefato(src, nome, kind, pkg_dir):
    return json.dumps({"reason": "compiler-artifact", "target": {"kind": [kind], "name": nome, "src_path": src}, "filenames": [f"/t/lib{nome}-h.rmeta"], "fresh": False})


def erro(src):
    return json.dumps({"reason": "compiler-message", "target": {"kind": ["lib"], "name": "ruim", "src_path": src}, "message": {"level": "error", "message": "x", "spans": [{"is_primary": True, "file_name": "a.rs", "line_start": 1}]}})


METADADOS = {
    "bom": {"dir": Path("/wt/crates/bom"), "alvos": [("lib", "bom"), ("bin", "bom-cli")]},
    "ruim": {"dir": Path("/wt/crates/ruim"), "alvos": [("lib", "ruim")]},
    "faltou": {"dir": Path("/wt/crates/faltou"), "alvos": [("lib", "faltou")]},
}


def test_pacote_limpo_tem_todos_os_alvos_compilados_e_nenhum_erro():
    linhas = "\n".join([
        artefato("/wt/crates/bom/src/lib.rs", "bom", "lib", None),
        artefato("/wt/crates/bom/src/main.rs", "bom-cli", "bin", None),
        erro("/wt/crates/ruim/src/lib.rs"),
        artefato("/wt/crates/ruim/src/lib.rs", "ruim", "lib", None),
        json.dumps({"reason": "build-finished", "success": False}),
    ])
    limpos, motivos = ch.pacotes_limpos(linhas, METADADOS)
    assert limpos == {"bom"} and motivos["ruim"] == "erro" and motivos["faltou"] == "alvo_nao_compilado"


def test_pacote_com_alvo_faltando_nao_e_limpo():
    linhas = artefato("/wt/crates/bom/src/lib.rs", "bom", "lib", None)
    limpos, motivos = ch.pacotes_limpos(linhas, {"bom": METADADOS["bom"]})
    assert limpos == set() and motivos["bom"] == "alvo_nao_compilado"


def test_alvo_do_artefato_normaliza_proc_macro_para_lib():
    assert ch.alvo_de_kind(["proc-macro"]) == "lib" and ch.alvo_de_kind(["lib"]) == "lib" and ch.alvo_de_kind(["bin"]) == "bins"
    assert ch.alvo_de_kind(["custom-build"]) is None and ch.alvo_de_kind(["test"]) is None
    assert all(ch.alvo_de_kind(k) == "lib" for k in (["cdylib", "rlib"], ["cdylib"], ["staticlib"], ["dylib"], ["rlib"]))
    assert ch.alvo_de_kind(["example"]) is None and ch.alvo_de_kind(["bench"]) is None


# ------------------------------------------------------------------ plano

TEXTO_A = """use crate::tipos::Coisa;
pub fn principal(c: &Config) -> u32 {
    let total = calcula_total(c.valor, 3);
    let r = Coisa::nova(total).com_nome("a");
    r.contagem
}
pub fn monta() -> Config {
    Config {
        nome: String::from("a"),
        limite: 10,
        ativo: true,
    }
}
"""


def arquivos_sinteticos(n=12):
    out = {}
    for i in range(n):
        pkg = f"pkg{i % 3}"
        rel = f"crates/{pkg}/src/m{i}.rs"
        out[rel] = ch.Arquivo(rel, pkg, "lib", TEXTO_A.replace("principal", f"principal{i}"))
    return out


def tokens(arqs):
    t = {}
    for a in arqs.values():
        t.setdefault(a.pkg, set()).update(op.tokens(a.texto))
    return t


def test_plano_e_deterministico_e_nao_repete_arquivo():
    arqs = arquivos_sinteticos()
    a = ch.planejar(arqs, tokens(arqs), SEMENTE, set(), {"nome": 6, "kwarg": 6})
    b = ch.planejar(dict(reversed(list(arqs.items()))), tokens(arqs), SEMENTE, set(), {"nome": 6, "kwarg": 6})
    assert [t.inj.id for t in a] == [t.inj.id for t in b] and a
    assert len({t.inj.arq for t in a}) == len(a), "no máximo 1 injeção por arquivo, entre as classes também"


def test_plano_respeita_cota_ordem_de_classe_e_usados():
    arqs = arquivos_sinteticos(16)
    usados = {"crates/pkg0/src/m0.rs"}
    p = ch.planejar(arqs, tokens(arqs), SEMENTE, usados, {"kwarg": 4, "atributo": 4, "nome": 4})
    assert "crates/pkg0/src/m0.rs" not in {t.inj.arq for t in p}
    por = {}
    for t in p:
        por[t.inj.classe] = por.get(t.inj.classe, 0) + 1
    assert por == {"kwarg": 4, "atributo": 4, "nome": 4}


def test_plano_estilos_alternam_e_nome_novo_nao_existe_no_crate():
    arqs = arquivos_sinteticos(12)
    tk = tokens(arqs)
    p = ch.planejar(arqs, tk, SEMENTE, set(), {"nome": 8})
    assert {t.estilo for t in p} == {"near_miss", "plausivel"}
    assert all(t.nome_novo not in tk[t.pkg] for t in p)
    assert all(t.inj.texto_novo != t.inj.texto_original for t in p)


def test_plano_kwarg_remove_usa_o_tipo_como_nome_da_mensagem():
    arqs = arquivos_sinteticos(12)
    p = [t for t in ch.planejar(arqs, tokens(arqs), SEMENTE, set(), {"kwarg": 12}) if t.inj.operador == "kwarg_remove"]
    assert p and all(t.inj.nome == "Config" and t.inj.codigo == "E0063" for t in p)
    assert all(len(t.inj.texto_novo.splitlines()) == len(t.inj.texto_original.splitlines()) - 1 for t in p)


def test_plano_espalha_entre_pacotes():
    arqs = arquivos_sinteticos(12)
    p = ch.planejar(arqs, tokens(arqs), SEMENTE, set(), {"atributo": 6})
    assert {t.pkg for t in p} == {"pkg0", "pkg1", "pkg2"}


def test_plano_sem_sitio_nao_inventa():
    arqs = {"crates/p/src/a.rs": ch.Arquivo("crates/p/src/a.rs", "p", "lib", "fn a() {}\n")}
    assert ch.planejar(arqs, tokens(arqs), SEMENTE, set(), {"nome": 5, "kwarg": 5}) == []


def test_grupos_de_rodada_por_pacote_alvo_e_fase():
    arqs = arquivos_sinteticos(12)
    p = ch.planejar(arqs, tokens(arqs), SEMENTE, set(), {"nome": 6, "atributo": 6})
    grupos = ch.grupos_de_rodada(p)
    assert all(len({(t.pkg, t.alvo) for t in g}) == 1 for g in grupos.values())
    assert {k[2] for k in grupos} == {"R", "T"}
    assert sum(len(g) for g in grupos.values()) == len(p)


# ------------------------------------------------------------------ ponta a ponta com cargo de verdade

CARGO = shutil.which("cargo")

LIB = """pub mod soma;
pub fn principal(c: &Config) -> u32 {
    let total = calcula_total(c.valor, 3);
    total + c.limite
}
pub struct Config { pub valor: u32, pub limite: u32 }
fn calcula_total(a: u32, b: u32) -> u32 { a + b }
pub fn monta() -> Config {
    Config {
        valor: 1,
        limite: 10,
    }
}
"""
SOMA = """use crate::Config;
pub fn dobra(c: &Config) -> u32 {
    let r = c.valor * 2;
    r + c.limite
}
pub fn usa() -> u32 {
    let c = crate::monta();
    dobra(&c)
}
"""


@pytest.fixture()
def workspace(tmp_path):
    raiz = tmp_path / "wt"
    for nome in ("alfa", "beta", "gama"):
        d = raiz / "crates" / nome / "src"
        d.mkdir(parents=True)
        dep = '\n[dependencies]\nalfa = { path = "../alfa" }\n' if nome == "beta" else ""
        dep += '\n[dev-dependencies]\nbeta = { path = "../beta" }\n' if nome == "gama" else ""
        (raiz / "crates" / nome / "Cargo.toml").write_text(f'[package]\nname = "{nome}"\nversion = "0.1.0"\nedition = "2021"\n{dep}')
        (d / "lib.rs").write_text(LIB.replace("principal", f"principal_{nome}").replace("pub mod soma;", "pub mod soma;\npub mod m1;\npub mod m2;"))
        (d / "soma.rs").write_text(SOMA)
        for k in (1, 2):
            (d / f"m{k}.rs").write_text(SOMA.replace("dobra", f"dobra{k}").replace("usa", f"usa{k}"))
    (raiz / "Cargo.toml").write_text('[workspace]\nmembers = ["crates/alfa", "crates/beta", "crates/gama"]\nresolver = "2"\n')
    subprocess.run(["cargo", "generate-lockfile", "--offline"], cwd=raiz, check=True, capture_output=True)
    subprocess.run(["git", "init", "-q"], cwd=raiz, check=True)
    subprocess.run(["git", "-c", "user.email=a@b", "-c", "user.name=t", "add", "-A"], cwd=raiz, check=True)
    subprocess.run(["git", "-c", "user.email=a@b", "-c", "user.name=t", "commit", "-qm", "base"], cwd=raiz, check=True)
    return raiz


@pytest.mark.skipif(CARGO is None, reason="sem cargo")
def test_ponta_a_ponta_base_plano_rodada_e_restauracao(workspace, tmp_path):
    target = tmp_path / "target"
    env = {**os.environ, "CARGO_TARGET_DIR": str(target)}
    base = subprocess.run(
        ["cargo", "check", "--workspace", "--lib", "--bins", "--keep-going", "--message-format=json", "--locked", "--offline"],
        cwd=workspace, env=env, capture_output=True, text=True, check=True,
    )
    meta = ch.metadados(workspace, via_slot=False)
    limpos, _ = ch.pacotes_limpos(base.stdout, meta)
    assert limpos == {"alfa", "beta", "gama"}
    assert meta["beta"]["deps"] == {"alfa"} and meta["alfa"]["deps"] == set()
    assert meta["gama"]["deps"] == set(), "dev-dependency não entra no grafo de `cargo check --lib`"
    cam = ch.mapa_camadas(meta, {("alfa", "lib"), ("beta", "lib"), ("gama", "lib")})
    assert cam[("alfa", "lib")] != cam[("beta", "lib")], "beta depende de alfa: não vão no mesmo cargo"
    assert cam[("alfa", "lib")] == cam[("gama", "lib")], "alfa e gama são independentes: um cargo só"
    por_alvo = ch.arquivos_por_alvo(base.stdout, meta, workspace)
    assert {"crates/alfa/src/lib.rs", "crates/alfa/src/soma.rs"} <= por_alvo[("alfa", "lib")]
    arqs = {rel: ch.Arquivo(rel, pkg, alvo, (workspace / rel).read_text()) for (pkg, alvo), fs in por_alvo.items() for rel in fs}
    p = ch.planejar(arqs, tokens(arqs), SEMENTE, set(), {"nome": 4, "atributo": 4, "kwarg": 4, "import": 4})
    assert len(p) >= 6
    antes = {rel: (workspace / rel).read_bytes() for rel in arqs}
    diags, observados = {}, {}
    res = ch.executar(p, workspace, target, via_slot=False, offline=True, diags_out=diags, camadas=ch.mapa_camadas(meta, set(por_alvo)), observados_out=observados)
    assert set(observados) == set(res), "todo id tem o que o rustc disse, válido ou não"
    assert all(observados[i] and observados[i][0].arq for i, m in res.items() if m == "")
    assert set(res) == {t.inj.id for t in p}
    por_id = {t.inj.id: t.inj for t in p}
    assert set(diags) == {i for i, m in res.items() if m == ""}
    assert all(d.codigo == por_id[i].codigo and d.arq == por_id[i].arq for i, d in diags.items())
    validos = [i for i, m in res.items() if m == ""]
    assert len(validos) >= 3, res
    assert {rel: (workspace / rel).read_bytes() for rel in arqs} == antes, "restauração por sha"
    status = subprocess.run(["git", "status", "--porcelain"], cwd=workspace, capture_output=True, text=True).stdout
    assert status.strip() == "", "worktree limpo"
    # o controle negativo do oráculo: o mutado de um pacote só tem erro NAQUELE arquivo, e o original volta limpo
    t = p[0]
    with oc.aplicado(workspace, [t.inj]):
        s = oc.cargo_check(workspace, target, t.pkg, t.alvo, via_slot=False, offline=True)
        assert s.diags and {d.arq for d in s.diags} == {t.inj.arq}
    assert oc.cargo_check(workspace, target, t.pkg, t.alvo, via_slot=False, offline=True).diags == []


# ------------------------------------------------------------------ corpus


def test_escreve_corpus_com_o_formato_do_juiz(tmp_path):
    raiz = Path("/wt")
    pos = [ch.Positivo(
        id="aaaaaaaaaaaa", rel="crates/p/src/a.rs", pkg="p", classe="nome", operador="nome_valor", codigo="E0425", linha=3, coluna=9,
        nome_esperado="calcula_totel", nome_orig="calcula_total", estilo="near_miss", mensagem="cannot find value `calcula_totel` in this scope",
        texto_original="x\n", texto_novo="y\n",
    )]
    neg = [ch.Negativo(id="bbbbbbbbbbbb", rel="crates/p/src/b.rs", pkg="p", texto="z\n")]
    saida = tmp_path / "corpus"
    ch.escrever_corpus(saida, raiz, pos, neg, meta={"k": 1})
    linhas = [json.loads(x) for x in (saida / "entradas.jsonl").read_text().splitlines()]
    p, n = linhas
    assert (p["rotulo"], p["reproduz"], p["lang"], p["origem"], p["classe"]) == ("positivo", "true", "rust", "semeado", "nome")
    assert p["path"] == "/wt/crates/p/src/a.rs" and p["project_root"] == "/wt" and p["snap"] == "snap/aaaaaaaaaaaa.rs"
    assert p["mensagem"].startswith("error[E0425]: ") and p["nome_esperado"] == "calcula_totel" and p["contexto"] == "[]"
    assert (saida / "snap/aaaaaaaaaaaa.rs").read_text() == "y\n" and (saida / "base/aaaaaaaaaaaa.rs").read_text() == "x\n"
    assert (n["rotulo"], n["nome_esperado"], n["classe"], n["reproduz"]) == ("negativo", "", "", "false")
    assert not (saida / "base/bbbbbbbbbbbb.rs").exists() and (saida / "snap/bbbbbbbbbbbb.rs").read_text() == "z\n"
    assert json.loads((saida / "holdout_meta.json").read_text()) == {"k": 1}
    assert not (saida / "estado_no_instante").exists(), "régua estrita: sem manifesto"


def test_nao_sobrescreve_corpus_existente(tmp_path):
    saida = tmp_path / "corpus"
    saida.mkdir()
    (saida / "x").write_text("a")
    with pytest.raises(FileExistsError):
        ch.escrever_corpus(saida, Path("/wt"), [], [], meta={})


def test_mapa_camadas_separa_dependentes_e_junta_independentes_e_bins_ficam_juntos():
    meta = {"a": {"deps": {"b"}}, "b": {"deps": set()}, "c": {"deps": set()}, "d": {"deps": {"c"}}}
    nos = {("a", "lib"), ("b", "lib"), ("c", "lib"), ("d", "lib"), ("a", "bins"), ("d", "bins")}
    cam = ch.mapa_camadas(meta, nos)
    assert cam[("a", "lib")] != cam[("b", "lib")] and cam[("d", "lib")] != cam[("c", "lib")]
    assert cam[("a", "bins")] == cam[("d", "bins")], "bins não dependem uns dos outros: um cargo só"


def test_grupos_de_rodada_com_camadas_juntam_pacotes_independentes():
    arqs = arquivos_sinteticos(12)
    p = ch.planejar(arqs, tokens(arqs), SEMENTE, set(), {"nome": 6})
    cam = {(t.pkg, t.alvo): 0 for t in p}
    g = ch.grupos_de_rodada(p, cam)
    assert len(g) == 1 and sum(len(x) for x in g.values()) == len(p)
    assert len(ch.grupos_de_rodada(p)) > 1


def test_plano_com_filtro_de_sitio_e_cotas_por_operador_so_pega_o_que_foi_pedido():
    arqs = arquivos_sinteticos(16)
    p = ch.planejar(arqs, tokens(arqs), SEMENTE, set(), {}, operadores={"nome": {"nome_tipo": 5}, "import": {"import_use": 5}},
                    filtro_sitio=lambda s: s.operador != "import_use" or s.posicao == "penultimo")
    ops = collections.Counter(t.inj.operador for t in p)
    assert set(ops) <= {"nome_tipo", "import_use"} and ops["nome_tipo"] == 5
    assert all(t.nome_orig != "Coisa" for t in p if t.inj.operador == "import_use"), "só o penúltimo segmento (o módulo), nunca o nome importado"
    assert all(t.inj.aceitos == ("E0425", "E0412") for t in p if t.inj.operador == "nome_tipo")


def test_planejar_sem_operadores_continua_usando_as_cotas_por_classe():
    arqs = arquivos_sinteticos(12)
    a = ch.planejar(arqs, tokens(arqs), SEMENTE, set(), {"nome": 6})
    b = ch.planejar(arqs, tokens(arqs), SEMENTE, set(), {"nome": 6}, operadores=None, filtro_sitio=None)
    assert [t.inj.id for t in a] == [t.inj.id for t in b]
