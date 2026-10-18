//! VGP layer 2 against the REAL `pyright-langserver` of the `analise` project (spec V3 §5.2).
//!
//! Every test is `#[ignore]`: it needs the project venv's pyright and costs a cold start. Run explicitly:
//! `cargo test -p touring-pyright --test pyright_real -- --ignored`.
//! `VGP_L2_PROJECT` overrides the project root (default `~/projects/analise`).

use std::path::{Path, PathBuf};
use std::time::Duration;

use touring_pyright::pyright::{
    Escopo, L2Achado, L2Resultado, PyrightSession, Severidade, Verdict,
};

/// A real, import-clean module of the project used as the "real module" fixture and as an overlay target.
const REAL_MODULE: &str = "scripts/utils/leitura_fiel.py";
/// Cold start included: the first check of a fresh langserver analyses the project's import graph.
const COLD_BUDGET: Duration = Duration::from_secs(90);

fn project_root() -> PathBuf {
    if let Some(custom) = std::env::var_os("VGP_L2_PROJECT") {
        return PathBuf::from(custom);
    }
    let home = std::env::var_os("HOME").expect("HOME must be set");
    PathBuf::from(home).join("projects/analise")
}

fn session() -> PyrightSession {
    PyrightSession::discover(&project_root())
        .expect("the analise venv must provide pyright-langserver (never installed by Touring)")
        .with_budget(COLD_BUDGET)
}

fn real_module_path() -> PathBuf {
    project_root().join(REAL_MODULE)
}

fn real_module_text() -> String {
    std::fs::read_to_string(real_module_path()).expect("real module must be readable")
}

fn only<'a>(r: &'a L2Resultado, nome: &str) -> &'a L2Achado {
    let hits: Vec<&L2Achado> = r.achados.iter().filter(|a| a.nome == nome).collect();
    assert_eq!(
        hits.len(),
        1,
        "expected exactly one achado named {nome}: {r:#?}"
    );
    hits[0]
}

/// One invented form checked both as running code (inline) and as a project file (policy P3).
struct Invented {
    code: &'static str,
    nome: &'static str,
    classe: &'static str,
    escopo: Escopo,
    /// Severity when the code runs NOW (inline): deny, except `forma` which only warns.
    inline: Severidade,
    /// Severity when the same code sits in a project FILE.
    in_file: Severidade,
}

fn verdict_of(severity: Severidade) -> Verdict {
    match severity {
        Severidade::Deny => Verdict::Deny,
        Severidade::Warn => Verdict::Warn,
    }
}

fn check_invented(form: &Invented) {
    let mut s = session();
    let inline = s.check("<string>", form.code, None, &[]);
    let a = only(&inline, form.nome);
    assert_eq!(
        inline.verdict,
        verdict_of(form.inline),
        "inline policy: {inline:#?}"
    );
    assert_eq!(
        (a.classe.as_str(), a.escopo),
        (form.classe, form.escopo),
        "{a:#?}"
    );
    assert_eq!(a.severidade, form.inline);

    let file = project_root().join("scripts/_vgp_l2_forma_inventada.py");
    let in_file = s.check(file.to_str().expect("utf-8 path"), form.code, None, &[]);
    let b = only(&in_file, form.nome);
    assert_eq!(
        b.severidade, form.in_file,
        "policy P3 for a project file: {b:#?}"
    );
    assert_eq!(in_file.verdict, verdict_of(form.in_file), "{in_file:#?}");
}

#[test]
#[ignore = "needs the real pyright-langserver of the analise project"]
fn invented_import_of_a_name_missing_from_a_project_module() {
    check_invented(&Invented {
        code: "from scripts.utils.leitura_fiel import nome_inventado_vgp\n",
        nome: "nome_inventado_vgp",
        classe: "import",
        escopo: Escopo::Projeto,
        inline: Severidade::Deny,
        in_file: Severidade::Warn,
    });
}

#[test]
#[ignore = "needs the real pyright-langserver of the analise project"]
fn invented_attribute_of_a_stdlib_module() {
    check_invented(&Invented {
        code: "import os\nprint(os.caminho_inventado)\n",
        nome: "caminho_inventado",
        classe: "atributo",
        escopo: Escopo::Externo,
        inline: Severidade::Deny,
        in_file: Severidade::Deny,
    });
}

#[test]
#[ignore = "needs the real pyright-langserver of the analise project"]
fn invented_method_of_a_str() {
    // `read_text()` is inferred `str`, so pyright reports `class "str"` (a concrete class): a member
    // of a builtin value type is `forma` and only warns, inline included. A bare literal receiver
    // reads `class "Literal['x']"` — see `known_gaps_*` below.
    check_invented(&Invented {
        code: "import pathlib\nconteudo = pathlib.Path('.').read_text()\nconteudo.metodo_inventado()\n",
        nome: "metodo_inventado",
        classe: "membro",
        escopo: Escopo::Forma,
        inline: Severidade::Warn,
        in_file: Severidade::Warn,
    });
}

#[test]
#[ignore = "needs the real pyright-langserver of the analise project"]
fn invented_member_of_a_stdlib_class_instance_is_project_scope_and_denies_inline() {
    // `os.stat()` returns a `stat_result`: a concrete class that is NOT a builtin value type, so it is
    // not `forma`. The probe only knows builtins by name, so it cannot tell a stdlib class from a
    // project one and answers the conservative `projeto`: inline still denies, a project file warns.
    check_invented(&Invented {
        code: "import os\nresultado = os.stat('.')\nresultado.campo_inventado\n",
        nome: "campo_inventado",
        classe: "membro",
        escopo: Escopo::Projeto,
        inline: Severidade::Deny,
        in_file: Severidade::Warn,
    });
}

#[test]
#[ignore = "needs the real pyright-langserver of the analise project"]
fn invented_undefined_name() {
    check_invented(&Invented {
        code: "print(nome_indefinido_vgp)\n",
        nome: "nome_indefinido_vgp",
        classe: "nome",
        escopo: Escopo::Proprio,
        inline: Severidade::Deny,
        in_file: Severidade::Warn,
    });
}

#[test]
#[ignore = "needs the real pyright-langserver of the analise project"]
fn undefined_name_under_a_star_import_is_dynamic_and_only_warns() {
    // `from os.path import *` makes the name space unknowable: the checker cannot say the name is
    // missing. pyright also emits `reportWildcardImportFromLibrary`, which is not a negation.
    check_invented(&Invented {
        code: "from os.path import *\nprint(nome_que_nao_existe)\n",
        nome: "nome_que_nao_existe",
        classe: "nome",
        escopo: Escopo::Dinamico,
        inline: Severidade::Warn,
        in_file: Severidade::Warn,
    });
}

#[test]
#[ignore = "needs the real pyright-langserver of the analise project"]
fn dunder_attribute_of_a_class_is_dynamic_and_only_warns() {
    // Protocol attributes (`__wrapped__` from functools.wraps, `__self__`…) are created at run time by
    // decorators, descriptors or C extensions; the static checker does not model them.
    check_invented(&Invented {
        code: "class K:\n    pass\n\n\nK().__wrapped__\n",
        nome: "__wrapped__",
        classe: "membro",
        escopo: Escopo::Dinamico,
        inline: Severidade::Warn,
        in_file: Severidade::Warn,
    });
}

#[test]
#[ignore = "needs the real pyright-langserver of the analise project"]
fn invented_keyword_argument() {
    // A non-overloaded callee: pyright says `No parameter named "…"`. An overloaded one (`print`)
    // says `No overloads for "print" match…` — see `known_gaps_*` below.
    check_invented(&Invented {
        code: "def f(a):\n    return a\n\n\nf(a=1, nao_existe=2)\n",
        nome: "nao_existe",
        classe: "kwarg",
        escopo: Escopo::Projeto,
        inline: Severidade::Deny,
        in_file: Severidade::Warn,
    });
}

/// Pins what the faithful port of the measured probe does NOT deny today (measured 09/10/2026 on
/// pyright 1.1.408): these shapes are invented symbols, but pyright's message carries no concrete
/// class / parameter name. If `negacao` is ever extended (e.g. to read `Literal['x']` as `str`), this
/// test must change on purpose — after re-measuring the false-deny rate on the V0d corpus.
#[test]
#[ignore = "needs the real pyright-langserver of the analise project"]
fn known_gaps_literal_receiver_overloaded_callee_and_class_object_are_not_negations() {
    let mut s = session();
    for code in [
        "valor = \"x\".metodo_inventado()\n",
        "print(nao_existe=1)\n",
        // a class object reads `class "type[stat_result]"`, not a plain class name
        "import os\nos.stat_result.campo_inventado\n",
    ] {
        let r = s.check("<string>", code, None, &[]);
        assert_eq!(r.verdict, Verdict::Allow, "{code:?} → {r:#?}");
        assert!(r.achados.is_empty(), "{code:?} → {r:#?}");
    }
}

#[test]
#[ignore = "needs the real pyright-langserver of the analise project"]
fn clean_code_and_a_real_module_have_no_achado() {
    let mut s = session();
    let snippet = s.check("<string>", "import os\nprint(os.getcwd())\n", None, &[]);
    assert_eq!(snippet.verdict, Verdict::Allow, "{snippet:#?}");
    assert!(snippet.achados.is_empty(), "{snippet:#?}");

    let path = real_module_path();
    let real = s.check(
        path.to_str().expect("utf-8 path"),
        &real_module_text(),
        None,
        &[],
    );
    assert_eq!(
        real.verdict,
        Verdict::Allow,
        "a real, committed module must be clean: {real:#?}"
    );
    assert!(real.achados.is_empty(), "{real:#?}");
    assert_eq!(real.motivo, None);
}

#[test]
#[ignore = "needs the real pyright-langserver of the analise project"]
fn reopening_the_same_uri_returns_each_contents_own_result_not_the_cleanup() {
    let mut s = session();
    let file = project_root().join("scripts/_vgp_l2_reabertura.py");
    let path = file.to_str().expect("utf-8 path");
    let contents = [
        "print(nome_a)\n",
        "print(nome_b)\n",
        "print('limpo')\n",
        "print(nome_c)\n",
    ];
    let names: Vec<Vec<String>> = contents
        .iter()
        .map(|c| {
            s.check(path, c, None, &[])
                .achados
                .into_iter()
                .map(|a| a.nome)
                .collect()
        })
        .collect();
    assert_eq!(names[0], ["nome_a"]);
    assert_eq!(
        names[1],
        ["nome_b"],
        "2nd result must be the 2nd content's, not the didClose cleanup"
    );
    assert!(names[2].is_empty(), "{names:?}");
    assert_eq!(names[3], ["nome_c"]);
    // …and the same holds when the baseline (disk) is opened on the very same uri
    let r = s.check(
        path,
        "print(nome_novo)\nprint(nome_a)\n",
        Some("print(nome_a)\n"),
        &[],
    );
    let got: Vec<&str> = r.achados.iter().map(|a| a.nome.as_str()).collect();
    assert_eq!(got, ["nome_novo"], "{r:#?}");
}

#[test]
#[ignore = "needs the real pyright-langserver of the analise project"]
fn delta_against_the_real_disk_content_blames_only_the_edit() {
    let mut s = session();
    let disk = real_module_text();
    let proposed = format!("{disk}\nprint(nome_da_edicao_vgp)\n");
    let path = real_module_path();
    let r = s.check(
        path.to_str().expect("utf-8 path"),
        &proposed,
        Some(&disk),
        &[],
    );
    let got: Vec<&str> = r.achados.iter().map(|a| a.nome.as_str()).collect();
    assert_eq!(got, ["nome_da_edicao_vgp"], "{r:#?}");
    assert_eq!(
        r.verdict,
        Verdict::Warn,
        "own name in a project file only warns: {r:#?}"
    );
}

#[test]
#[ignore = "needs the real pyright-langserver of the analise project"]
fn overlay_makes_an_unsaved_symbol_resolvable() {
    let mut s = session();
    let code = "from scripts.utils.leitura_fiel import simbolo_so_no_overlay_vgp\n";
    let without = s.check("<string>", code, None, &[]);
    assert_eq!(only(&without, "simbolo_so_no_overlay_vgp").classe, "import");

    let overlay = (
        real_module_path().to_string_lossy().into_owned(),
        format!("{}\nsimbolo_so_no_overlay_vgp = 1\n", real_module_text()),
    );
    let with = s.check("<string>", code, None, std::slice::from_ref(&overlay));
    assert!(
        with.achados.is_empty(),
        "the open overlay defines the symbol: {with:#?}"
    );
    // closed afterwards: the next check without the overlay sees the disk again
    let after = s.check("<string>", code, None, &[]);
    only(&after, "simbolo_so_no_overlay_vgp");
}

#[test]
#[ignore = "needs the real pyright-langserver of the analise project"]
fn warm_latency_p95_fits_the_one_second_budget() {
    let mut s = session();
    let code = "import os\nprint(os.caminho_inventado)\n";
    let _warmup = s.check("<string>", code, None, &[]);
    let mut s = s.with_budget(Duration::from_secs(1));
    let mut ms: Vec<u64> = (0..12)
        .map(|i| {
            let r = s.check("<string>", &format!("{code}# {i}\n"), None, &[]);
            assert_ne!(
                r.verdict,
                Verdict::Unknown,
                "warm check must not time out: {r:#?}"
            );
            r.elapsed_ms
        })
        .collect();
    ms.sort_unstable();
    let p95 = ms[ms.len() * 95 / 100];
    println!("warm latency ms (sorted): {ms:?} p95={p95}");
    assert!(p95 <= 1000, "p95 {p95} ms exceeds the V3 budget");
}

#[test]
#[ignore = "needs the real pyright-langserver of the analise project"]
fn session_for_a_project_without_a_venv_fails_open_with_a_reason() {
    let missing = Path::new("/nonexistent/vgp_l2_project");
    let Err(e) = PyrightSession::discover(missing) else {
        panic!("a project without a venv has no layer 2");
    };
    assert!(e.to_string().contains("pyright-langserver"), "{e}");
}
