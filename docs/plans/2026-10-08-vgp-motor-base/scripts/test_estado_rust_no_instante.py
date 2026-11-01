"""Estado Rust NO INSTANTE (HORUST, 10/10/2026) — TDD com fixtures SINTETICAS (nenhum dado de held-out, nenhum transcript real).

Por que: no corpus natural de Rust os `overlays` do manifesto sao VAZIOS. O motor le entao o disco de HOJE para todo modulo que
nao e o proprio arquivo — e `crate::m::f` ou `use super::*` apontam para OUTRO modulo cujo estado no instante nao esta la. Para
Rust o fecho e o CRATE (`crate::` alcanca qualquer modulo): a regra pre-registrada sobrepoe TODO `.rs` da arvore de modulos do
crate (a partir de lib.rs/main.rs/bin/tests/examples/benches/build.rs, com `mod x;`, `mod x/mod.rs` e `#[path]`) no conteudo do
INSTANTE (`git show <rev>:<rel>` + as edicoes da sessao ate o instante). Crate com qualquer modulo irreconstruivel e INCOMPLETO.
E a classe A: o snapshot reconstruido DEFINE o nome no escopo do uso mas o rustc disse «cannot find» — a reconstrucao diverge do
que o rustc compilou (provavel edicao via Bash que a dobra nao ve): positivo NAO REPRODUZIVEL.

O modulo sob teste vem de `$ESTADO_RUST_MODULO` (o mutante roda sobre uma COPIA); sem a variavel, o original ao lado.
"""

import importlib.util
import json
import os
import subprocess
import sys
from pathlib import Path

import pytest

SCRIPTS = Path(__file__).resolve().parent
sys.path.insert(0, str(SCRIPTS))  # a copia mutada importa `corpus_v0d`/`corpus_rust_natural` daqui
import corpus_rust_natural as crn  # noqa: E402

CAMINHO = Path(os.environ.get("ESTADO_RUST_MODULO", SCRIPTS / "estado_rust_no_instante.py"))
_spec = importlib.util.spec_from_file_location("estado_rust_sob_teste", CAMINHO)
ers = importlib.util.module_from_spec(_spec)
sys.modules["estado_rust_sob_teste"] = ers  # dataclasses resolve as anotacoes pelo modulo registrado
_spec.loader.exec_module(ers)

C = Path("/c")


class Falso:
    """Provedor em memoria: {caminho: texto}; `quebrados` existem mas a historia nao reconstroi; `suspeitos` foram reescritos por Bash."""

    def __init__(self, arquivos: dict[str, str], quebrados=(), suspeitos=()):
        self.a = {Path(k): v for k, v in arquivos.items()}
        self.quebrados = {Path(q) for q in quebrados}
        self.suspeitos = {Path(s) for s in suspeitos}

    def texto(self, p: Path):
        if p in self.quebrados:
            return None, "dobra_quebrada"
        if p not in self.a:
            return None, "sem_origem"
        return self.a[p], ("reescrito_por_bash" if p in self.suspeitos else "")

    def arquivos(self, d: Path):
        return sorted({p for p in (*self.a, *self.quebrados) if d in p.parents})


def rels(est, base=C) -> list[str]:
    return sorted(str(a.path.relative_to(base)) for a in est.arquivos)


def faltas(est) -> dict[str, str]:
    return {str(Path(f.path).relative_to(C)): f.motivo for f in est.faltas}


# --------------------------------------------------------------------------- limpar / declaracoes_mod


def test_limpar_apaga_comentarios_e_strings_e_preserva_os_offsets():
    t = 'let a = "mod x;"; // mod y;\n/* mod z; /* aninhado */ mod w; */ let b = r#"mod v;"#; let c = \'m\'; fn f<\'a>() {}\n'
    limpo = ers.limpar(t)
    assert len(limpo) == len(t) and limpo.count("\n") == t.count("\n")
    assert "mod" not in limpo
    assert "fn f<" in limpo and "let a" in limpo
    assert "{" not in ers.limpar("let a = '{'; let b = '}'; let c = '\\n'; let d = \"{\";")  # chaves em literais nao sobram
    assert "'a" in ers.limpar("fn f<'a>(x: &'a str) {}")  # lifetime nao e char literal


def test_declaracoes_mod_externas_inline_atributos_e_aninhamento():
    t = '''#![allow(dead_code)]
pub mod a;
mod b;
pub(crate) mod c;
#[cfg(test)]
mod tests;
#[path = "x_tests.rs"]
mod xt;
mod inline { pub fn f() {} mod fundo; }
// mod comentado;
const S: &str = "mod em_string;";
'''
    ds = {d.nome: d for d in ers.declaracoes_mod(t)}
    assert set(ds) == {"a", "b", "c", "tests", "xt", "inline", "fundo"}
    assert ds["a"].externo and ds["c"].externo and not ds["inline"].externo
    assert ds["xt"].path_attr == "x_tests.rs" and ds["tests"].path_attr is None and "cfg(test)" in ds["tests"].attrs
    assert ds["fundo"].aninhamento == ("inline",) and ds["a"].aninhamento == ()


# --------------------------------------------------------------------------- a arvore de modulos do crate


def test_arvore_basica_mod_rs_e_arquivo_com_filhos():
    arq = {
        "/c/src/lib.rs": "pub mod m;\nmod n;\n",
        "/c/src/m.rs": "pub fn f() {}\nmod sub;\n",  # m.rs (nao mod-rs): os filhos moram em src/m/
        "/c/src/m/sub.rs": "pub fn g() {}\n",
        "/c/src/n/mod.rs": "mod fundo;\npub fn h() {}\n",  # n/mod.rs: os filhos moram ao lado
        "/c/src/n/fundo.rs": "",
    }
    est = ers.estado_do_crate(C, Falso(arq))
    assert rels(est) == ["src/lib.rs", "src/m.rs", "src/m/sub.rs", "src/n/fundo.rs", "src/n/mod.rs"]
    assert {str(a.path.relative_to(C)): a.profundidade for a in est.arquivos} == {
        "src/lib.rs": 0, "src/m.rs": 1, "src/n/mod.rs": 1, "src/m/sub.rs": 2, "src/n/fundo.rs": 2}
    assert ers.motivo_incompleto(est) == (True, "ok")
    assert est.faltas == [] and est.raizes == ["src/lib.rs"] or est.raizes == [str(C / "src/lib.rs")]


def test_todas_as_raizes_do_crate_entram():
    arq = {
        "/c/src/lib.rs": "", "/c/src/main.rs": "mod cli;\n", "/c/src/cli.rs": "",
        "/c/src/bin/x.rs": "mod y;\n", "/c/src/bin/y.rs": "", "/c/src/bin/z/main.rs": "",
        "/c/tests/t.rs": "mod h;\n", "/c/tests/h.rs": "", "/c/examples/e.rs": "", "/c/benches/b.rs": "", "/c/build.rs": "",
    }
    est = ers.estado_do_crate(C, Falso(arq))
    assert rels(est) == sorted(p.removeprefix("/c/") for p in arq)
    assert ers.motivo_incompleto(est) == (True, "ok")


def test_raiz_declarada_no_manifesto_entra_por_extras_raiz():
    arq = {"/c/custom/entrada.rs": "mod a;\n", "/c/custom/a.rs": ""}
    est = ers.estado_do_crate(C, Falso(arq), extras_raiz=[C / "custom/entrada.rs"])
    assert rels(est) == ["custom/a.rs", "custom/entrada.rs"]


def test_orfao_nao_entra_e_nao_torna_o_crate_incompleto():
    arq = {"/c/src/lib.rs": "mod a;\n", "/c/src/a.rs": "", "/c/src/solto.rs": "fn x() {}\n"}
    est = ers.estado_do_crate(C, Falso(arq))
    assert rels(est) == ["src/a.rs", "src/lib.rs"]
    assert [str(Path(o).relative_to(C)) for o in est.orfaos] == ["src/solto.rs"]
    assert ers.motivo_incompleto(est) == (True, "ok")


def test_path_attr_e_relativo_ao_diretorio_do_arquivo_e_o_carregado_vira_mod_rs():
    arq = {
        "/c/src/lib.rs": '#[path = "x_tests.rs"]\nmod tests;\nmod m;\n#[path = "../shared/s.rs"]\nmod s;\n',
        "/c/src/x_tests.rs": "mod deep;\n",  # carregado por #[path]: age como mod-rs -> filhos em src/
        "/c/src/deep.rs": "",
        "/c/src/m.rs": '#[path = "m_tests.rs"]\nmod tests;\n',  # relativo a src/ (onde m.rs mora), nao a src/m/
        "/c/src/m_tests.rs": "",
        "/c/shared/s.rs": "",
    }
    est = ers.estado_do_crate(C, Falso(arq))
    assert rels(est) == ["shared/s.rs", "src/deep.rs", "src/lib.rs", "src/m.rs", "src/m_tests.rs", "src/x_tests.rs"]


def test_modulo_inline_aninha_o_diretorio_dos_filhos():
    arq = {
        "/c/src/lib.rs": "mod a { mod b; }\nmod m;\n",
        "/c/src/a/b.rs": "",
        "/c/src/m.rs": 'mod a { #[path = "p.rs"] mod q; mod r; }\n',
        "/c/src/m/a/p.rs": "",
        "/c/src/m/a/r.rs": "",
    }
    est = ers.estado_do_crate(C, Falso(arq))
    assert rels(est) == ["src/a/b.rs", "src/lib.rs", "src/m.rs", "src/m/a/p.rs", "src/m/a/r.rs"]


def test_ciclo_por_path_attr_termina():
    arq = {"/c/src/lib.rs": '#[path = "a.rs"]\nmod a;\n', "/c/src/a.rs": '#[path = "lib.rs"]\nmod l;\n'}
    est = ers.estado_do_crate(C, Falso(arq))
    assert rels(est) == ["src/a.rs", "src/lib.rs"] and len(est.arquivos) == 2


def test_modulo_declarado_sem_arquivo_deixa_o_crate_incompleto():
    est = ers.estado_do_crate(C, Falso({"/c/src/lib.rs": "mod z;\n"}))
    assert faltas(est) == {"src/z.rs": "mod_sem_arquivo"} or faltas(est) == {"src/z/mod.rs": "mod_sem_arquivo"}
    ok, motivo = ers.motivo_incompleto(est)
    assert ok is False and motivo == ers.MOTIVO_INCOMPLETO["mod_sem_arquivo"] and motivo.startswith("estado incompleto: ")


def test_historia_quebrada_vence_a_ausencia_entre_os_candidatos_de_um_mod():
    arq = {"/c/src/lib.rs": "mod m;\n"}
    est = ers.estado_do_crate(C, Falso(arq, quebrados=["/c/src/m.rs"]))
    assert list(faltas(est).values()) == ["dobra_quebrada"]
    assert ers.motivo_incompleto(est) == (False, ers.MOTIVO_INCOMPLETO["dobra_quebrada"])


def test_arquivo_reescrito_por_bash_deixa_o_crate_incompleto_e_nao_vira_overlay():
    arq = {"/c/src/lib.rs": "mod m;\n", "/c/src/m.rs": "pub fn f() {}\n"}
    est = ers.estado_do_crate(C, Falso(arq, suspeitos=["/c/src/m.rs"]))
    assert faltas(est) == {"src/m.rs": "reescrito_por_bash"} and rels(est) == ["src/lib.rs"]
    assert ers.motivo_incompleto(est) == (False, ers.MOTIVO_INCOMPLETO["reescrito_por_bash"])


def test_crate_sem_raiz_no_instante_e_incompleto():
    est = ers.estado_do_crate(C, Falso({"/c/src/solto.rs": ""}))
    assert [f.motivo for f in est.faltas] == ["sem_raiz"]
    assert ers.motivo_incompleto(est) == (False, ers.MOTIVO_INCOMPLETO["sem_raiz"])


def test_entrada_fora_da_arvore_de_modulos_e_incompleta():
    """O rustc so reporta erro em arquivo que compilou, logo ele ESTAVA na arvore: se a reconstrucao nao o alcanca, ela diverge."""
    arq = {"/c/src/lib.rs": "mod a;\n", "/c/src/a.rs": "", "/c/src/solto.rs": ""}
    est = ers.estado_do_crate(C, Falso(arq), proprio="/c/src/solto.rs")
    assert [f.motivo for f in est.faltas] == ["entrada_fora_da_arvore"]
    assert ers.motivo_incompleto(est) == (False, ers.MOTIVO_INCOMPLETO["entrada_fora_da_arvore"])
    assert ers.motivo_incompleto(ers.estado_do_crate(C, Falso(arq), proprio="/c/src/a.rs")) == (True, "ok")


def test_prioridade_entre_motivos():
    arq = {"/c/src/lib.rs": "mod m;\nmod n;\nmod z;\n", "/c/src/n.rs": ""}
    est = ers.estado_do_crate(C, Falso(arq, quebrados=["/c/src/m.rs"]))
    assert {f.motivo for f in est.faltas} == {"dobra_quebrada", "mod_sem_arquivo"}
    assert ers.motivo_incompleto(est)[1] == ers.MOTIVO_INCOMPLETO["dobra_quebrada"]


def test_overlays_do_crate_excluem_a_propria_entrada_e_carregam_o_conteudo_do_instante():
    arq = {"/c/src/lib.rs": "mod m;\n", "/c/src/m.rs": "pub fn f() {}\n"}
    est = ers.estado_do_crate(C, Falso(arq))
    ov = ers.overlays_do_crate(est, "/c/src/lib.rs")
    assert [(str(p), t) for p, t in ov] == [("/c/src/m.rs", "pub fn f() {}\n")]
    assert len(ers.overlays_do_crate(est, None)) == 2


def test_materializar_grava_os_modulos_no_formato_do_manifesto(tmp_path):
    arq = {"/c/src/lib.rs": "mod m;\n", "/c/src/m.rs": "pub fn f() {}\n"}
    est = ers.estado_do_crate(C, Falso(arq))
    destino = tmp_path / "corpus" / "estado_no_instante"
    destino.mkdir(parents=True)
    over = ers.materializar(est, "/c/src/lib.rs", destino, "eid1")
    assert [(o["path"], o["sha256"]) for o in over] == [("/c/src/m.rs", ers.sha("pub fn f() {}\n"))]
    assert (destino.parent / over[0]["file"]).read_text() == "pub fn f() {}\n"
    assert over[0]["file"].startswith("estado_no_instante/modulos/eid1/") and over[0]["file"].endswith(".rs")


# --------------------------------------------------------------------------- classe A: o snapshot DEFINE o nome no escopo do uso


def uso(codigo: str) -> tuple[str, int, int]:
    """O codigo com `@@` na posicao do uso -> (texto sem o marcador, linha, coluna) 1-based, como o rustc imprime."""
    i = codigo.index("@@")
    texto = codigo.replace("@@", "", 1)
    return texto, texto.count("\n", 0, i) + 1, i - (texto.rfind("\n", 0, i) + 1) + 1


def A(codigo: str, nome: str) -> str | None:
    texto, linha, coluna = uso(codigo)
    return ers.define_no_escopo(texto, linha, coluna, nome)


@pytest.mark.parametrize("decl", ["fn alvo() {}", "pub(crate) fn alvo() {}", "async fn alvo() {}", "struct alvo;", "enum alvo { A }",
                                  "trait alvo {}", "type alvo = u32;", "const alvo: u32 = 1;", "static alvo: u32 = 1;", "mod alvo {}",
                                  "union alvo { a: u32 }", "macro_rules! alvo { () => {} }"])
def test_item_declarado_no_modulo_do_uso_e_classe_a(decl):
    assert (A(f"fn principal() {{ @@alvo(); }}\n{decl}\n", "alvo") or "").startswith("item")


def test_use_traz_o_nome_para_o_modulo():
    for uso_ in ("use a::b::alvo;", "use a::{x, y::alvo};", "use a::b as alvo;", "use a::alvo::{self, c};", "pub use a::{b, c as alvo};", "extern crate alvo;"):
        assert (A(f"{uso_}\nfn principal() {{ @@alvo(); }}\n", "alvo") or "").startswith("use"), uso_


def test_o_nome_so_em_modulo_irmao_ou_pai_sem_glob_nao_e_classe_a():
    assert A("mod outro { pub fn alvo() {} }\nmod tests { fn principal() { @@alvo(); } }\n", "alvo") is None  # irmao
    assert A("fn alvo() {}\nmod tests { fn principal() { @@alvo(); } }\n", "alvo") is None  # pai sem `use super::*`
    assert A("fn alvo() {}\nmod tests { use super::*;\nfn principal() { @@alvo(); } }\n", "alvo") is None  # classe C, nao A


def test_item_no_mesmo_mod_inline_do_uso_e_classe_a():
    assert (A("mod tests { fn principal() { @@alvo(); }\nfn alvo() {} }\n", "alvo") or "").startswith("item")


def test_nome_definido_em_outra_funcao_ou_so_em_comentario_ou_string_nao_e_classe_a():
    assert A("fn outra() { fn alvo() {} }\nfn principal() { @@alvo(); }\n", "alvo") is None
    assert A("// ; fn alvo() {}\n/* ; fn alvo() {} */\nconst S: &str = \"; fn alvo() {}\";\nfn principal() { @@alvo(); }\n", "alvo") is None


def test_parametro_da_funcao_e_classe_a():
    assert (A("fn g(x: u32, mut alvo: u32) -> u32 { @@alvo }\n", "alvo") or "").startswith("parâmetro")
    assert (A("fn g((a, alvo): (u32, u32)) -> u32 { @@alvo }\n", "alvo") or "").startswith("parâmetro")
    assert A("fn g(x: u32) -> u32 { @@alvo }\nfn h(alvo: u32) {}\n", "alvo") is None  # parametro de OUTRA fn


def test_let_anterior_no_bloco_e_classe_a_e_o_posterior_ou_de_bloco_fechado_nao():
    assert (A("fn g() { let mut alvo = 1; @@alvo += 1; }\n", "alvo") or "").startswith("let")
    assert (A("fn g() { let (a, alvo) = (1, 2); if a > 0 { @@alvo; } }\n", "alvo") or "").startswith("let")
    assert A("fn g() { @@alvo; let alvo = 1; }\n", "alvo") is None  # depois do uso
    assert A("fn g() { { let alvo = 1; }\n @@alvo; }\n", "alvo") is None  # bloco ja fechado


def test_binding_de_padrao_do_bloco_que_envolve_o_uso():
    assert (A("fn g(x: Option<u32>) { if let Some(alvo) = x { @@alvo; } }\n", "alvo") or "").startswith("binding")
    assert (A("fn g(xs: Vec<u32>) { for alvo in xs { @@alvo; } }\n", "alvo") or "").startswith("binding")
    assert (A("fn g(x: Option<u32>) { match x { Some(alvo) => { @@alvo; } None => {} } }\n", "alvo") or "").startswith("binding")
    assert A("fn g(x: Option<u32>) { if let Some(alvo) = x { alvo; }\n @@alvo; }\n", "alvo") is None  # fora do bloco do `if let`


def test_parametro_generico_e_classe_a():
    assert (A("fn g<T: Clone>(x: T) -> @@T { x }\n", "T") or "").startswith("parâmetro genérico")


def test_posicao_com_caractere_nao_ascii_antes_da_coluna():
    assert (A("const S: &str = \"ç ã é\"; fn principal() { let _x = \"ñ\"; @@alvo(); }\nfn alvo() {}\n", "alvo") or "").startswith("item")


def test_uso_qualificado_por_caminho_nao_e_classe_a():
    """`crate::m::alvo()` (rustc: «cannot find function `alvo` in module `crate::m`»): ele procura em `m`, nao no escopo do uso,
    entao o `alvo` do proprio modulo nao e o que falta — achado na medicao do HORUST (2 dos 8 «A» eram assim)."""
    assert A("fn principal() { crate::m::@@alvo(); }\nfn alvo() {}\n", "alvo") is None
    assert A("fn principal() { Foo ::\n @@alvo(); }\nfn alvo() {}\n", "alvo") is None
    assert (A("fn principal() { @@alvo(); }\nfn alvo() {}\n", "alvo") or "").startswith("item")  # controle: o mesmo uso sem o caminho e A


def test_metodo_de_impl_ou_trait_nao_e_escopo_lexico():
    assert A("struct S;\nimpl S { fn alvo(&self) {}\nfn g(&self) { @@alvo(); } }\n", "alvo") is None
    assert A("trait T { fn alvo(&self);\nfn g(&self) { @@alvo(); } }\n", "alvo") is None


def test_chaves_em_char_literal_e_em_string_nao_abrem_bloco():
    assert "no módulo" in (A("fn principal() { let c = '{'; let s = \"}\"; @@alvo(); }\nfn alvo() {}\n", "alvo") or "")  # item do modulo, nao «local»


def test_nome_que_nao_e_o_do_uso_nao_confunde():
    assert A("fn alvo_longo() {}\nfn principal() { @@alvo(); }\n", "alvo") is None  # prefixo nao e o nome


# --------------------------------------------------------------------------- integracao: git real + o Reconstrutor do gerador

D1, D3 = "2026-01-01T00:00:00Z", "2026-03-01T00:00:00Z"
T = "2026-02-01T00:00:00.000Z"  # o instante: entre o commit 1 e o commit 2
T_SESSAO = "2026-01-15T00:00:00.000Z"  # uma edicao da sessao antes de T
T_DEPOIS = "2026-02-15T00:00:00.000Z"  # uma edicao da sessao depois de T


def git(raiz: Path, *args: str, data: str | None = None) -> None:
    env = {**os.environ, "GIT_AUTHOR_NAME": "t", "GIT_AUTHOR_EMAIL": "t@t", "GIT_COMMITTER_NAME": "t", "GIT_COMMITTER_EMAIL": "t@t"}
    if data:
        env["GIT_AUTHOR_DATE"] = env["GIT_COMMITTER_DATE"] = data
    subprocess.run(["git", "-C", str(raiz), *args], check=True, capture_output=True, env=env)


def escrever(raiz: Path, arquivos: dict[str, str]) -> None:
    for rel, texto in arquivos.items():
        (raiz / rel).parent.mkdir(parents=True, exist_ok=True)
        (raiz / rel).write_text(texto)


def repo(tmp_path: Path, antes: dict[str, str], hoje: dict[str, str], apagar: tuple[str, ...] = ()) -> Path:
    """Commit 1 (D1) com `antes`; depois o disco de HOJE = `hoje` menos `apagar` (commit 2 em D3)."""
    r = tmp_path / "proj"
    r.mkdir()
    git(r, "init", "-q")
    escrever(r, {"Cargo.toml": '[package]\nname = "cratex"\nversion = "0.1.0"\n', **antes})
    git(r, "add", "-A")
    git(r, "commit", "-q", "-m", "base", data=D1)
    escrever(r, hoje)
    for rel in apagar:
        (r / rel).unlink()
    git(r, "add", "-A")
    git(r, "commit", "-q", "--allow-empty", "-m", "hoje", data=D3)
    return r


def reg(ts: str, bloco: dict) -> dict:
    return {"timestamp": ts, "message": {"content": [bloco]}}


def edit(ts: str, tid: str, caminho: Path, velho: str, novo: str) -> dict:
    return reg(ts, {"type": "tool_use", "name": "Edit", "id": tid, "input": {"file_path": str(caminho), "old_string": velho, "new_string": novo}})


def write(ts: str, tid: str, caminho: Path, conteudo: str) -> dict:
    return reg(ts, {"type": "tool_use", "name": "Write", "id": tid, "input": {"file_path": str(caminho), "content": conteudo}})


def estado(r: Path, idx: dict | None = None, proprio: Path | None = None):
    rec = crn.Reconstrutor({str(k): v for k, v in (idx or {}).items()}, set())
    prov = ers.ProvedorReconstrutor(rec, T)
    return ers.estado_do_crate(r, prov, proprio=str(proprio) if proprio else None)


def test_crate_m_f_com_f_so_no_instante_o_overlay_e_o_do_instante_nao_o_de_hoje(tmp_path):
    r = repo(tmp_path, {"src/lib.rs": "pub mod m;\n", "src/m.rs": "pub fn f() -> u32 { 1 }\n"},
             {"src/m.rs": "pub fn g() -> u32 { 2 }\n"})
    est = estado(r, proprio=r / "src/lib.rs")
    ov = {str(p.relative_to(r)): t for p, t in ers.overlays_do_crate(est, str(r / "src/lib.rs"))}
    assert ov == {"src/m.rs": "pub fn f() -> u32 { 1 }\n"}
    assert (r / "src/m.rs").read_text() == "pub fn g() -> u32 { 2 }\n"  # o disco de hoje e outro
    assert ers.motivo_incompleto(est) == (True, "ok")


def test_edicao_da_sessao_antes_do_instante_entra_e_depois_do_instante_nao(tmp_path):
    r = repo(tmp_path, {"src/lib.rs": "pub mod m;\n", "src/m.rs": "pub fn f() -> u32 { 1 }\n"}, {"src/m.rs": "pub fn g() -> u32 { 2 }\n"})
    m = r / "src/m.rs"
    idx = {m: [edit(T_SESSAO, "e1", m, "{ 1 }", "{ 11 }"), edit(T_DEPOIS, "e2", m, "{ 11 }", "{ 111 }")]}
    ov = {str(p.relative_to(r)): t for p, t in ers.overlays_do_crate(estado(r, idx), str(r / "src/lib.rs"))}
    assert ov == {"src/m.rs": "pub fn f() -> u32 { 11 }\n"}


def test_use_super_glob_com_o_pai_alterado_depois_o_overlay_do_pai_e_o_do_instante(tmp_path):
    antes = {"src/lib.rs": "mod m;\n", "src/m/mod.rs": "mod filho;\npub fn helper() -> u32 { 1 }\n",
             "src/m/filho.rs": "use super::*;\npub fn usa() -> u32 { helper() }\n"}
    r = repo(tmp_path, antes, {"src/m/mod.rs": "mod filho;\npub fn ajuda() -> u32 { 1 }\n"})
    est = estado(r, proprio=r / "src/m/filho.rs")
    ov = {str(p.relative_to(r)): t for p, t in ers.overlays_do_crate(est, str(r / "src/m/filho.rs"))}
    assert set(ov) == {"src/lib.rs", "src/m/mod.rs"} and "pub fn helper()" in ov["src/m/mod.rs"] and "ajuda" not in ov["src/m/mod.rs"]


def test_modulo_criado_por_write_da_sessao_entra_no_crate(tmp_path):
    r = repo(tmp_path, {"src/lib.rs": "pub mod m;\n", "src/m.rs": "pub fn f() {}\n"}, {})
    lib, extra = r / "src/lib.rs", r / "src/extra.rs"
    idx = {lib: [edit(T_SESSAO, "e1", lib, "pub mod m;\n", "pub mod m;\npub mod extra;\n")], extra: [write(T_SESSAO, "w1", extra, "pub fn e() {}\n")]}
    est = estado(r, idx)
    assert sorted(str(a.path.relative_to(r)) for a in est.arquivos) == ["src/extra.rs", "src/lib.rs", "src/m.rs"]
    assert ers.motivo_incompleto(est) == (True, "ok")


def test_modulo_declarado_mas_criado_so_por_bash_deixa_o_crate_incompleto(tmp_path):
    r = repo(tmp_path, {"src/lib.rs": "pub mod m;\npub mod z;\n", "src/m.rs": ""}, {"src/z.rs": "pub fn z() {}\n"})
    # `z.rs` so existe no commit 2 (depois do instante): no instante nao ha commit nem Write que o crie
    est = estado(r)
    assert ers.motivo_incompleto(est) == (False, ers.MOTIVO_INCOMPLETO["mod_sem_arquivo"])


def test_edicao_que_nao_casa_com_a_base_quebra_a_dobra_e_o_crate_fica_incompleto(tmp_path):
    r = repo(tmp_path, {"src/lib.rs": "pub mod m;\n", "src/m.rs": "pub fn f() {}\n"}, {})
    m = r / "src/m.rs"
    est = estado(r, {m: [edit(T_SESSAO, "e1", m, "texto que nao existe", "x")]})
    assert ers.motivo_incompleto(est) == (False, ers.MOTIVO_INCOMPLETO["dobra_quebrada"])


def test_arquivo_criado_por_write_e_depois_quebrado_e_dobra_quebrada_nao_sem_origem(tmp_path):
    r = repo(tmp_path, {"src/lib.rs": "pub mod extra;\n"}, {})
    lib, extra = r / "src/lib.rs", r / "src/extra.rs"
    idx = {extra: [write(T_SESSAO, "w1", extra, "pub fn e() {}\n"), edit("2026-01-16T00:00:00.000Z", "e1", extra, "texto que nao existe", "x")]}
    assert ers.motivo_incompleto(estado(r, idx)) == (False, ers.MOTIVO_INCOMPLETO["dobra_quebrada"])


def test_raiz_que_existia_no_instante_e_foi_apagada_depois_entra_no_crate(tmp_path):
    r = repo(tmp_path, {"src/lib.rs": "", "tests/velho.rs": "fn t() {}\n"}, {}, apagar=("tests/velho.rs",))
    assert not (r / "tests/velho.rs").exists()
    est = estado(r)
    assert sorted(str(a.path.relative_to(r)) for a in est.arquivos) == ["src/lib.rs", "tests/velho.rs"]


def test_provedor_marca_como_suspeito_o_arquivo_que_o_predicado_acusa(tmp_path):
    r = repo(tmp_path, {"src/lib.rs": "pub mod m;\n", "src/m.rs": "pub fn f() {}\n"}, {})
    rec = crn.Reconstrutor({}, set())
    prov = ers.ProvedorReconstrutor(rec, T, suspeito=lambda p, lt: p.endswith("m.rs"))
    est = ers.estado_do_crate(r, prov)
    assert ers.motivo_incompleto(est) == (False, ers.MOTIVO_INCOMPLETO["reescrito_por_bash"])


def test_raiz_criada_so_pela_sessao_entra_no_crate(tmp_path):
    r = repo(tmp_path, {"src/lib.rs": ""}, {})
    novo = r / "tests/novo.rs"
    idx = {novo: [write(T_SESSAO, "w1", novo, "fn t() {}\n")]}
    assert sorted(str(a.path.relative_to(r)) for a in estado(r, idx).arquivos) == ["src/lib.rs", "tests/novo.rs"]


def test_extras_de_raiz_do_manifesto_do_crate(tmp_path):
    r = repo(tmp_path, {"custom/entrada.rs": "mod a;\n", "custom/a.rs": ""}, {})
    (r / "Cargo.toml").write_text('[package]\nname = "cratex"\n[lib]\npath = "custom/entrada.rs"\n')
    assert ers.raizes_do_manifesto(r) == [r / "custom/entrada.rs"]


def test_selftest_do_modulo():
    assert ers.sha("a") == ers.sha("a") and ers.sha("a") != ers.sha("b")
    assert json.loads(json.dumps({"ok": True}))["ok"]
