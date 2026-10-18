//! TDD fixtures for the VGP checks — each test replicates a measured corpus positive (or a
//! measured negative, in the precision guards). The fixture project mirrors the shapes of the
//! analise codebase: `scripts/memoria/grafo_gravacao.py` (Recibo dataclass) and
//! `scripts/eleitoral/auditar_coerencia.py` (module constants).

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::vgp::{CheckInput, CheckOutput, Contexto, check_em};

/// Every fixture here measures what the adapter finds in the buckets where the code RUNS now
/// (`/tmp/vgp-t/a.py`, `<string>`), so they ask in the EXECUTION context. Written to `/tmp`, a
/// project symbol only advises (FRAG, 10/10/2026): `lang::tests::na_escrita_*` and
/// `politica::tests` pin that side; the expectations below are unchanged by it.
fn check(input: &CheckInput) -> CheckOutput {
    check_em(input, Contexto::Execucao)
}

fn projeto(tmp: &Path) -> PathBuf {
    let g = tmp.join("scripts/memoria");
    std::fs::create_dir_all(&g).unwrap();
    std::fs::write(
        g.join("grafo_gravacao.py"),
        "from dataclasses import dataclass\n\
         from scripts.memoria.grafo_modelo import No\n\
         @dataclass(frozen=True)\n\
         class Recibo:\n\
         \x20   chave: str\n\
         \x20   gravado: bool\n\
         \x20   arestas_criadas: int\n\
         \x20   total_na_leitura: int\n\
         def gravar(no: No) -> Recibo:\n\
         \x20   return Recibo('k', True, 1, 1)\n\
         class No:\n\
         \x20   tipo: str\n\
         \x20   slug: str\n",
    )
    .unwrap();
    let e = tmp.join("scripts/eleitoral");
    std::fs::create_dir_all(&e).unwrap();
    std::fs::write(
        e.join("auditar_coerencia.py"),
        "JUIZES_REAIS = ['design_html']\ndef auditar(x):\n    return x\n",
    )
    .unwrap();
    std::fs::write(e.join("__init__.py"), "").unwrap();
    std::fs::write(tmp.join("scripts/__init__.py"), "").unwrap();
    std::fs::write(tmp.join("scripts/memoria/__init__.py"), "").unwrap();
    tmp.to_path_buf()
}

fn input<'a>(
    root: PathBuf,
    path: &'a str,
    content: &'a str,
    overlays: HashMap<String, String>,
) -> CheckInput {
    CheckInput {
        path: path.to_string(),
        content: content.to_string(),
        project_root: root,
        disk: None,
        overlays,
        lang: "python".into(),
    }
}

fn denies(out: &crate::vgp::CheckOutput) -> Vec<(String, String)> {
    let mut v: Vec<(String, String)> = out
        .findings
        .iter()
        .filter(|f| f.severidade == "deny")
        .map(|f| (f.classe.clone(), f.nome.clone()))
        .collect();
    v.sort();
    v
}

// --------------------------------------------------------------------------- nome

#[test]
fn nome_ausente_nega_com_linha() {
    let tmp = std::env::temp_dir().join("vgp-t-nome");
    let _ = std::fs::remove_dir_all(&tmp);
    let root = projeto(&tmp);
    let out = check(&input(
        root,
        "/tmp/vgp-t/a.py",
        "print(cp.PASTA)\n",
        HashMap::new(),
    ));
    assert_eq!(denies(&out), vec![("nome".to_string(), "cp".to_string())]);
    let _ = std::fs::remove_dir_all(&tmp);
}

// --------------------------------------------------------------------------- import

#[test]
fn from_import_de_nome_inventado_nega_e_real_nao() {
    let tmp = std::env::temp_dir().join("vgp-t-import");
    let _ = std::fs::remove_dir_all(&tmp);
    let root = projeto(&tmp);
    let mau = "from scripts.memoria.grafo_gravacao import gravar_gravacao as g\n";
    let out = check(&input(root.clone(), "/tmp/vgp-t/a.py", mau, HashMap::new()));
    assert_eq!(
        denies(&out),
        vec![("import".to_string(), "gravar_gravacao".to_string())]
    );
    let bom = "from scripts.memoria.grafo_gravacao import gravar, Recibo\n";
    let out = check(&input(root, "/tmp/vgp-t/a.py", bom, HashMap::new()));
    assert_eq!(denies(&out), Vec::<(String, String)>::new());
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn from_import_sem_alias_nega_o_nome_inventado() {
    // (a) `from m import zzz` SEM alias — a gramática expõe cada nome como dotted_name no campo
    // `name` (o módulo é dotted_name no campo `module_name`); bug medido pela coordenadora:
    // o match por kind() pulava o dotted_name e o motor devolvia allow.
    let tmp = std::env::temp_dir().join("vgp-t-import-sem-alias");
    let _ = std::fs::remove_dir_all(&tmp);
    let root = projeto(&tmp);
    let mau = "from scripts.memoria.grafo_gravacao import zzz\n";
    let out = check(&input(root.clone(), "/tmp/vgp-t/a.py", mau, HashMap::new()));
    assert_eq!(
        denies(&out),
        vec![("import".to_string(), "zzz".to_string())]
    );
    let bom = "from scripts.memoria.grafo_gravacao import gravar\n";
    let out = check(&input(root, "/tmp/vgp-t/a.py", bom, HashMap::new()));
    assert_eq!(denies(&out), Vec::<(String, String)>::new());
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn from_import_lista_mista_nega_so_o_inventado() {
    // (b) `from m import a, zzz` — só o zzz nega; o real não.
    let tmp = std::env::temp_dir().join("vgp-t-import-misto");
    let _ = std::fs::remove_dir_all(&tmp);
    let root = projeto(&tmp);
    let src = "from scripts.memoria.grafo_gravacao import gravar, zzz\n";
    let out = check(&input(root, "/tmp/vgp-t/a.py", src, HashMap::new()));
    assert_eq!(
        denies(&out),
        vec![("import".to_string(), "zzz".to_string())]
    );
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn from_import_parenteses_multilinha_nega_so_o_inventado() {
    // (c) `from m import (a,\n    zzz)` — parênteses não mudam o campo `name`.
    let tmp = std::env::temp_dir().join("vgp-t-import-paren");
    let _ = std::fs::remove_dir_all(&tmp);
    let root = projeto(&tmp);
    let src = "from scripts.memoria.grafo_gravacao import (gravar,\n    zzz)\n";
    let out = check(&input(root, "/tmp/vgp-t/a.py", src, HashMap::new()));
    assert_eq!(
        denies(&out),
        vec![("import".to_string(), "zzz".to_string())]
    );
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn from_import_relativo_com_irmao_nega_o_inventado() {
    // (d) `from .m import zzz` em arquivo de /tmp com irmão m.py — resolve pelo irmão.
    let tmp = std::env::temp_dir().join("vgp-t-import-rel");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    std::fs::write(tmp.join("m.py"), "a = 1\nb = 2\n").unwrap();
    let prog = tmp.join("prog.py");
    let mau = "from .m import zzz\n";
    let out = check(&input(
        tmp.clone(),
        prog.to_str().unwrap(),
        mau,
        HashMap::new(),
    ));
    assert_eq!(
        denies(&out),
        vec![("import".to_string(), "zzz".to_string())]
    );
    let bom = "from .m import a, b\n";
    let out = check(&input(
        tmp.clone(),
        prog.to_str().unwrap(),
        bom,
        HashMap::new(),
    ));
    assert_eq!(denies(&out), Vec::<(String, String)>::new());
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn inferencia_autorreferente_nao_crasha_e_nao_nega() {
    // stack overflow medido no baseline v2 (58 chamadas; 1ª: jev_client.py): `line = line.strip()`
    // fazia a inferência seguir nome → atribuição → receptor → o MESMO nome sem visitados.
    let tmp = std::env::temp_dir().join("vgp-t-ciclo-inf");
    let _ = std::fs::remove_dir_all(&tmp);
    let root = projeto(&tmp);
    let src = "from pathlib import Path\nfor line in Path('/x').read_text().splitlines():\n    line = line.strip()\n    print(line)\n";
    let out = check(&input(root, "/tmp/vgp-t/a.py", src, HashMap::new()));
    assert_eq!(denies(&out), Vec::<(String, String)>::new());
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn inferencia_mutua_ciclica_nao_crasha_e_nao_nega() {
    // a = b / b = a — ciclo por dois nomes; unknown controlado, nunca crash nem deny inventado.
    let tmp = std::env::temp_dir().join("vgp-t-ciclo-mutuo");
    let _ = std::fs::remove_dir_all(&tmp);
    let root = projeto(&tmp);
    let src = "a = b\nb = a\nprint(a.qualquer)\n";
    let out = check(&input(root, "/tmp/vgp-t/a.py", src, HashMap::new()));
    assert_eq!(out.verdict.as_str(), "allow");
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn hierarquia_ciclica_de_classes_nao_crasha_nem_nega() {
    // overlay/instante quebrado pode conter A(B) e B(A) — members_of corta por profundidade e o
    // membro fica unknown (sem deny), nunca stack overflow.
    let tmp = std::env::temp_dir().join("vgp-t-ciclo-cls");
    let _ = std::fs::remove_dir_all(&tmp);
    let root = projeto(&tmp);
    std::fs::create_dir_all(root.join("scripts")).unwrap();
    std::fs::write(
        root.join("scripts/ciclo.py"),
        "class A(B):\n    pass\nclass B(A):\n    pass\n",
    )
    .unwrap();
    let src = "from scripts.ciclo import A\nx = A()\nprint(x.qq)\n";
    let out = check(&input(root, "/tmp/vgp-t/a.py", src, HashMap::new()));
    assert_eq!(denies(&out), Vec::<(String, String)>::new());
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn from_import_submodulo_fora_do_dunder_all_nao_nega() {
    // falso deny medido no corpus (`from defusedxml import ElementTree`): o __all__ do pacote não
    // lista o submódulo, e o module_has estreitava pelo __all__ ANTES de checar o submódulo —
    // mas `from pkg import submod` é legítimo independente de __all__ (que só governa import *).
    let tmp = std::env::temp_dir().join("vgp-t-dunder-all");
    let _ = std::fs::remove_dir_all(&tmp);
    let root = projeto(&tmp);
    std::fs::create_dir_all(root.join("scripts/pacote_all")).unwrap();
    std::fs::write(
        root.join("scripts/pacote_all/__init__.py"),
        "__all__ = [\"a\"]\na = 1\n",
    )
    .unwrap();
    std::fs::write(root.join("scripts/pacote_all/submod.py"), "X = 2\n").unwrap();
    let bom = "from scripts.pacote_all import submod\n";
    let out = check(&input(root.clone(), "/tmp/vgp-t/a.py", bom, HashMap::new()));
    assert_eq!(denies(&out), Vec::<(String, String)>::new());
    let mau = "from scripts.pacote_all import zzz\n";
    let out = check(&input(root, "/tmp/vgp-t/a.py", mau, HashMap::new()));
    assert_eq!(
        denies(&out),
        vec![("import".to_string(), "zzz".to_string())]
    );
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn from_import_de_modulo_irresoluvel_e_unknown_nunca_deny() {
    let tmp = std::env::temp_dir().join("vgp-t-import2");
    let _ = std::fs::remove_dir_all(&tmp);
    let root = projeto(&tmp);
    let out = check(&input(
        root,
        "/tmp/vgp-t/a.py",
        "from pacote.inexistente import algo\n",
        HashMap::new(),
    ));
    assert_eq!(denies(&out), Vec::<(String, String)>::new());
    let _ = std::fs::remove_dir_all(&tmp);
}

// --------------------------------------------------------------------------- atributo de módulo

#[test]
fn construtor_com_alias_tipa_a_instancia_pelo_nome_original() {
    // `from m import Recibo as R` — a instância x = R() herda os membros de Recibo (o lookup
    // por alias "R" na tabela do módulo falhava e a instância ficava sem tipo)
    let tmp = std::env::temp_dir().join("vgp-t-alias-ctor");
    let _ = std::fs::remove_dir_all(&tmp);
    let root = projeto(&tmp);
    let mau = "from scripts.memoria.grafo_gravacao import Recibo as R\nx = R('k', True, 1, 1)\nprint(x.inexistente)\n";
    let out = check(&input(root.clone(), "/tmp/vgp-t/a.py", mau, HashMap::new()));
    assert_eq!(
        denies(&out),
        vec![("membro".to_string(), "inexistente".to_string())]
    );
    let bom = "from scripts.memoria.grafo_gravacao import Recibo as R\nx = R('k', True, 1, 1)\nprint(x.chave)\n";
    let out = check(&input(root, "/tmp/vgp-t/a.py", bom, HashMap::new()));
    assert_eq!(denies(&out), Vec::<(String, String)>::new());
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn splat_tipado_kwargs_absorve_os_nomeados() {
    // falso deny medido (curl_cffi): `def get(url: str, **kwargs: Unpack[X])` — o splat tipado
    // é typed_parameter embrulhando dictionary_splat_pattern; sem has_kwargs, timeout/impersonate
    // viravam "argumento-nome inexistente"
    let tmp = std::env::temp_dir().join("vgp-t-typed-splat");
    let _ = std::fs::remove_dir_all(&tmp);
    let root = projeto(&tmp);
    std::fs::write(
        root.join("scripts/lib_http.py"),
        "def get(url: str, **kwargs: Unpack[Params]) -> int:\n    return 1\ndef post(url: str, *args: str) -> int:\n    return 2\n",
    )
    .unwrap();
    let bom = "from scripts.lib_http import get\nr = get('u', impersonate='chrome', timeout=30, allow_redirects=True)\n";
    let out = check(&input(root.clone(), "/tmp/vgp-t/a.py", bom, HashMap::new()));
    assert_eq!(denies(&out), Vec::<(String, String)>::new());
    let mau = "from scripts.lib_http import post\nr = post('u', method='GET')\n";
    let out = check(&input(root, "/tmp/vgp-t/a.py", mau, HashMap::new()));
    assert_eq!(
        denies(&out),
        vec![("kwarg".to_string(), "method".to_string())],
        "*args tipado não absorve kwargs"
    );
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn construtor_pontuado_pathlib_path_tipa() {
    // recall medido (st_m_size): `D = pathlib.Path(...)` — o construtor PONTUADO não era modelado
    // (só o `Path` nu), e a cadeia `D / 'x' → .stat() → .st_m_size` morria no primeiro elo
    let tmp = std::env::temp_dir().join("vgp-t-pathlib-ctor");
    let _ = std::fs::remove_dir_all(&tmp);
    let root = projeto(&tmp);
    let src = "import pathlib\nD = pathlib.Path('/tmp')\nsaida = D / 'saida.txt'\nprint(saida.stat().st_m_size)\n";
    let out = check(&input(root.clone(), "/tmp/vgp-t/a.py", src, HashMap::new()));
    let st = out.findings.iter().find(|f| f.nome == "st_m_size");
    assert!(
        st.is_some(),
        "st_m_size deve aparecer (forma → warn): {:?}",
        out.findings
    );
    assert_eq!(st.unwrap().severidade, "warn", "forma é warn sempre (P4)");
    let bom = "import pathlib\nD = pathlib.Path('/tmp')\nsaida = D / 'saida.txt'\nprint(saida.stat().st_size)\n";
    let out = check(&input(root, "/tmp/vgp-t/a.py", bom, HashMap::new()));
    assert_eq!(denies(&out), Vec::<(String, String)>::new());
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn reexport_segue_o_salto_e_nega_o_membro_inventado() {
    // `from grafo_memoria import gravar` — gravar é RE-EXPORT (grafo_memoria o importa de
    // grafo_gravacao): a cadeia de tipagem tem de seguir o salto (2 perdidos `total` no corpus)
    let tmp = std::env::temp_dir().join("vgp-t-reexport");
    let _ = std::fs::remove_dir_all(&tmp);
    let root = projeto(&tmp);
    std::fs::write(
        root.join("scripts/memoria/__init__.py"),
        "from scripts.memoria.grafo_gravacao import gravar, Recibo\n",
    )
    .unwrap();
    let mau =
        "from scripts.memoria import gravar\nno = None\nr = gravar(no)\nprint(r.inexistente)\n";
    let out = check(&input(root.clone(), "/tmp/vgp-t/a.py", mau, HashMap::new()));
    assert_eq!(
        denies(&out),
        vec![("membro".to_string(), "inexistente".to_string())]
    );
    let bom = "from scripts.memoria import gravar\nno = None\nr = gravar(no)\nprint(r.chave)\n";
    let out = check(&input(root, "/tmp/vgp-t/a.py", bom, HashMap::new()));
    assert_eq!(denies(&out), Vec::<(String, String)>::new());
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn kwarg_atraves_de_reexport_usa_a_assinatura_real() {
    let tmp = std::env::temp_dir().join("vgp-t-reexport-kw");
    let _ = std::fs::remove_dir_all(&tmp);
    let root = projeto(&tmp);
    std::fs::write(
        root.join("scripts/memoria/__init__.py"),
        "from scripts.memoria.grafo_gravacao import gravar\n",
    )
    .unwrap();
    let mau = "from scripts.memoria import gravar\nno = None\nr = gravar(no, tier='x')\n";
    let out = check(&input(root, "/tmp/vgp-t/a.py", mau, HashMap::new()));
    assert_eq!(
        denies(&out),
        vec![("kwarg".to_string(), "tier".to_string())]
    );
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn atributo_em_submodulo_importado_nega_com_o_nome_do_submodulo() {
    // R1 (74 atributo + 4 import do baseline): `from pkg import sub as s` liga o receptor ao
    // SUBMÓDULO pkg.sub — nunca ao pacote. Antes: "módulo `scripts.eleitoral` não tem atributo
    // `paginas_de_pdf`" para `corpus.paginas_de_pdf` com corpus = scripts.eleitoral.corpus_pesquisas.
    let tmp = std::env::temp_dir().join("vgp-t-submod-recv");
    let _ = std::fs::remove_dir_all(&tmp);
    let root = projeto(&tmp);
    std::fs::create_dir_all(root.join("scripts/pacote")).unwrap();
    std::fs::write(root.join("scripts/pacote/__init__.py"), "").unwrap();
    std::fs::write(root.join("scripts/pacote/submod.py"), "A = 1\n").unwrap();
    let mau = "from scripts.pacote import submod as sm\nprint(sm.B)\n";
    let out = check(&input(root.clone(), "/tmp/vgp-t/a.py", mau, HashMap::new()));
    assert_eq!(
        denies(&out),
        vec![("atributo".to_string(), "B".to_string())]
    );
    let achado = out.findings.iter().find(|f| f.nome == "B").unwrap();
    assert!(
        achado.sugestao.contains("scripts.pacote.submod"),
        "a mensagem nomeia o SUBMÓDULO: {}",
        achado.sugestao
    );
    let bom = "from scripts.pacote import submod as sm\nprint(sm.A)\n";
    let out = check(&input(root, "/tmp/vgp-t/a.py", bom, HashMap::new()));
    assert_eq!(denies(&out), Vec::<(String, String)>::new());
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn membro_importado_de_tipo_desconhecido_nao_nega() {
    // R1, outra metade: `from pkg import CONST` — CONST é MEMBRO de tipo desconhecido, nunca o
    // módulo pkg. Antes: `PECA.read_text` negava como "módulo não tem atributo" (PECA é Path).
    let tmp = std::env::temp_dir().join("vgp-t-membro-unknown");
    let _ = std::fs::remove_dir_all(&tmp);
    let root = projeto(&tmp);
    std::fs::create_dir_all(root.join("scripts/pacote2")).unwrap();
    std::fs::write(root.join("scripts/pacote2/__init__.py"), "CONST = 1\n").unwrap();
    let src = "from scripts.pacote2 import CONST\nprint(CONST.qq)\n";
    let out = check(&input(root, "/tmp/vgp-t/a.py", src, HashMap::new()));
    assert_eq!(
        denies(&out),
        Vec::<(String, String)>::new(),
        "membro de tipo desconhecido é unknown, nunca deny"
    );
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn atributo_em_funcao_importada_nega() {
    // recall medido no corpus (slides_fisica_eleitoral): `sfe.slides_fisica_eleitoral` com
    // sfe = a FUNÇÃO importada — AttributeError certo em runtime. Função não tem atributo solto.
    let tmp = std::env::temp_dir().join("vgp-t-func-recv");
    let _ = std::fs::remove_dir_all(&tmp);
    let root = projeto(&tmp);
    let mau = "from scripts.memoria.grafo_gravacao import gravar\nprint(gravar.bogus)\n";
    let out = check(&input(root.clone(), "/tmp/vgp-t/a.py", mau, HashMap::new()));
    assert_eq!(
        denies(&out),
        vec![("atributo".to_string(), "bogus".to_string())]
    );
    let bom = "from scripts.memoria.grafo_gravacao import gravar\nprint(gravar.__name__)\n";
    let out = check(&input(root, "/tmp/vgp-t/a.py", bom, HashMap::new()));
    assert_eq!(
        denies(&out),
        Vec::<(String, String)>::new(),
        "dunder é dinamico (warn), nunca deny"
    );
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn atributo_de_modulo_do_projeto_nega() {
    let tmp = std::env::temp_dir().join("vgp-t-attrmod");
    let _ = std::fs::remove_dir_all(&tmp);
    let root = projeto(&tmp);
    let src = "from scripts.eleitoral import auditar_coerencia as ac\nj = ac.JUIZES\n";
    let out = check(&input(root, "/tmp/vgp-t/a.py", src, HashMap::new()));
    assert_eq!(
        denies(&out),
        vec![("atributo".to_string(), "JUIZES".to_string())]
    );
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn atributo_de_modulo_stdlib_fechado_nega_combending() {
    let tmp = std::env::temp_dir().join("vgp-t-unic");
    let _ = std::fs::remove_dir_all(&tmp);
    let root = projeto(&tmp);
    let src =
        "import unicodedata\nok = unicodedata.combining('a')\nbad = unicodedata.combending('a')\n";
    let out = check(&input(root, "/tmp/vgp-t/a.py", src, HashMap::new()));
    assert_eq!(
        denies(&out),
        vec![("atributo".to_string(), "combending".to_string())]
    );
    let _ = std::fs::remove_dir_all(&tmp);
}

// --------------------------------------------------------------------------- membro de instância

#[test]
fn membro_de_instancia_via_retorno_anotado_nega_total() {
    let tmp = std::env::temp_dir().join("vgp-t-membro");
    let _ = std::fs::remove_dir_all(&tmp);
    let root = projeto(&tmp);
    // o caso do corpus: gravar() -> Recibo; r.total NÃO existe (real: total_na_leitura)
    let src = "from scripts.memoria.grafo_gravacao import gravar\n\
               r = gravar(None)\n\
               print(r.total)\n\
               ok = r.total_na_leitura\n";
    let out = check(&input(root, "/tmp/vgp-t/a.py", src, HashMap::new()));
    assert_eq!(
        denies(&out),
        vec![("membro".to_string(), "total".to_string())]
    );
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn membro_via_construtor_direto_e_self_attrs() {
    let tmp = std::env::temp_dir().join("vgp-t-membro2");
    let _ = std::fs::remove_dir_all(&tmp);
    let root = projeto(&tmp);
    let src = "from scripts.memoria.grafo_gravacao import Recibo\n\
               r = Recibo('k', True, 1, 1)\n\
               print(r.chave)\n\
               print(r.arestas_pedidas)\n";
    let out = check(&input(root, "/tmp/vgp-t/a.py", src, HashMap::new()));
    assert_eq!(
        denies(&out),
        vec![("membro".to_string(), "arestas_pedidas".to_string())]
    );
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn slots_sem_dunder_dict_nega() {
    let tmp = std::env::temp_dir().join("vgp-t-slots");
    let _ = std::fs::remove_dir_all(&tmp);
    let root = projeto(&tmp);
    let g = tmp.join("scripts/memoria/grafo_gravacao.py");
    std::fs::write(
        &g,
        "from dataclasses import dataclass\n\
         @dataclass(frozen=True, slots=True)\n\
         class Ev:\n\
         \x20   x: int\n",
    )
    .unwrap();
    let src = "from scripts.memoria.grafo_gravacao import Ev\n\
               e = Ev(1)\n\
               print(e.__dict__)\n";
    let out = check(&input(root, "/tmp/vgp-t/a.py", src, HashMap::new()));
    // P4 (política final): dunder em classe é "dinamico" → warn sempre, nunca deny
    let f = out.findings.iter().find(|f| f.nome == "__dict__");
    assert_eq!(
        f.map(|f| f.severidade.as_str()),
        Some("warn"),
        "findings: {:?}",
        out.findings
    );
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn getattr_de_instancia_no_caminho_e_unknown_nunca_deny() {
    let tmp = std::env::temp_dir().join("vgp-t-getattr");
    let _ = std::fs::remove_dir_all(&tmp);
    let root = projeto(&tmp);
    let g = tmp.join("scripts/memoria/grafo_gravacao.py");
    std::fs::write(
        &g,
        "class D:\n    def __getattr__(self, n):\n        return 0\n",
    )
    .unwrap();
    let src = "from scripts.memoria.grafo_gravacao import D\n\
               d = D()\n\
               print(d.qualquer_coisa)\n";
    let out = check(&input(root, "/tmp/vgp-t/a.py", src, HashMap::new()));
    assert_eq!(denies(&out), Vec::<(String, String)>::new());
    let _ = std::fs::remove_dir_all(&tmp);
}

// --------------------------------------------------------------------------- kwarg

#[test]
fn kwarg_inexistente_nega_e_kwargs_absorve() {
    let tmp = std::env::temp_dir().join("vgp-t-kwarg");
    let _ = std::fs::remove_dir_all(&tmp);
    let root = projeto(&tmp);
    let g = tmp.join("scripts/memoria/grafo_gravacao.py");
    std::fs::write(
        &g,
        "def gravar(chave, corpo, **kwargs):\n    return 1\ndef ler(chave):\n    return chave\n",
    )
    .unwrap();
    let src = "from scripts.memoria.grafo_gravacao import gravar, ler\n\
               a = gravar(chave='k', corpo='x', tipo='licao')\n\
               b = ler(chave='k', quanto=2)\n";
    let out = check(&input(root, "/tmp/vgp-t/a.py", src, HashMap::new()));
    // **kwargs absorve `tipo`; `quanto` não existe em ler(chave) → deny
    assert_eq!(
        denies(&out),
        vec![("kwarg".to_string(), "quanto".to_string())]
    );
    let _ = std::fs::remove_dir_all(&tmp);
}

// --------------------------------------------------------------------------- delta

#[test]
fn delta_so_nega_o_que_a_edicao_introduz() {
    let tmp = std::env::temp_dir().join("vgp-t-delta");
    let _ = std::fs::remove_dir_all(&tmp);
    let root = projeto(&tmp);
    let disco = "print(cp.PASTA)\n";
    let proposto = "print(cp.PASTA)\nprint(cq.PASTA)\n";
    let mut inp = input(root, "/tmp/vgp-t/a.py", proposto, HashMap::new());
    inp.disk = Some(disco.to_string());
    let out = check(&inp);
    assert_eq!(denies(&out), vec![("nome".to_string(), "cq".to_string())]);
    assert!(
        out.findings
            .iter()
            .any(|f| f.nome == "cp" && f.severidade == "warn")
    );
    let _ = std::fs::remove_dir_all(&tmp);
}

// --------------------------------------------------------------------------- precisão (negativos medidos)

#[test]
fn guardas_de_precisao_em_codigo_comum() {
    let tmp = std::env::temp_dir().join("vgp-t-prec");
    let _ = std::fs::remove_dir_all(&tmp);
    let root = projeto(&tmp);
    let casos = [
        // json.loads devolve Any: .get nunca é deny (o corpus tem 7 rc-0 assim)
        "import json\nd = json.loads('{}')\nprint(d.get('x'))\n",
        // Optional[Match]: .group existe em Match — runtime None não é deny estático
        "import re\nm = re.search('a', 'b')\nprint(m.group(0) if m else None)\n",
        // union de retorno desconhecido
        "x = open('/etc/hostname').read()\nprint(x.strip())\n",
        // atribuição em loop/condicional com fonte desconhecida
        "for i in range(3):\n    v = input()\nprint(v.upper())\n",
    ];
    for (i, src) in casos.iter().enumerate() {
        let out = check(&input(root.clone(), "/tmp/vgp-t/a.py", src, HashMap::new()));
        assert_eq!(
            denies(&out),
            Vec::<(String, String)>::new(),
            "caso {i} gerou falso deny: {:?}",
            out.findings
        );
    }
    let _ = std::fs::remove_dir_all(&tmp);
}

// --------------------------------------------------------------------------- P4 (política final medida)

#[test]
fn p4_forma_e_dinamico_sao_warn_mesmo_em_tmp() {
    let tmp = std::env::temp_dir().join("vgp-t-p4");
    let _ = std::fs::remove_dir_all(&tmp);
    let root = projeto(&tmp);
    // forma: stat_result fechado em /tmp → warn (nunca deny) — o shape do corpus (Path.stat())
    let src = "from pathlib import Path\nsaida = Path('/d') / 'saida.txt'\nprint(saida.stat().st_m_size)\n";
    let out = check(&input(root.clone(), "/tmp/vgp-t/a.py", src, HashMap::new()));
    let f = out.findings.iter().find(|f| f.nome == "st_m_size");
    assert!(f.is_some(), "forma não detectada: {:?}", out.findings);
    assert_eq!(f.unwrap().severidade, "warn");
    // dinamico: import * no caminho → warn (nunca deny)
    let out2 = check(&input(
        root,
        "/tmp/vgp-t/a.py",
        "from m import *\nprint(qualquer)\n",
        HashMap::new(),
    ));
    let f2 = out2.findings.iter().find(|f| f.nome == "qualquer");
    assert_eq!(f2.map(|f| f.severidade.as_str()), Some("warn"));
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn p4_externo_nega_em_arquivo_e_em_tmp() {
    let tmp = std::env::temp_dir().join("vgp-t-p4b");
    let _ = std::fs::remove_dir_all(&tmp);
    let root = projeto(&tmp);
    let src = "import unicodedata\nbad = unicodedata.combending('a')\n";
    for path in ["/tmp/vgp-t/a.py", "/proj/arquivo.py"] {
        let out = check(&input(root.clone(), path, src, HashMap::new()));
        let f = out.findings.iter().find(|f| f.nome == "combending");
        assert_eq!(
            f.map(|f| f.severidade.as_str()),
            Some("deny"),
            "path {path}"
        );
    }
    let _ = std::fs::remove_dir_all(&tmp);
}

// --------------------------------------------------------------------------- P0 do held-out
// (atributo de módulo: bloco composto e ligação dinâmica). Cada fixture é a FORMA destilada de um
// módulo real do venv da analise (numpy 2.4.1, Pillow 11.3.0, scripts/aco/rust_bridge.py); a
// medição contra os módulos reais mora no relatório do P0.

/// Três baldes: /tmp (na execução) e inline negam por natureza (P2); o arquivo do projeto só avisa
/// em alvo não-externo. A mesma incerteza tem de produzir a MESMA resposta nos três.
const BALDES: [(&str, &str); 3] = [
    ("tmp", "/tmp/vgp-t/a.py"),
    ("inline", "<string>"),
    ("arquivo", "/proj/arquivo.py"),
];

const NUMPY_INIT: &str = r#"
import os
import sys
try:
    __NUMPY_SETUP__
except NameError:
    __NUMPY_SETUP__ = False

if __NUMPY_SETUP__:
    sys.stderr.write('setup\n')
else:
    from . import _core
    from ._core import (
        array,
        where,
        zeros,
    )
    for ta in ["float96", "float128"]:
        try:
            globals()[ta] = getattr(_core, ta)
        except AttributeError:
            pass
    del ta

    def __getattr__(attr):
        if attr == "linalg":
            import numpy.linalg as linalg
            return linalg
        if attr in __expired_attributes__:
            raise AttributeError(f"`np.{attr}` was removed", name=None)
        raise AttributeError(f"module {__name__!r} has no attribute {attr!r}")
"#;

const PIL_IMAGE: &str = r#"
import sys
from enum import IntEnum

class Resampling(IntEnum):
    NEAREST = 0
    LANCZOS = 1

def new(mode, size):
    return None

module = sys.modules[__name__]
for enum in (Resampling,):
    for item in enum:
        setattr(module, item.name, item.value)
"#;

const RUST_BRIDGE: &str = r#"
import logging
logger = logging.getLogger(__name__)
try:
    from claude_learning_kernel.claude_learning_kernel import AcoGraph as _RustAcoGraph
    RUST_AVAILABLE = True
    def _verify() -> bool:
        return True
except ImportError:
    RUST_AVAILABLE = False

def is_rust_available():
    return RUST_AVAILABLE
"#;

const LAZY_INIT: &str = r#"
import importlib
_LAZY = {"heavy": "pkg._heavy"}
def __getattr__(name):
    if name in _LAZY:
        return importlib.import_module(_LAZY[name])
    raise AttributeError(name)
"#;

fn escreve(root: &Path, rel: &str, corpo: &str) {
    let p = root.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, corpo).unwrap();
}

/// Raiz com as formas: terceiros no venv (`numpy`, `PIL`, `lazy`) e o módulo do projeto (`rust_bridge`).
fn projeto_p0(tmp: &Path) -> PathBuf {
    let sp = ".venv/lib/python3.12/site-packages";
    escreve(tmp, &format!("{sp}/numpy/__init__.py"), NUMPY_INIT);
    escreve(tmp, &format!("{sp}/PIL/__init__.py"), "");
    escreve(tmp, &format!("{sp}/PIL/Image.py"), PIL_IMAGE);
    escreve(tmp, &format!("{sp}/lazy/__init__.py"), LAZY_INIT);
    escreve(tmp, "scripts/aco/rust_bridge.py", RUST_BRIDGE);
    tmp.to_path_buf()
}

/// (severidade, classe, nome) de todos os achados, ordenados — a resposta COMPLETA do balde.
fn achados(out: &crate::vgp::CheckOutput) -> Vec<(String, String, String)> {
    let mut v: Vec<(String, String, String)> = out
        .findings
        .iter()
        .map(|f| (f.severidade.clone(), f.classe.clone(), f.nome.clone()))
        .collect();
    v.sort();
    v
}

fn roda(root: &Path, path: &str, src: &str) -> crate::vgp::CheckOutput {
    check(&input(root.to_path_buf(), path, src, HashMap::new()))
}

#[test]
fn p0_numpy_bloco_composto_liga_array_e_where_em_todo_balde() {
    let t = tempfile::tempdir().unwrap();
    let root = projeto_p0(t.path());
    let src = "import numpy as np\n\nx = np.array([1, 0, 2])\nprint(np.where(x), np.zeros(3))\n";
    for (balde, path) in BALDES {
        let out = roda(&root, path, src);
        assert_eq!(achados(&out), vec![], "balde {balde}: {:?}", out.findings);
        assert_eq!(out.verdict, "allow", "balde {balde}");
    }
}

#[test]
fn p0_numpy_nomes_dos_dois_mecanismos_dinamicos_fechados_existem() {
    // globals()[ta] sobre lista literal + __getattr__ guardado por literal: NOMES conhecidos
    let t = tempfile::tempdir().unwrap();
    let root = projeto_p0(t.path());
    let src = "import numpy as np\nprint(np.float96, np.float128, np.linalg)\n";
    for (balde, path) in BALDES {
        let out = roda(&root, path, src);
        assert_eq!(achados(&out), vec![], "balde {balde}");
    }
}

#[test]
fn p0_controle_numpy_typo_continua_negando_em_tmp_e_inline() {
    // o namespace do numpy é FECHADO (composto + globals literal + getattr guardado): o typo
    // `np.arrai` segue deny nos baldes naturais e warn no arquivo do projeto (P2 intacta)
    let t = tempfile::tempdir().unwrap();
    let root = projeto_p0(t.path());
    let src = "import numpy as np\nprint(np.arrai([1]))\n";
    for (balde, path) in BALDES {
        let out = roda(&root, path, src);
        let esperado = if balde == "arquivo" { "warn" } else { "deny" };
        assert_eq!(
            achados(&out),
            vec![(
                esperado.to_string(),
                "atributo".to_string(),
                "arrai".to_string()
            )],
            "balde {balde}"
        );
    }
}

#[test]
fn p0_numpy_from_import_do_composto_e_do_getattr_nao_nega_e_do_typo_nega() {
    let t = tempfile::tempdir().unwrap();
    let root = projeto_p0(t.path());
    let bom = roda(
        &root,
        "/tmp/vgp-t/a.py",
        "from numpy import array, where, linalg\n",
    );
    assert_eq!(achados(&bom), vec![]);
    let mau = roda(&root, "/tmp/vgp-t/a.py", "from numpy import arrai\n");
    assert_eq!(
        achados(&mau),
        vec![(
            "deny".to_string(),
            "import".to_string(),
            "arrai".to_string()
        )]
    );
}

#[test]
fn p0_pillow_setattr_dinamico_abstem_com_a_mesma_resposta_nos_tres_baldes() {
    let t = tempfile::tempdir().unwrap();
    let root = projeto_p0(t.path());
    let ok = "from PIL import Image\n\nim = Image.new('RGB', (4, 4))\nprint(Image.LANCZOS)\n";
    // o typo de um módulo que se liga em runtime também abstém: a incerteza não depende do balde
    let typo = "from PIL import Image\nprint(Image.LANCZSO)\n";
    for src in [ok, typo] {
        let respostas: Vec<_> = BALDES
            .iter()
            .map(|(_, path)| {
                let out = roda(&root, path, src);
                (achados(&out), out.verdict)
            })
            .collect();
        assert!(
            respostas.iter().all(|r| r == &respostas[0]),
            "severidade/veredito divergem entre arquivo e inline: {respostas:?}"
        );
        assert_eq!(respostas[0].0, vec![], "módulo dinâmico nunca nega: {src}");
        assert_ne!(respostas[0].1, "deny");
    }
}

#[test]
fn p0_pillow_import_de_nome_dinamico_nao_nega() {
    let t = tempfile::tempdir().unwrap();
    let root = projeto_p0(t.path());
    let out = roda(
        &root,
        "/tmp/vgp-t/a.py",
        "from PIL.Image import LANCZOS, new\n",
    );
    assert_eq!(achados(&out), vec![]);
}

#[test]
fn p0_rust_bridge_nome_ligado_em_try_except_existe() {
    let t = tempfile::tempdir().unwrap();
    let root = projeto_p0(t.path());
    let src = "import scripts.aco.rust_bridge as rb\n\nprint(rb.RUST_AVAILABLE, rb.is_rust_available())\n";
    for (balde, path) in BALDES {
        let out = roda(&root, path, src);
        assert_eq!(achados(&out), vec![], "balde {balde}");
    }
    // o typo do mesmo módulo segue negando (o módulo do projeto é fechado)
    let mau = "import scripts.aco.rust_bridge as rb\nprint(rb.RUST_AVAILABLEE)\n";
    let out = roda(&root, "/tmp/vgp-t/a.py", mau);
    assert_eq!(
        achados(&out),
        vec![(
            "deny".to_string(),
            "atributo".to_string(),
            "RUST_AVAILABLEE".to_string()
        )]
    );
}

#[test]
fn p0_getattr_preguicoso_aberto_abstem_em_todo_balde() {
    let t = tempfile::tempdir().unwrap();
    let root = projeto_p0(t.path());
    let src = "import lazy\nprint(lazy.heavy, lazy.zzz)\n";
    for (balde, path) in BALDES {
        let out = roda(&root, path, src);
        assert_eq!(achados(&out), vec![], "balde {balde}");
    }
}

#[test]
fn p0_stdlib_sem_tabela_fechada_nao_e_modelada_por_desenho() {
    // `os.pathh` é allow DE PROPÓSITO, não por defeito do coletor: o motor só prova o que é certeza
    // (`stdlib_model.rs` fecha `unicodedata` e `math`; `os` não resolve em projeto/venv e não tem
    // tabela). Lacuna de cobertura conhecida; `math.sqr` mostra o lado fechado.
    let t = tempfile::tempdir().unwrap();
    let root = projeto_p0(t.path());
    for (balde, path) in BALDES {
        let out = roda(
            &root,
            path,
            "import os\nprint(os.path.join('a', 'b'), os.pathh)\n",
        );
        assert_eq!(achados(&out), vec![], "balde {balde}: os não é modelado");
        let out = roda(&root, path, "import math\nprint(math.sqr(2))\n");
        assert_eq!(
            achados(&out),
            vec![(
                "deny".to_string(),
                "atributo".to_string(),
                "sqr".to_string()
            )],
            "balde {balde}: math é tabela fechada, nega em todo balde"
        );
    }
}

#[test]
fn p0_submodulo_compilado_existe_e_a_ausencia_do_py_nao_o_nega() {
    // `from lxml import etree` (lxml/etree.cpython-312-x86_64-linux-gnu.so): sem fonte, mas EXISTE.
    // Medido: 112 de 159 `from pkg import <extensão>` negados no venv da analise (lxml, PIL._imaging, pyarrow…).
    let t = tempfile::tempdir().unwrap();
    let root = projeto_p0(t.path());
    let sp = ".venv/lib/python3.12/site-packages/lxml";
    escreve(&root, &format!("{sp}/__init__.py"), "");
    escreve(
        &root,
        &format!("{sp}/etree.cpython-312-x86_64-linux-gnu.so"),
        "ELF",
    );
    escreve(&root, &format!("{sp}/objectify.abi3.so"), "ELF");
    escreve(&root, &format!("{sp}/_speedups.pyd"), "MZ");
    escreve(
        &root,
        &format!("{sp}/etree_extra.cpython-312-x86_64-linux-gnu.so"),
        "ELF",
    );
    let bom = "from lxml import etree, objectify, _speedups\nimport lxml\nprint(etree.fromstring, lxml.etree, lxml.objectify)\n";
    for (balde, path) in BALDES {
        let out = roda(&root, path, bom);
        assert_eq!(achados(&out), vec![], "balde {balde}: {:?}", out.findings);
    }
    // o nome é exato: `etree_extra.so` não é `etre` nem `etree_`; e o typo segue negando
    let mau = "from lxml import etre\nimport lxml\nprint(lxml.etre)\n";
    let out = roda(&root, "/tmp/vgp-t/a.py", mau);
    assert_eq!(
        achados(&out),
        vec![
            (
                "deny".to_string(),
                "atributo".to_string(),
                "etre".to_string()
            ),
            ("deny".to_string(), "import".to_string(), "etre".to_string()),
        ]
    );
    let out = roda(&root, "/tmp/vgp-t/a.py", "from lxml import etree_\n");
    assert_eq!(
        achados(&out),
        vec![(
            "deny".to_string(),
            "import".to_string(),
            "etree_".to_string()
        )],
        "o ponto depois do nome é obrigatório: `etree_extra.so` não é `etree_`"
    );
}

// --------------------------------------------------------------------------- determinismo
//
// `HashSet` itera numa ordem diferente a CADA instância (RandomState por instância): tudo que sai
// de um conjunto sem ordem total vaza para o JSON. Medido em 10/10/2026 no binário e935 (id do corpus
// vivo, 10 chamadas do MESMO input): 10 ordens de `reais` e até 10 CONJUNTOS — o `take(8)` cortava uma
// coleção sem ordem —, e a `sugestao` («Você quis dizer») oscilava entre ter e não ter o nome.

const DET_CLASSES: &str = r#"
from dataclasses import dataclass


class Config:
    alpha: int = 0
    beta: int = 0
    gamma: int = 0
    delta: int = 0
    epsilon: int = 0
    zeta: int = 0
    eta: int = 0
    theta: int = 0
    iota: int = 0
    kappa: int = 0

    def __init__(self):
        self.mu = 1
        self.nu = 2

    def nome(self):
        return 1

    def lambda_(self):
        return 2


class Pequena:
    um: int = 1
    dois: int = 2
    tres: int = 3


@dataclass
class Ficha:
    zulu: int
    yankee: int
    xray: int
    whiskey: int
    victor: int
    uniform: int
    tango: int
    sierra: int
    romeo: int
    quebec: int
    papa: int
    oscar: int
"#;

/// Os quatro caminhos que tiram `reais` de uma coleção: membro de classe larga (corte de 8 sobre um
/// conjunto), membro de classe pequena (só a ordem), kwarg de dataclass (campos do `__init__`) e
/// atributo de módulo do projeto (nomes de topo, 12 — o corte também importa).
const DET_USO: &str = "from pkg.cfg import Config, Pequena, Ficha\n\
                       import pkg.util as u\n\
                       c = Config()\n\
                       print(c.betta)\n\
                       p = Pequena()\n\
                       print(p.dos)\n\
                       f = Ficha(uniforme=1)\n\
                       print(u.f1)\n";

fn projeto_det(tmp: &Path) -> PathBuf {
    escreve(tmp, "pkg/__init__.py", "");
    escreve(tmp, "pkg/cfg.py", DET_CLASSES);
    let funcoes: String = (1..=12)
        .map(|n| format!("def f{n:02}():\n    return {n}\n"))
        .collect();
    escreve(tmp, "pkg/util.py", &funcoes);
    tmp.to_path_buf()
}

/// A saída do motor sem o relógio (`elapsed_ms` é medida, não resposta): veredito + achados em JSON.
fn bytes_da_resposta(root: &Path, path: &str) -> String {
    let out = roda(root, path, DET_USO);
    format!(
        "{}|{}",
        out.verdict,
        serde_json::to_string(&out.findings).unwrap()
    )
}

#[test]
fn det_mesma_entrada_dez_chamadas_dao_os_mesmos_bytes_nos_tres_baldes() {
    let tmp = std::env::temp_dir().join("vgp-t-det-bytes");
    let _ = std::fs::remove_dir_all(&tmp);
    let root = projeto_det(&tmp);
    for (balde, path) in BALDES {
        let primeira = bytes_da_resposta(&root, path);
        for n in 2..=10 {
            assert_eq!(
                bytes_da_resposta(&root, path),
                primeira,
                "balde {balde}: a chamada {n} difere da 1ª — ordem/conjunto de `reais` ou `sugestao` vazou de um conjunto sem ordem"
            );
        }
    }
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn det_reais_sao_o_corte_de_uma_ordem_total_e_a_sugestao_sai_dela() {
    let tmp = std::env::temp_dir().join("vgp-t-det-reais");
    let _ = std::fs::remove_dir_all(&tmp);
    let root = projeto_det(&tmp);
    let out = roda(&root, "/tmp/vgp-t/a.py", DET_USO);
    let achado = |nome: &str| {
        out.findings
            .iter()
            .find(|f| f.nome == nome)
            .unwrap_or_else(|| panic!("sem achado `{nome}`: {:?}", out.findings))
    };
    let lista = |xs: &[&str]| xs.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    // membro de classe larga: públicos em ordem alfabética, cortados em 8 (`nome` fica de fora)
    let f = achado("betta");
    assert_eq!(
        f.reais,
        lista(&[
            "alpha", "beta", "delta", "epsilon", "eta", "gamma", "iota", "kappa"
        ])
    );
    assert!(
        f.sugestao.ends_with("Você quis dizer `beta`?"),
        "{}",
        f.sugestao
    );
    // classe pequena: todos os membros, na mesma ordem total
    let f = achado("dos");
    assert_eq!(f.reais, lista(&["dois", "tres", "um"]));
    assert!(
        f.sugestao.ends_with("Você quis dizer `dois`?"),
        "{}",
        f.sugestao
    );
    // kwarg: os campos do dataclass na ORDEM DE DECLARAÇÃO (a do `__init__`), não na do conjunto
    let f = achado("uniforme");
    assert_eq!(
        f.reais,
        lista(&[
            "zulu", "yankee", "xray", "whiskey", "victor", "uniform", "tango", "sierra", "romeo",
            "quebec"
        ])
    );
    assert!(
        f.sugestao.ends_with("Você quis dizer `uniform`?"),
        "{}",
        f.sugestao
    );
    // atributo de módulo: nomes de topo públicos em ordem alfabética, cortados em 8
    let f = achado("f1");
    assert_eq!(
        f.reais,
        lista(&["f01", "f02", "f03", "f04", "f05", "f06", "f07", "f08"])
    );
    assert!(
        f.sugestao.ends_with("Você quis dizer `f01`?"),
        "{}",
        f.sugestao
    );
    let _ = std::fs::remove_dir_all(&tmp);
}

/// Todas as permutações de `v` (n! chamadas de `f`) — o desempate não pode depender da ordem de entrada.
fn permuta<T>(v: &mut Vec<T>, k: usize, f: &mut impl FnMut(&[T])) {
    if k == v.len() {
        f(v);
        return;
    }
    for i in k..v.len() {
        v.swap(k, i);
        permuta(v, k + 1, f);
        v.swap(k, i);
    }
}

#[test]
fn det_sugestao_desempata_por_distancia_e_depois_pelo_nome_em_toda_permutacao() {
    // «casa»: `asa` · `cada` · `casas` · `caso` estão a distância 1 (empate de 4); `aaaa` está a 2 e
    // vem ANTES de todos em ordem alfabética (só a distância o derrota); `casa` (0) nunca é sugestão.
    let mut v: Vec<String> = ["cada", "casas", "asa", "caso", "aaaa", "casa"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let mut respostas = std::collections::BTreeSet::new();
    permuta(&mut v, 0, &mut |p| {
        respostas.insert(super::sugestao_proxima("casa", p));
    });
    assert_eq!(
        respostas.into_iter().collect::<Vec<_>>(),
        vec![Some("asa".to_string())],
        "a sugestão mudou com a ordem de entrada: o desempate não é total"
    );
}

#[test]
fn det_ordem_dos_membros_publicos_primeiro_alfabetica_e_cortada_em_oito() {
    let nomes = [
        "b", "__init__", "a", "_x", "C", "d", "e", "f", "g", "h", "i",
    ];
    // 50 conjuntos NOVOS: cada um tem seu RandomState, logo uma ordem de iteração própria
    for _ in 0..50 {
        let membros: HashSet<String> = nomes.iter().map(|s| s.to_string()).collect();
        // maiúscula antes de minúscula (ASCII); `__init__` e `_x` (privados) ficam depois dos 8 cortados
        assert_eq!(
            super::reais_dos_membros(&membros),
            ["C", "a", "b", "d", "e", "f", "g", "h"]
        );
    }
    // só privados: sem públicos para passar à frente, a ordem alfabética (`__` < `_a`) ainda é total
    let so_privados: HashSet<String> = ["_b", "_a", "__z"].iter().map(|s| s.to_string()).collect();
    assert_eq!(super::reais_dos_membros(&so_privados), ["__z", "_a", "_b"]);
}

// --------------------------------------------------------------------------- dataclass: o `__init__` gerado
//
// A assinatura gerada é POSICIONAL, então os `reais` de um `kwarg` inexistente saem na ordem do
// `dataclasses` (CPython 3.13, `Doc/library/dataclasses.rst`): campos coletados no MRO REVERSO (bases
// primeiro, a classe por último); redeclarar mantém a posição da primeira declaração; só ANOTADOS são
// campos (`ClassVar` fica de fora, `InitVar` entra); os `kw_only` vão depois dos posicionais. Base que o
// coletor não enxerga pode declarar campos: o motor se ABSTÉM em vez de negar.

const DC_BASE: &str = r#"
from dataclasses import dataclass
from typing import Any


@dataclass
class Base:
    x: Any = 15.0
    y: int = 0


@dataclass
class C(Base):
    z: int = 10
    x: int = 15
"#;

const DC_KW: &str = r#"
from dataclasses import KW_ONLY, dataclass, field
from typing import Any


@dataclass
class Base:
    x: Any = 15.0
    _: KW_ONLY
    y: int = 0
    w: int = 1


@dataclass
class D(Base):
    z: int = 10
    t: int = field(kw_only=True, default=0)


@dataclass(kw_only=True)
class K:
    a: int
    b: int = field(kw_only=False, default=1)
    c: int = 0
"#;

const DC_VARS: &str = r#"
import typing
from dataclasses import InitVar, dataclass, field
from typing import ClassVar


@dataclass
class V:
    n: ClassVar[int] = 3
    m: typing.ClassVar[int] = 4
    db: InitVar[str] = ""
    oculto: int = field(init=False, default=0)
    sem_anotacao = 7
    real: int = 1
"#;

const DC_MI: &str = r#"
from dataclasses import dataclass


@dataclass
class A:
    a: int = 0


@dataclass
class B(A):
    b: int = 0


@dataclass
class C(A):
    c: int = 0


@dataclass
class D(B, C):
    d: int = 0
"#;

/// A MESMA classe `A` alcançada por duas rotas (direta e por reexport): é um nó só no C3.
const DC_MI2: &str = r#"
from dataclasses import dataclass
from pkg.dc_mi import A
from pkg.dc_reexp import A as A2


@dataclass
class B2(A):
    b: int = 0


@dataclass
class C2(A2):
    c: int = 0


@dataclass
class D2(B2, C2):
    d: int = 0
"#;

/// Duas classes DISTINTAS com o mesmo nome `A` (arquivos diferentes): identidade é (arquivo, nome).
const DC_MI3: &str = r#"
from dataclasses import dataclass
from pkg.dc_mi import A as A1
from pkg.dc_outra import A as A2


@dataclass
class P(A1, A2):
    p: int = 0
"#;

const DC_OUTRA: &str = r#"
from dataclasses import dataclass


@dataclass
class A:
    o: int = 0
"#;

const DC_EXT: &str = r#"
from dataclasses import dataclass
from nao_existe.lib import Fora


def fabrica():
    return object


@dataclass
class E(Fora):
    e: int = 0


@dataclass
class F(fabrica()):
    f: int = 0
"#;

const DC_MISC: &str = r#"
from dataclasses import dataclass
from typing import Generic, TypeVar
from pkg.dc_base import Base

T = TypeVar("T")


@dataclass
class H(Generic[T]):
    h: int = 0


@dataclass
class G(Base[int]):
    g: int = 0


@dataclass
class I:
    x: int = 0

    def __init__(self, x=0, extra=0):
        self.x = x


@dataclass(init=False)
class J:
    x: int = 0
"#;

fn projeto_dc(tmp: &Path) -> PathBuf {
    escreve(tmp, "pkg/__init__.py", "");
    escreve(tmp, "pkg/dc_base.py", DC_BASE);
    escreve(tmp, "pkg/dc_kw.py", DC_KW);
    escreve(tmp, "pkg/dc_vars.py", DC_VARS);
    escreve(tmp, "pkg/dc_mi.py", DC_MI);
    escreve(tmp, "pkg/dc_reexp.py", "from pkg.dc_mi import A\n");
    escreve(tmp, "pkg/dc_mi2.py", DC_MI2);
    escreve(tmp, "pkg/dc_mi3.py", DC_MI3);
    escreve(tmp, "pkg/dc_outra.py", DC_OUTRA);
    escreve(tmp, "pkg/dc_ext.py", DC_EXT);
    escreve(tmp, "pkg/dc_misc.py", DC_MISC);
    tmp.to_path_buf()
}

/// (nome, reais) de cada `kwarg` negado/avisado, na ordem de descoberta.
fn kwargs_achados(root: &Path, path: &str, src: &str) -> Vec<(String, Vec<String>)> {
    roda(root, path, src)
        .findings
        .iter()
        .filter(|f| f.classe == "kwarg")
        .map(|f| (f.nome.clone(), f.reais.clone()))
        .collect()
}

fn lista(xs: &[&str]) -> Vec<String> {
    xs.iter().map(|s| s.to_string()).collect()
}

#[test]
fn dc_heranca_bases_primeiro_e_campo_redeclarado_mantem_a_posicao() {
    let tmp = std::env::temp_dir().join("vgp-t-dc-heranca");
    let _ = std::fs::remove_dir_all(&tmp);
    let root = projeto_dc(&tmp);
    // doc: `C(Base)` com `z` e `x` redeclarado gera `__init__(self, x, y, z)` — `x` NÃO vai para o fim
    let src = "from pkg.dc_base import C\nC(errado=1)\n";
    assert_eq!(
        kwargs_achados(&root, "/tmp/vgp-t/a.py", src),
        vec![("errado".to_string(), lista(&["x", "y", "z"]))]
    );
    // os campos da BASE são aceitos na filha (antes: negados, só os da própria classe valiam)
    let src = "from pkg.dc_base import C\nC(x=1, y=2, z=3)\n";
    assert_eq!(kwargs_achados(&root, "/tmp/vgp-t/a.py", src), vec![]);
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn dc_kw_only_vai_depois_dos_posicionais() {
    let tmp = std::env::temp_dir().join("vgp-t-dc-kw");
    let _ = std::fs::remove_dir_all(&tmp);
    let root = projeto_dc(&tmp);
    // doc: `D(Base)` com `_: KW_ONLY` na base e `field(kw_only=True)` na filha gera
    // `__init__(self, x, z, *, y, w, t)`; `@dataclass(kw_only=True)` + `field(kw_only=False)` → `b` posicional
    let src = "from pkg.dc_kw import D, K\nD(err1=1)\nK(err2=1)\n";
    assert_eq!(
        kwargs_achados(&root, "/tmp/vgp-t/a.py", src),
        vec![
            ("err1".to_string(), lista(&["x", "z", "y", "w", "t"])),
            ("err2".to_string(), lista(&["b", "a", "c"])),
        ]
    );
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn dc_classvar_fora_initvar_dentro_init_false_fora_e_sem_anotacao_nao_e_campo() {
    let tmp = std::env::temp_dir().join("vgp-t-dc-vars");
    let _ = std::fs::remove_dir_all(&tmp);
    let root = projeto_dc(&tmp);
    let src = "from pkg.dc_vars import V\nV(n=1, m=2, oculto=3, sem_anotacao=4, db='x', real=5)\n";
    let achados = kwargs_achados(&root, "/tmp/vgp-t/a.py", src);
    let nomes: Vec<&str> = achados.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(
        nomes,
        ["n", "m", "oculto", "sem_anotacao"],
        "ClassVar (nu e `typing.`), `field(init=False)` e atributo sem anotação NÃO são parâmetros; `InitVar` é"
    );
    assert!(achados.iter().all(|(_, r)| *r == lista(&["db", "real"])));
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn dc_heranca_multipla_segue_o_c3_e_a_mesma_base_por_duas_rotas_e_um_no_so() {
    let tmp = std::env::temp_dir().join("vgp-t-dc-mi");
    let _ = std::fs::remove_dir_all(&tmp);
    let root = projeto_dc(&tmp);
    // `D(B, C)` com `B(A)`, `C(A)`: MRO = D, B, C, A; reverso sem D = A, C, B → a, c, b, d (não a, b, c, d)
    let src = "from pkg.dc_mi import D\nD(errado=1)\n";
    assert_eq!(
        kwargs_achados(&root, "/tmp/vgp-t/a.py", src),
        vec![("errado".to_string(), lista(&["a", "c", "b", "d"]))]
    );
    // a mesma `A` pela rota direta e pelo reexport: se a identidade falhasse, `A` apareceria duas vezes,
    // o C3 não fecharia e o motor se absteria (sem achado)
    let src = "from pkg.dc_mi2 import D2\nD2(errado=1)\n";
    assert_eq!(
        kwargs_achados(&root, "/tmp/vgp-t/a.py", src),
        vec![("errado".to_string(), lista(&["a", "c", "b", "d"]))]
    );
    // duas classes DISTINTAS chamadas `A` (arquivos diferentes) não são o mesmo nó: reverso = A2, A1, P
    let src = "from pkg.dc_mi3 import P\nP(errado=1)\n";
    assert_eq!(
        kwargs_achados(&root, "/tmp/vgp-t/a.py", src),
        vec![("errado".to_string(), lista(&["o", "a", "p"]))]
    );
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn dc_base_irresolvida_ou_opaca_abstem_em_todo_balde() {
    let tmp = std::env::temp_dir().join("vgp-t-dc-ext");
    let _ = std::fs::remove_dir_all(&tmp);
    let root = projeto_dc(&tmp);
    // `E(Fora)` com `Fora` de um módulo que não resolve e `F(fabrica())` (base calculada): o campo de uma
    // base que não se enxerga pode ser exatamente o kwarg — silêncio idêntico em /tmp, inline e arquivo
    let src = "from pkg.dc_ext import E, F\nE(q1=1)\nF(q2=1)\n";
    for (balde, path) in BALDES {
        assert_eq!(kwargs_achados(&root, path, src), vec![], "balde {balde}");
    }
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn dc_generic_e_inofensivo_base_subscrita_resolve_init_explicito_vale_e_init_false_abstem() {
    let tmp = std::env::temp_dir().join("vgp-t-dc-misc");
    let _ = std::fs::remove_dir_all(&tmp);
    let root = projeto_dc(&tmp);
    let src = "from pkg.dc_misc import H, G, I, J\n\
               H(h1=1)\n\
               G(x=1, g=2)\n\
               G(g1=1)\n\
               I(extra=1)\n\
               I(i1=1)\n\
               J(j1=1)\n";
    let achados = kwargs_achados(&root, "/tmp/vgp-t/a.py", src);
    let nomes: Vec<&str> = achados.iter().map(|(n, _)| n.as_str()).collect();
    // H: `Generic[T]` não declara campos → só `h`; G: `Base[int]` é a `Base` (x, y) + g; I: o `__init__`
    // EXPLÍCITO vale (o dataclass não o sobrescreve) → `extra` existe, `i1` não; J: `init=False` → abstém
    assert_eq!(nomes, ["h1", "g1", "i1"]);
    assert_eq!(achados[0].1, lista(&["h"]));
    assert_eq!(achados[1].1, lista(&["x", "y", "g"]));
    let _ = std::fs::remove_dir_all(&tmp);
}

// ---------------------------------------------------------------------------------------------
// P0B (a) — atributos implícitos de módulo, na camada 1.
//
// Fonte (Context7 `/python/cpython/v3.13.9`): `Doc/reference/import.rst` — o import liga os atributos
// do módulo com `_init_module_attrs(spec, module)` (`__name__`, `__loader__`, `__package__`, `__spec__`,
// `__file__` quando há arquivo, `__path__` em pacote); `Doc/reference/datamodel.rst` — `__name__` e
// `__spec__.name` de um módulo; `Doc/c-api/module.rst` — `PyModule_NewObject` inicia `__name__`,
// `__doc__`, `__package__`, `__loader__` e deixa `__file__` a cargo de quem carrega;
// `Doc/using/configure.rst` — módulo embutido (`sys`) NÃO tem `__file__` (`AttributeError`).
// A forma medida no P0M: `import folium; folium.__file__` (id 636f9a7adec1), onde o `__init__.py` do
// pacote de terceiros tem `__all__` e não liga `__file__`.

fn projeto_pa(tmp: &Path) -> PathBuf {
    let sp = ".venv/lib/python3.12/site-packages";
    escreve(tmp, "pkg/__init__.py", "def topo():\n    return 1\n");
    escreve(tmp, "pkg/m.py", "X = 1\n");
    escreve(
        tmp,
        &format!("{sp}/folium/__init__.py"),
        "from folium.map import Map\n__all__ = ['Map']\n",
    );
    escreve(
        tmp,
        &format!("{sp}/folium/map.py"),
        "class Map:\n    pass\n",
    );
    tmp.to_path_buf()
}

#[test]
fn p0b_a_todo_modulo_carregado_tem_os_atributos_implicitos_em_todo_balde() {
    let tmp = std::env::temp_dir().join("vgp-t-p0b-a1");
    let _ = std::fs::remove_dir_all(&tmp);
    let root = projeto_pa(&tmp);
    let src = "import pkg, pkg.m as m, folium\n\
               print(pkg.__name__, pkg.__doc__, pkg.__file__, pkg.__spec__, pkg.__loader__, pkg.__package__, pkg.__dict__, pkg.__builtins__, pkg.__path__)\n\
               print(m.__name__, m.__doc__, m.__file__, m.__spec__, m.__loader__, m.__package__, m.__dict__, m.__builtins__)\n\
               print(folium.__name__, folium.__file__, folium.__path__, folium.__spec__)\n";
    for (balde, path) in BALDES {
        assert_eq!(achados(&roda(&root, path, src)), vec![], "balde {balde}");
    }
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn p0b_a_controles_inexistencia_de_verdade_e_path_de_modulo_comum_seguem_negando() {
    let tmp = std::env::temp_dir().join("vgp-t-p0b-a2");
    let _ = std::fs::remove_dir_all(&tmp);
    let root = projeto_pa(&tmp);
    // `__path__` só existe em PACOTE: `pkg.m` é um módulo comum (AttributeError em runtime)
    let src = "import pkg, pkg.m as m, folium\n\
               print(pkg.nada1)\n\
               print(m.__path__)\n\
               print(folium.nada2)\n";
    let d = |n: &str| ("deny".to_string(), "atributo".to_string(), n.to_string());
    assert_eq!(
        achados(&roda(&root, "/tmp/vgp-t/a.py", src)),
        vec![d("__path__"), d("nada1"), d("nada2")]
    );
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn p0b_a_from_import_do_atributo_implicito_nao_nega_e_do_inventado_nega() {
    let tmp = std::env::temp_dir().join("vgp-t-p0b-a3");
    let _ = std::fs::remove_dir_all(&tmp);
    let root = projeto_pa(&tmp);
    let src = "from pkg import __file__, __name__, __path__\nfrom pkg import nada3\n";
    assert_eq!(
        achados(&roda(&root, "/tmp/vgp-t/a.py", src)),
        vec![(
            "deny".to_string(),
            "import".to_string(),
            "nada3".to_string()
        )]
    );
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn p0b_a_modulo_stdlib_de_tabela_fechada_tem_os_implicitos_e_o_inventado_nega() {
    let tmp = std::env::temp_dir().join("vgp-t-p0b-a4");
    let _ = std::fs::remove_dir_all(&tmp);
    let root = projeto_pa(&tmp);
    // `math` é C (embutido ou extensão conforme o build): os cinco que TODO módulo tem passam; `__file__`
    // depende de como o interpretador foi construído — abstém, nunca nega
    let src = "import math\n\
               print(math.__name__, math.__doc__, math.__spec__, math.__loader__, math.__package__, math.__file__)\n\
               print(math.nada4)\n";
    for (balde, path) in BALDES {
        let nomes: Vec<String> = roda(&root, path, src)
            .findings
            .iter()
            .map(|f| f.nome.clone())
            .collect();
        assert_eq!(nomes, ["nada4"], "balde {balde}");
    }
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn p0b_a_atributo_implicito_depende_de_arquivo_e_de_pacote() {
    use super::modules::ModuleInfo;
    let universais = [
        "__name__",
        "__doc__",
        "__spec__",
        "__loader__",
        "__package__",
        "__dict__",
        "__builtins__",
    ];
    let sem_arquivo = ModuleInfo::default();
    let comum = ModuleInfo {
        file: Some(PathBuf::from("/x/pkg/m.py")),
        ..Default::default()
    };
    let pacote = ModuleInfo {
        file: Some(PathBuf::from("/x/pkg/__init__.py")),
        ..Default::default()
    };
    for nome in universais {
        for (rotulo, info) in [
            ("sem arquivo", &sem_arquivo),
            ("comum", &comum),
            ("pacote", &pacote),
        ] {
            assert!(info.atributo_implicito(nome), "{rotulo}: {nome}");
        }
    }
    assert!(
        !sem_arquivo.atributo_implicito("__file__"),
        "sem arquivo não há __file__"
    );
    assert!(comum.atributo_implicito("__file__") && pacote.atributo_implicito("__file__"));
    assert!(!sem_arquivo.atributo_implicito("__path__"));
    assert!(
        !comum.atributo_implicito("__path__"),
        "módulo comum não é pacote"
    );
    assert!(pacote.atributo_implicito("__path__"));
    for info in [&sem_arquivo, &comum, &pacote] {
        for nome in [
            "__nada__",
            "__version__",
            "_privado",
            "nada",
            "file",
            "__init__",
        ] {
            assert!(!info.atributo_implicito(nome), "{nome} não é implícito");
        }
    }
}

// ---------------------------------------------------------------------------------------------
// P0B (c) — `__init__` herdado em classe COMUM sem `__init__` próprio (P0BMRO).
// A assinatura é a do PRIMEIRO `__init__` no MRO C3; qualquer elo que não se enxerga abstém.

const PC_M: &str = r#"
from dataclasses import dataclass

class Base:
    def __init__(self, a, b=2):
        self.a = a

@dataclass
class DcBase:
    x: int
    y: int = 0

class DcFilha(DcBase):
    pass

class Filha(Base):
    pass

class Neta(Filha):
    pass

class Sozinha:
    pass

class ComInit(Base):
    def __init__(self, z):
        pass

class P1:
    pass

class P2:
    def __init__(self, q):
        pass

class F(P1, P2):
    pass

class DA:
    def __init__(self, a):
        pass

class DB(DA):
    pass

class DC(DA):
    def __init__(self, c):
        pass

class DD(DB, DC):
    pass

class Erro(Exception):
    pass
"#;

const PC_ABSTEM: &str = r#"
from typing import NamedTuple
from outside import Fora
from pkg.pc_m import Base

class Meta(type):
    def __call__(cls, *a, **k):
        return super().__call__(*a, **k)

class SemBase(Fora):
    pass

class ComNew(Base):
    def __new__(cls, *a, **k):
        return super().__new__(cls)

class ComMeta(Base, metaclass=Meta):
    pass

class Hook:
    def __init_subclass__(cls, **kw):
        pass

class ComHook(Hook):
    pass

class ComSig(Base):
    __signature__ = None

class InitPorAtribuicao(Base):
    __init__ = Base.__init__

class Pt(NamedTuple):
    x: int
"#;

const PC_B1: &str = "class Base:\n    def __init__(self, a):\n        pass\n";
const PC_B2: &str = "class Base:\n    def __init__(self, b):\n        pass\n";
const PC_K: &str = "from pkg.pc_b1 import Base as B1\nfrom pkg.pc_b2 import Base as B2\n\nclass K(B1, B2):\n    pass\n";

fn projeto_pc(tmp: &Path) -> PathBuf {
    escreve(tmp, "pkg/__init__.py", "");
    escreve(tmp, "pkg/pc_m.py", PC_M);
    escreve(tmp, "pkg/pc_abstem.py", PC_ABSTEM);
    escreve(tmp, "pkg/pc_b1.py", PC_B1);
    escreve(tmp, "pkg/pc_b2.py", PC_B2);
    escreve(tmp, "pkg/pc_k.py", PC_K);
    tmp.to_path_buf()
}

#[test]
fn p0b_c_classe_comum_sem_init_herda_a_assinatura_do_primeiro_init_no_mro() {
    let tmp = std::env::temp_dir().join("vgp-t-p0b-c1");
    let _ = std::fs::remove_dir_all(&tmp);
    let root = projeto_pc(&tmp);
    // `Filha(Base)`: a sonda `sonda_init` — `Filha(a=1)` NÃO é achado (herda `Base.__init__(self, a, b=2)`)
    let src = "from pkg.pc_m import Filha, Neta, ComInit, F, DD, DcFilha\n\
               Filha(a=1)\nFilha(a=1, b=3)\nNeta(a=1)\nComInit(z=1)\n\
               F(q=1)\n\
               DD(c=1)\n\
               DcFilha(x=1, y=2)\n";
    for (balde, path) in BALDES {
        assert_eq!(kwargs_achados(&root, path, src), vec![], "balde {balde}");
    }
    // controles: o que é inexistência de verdade continua negando, com a assinatura HERDADA nas `reais`
    let src = "from pkg.pc_m import Filha, Neta, Sozinha, ComInit, F, DD, Erro, DcFilha\n\
               Filha(zz=1)\n\
               Sozinha(x=1)\n\
               ComInit(a=1)\n\
               F(p=1)\n\
               DD(w=1)\n\
               Erro(code=1)\n\
               DcFilha(nada=1)\n";
    assert_eq!(
        kwargs_achados(&root, "/tmp/vgp-t/a.py", src),
        vec![
            ("zz".to_string(), lista(&["a", "b"])),
            ("x".to_string(), vec![]),
            ("a".to_string(), lista(&["z"])),
            ("p".to_string(), lista(&["q"])),
            // C3: DD → DB → DC → DA; o primeiro `__init__` é o de DC (`c`), não o de DA (`a`)
            ("w".to_string(), lista(&["c"])),
            ("code".to_string(), vec![]),
            // o `__init__` GERADO do dataclass da base é herdado pela filha comum
            ("nada".to_string(), lista(&["x", "y"])),
        ]
    );
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn p0b_c_o_primeiro_parametro_do_init_e_a_instancia_e_nao_um_kwarg() {
    let tmp = std::env::temp_dir().join("vgp-t-p0b-c3");
    let _ = std::fs::remove_dir_all(&tmp);
    let root = projeto_pc(&tmp);
    // `self` está ligado à instância: `C(self=1)` é TypeError no CPython, tanto no `__init__` próprio
    // quanto no herdado pelo MRO, e não aparece entre as `reais`. Uma fonte por caso: o motor guarda
    // um achado por (classe, nome), então dois `self=` na mesma fonte viram um só.
    let proprio = "from pkg.pc_m import ComInit\nComInit(self=1)\n";
    assert_eq!(
        kwargs_achados(&root, "/tmp/vgp-t/a.py", proprio),
        vec![("self".to_string(), lista(&["z"]))]
    );
    let herdado = "from pkg.pc_m import Filha\nFilha(self=1)\n";
    assert_eq!(
        kwargs_achados(&root, "/tmp/vgp-t/a.py", herdado),
        vec![("self".to_string(), lista(&["a", "b"]))]
    );
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn p0b_c_a_identidade_do_no_e_arquivo_e_nome_e_nao_so_o_nome() {
    let tmp = std::env::temp_dir().join("vgp-t-p0b-c2");
    let _ = std::fs::remove_dir_all(&tmp);
    let root = projeto_pc(&tmp);
    // duas classes `Base` de arquivos distintos: o MRO de `K(B1, B2)` é K, B1.Base, B2.Base — o primeiro
    // `__init__` é o de pc_b1 (`a`); com identidade só pelo nome as duas virariam um nó
    let src = "from pkg.pc_k import K\nK(a=1)\nK(b=1)\n";
    assert_eq!(
        kwargs_achados(&root, "/tmp/vgp-t/a.py", src),
        vec![("b".to_string(), lista(&["a"]))]
    );
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn p0b_c_base_irresolvida_new_metaclasse_init_subclass_e_signature_abstem_em_todo_balde() {
    let tmp = std::env::temp_dir().join("vgp-t-p0b-c3-abstem");
    let _ = std::fs::remove_dir_all(&tmp);
    let root = projeto_pc(&tmp);
    let src = "from pkg.pc_abstem import SemBase, ComNew, ComMeta, ComHook, ComSig, Pt, InitPorAtribuicao\n\
               SemBase(zz1=1)\n\
               ComNew(zz2=1)\n\
               ComMeta(zz3=1)\n\
               ComHook(zz4=1)\n\
               ComSig(zz5=1)\n\
               Pt(zz6=1)\n\
               InitPorAtribuicao(zz7=1)\n";
    for (balde, path) in BALDES {
        assert_eq!(kwargs_achados(&root, path, src), vec![], "balde {balde}");
    }
    let _ = std::fs::remove_dir_all(&tmp);
}

/// The tests of this file run as parallel threads of ONE binary, and each one `remove_dir_all`s and
/// writes its own fixed directory under the temp dir — two tests naming the same directory race (the
/// P0B `p0b_c3` pair did: one test's source vanished under the other, `left: []` in ~1 run of N).
/// One directory name, one test.
#[test]
fn nenhum_par_de_testes_compartilha_o_diretorio_temporario() {
    const PREFIXO: &str = "join(\"vgp-t";
    let fonte = include_str!("tests.rs");
    let mut dono: HashMap<&str, &str> = HashMap::new();
    for bloco in fonte.split("\nfn ").skip(1) {
        let teste = bloco.split('(').next().unwrap_or("");
        let mut resto = bloco;
        while let Some(i) = resto.find(PREFIXO) {
            resto = &resto[i + "join(\"".len()..];
            let Some(fim) = resto.find('"') else { break };
            let dir = &resto[..fim];
            if let Some(outro) = dono.insert(dir, teste) {
                assert_eq!(
                    outro, teste,
                    "`{dir}` is the temp directory of both `{outro}` and `{teste}`: they race"
                );
            }
        }
    }
    assert!(dono.len() > 40, "the scan must see the file's directories");
}

// ---- P0B-d (17/10/2026): "tipo fechado" corroborado pelas duas camadas ---------------------------
//
// `st = <path>.stat(); st.st_m_size` raises AttributeError at run time (`d94f8090d150`). Layer 2's
// `Escopo::Tipo` warns — right in general (the INFERRED type is not the object that arrives) — and
// layer 1 warned as `Incerto`, which the merge never escalates: the real error stayed a warning. A
// closed type changes the doubt: the absence is certain for an instance; what layer 1 cannot prove is
// that the value IS one. `Alvo::Fechado` says so, and the merge (`sombra::fundir_findings`) denies it
// when layer 2 names the same receiver type.

/// Receivers layer 1 types as `stat_result`: the `st = …stat()` form of the case, the chained one,
/// and the dotted constructor.
const FONTES_STAT: [&str; 3] = [
    "import pathlib\nst = pathlib.Path('x').stat()\nprint(st.st_m_size)\n",
    "from pathlib import Path\nsaida = Path('/d') / 'saida.txt'\nprint(saida.stat().st_m_size)\n",
    "from pathlib import Path\nst = Path('x').stat()\nprint(st.st_m_size)\n",
];

#[test]
fn p0bd_membro_ausente_de_tipo_fechado_e_alvo_fechado_com_o_receptor_e_avisa_sozinho() {
    use crate::vgp::Alvo;
    let tmp = std::env::temp_dir().join("vgp-t-p0bd-fechado");
    let _ = std::fs::remove_dir_all(&tmp);
    let root = projeto(&tmp);
    for src in FONTES_STAT {
        for path in ["/tmp/vgp-t/a.py", "<string>", "/proj/arquivo.py"] {
            let out = check(&input(root.clone(), path, src, HashMap::new()));
            let f = out
                .findings
                .iter()
                .find(|f| f.nome == "st_m_size")
                .unwrap_or_else(|| panic!("{path} {src:?}: {:?}", out.findings));
            assert_eq!(f.alvo, Alvo::Fechado, "{path} {src:?}");
            assert_eq!(f.receptor.as_deref(), Some("stat_result"), "{path} {src:?}");
            assert_eq!(
                f.severidade, "warn",
                "layer 1 alone follows no flow — it must not deny ({path})"
            );
            assert_eq!(out.verdict, "warn", "{path}");
        }
    }
    // the member that exists is silent
    let bom = "from pathlib import Path\nst = Path('x').stat()\nprint(st.st_size)\n";
    let out = check(&input(root, "<string>", bom, HashMap::new()));
    assert_eq!(out.findings, vec![]);
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn p0bd_os_demais_builtins_seguem_incertos_e_sem_receptor() {
    use crate::vgp::Alvo;
    let tmp = std::env::temp_dir().join("vgp-t-p0bd-incerto");
    let _ = std::fs::remove_dir_all(&tmp);
    let root = projeto(&tmp);
    // str/int/list/Path may be a subclass or another object than inferred: not closed
    for (src, nome) in [
        ("x = 'abc'\nx.setdefault(1)\n", "setdefault"),
        ("n = 1\nn.add(2)\n", "add"),
        ("l = [1]\nl.push(2)\n", "push"),
        (
            "from pathlib import Path\np = Path('/d')\nprint(p.st_m_size)\n",
            "st_m_size",
        ),
    ] {
        let out = check(&input(root.clone(), "<string>", src, HashMap::new()));
        let f = out
            .findings
            .iter()
            .find(|f| f.nome == nome)
            .unwrap_or_else(|| panic!("{src:?}: {:?}", out.findings));
        assert_eq!(f.alvo, Alvo::Incerto, "{src:?}");
        assert_eq!(f.receptor, None, "{src:?}");
        assert_eq!(f.severidade, "warn", "{src:?}");
    }
    let _ = std::fs::remove_dir_all(&tmp);
}

/// The oracle of "closed": the REAL interpreter, asked for the three conditions in this order —
/// not subclassable (`Py_TPFLAGS_BASETYPE`, bit 10, clear), instances without `__dict__`
/// (`__dictoffset__ == 0`), and no `__getattr__` in the MRO but `object`'s.
const CRITERIO_FECHADO_PY: &str = "\
import importlib, sys\n\
mod, _, nome = sys.argv[1].rpartition('.')\n\
t = getattr(importlib.import_module(mod), nome)\n\
nao_subclassavel = not (t.__flags__ & (1 << 10))\n\
sem_dict = t.__dictoffset__ == 0\n\
sem_getattr = not any('__getattr__' in vars(c) for c in t.__mro__ if c is not object)\n\
print(int(nao_subclassavel), int(sem_dict), int(sem_getattr))\n";

/// `(not subclassable, no __dict__, no __getattr__)` of the type at `caminho`, from `python3 -I`.
/// No `python3` FAILS the test — the model must never be unchecked because the interpreter is absent.
fn condicoes_no_runtime(caminho: &str) -> (bool, bool, bool) {
    let r = std::process::Command::new("python3")
        .args(["-I", "-c", CRITERIO_FECHADO_PY, caminho])
        .output()
        .expect(
            "python3 is required: the closed-type model is checked against the real interpreter",
        );
    let saida = String::from_utf8_lossy(&r.stdout).into_owned();
    assert!(
        r.status.success(),
        "{caminho}: {}",
        String::from_utf8_lossy(&r.stderr)
    );
    let bits: Vec<bool> = saida.split_whitespace().map(|b| b == "1").collect();
    assert_eq!(bits.len(), 3, "{caminho}: {saida:?}");
    (bits[0], bits[1], bits[2])
}

#[test]
fn p0bd_o_modelo_de_tipos_fechados_confere_com_o_runtime() {
    use super::stdlib_model::{TIPOS_FECHADOS, builtin_type_members, tipo_fechado};
    assert!(
        tipo_fechado("stat_result"),
        "os.stat_result is the closed type layer 1 already types (Path.stat())"
    );
    // every type the model marks closed satisfies the three conditions in the interpreter
    for (nome, caminho) in TIPOS_FECHADOS {
        assert!(
            builtin_type_members(nome).is_some(),
            "{nome}: a closed type needs a member table to be judged against"
        );
        assert_eq!(
            condicoes_no_runtime(caminho),
            (true, true, true),
            "{nome} ({caminho}) is marked closed but the runtime says otherwise"
        );
    }
    // the oracle discriminates: each control fails the condition it is here for
    for (caminho, esperado) in [
        ("builtins.str", (false, true, true)),
        ("builtins.int", (false, true, true)),
        ("builtins.list", (false, true, true)),
        ("builtins.dict", (false, true, true)),
        ("builtins.object", (false, true, true)),
        ("ast.expr", (false, false, true)),
        ("logging.LogRecord", (false, false, true)),
        // not subclassable, but its instances have a `__dict__`: only the `__dict__` condition rejects it
        ("types.FunctionType", (true, false, true)),
    ] {
        assert_eq!(condicoes_no_runtime(caminho), esperado, "{caminho}");
    }
    // and the model leaves every open type of its tables unmarked
    for nome in [
        "str",
        "int",
        "float",
        "list",
        "dict",
        "set",
        "frozenset",
        "Path",
    ] {
        assert!(!tipo_fechado(nome), "{nome} is subclassable: not closed");
    }
}
