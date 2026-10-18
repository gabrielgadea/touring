//! TDD fixtures for the VGP checks — each test replicates a measured corpus positive (or a
//! measured negative, in the precision guards). The fixture project mirrors the shapes of the
//! analise codebase: `scripts/memoria/grafo_gravacao.py` (Recibo dataclass) and
//! `scripts/eleitoral/auditar_coerencia.py` (module constants).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::vgp::{CheckInput, check};

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
