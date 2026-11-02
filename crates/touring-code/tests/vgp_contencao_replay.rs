//! Replay of the VGP verdicts over a synthetic fixture tree (deepening #8, 02/11/2026).
//!
//! The language adapters read the filesystem; whether they may read a path is decided by
//! `vgp::lang::caminho`. This test pins WHAT THE MOTOR ANSWERS, per adapter, over a tree with
//! the normal cases and the adversarial ones: a `..` escape, a symlink that leaves the root, an
//! absolute path outside the root, a `#[path]` escape, an `include_str!` of a file outside, a
//! tsconfig whose `paths` / `extends` point out, a non-UTF-8 file, a missing file, a file over
//! the read cap.
//!
//! The recording (`fixtures/vgp_contencao_replay.json`) was taken from the code BEFORE the
//! contained read moved into `caminho.rs`. The replay reproduces it byte for byte, except for
//! the cases named in [`JUSTIFIED_DIFFERENCES`] — and an entry there needs a reason: a
//! difference that CLOSES an escape may be listed; one that OPENS access is a bug, never an
//! entry.
//!
//! The tree is built at run time (symlinks and a file over the read cap cannot be committed),
//! and the temp dir is replaced by `<TMP>` in the answers, so the recording holds no
//! machine-specific path.
//!
//! Re-record (only when a verdict is meant to change): `VGP_REPLAY_RECORD=<file> cargo test
//! -p touring-code --test vgp_contencao_replay`.

#![cfg(unix)]

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::time::Instant;

use serde_json::Value;
use touring_code::vgp::{CheckInput, Contexto, check_em};

const GRAVADO: &str = include_str!("fixtures/vgp_contencao_replay.json");

/// Cases whose answer may differ from the recording: (id, WHY, the NEW answer, reviewed and pinned).
/// Pinned, not just "differs": a difference that is allowed but not exact would hide a regression
/// that still happens to differ. Empty = byte for byte.
const JUSTIFIED_DIFFERENCES: &[(&str, &str, &str)] = &[
    (
        "html_raiz_relativa_com_dotdot",
        "Closes the existence oracle: a root-relative `/..` climbed out of the project and `Path::exists` \
         answered for a file OUTSIDE the roots (the recording is `allow`: the file is there). The probe is \
         contained now; the reference reads as unresolved (`Alvo::Incerto`, a warning, never a deny).",
        r##"{"findings":[{"classe":"recurso","linha":7,"nome":"/../../../fora/segredo.png","reais":[],"severidade":"warn","sugestao":"recurso local inexistente: `/../../../fora/segredo.png` (img src) — resolvido como `<TMP>/proj/../../../fora/segredo.png`"}],"lang":"html","layer":1,"verdict":"warn"}"##,
    ),
    (
        "rust_lib_path_fora",
        "A manifest `[lib] path` that points outside the project is no longer probed with `is_file` (an \
         existence probe outside the roots, closed): it counts as an absent path, so the crate root falls \
         back to `src/lib.rs` and `crate::…` is measured instead of left unresolved. No file outside the \
         roots is read in either version.",
        r##"{"findings":[{"classe":"import","linha":1,"nome":"nada","reais":[],"severidade":"warn","sugestao":"módulo `nada` declarado sem arquivo (`nada.rs` | `nada/mod.rs`)"},{"classe":"import","linha":2,"nome":"fantasma_lp","reais":[],"severidade":"warn","sugestao":"`fantasma_lp` não encontrado ao resolver `crate::fantasma_lp`"}],"lang":"rust","layer":1,"verdict":"warn"}"##,
    ),
    (
        "ts_base_url_aponta_para_fora",
        "Closes an escape: a tsconfig `baseUrl` outside the project made the import resolve to a file \
         OUTSIDE the roots, which was read (its export names were printed in `reais`). Now the import is \
         not measured, with the reason and without the path.",
        r##"{"findings":[],"lang":"typescript","layer":1,"motivo":"`externo`: caminho fora das raízes permitidas","verdict":"unknown"}"##,
    ),
    (
        "ts_extends_aponta_para_fora",
        "Closes an escape: a tsconfig `extends` outside the project was read as JSON and its `paths` \
         were followed to a file outside the roots. The `extends` is refused, the alias is unknown, and \
         the import is not measured.",
        r##"{"findings":[],"lang":"typescript","layer":1,"motivo":"1 pacote(s) externo(s) importado(s), não verificados na camada 1: @b/externo (a camada 2 do projeto verifica)","verdict":"unknown"}"##,
    ),
    (
        "ts_package_json_main_aponta_para_fora",
        "Closes an escape: a package.json `main` pointing out of the project resolved the import to a \
         file OUTSIDE the roots, which was read. Now the import is not measured, with the reason and \
         without the path.",
        r##"{"findings":[],"lang":"typescript","layer":1,"motivo":"`./pkgesc`: caminho fora das raízes permitidas","verdict":"unknown"}"##,
    ),
    (
        "ts_paths_apontam_para_fora",
        "Closes an escape: a tsconfig `paths` target outside the project resolved the import to a file \
         OUTSIDE the roots, which was read (its export names were printed in `reais`). Now the import is \
         not measured, with the reason and without the path.",
        r##"{"findings":[],"lang":"typescript","layer":1,"motivo":"`@o/externo`: caminho fora das raízes permitidas","verdict":"unknown"}"##,
    ),
    (
        "ts_symlink_fora",
        "Closes an escape: an import that resolved to a symlink leaving the project read the file it \
         points to (its export names were printed in `reais`). The relative-specifier check only judged \
         the extension-less base; the probe and the read are contained now, and the import is not \
         measured.",
        r##"{"findings":[],"lang":"typescript","layer":1,"motivo":"`./link_fora`: caminho fora das raízes permitidas","verdict":"unknown"}"##,
    ),
];

/// A path that climbs `n` levels, built by component (the literal traversal in a source file is
/// what the F2.1/CWE-22 rule flags — and the corpus needs exactly the dangerous path).
fn sobe(n: usize) -> String {
    vec![".."; n].join("/")
}

fn escreve(base: &Path, rel: &str, conteudo: impl AsRef<[u8]>) {
    let alvo = base.join(rel);
    fs::create_dir_all(alvo.parent().expect("pai")).expect("mkdir");
    fs::write(alvo, conteudo).expect("write");
}

/// `onde` becomes a symlink to `alvo`.
fn elo(alvo: &Path, base: &Path, onde: &str) {
    let destino = base.join(onde);
    fs::create_dir_all(destino.parent().expect("pai")).expect("mkdir");
    symlink(alvo, destino).expect("symlink");
}

/// One input of the motor: the project it runs in, the file it proposes, the content.
struct Caso {
    id: String,
    raiz: PathBuf,
    caminho: PathBuf,
    lang: &'static str,
    conteudo: String,
    /// Overlay path -> content: module files at the entry's instant, laid over the disk.
    overlays: Vec<(PathBuf, String)>,
}

fn caso(id: &str, raiz: &Path, rel: &str, lang: &'static str, conteudo: &str) -> Caso {
    Caso {
        id: id.to_string(),
        raiz: raiz.to_path_buf(),
        // `join` with an absolute `rel` replaces the base: a fragment OUTSIDE the project
        caminho: raiz.join(rel),
        lang,
        conteudo: conteudo.to_string(),
        overlays: Vec::new(),
    }
}

/// The same case with one overlay laid over the disk.
fn com_overlay(mut c: Caso, caminho: PathBuf, conteudo: &str) -> Caso {
    c.overlays.push((caminho, conteudo.to_string()));
    c
}

/// The tree. `proj` is the project; `fora` is a sibling directory OUTSIDE every root.
struct Mundo {
    proj: PathBuf,
    fora: PathBuf,
    ws: PathBuf,
    lp: PathBuf,
    /// Small separate projects for the JSX class check: it reads EVERY stylesheet the project
    /// owns, so one adversarial sheet in a shared root would make every other case abstain.
    jx: Jsx,
    /// The Tailwind project: the generator marker makes the class check abstain.
    tw: PathBuf,
    /// A project with no `node_modules` of its own (the globals come from the one above it).
    alto: PathBuf,
}

/// One root per way a JSX project can feed the class check.
struct Jsx {
    normal: PathBuf,
    nao_utf8: PathBuf,
    import_fora: PathBuf,
    entrada_html: PathBuf,
    pacote: PathBuf,
    alias: PathBuf,
    alias_fora: PathBuf,
    folha_gigante: PathBuf,
}

fn monta(tmp: &Path) -> Mundo {
    let proj = tmp.join("proj");
    let fora = tmp.join("fora");
    let ws = tmp.join("ws");
    let lp = tmp.join("lp");
    let nao_utf8: &[u8] = &[0xff, 0xfe, 0xfa, b'=', 0x80, b'\n'];

    // ---------------------------------------------------------------- outside the roots
    escreve(&fora, "externo.rs", "pub fn real_externo() {}\n");
    escreve(&fora, "externo.py", "def real_ext():\n    pass\n");
    escreve(&fora, "segredo.txt", "segredo\n");
    escreve(&fora, "segredo.png", "png");
    escreve(&fora, "estilo.css", ".so-no-fora { color: red }\n");
    escreve(&fora, "frag_helper.py", "def helper_fn():\n    pass\n");
    escreve(&fora, "ts/externo.ts", "export function realExt() {}\n");
    // a TS file OUTSIDE the project looks for its config walking up: this one stops the walk, so
    // the recording never depends on what lives above the temp dir on the machine that runs it
    escreve(&fora, "tsconfig.json", "{}\n");
    escreve(
        &fora,
        "base.json",
        r#"{"compilerOptions":{"baseUrl":".","paths":{"@b/*":["ts/*"]}}}"#,
    );
    escreve(&fora, "extdir/nativo.cpython-312-x86_64-linux-gnu.so", "x");
    escreve(&fora, "pagina/segredo.png", "png");
    escreve(&fora, "pagina/solto.rs", "pub fn x() {}\n");
    escreve(&fora, "pagina/vizinho.rs", "pub fn y() {}\n");
    // above the roots: the hoisted node_modules a monorepo layout leaves next to the project
    escreve(
        tmp,
        "node_modules/alto/package.json",
        r#"{"name":"alto","types":"index.d.ts"}"#,
    );
    escreve(
        tmp,
        "node_modules/@types/hoistedtipos/index.d.ts",
        "declare const globalHoisted: number;\n",
    );
    escreve(
        &ws.join("../fora_ws"),
        "outro/Cargo.toml",
        "[package]\nname = 'outro_externo'\nversion = '0.1.0'\n",
    );
    escreve(
        &ws.join("../fora_ws"),
        "outro/src/lib.rs",
        "pub fn x() {}\n",
    );

    // ---------------------------------------------------------------- rust
    escreve(
        &proj,
        "Cargo.toml",
        "[package]\nname = 'demo'\nversion = '0.1.0'\nedition = '2021'\n",
    );
    escreve(&proj, "src/lib.rs", "pub mod a;\npub mod sub;\nmod decl;\n");
    escreve(&proj, "src/a.rs", "pub fn real_a() {}\n");
    escreve(&proj, "src/sub/mod.rs", "pub fn real_sub() {}\n");
    escreve(&proj, "src/binario.rs", nao_utf8);
    escreve(
        &proj,
        "src/decl.rs",
        "#[path = \"alvo_tests.rs\"]\nmod alvo_tests;\npub fn do_decl() {}\n",
    );
    escreve(&proj, "src/alvo_tests.rs", "pub fn do_alvo() {}\n");
    elo(&fora.join("externo.rs"), &proj, "src/link_fora.rs");
    // a workspace whose member glob climbs out of the project
    escreve(
        &ws,
        "Cargo.toml",
        format!(
            "[workspace]\nmembers = ['{}/fora_ws/*', 'member']\n",
            sobe(1)
        ),
    );
    escreve(
        &ws,
        "member/Cargo.toml",
        "[package]\nname = 'member'\nversion = '0.1.0'\n",
    );
    escreve(&ws, "member/src/lib.rs", "pub fn y() {}\n");
    // a manifest whose `[lib] path` points outside the project
    escreve(
        &lp,
        "Cargo.toml",
        format!(
            "[package]\nname = 'lp'\nversion = '0.1.0'\n[lib]\npath = '{}/fora/externo.rs'\n",
            sobe(1)
        ),
    );
    escreve(&lp, "src/lib.rs", "pub fn do_lp() {}\n");

    // ---------------------------------------------------------------- python
    escreve(&proj, "pkg/__init__.py", "");
    escreve(
        &proj,
        "pkg/mod.py",
        "def real_fn():\n    pass\n\n\nclass RealCls:\n    pass\n",
    );
    escreve(&proj, "pkg/vazio.py", "");
    fs::create_dir_all(proj.join("pkg/dirmod.py")).expect("mkdir");
    escreve(&proj, "pkg/ruim.py", nao_utf8);
    elo(&fora.join("externo.py"), &proj, "pkg/link_fora.py");
    escreve(&proj, "pkg/_ext.cpython-312-x86_64-linux-gnu.so", "x");
    elo(&fora.join("extdir"), &proj, "pkg/linkdir");
    escreve(
        &proj,
        ".venv/lib/python3.12/site-packages/sitepkg/__init__.py",
        "def sp_fn():\n    pass\n",
    );
    elo(
        &fora.join("externo.py"),
        &proj,
        ".venv/lib/python3.12/site-packages/escape.py",
    );
    escreve(
        &proj,
        ".venv/lib/python3.12/site-packages/nativo_sp.abi3.so",
        "x",
    );
    escreve(&proj, "pkg/inner.py", "def inner():\n    pass\n");

    // ---------------------------------------------------------------- typescript / javascript
    escreve(
        &proj,
        "web/tsconfig.json",
        format!(
            r#"{{"compilerOptions":{{"baseUrl":".","paths":{{"@app/*":["src/*"],"@o/*":["{up}/fora/ts/*"],"alto/*":["nada/*"],"hoist/*":["nada/*"]}}}}}}"#,
            up = sobe(2)
        ),
    );
    escreve(
        &proj,
        "web/src/util.ts",
        "export function realFn() {}\nexport const REAL = 1;\n",
    );
    escreve(&proj, "web/src/util.js", "export function realJs() {}\n");
    escreve(&proj, "web/src/ruim.ts", nao_utf8);
    escreve(&proj, "web/src/enorme.ts", vec![b'a'; 3 * 1024 * 1024 + 8]);
    elo(&fora.join("ts/externo.ts"), &proj, "web/src/link_fora.ts");
    escreve(&proj, "web/node_modules/instalado/package.json", "{}");
    escreve(
        &proj,
        "web/node_modules/@types/dentrotipos/index.d.ts",
        "declare const globalDentro: number;\n",
    );
    escreve(
        &proj,
        "web/node_modules/typescript/lib/lib.es5.d.ts",
        "declare const globalDaLib: number;\n",
    );
    escreve(
        &proj,
        "web2/tsconfig.json",
        format!(r#"{{"extends":"{}/fora/base.json"}}"#, sobe(2)),
    );
    escreve(&proj, "web2/src/util.ts", "export function realFn2() {}\n");
    // a package.json whose `main` points out of the project
    escreve(
        &proj,
        "web/src/pkgesc/package.json",
        format!(r#"{{"main":"{}/fora/ts/externo.ts"}}"#, sobe(4)),
    );
    escreve(
        &proj,
        "web/src/pkgok/package.json",
        r#"{"types":"./tipos.d.ts"}"#,
    );
    escreve(
        &proj,
        "web/src/pkgok/tipos.d.ts",
        "export declare function realTipo(): void;\n",
    );
    // a baseUrl that points out of the project
    escreve(
        &proj,
        "web3/tsconfig.json",
        format!(
            r#"{{"compilerOptions":{{"baseUrl":"{}/fora/ts"}}}}"#,
            sobe(2)
        ),
    );
    // a project with NO node_modules of its own: the globals come from the one above it
    let alto = tmp.join("hoistroot");
    escreve(&alto, "src/main.ts", "export const A = 1;\n");
    escreve(
        tmp,
        "node_modules/typescript/lib/lib.es5.d.ts",
        "declare const globalLibAlto: number;\n",
    );

    // ---------------------------------------------------------------- html / css
    escreve(&proj, "site/css/a.css", ".in-a { } .also-a { }\n");
    escreve(&proj, "site/css/b.css", "@import \"c.css\";\n.in-b { }\n");
    escreve(&proj, "site/css/c.css", ".in-c { }\n");
    escreve(
        &proj,
        "site/css/imp_fora.css",
        format!("@import \"{}/fora/estilo.css\";\n.in-impf {{ }}\n", sobe(3)),
    );
    escreve(&proj, "site/css/ruim.css", nao_utf8);
    elo(&fora.join("estilo.css"), &proj, "site/css/link_fora.css");
    escreve(&proj, "site/img/logo.png", "png");
    elo(&fora.join("segredo.png"), &proj, "site/img/link.png");
    elo(&fora.join("pagina"), &proj, "site/linkdir");
    escreve(
        &proj,
        "site/node_modules/lib-ui/dist/ui.css",
        ".ui-btn { }\n",
    );
    escreve(&proj, "site/js/app.js", "var x = 1;\n");

    // ---------------------------------------------------------------- the Tailwind project
    let tw = tmp.join("tw");
    escreve(
        &tw,
        "package.json",
        r#"{"devDependencies":{"tailwindcss":"^3"}}"#,
    );
    escreve(&tw, "css/a.css", ".in-a { }\n");

    // ---------------------------------------------------------------- the JSX projects
    let jsx = |nome: &str| tmp.join(nome);
    let jx = Jsx {
        normal: jsx("jx_normal"),
        nao_utf8: jsx("jx_nao_utf8"),
        import_fora: jsx("jx_import_fora"),
        entrada_html: jsx("jx_entrada_html"),
        pacote: jsx("jx_pacote"),
        alias: jsx("jx_alias"),
        alias_fora: jsx("jx_alias_fora"),
        folha_gigante: jsx("jx_gigante"),
    };
    escreve(&jx.normal, "src/ui.css", ".ui-a { }\n.ui-b { }\n");
    elo(&fora.join("estilo.css"), &jx.normal, "src/link.css");
    escreve(&jx.nao_utf8, "src/ui.css", ".ui-a { }\n");
    escreve(&jx.nao_utf8, "src/ruim.css", nao_utf8);
    escreve(
        &jx.import_fora,
        "src/ui.css",
        format!("@import \"{}/fora/estilo.css\";\n.ui-a {{ }}\n", sobe(3)),
    );
    escreve(
        &jx.entrada_html,
        "index.html",
        pagina("<link rel=\"stylesheet\" href=\"css/b.css\">", "<p>x</p>"),
    );
    escreve(
        &jx.entrada_html,
        "css/b.css",
        "@import \"c.css\";\n.in-b { }\n",
    );
    escreve(&jx.entrada_html, "css/c.css", ".in-c { }\n");
    escreve(&jx.entrada_html, "src/app.tsx", "export const A = 1;\n");
    escreve(
        &jx.pacote,
        "node_modules/lib-ui/dist/ui.css",
        ".ui-btn { }\n",
    );
    escreve(&jx.pacote, "src/app.tsx", "export const A = 1;\n");
    escreve(&jx.alias, "src/styles/x.css", ".alias-a { }\n");
    escreve(&jx.alias, "src/app.tsx", "export const A = 1;\n");
    escreve(&jx.alias_fora, "src/app.tsx", "export const A = 1;\n");
    escreve(&jx.alias_fora, "src/ok.css", ".ok { }\n");
    escreve(
        &jx.folha_gigante,
        "src/enorme.css",
        vec![b'a'; 4 * 1024 * 1024 + 8],
    );

    Mundo {
        proj,
        fora,
        ws,
        lp,
        jx,
        tw,
        alto,
    }
}

fn pagina(cabeca: &str, corpo: &str) -> String {
    format!(
        "<!DOCTYPE html>\n<html>\n<head>\n{cabeca}\n</head>\n<body>\n{corpo}\n</body>\n</html>\n"
    )
}

fn casos(m: &Mundo) -> Vec<Caso> {
    let p = m.proj.as_path();
    let f = m.fora.as_path();
    let abs_fora = f.to_string_lossy();
    let abs_proj = p.to_string_lossy();
    let mut c: Vec<Caso> = Vec::new();

    // ------------------------------------------------------------------ rust
    let rs = |id: &str, conteudo: &str| caso(id, p, "src/lib.rs", "rust", conteudo);
    c.push(rs(
        "rust_modulo_no_disco",
        "mod a;\nfn f() { a::real_a(); a::fantasma_a(); }\n",
    ));
    c.push(rs(
        "rust_modulo_dir_mod",
        "mod sub;\nfn f() { sub::real_sub(); sub::fantasma_sub(); }\n",
    ));
    c.push(rs(
        "rust_path_relativo_dentro",
        "#[path = \"a.rs\"]\nmod b;\nfn f() { b::real_a(); b::fantasma_b(); }\n",
    ));
    c.push(rs(
        "rust_path_dotdot_fora",
        &format!(
            "#[path = \"{}/fora/externo.rs\"]\nmod x;\nfn f() {{ x::real_externo(); x::fantasma_x(); }}\n",
            sobe(2)
        ),
    ));
    c.push(rs(
        "rust_path_absoluto_fora",
        &format!(
            "#[path = \"{abs_fora}/externo.rs\"]\nmod x;\nfn f() {{ x::real_externo(); x::fantasma_xa(); }}\n"
        ),
    ));
    c.push(rs(
        "rust_path_absoluto_dentro",
        &format!(
            "#[path = \"{abs_proj}/src/a.rs\"]\nmod c;\nfn f() {{ c::real_a(); c::fantasma_c(); }}\n"
        ),
    ));
    c.push(rs(
        "rust_symlink_fora",
        "mod link_fora;\nfn f() { link_fora::real_externo(); link_fora::fantasma_l(); }\n",
    ));
    c.push(rs(
        "rust_arquivo_nao_utf8",
        "mod binario;\nfn f() { binario::x(); binario::fantasma_b(); }\n",
    ));
    c.push(rs(
        "rust_arquivo_ausente",
        "mod inexistente;\nfn f() { inexistente::x(); }\n",
    ));
    c.push(rs(
        "rust_include_str_fora",
        &format!(
            "const X: &str = include_str!(\"{}/fora/segredo.txt\");\nfn f() {{ fantasma_inc(); }}\n",
            sobe(2)
        ),
    ));
    c.push(rs(
        "rust_use_crate",
        "use crate::a::real_a;\nuse crate::a::fantasma_use;\n",
    ));
    c.push(caso(
        "rust_vizinhos_declarante",
        p,
        "src/alvo_tests.rs",
        "rust",
        "use super::do_decl;\nuse super::fantasma_super;\n",
    ));
    c.push(caso(
        "rust_vizinhos_fora_da_raiz",
        p,
        &format!("{abs_fora}/pagina/solto.rs"),
        "rust",
        "use super::x;\nfn f() { fantasma_solto(); }\n",
    ));
    c.push(caso(
        "rust_workspace_membro_fora",
        &m.ws,
        "member/src/lib.rs",
        "rust",
        "fn f() { outro_externo::x(); outro_externo::fantasma_w(); member::y(); }\n",
    ));
    c.push(caso(
        "rust_lib_path_fora",
        &m.lp,
        "src/lib.rs",
        "rust",
        "mod nada;\nfn f() { crate::fantasma_lp(); }\n",
    ));

    c.push(com_overlay(
        caso(
            "rust_overlay_de_modulo_novo_dentro",
            p,
            "src/lib.rs",
            "rust",
            "mod novo;\nfn f() { novo::real_novo(); novo::fantasma_nv(); }\n",
        ),
        p.join("src/novo.rs"),
        "pub fn real_novo() {}\n",
    ));
    c.push(com_overlay(
        caso(
            "rust_overlay_de_arquivo_fora_da_raiz",
            p,
            "src/lib.rs",
            "rust",
            &format!(
                "#[path = \"{abs_fora}/overlay_fora.rs\"]\nmod ov;\nfn f() {{ ov::real_ov(); ov::fantasma_ov(); }}\n"
            ),
        ),
        f.join("overlay_fora.rs"),
        "pub fn real_ov() {}\n",
    ));

    // ------------------------------------------------------------------ python
    let py = |id: &str, conteudo: &str| caso(id, p, "main.py", "python", conteudo);
    c.push(py(
        "python_no_projeto",
        "from pkg.mod import real_fn, fantasma_py\nimport pkg.sem_modulo\n",
    ));
    c.push(py(
        "python_symlink_fora",
        "from pkg.link_fora import real_ext, fantasma_ln\n",
    ));
    c.push(py(
        "python_venv_site_packages",
        "from sitepkg import sp_fn, fantasma_sp\n",
    ));
    c.push(py(
        "python_venv_symlink_fora",
        "from escape import real_ext, fantasma_es\n",
    ));
    c.push(py("python_nao_utf8", "from pkg.ruim import x\n"));
    c.push(py("python_ausente", "import modulo_que_nao_existe_xyz\n"));
    c.push(py("python_arquivo_vazio", "from pkg.vazio import algo\n"));
    c.push(py(
        "python_diretorio_como_arquivo",
        "from pkg.dirmod import algo\n",
    ));
    c.push(py("python_extensao_no_projeto", "from pkg import _ext\n"));
    c.push(py(
        "python_extensao_em_dir_symlink",
        "from pkg.linkdir import nativo\n",
    ));
    c.push(py(
        "python_extensao_no_venv",
        "import nativo_sp\nimport pkg\n",
    ));
    c.push(caso(
        "python_fragmento_irmao",
        p,
        &format!("{abs_fora}/frag.py"),
        "python",
        "from frag_helper import helper_fn, fantasma_fr\n",
    ));
    c.push(caso(
        "python_fragmento_no_pai",
        p,
        &format!("{abs_fora}/sub/frag2.py"),
        "python",
        "from frag_helper import helper_fn\n",
    ));
    c.push(caso(
        "python_relativo_fundo",
        p,
        "pkg/inner.py",
        "python",
        "from ....pkg import mod\nfrom . import fantasma_rel\n",
    ));

    c.push(com_overlay(
        py(
            "python_overlay_dentro",
            "from pkg.novo_mod import real_nv, fantasma_ovp\n",
        ),
        p.join("pkg/novo_mod.py"),
        "def real_nv():\n    pass\n",
    ));

    // ------------------------------------------------------------------ typescript / javascript
    let ts = |id: &str, conteudo: &str| caso(id, p, "web/src/main.ts", "typescript", conteudo);
    c.push(ts(
        "ts_no_projeto",
        "import { realFn, fantasmaFn } from './util';\nimport sem from './sem_modulo';\n",
    ));
    c.push(ts(
        "ts_relativo_fora",
        &format!("import y from '{}/fora/ts/externo';\n", sobe(3)),
    ));
    c.push(ts(
        "ts_symlink_fora",
        "import { realExt, fantasmaLn } from './link_fora';\n",
    ));
    c.push(ts("ts_nao_utf8", "import { x } from './ruim';\n"));
    c.push(ts("ts_enorme", "import { x } from './enorme';\n"));
    c.push(ts("ts_ausente", "import z from './nao_existe';\n"));
    c.push(ts(
        "ts_paths_apontam_para_fora",
        "import { realExt, fantasmaPaths } from '@o/externo';\n",
    ));
    c.push(ts(
        "ts_paths_dentro",
        "import { realFn, fantasmaAlias } from '@app/util';\n",
    ));
    c.push(ts(
        "ts_paths_node_modules_acima_da_raiz",
        "import a from 'alto/x';\n",
    ));
    c.push(ts(
        "ts_paths_sem_pacote_instalado",
        "import a from 'hoist/x';\n",
    ));
    c.push(ts(
        "ts_pacote_dentro",
        "import i from 'instalado';\nimport r from 'react';\n",
    ));
    c.push(ts(
        "ts_global_na_lib_do_typescript",
        "const a = globalDaLib;\nconst b = globalInexistenteLib;\n",
    ));
    c.push(ts(
        "ts_global_em_types_dentro",
        "const a = globalDentro;\nconst b = globalInexistenteTipos;\n",
    ));
    c.push(caso(
        "ts_global_em_types_acima_da_raiz",
        &m.alto,
        "src/main.ts",
        "typescript",
        "const a = globalHoisted;\nconst b = globalInexistenteAlto;\n",
    ));
    c.push(caso(
        "ts_global_na_lib_acima_da_raiz",
        &m.alto,
        "src/main.ts",
        "typescript",
        "const a = globalLibAlto;\nconst b = globalInexistenteLibAlto;\n",
    ));
    c.push(ts(
        "ts_package_json_main_aponta_para_fora",
        "import { realExt, fantasmaMain } from './pkgesc';\n",
    ));
    c.push(ts(
        "ts_package_json_types_dentro",
        "import { realTipo, fantasmaTipo } from './pkgok';\n",
    ));
    c.push(caso(
        "ts_base_url_aponta_para_fora",
        p,
        "web3/main.ts",
        "typescript",
        "import { realExt, fantasmaBase } from 'externo';\n",
    ));
    c.push(caso(
        "ts_extends_aponta_para_fora",
        p,
        "web2/src/main.ts",
        "typescript",
        "import { realExt, fantasmaExt } from '@b/externo';\nimport { realFn2 } from './util';\n",
    ));
    c.push(caso(
        "ts_fragmento_irmao",
        p,
        &format!("{abs_fora}/ts/frag.ts"),
        "typescript",
        "import { realExt, fantasmaFrag } from './externo';\n",
    ));
    c.push(caso(
        "js_no_projeto",
        p,
        "web/src/main.js",
        "javascript",
        "import { realJs, fantasmaJs } from './util.js';\nimport q from './sem_js';\n",
    ));
    c.push(caso(
        "tsx_no_projeto",
        p,
        "web/src/app.tsx",
        "tsx",
        "import { realFn, fantasmaTsx } from './util';\nexport const A = () => <div />;\n",
    ));

    c.push(com_overlay(
        ts(
            "ts_overlay_dentro",
            "import { realNovo, fantasmaOvt } from './novo';\n",
        ),
        p.join("web/src/novo.ts"),
        "export function realNovo() {}\n",
    ));

    // ------------------------------------------------------------------ html
    let h = |id: &str, conteudo: &str| caso(id, p, "site/index.html", "html", conteudo);
    c.push(h(
        "html_no_projeto",
        &pagina(
            "<link rel=\"stylesheet\" href=\"css/a.css\">\n<script src=\"js/app.js\"></script>\n<script src=\"js/ausente.js\"></script>",
            "<img src=\"img/logo.png\"><img src=\"img/ghost.png\">",
        ),
    ));
    c.push(h(
        "html_escape_dotdot",
        &pagina("", &format!("<img src=\"{}/fora/segredo.png\">", sobe(3))),
    ));
    c.push(h(
        "html_absoluto_fora",
        &pagina("", &format!("<img src=\"{abs_fora}/segredo.png\">")),
    ));
    c.push(h(
        "html_raiz_relativa",
        &pagina(
            "",
            "<img src=\"/site/img/logo.png\"><img src=\"/site/img/ghost2.png\">",
        ),
    ));
    c.push(h(
        "html_raiz_relativa_com_dotdot",
        &pagina("", &format!("<img src=\"/{}/fora/segredo.png\">", sobe(3))),
    ));
    c.push(h(
        "html_symlink_fora",
        &pagina("", "<img src=\"img/link.png\">"),
    ));
    c.push(h(
        "html_typo_sugere_vizinho",
        &pagina("", "<img src=\"img/logoo.png\"><img src=\"nodir/x.png\">"),
    ));
    c.push(h(
        "html_typo_em_dir_symlink_fora",
        &pagina("", "<img src=\"linkdir/segredx.png\">"),
    ));
    c.push(caso(
        "html_fragmento_fora",
        p,
        &format!("{abs_fora}/pagina/frag.html"),
        "html",
        &pagina("", "<img src=\"segredo.png\"><img src=\"ghost3.png\">"),
    ));
    c.push(h(
        "html_base_href_local",
        &pagina(
            "<base href=\"img/\">",
            "<img src=\"logo.png\"><img src=\"ghost4.png\">",
        ),
    ));

    c.push(com_overlay(
        h(
            "html_overlay_dentro",
            &pagina("", "<img src=\"img/novo.png\"><img src=\"img/ghost5.png\">"),
        ),
        p.join("site/img/novo.png"),
        "png",
    ));
    c.push(com_overlay(
        caso(
            "css_overlay_dentro",
            p,
            "site/index.html",
            "html",
            &pagina(
                "<link rel=\"stylesheet\" href=\"css/novo.css\">",
                "<div class=\"in-novo fantasma-ovc\"></div>",
            ),
        ),
        p.join("site/css/novo.css"),
        ".in-novo { }\n",
    ));

    // ------------------------------------------------------------------ classe_css (html)
    let hc = |id: &str, cabeca: &str, corpo: &str| {
        caso(id, p, "site/index.html", "html", &pagina(cabeca, corpo))
    };
    c.push(hc(
        "css_folha_no_projeto",
        "<link rel=\"stylesheet\" href=\"css/a.css\">",
        "<div class=\"in-a fantasma-css\"></div>",
    ));
    c.push(hc(
        "css_folha_com_import",
        "<link rel=\"stylesheet\" href=\"css/b.css\">",
        "<div class=\"in-b in-c fantasma-imp\"></div>",
    ));
    c.push(hc(
        "css_folha_escape_dotdot",
        &format!(
            "<link rel=\"stylesheet\" href=\"{}/fora/estilo.css\">",
            sobe(3)
        ),
        "<div class=\"so-no-fora fantasma-esc\"></div>",
    ));
    c.push(hc(
        "css_folha_symlink_fora",
        "<link rel=\"stylesheet\" href=\"css/link_fora.css\">",
        "<div class=\"so-no-fora fantasma-sl\"></div>",
    ));
    c.push(hc(
        "css_import_aponta_para_fora",
        "<link rel=\"stylesheet\" href=\"css/imp_fora.css\">",
        "<div class=\"in-impf so-no-fora fantasma-imf\"></div>",
    ));
    c.push(hc(
        "css_folha_nao_utf8",
        "<link rel=\"stylesheet\" href=\"css/ruim.css\">",
        "<div class=\"qualquer\"></div>",
    ));
    c.push(hc(
        "css_folha_ausente",
        "<link rel=\"stylesheet\" href=\"css/ausente.css\">",
        "<div class=\"qualquer\"></div>",
    ));
    c.push(hc(
        "css_folha_raiz_relativa",
        "<link rel=\"stylesheet\" href=\"/site/css/a.css\">",
        "<div class=\"in-a fantasma-rr\"></div>",
    ));
    c.push(caso(
        "css_gerador_no_package_json",
        &m.tw,
        "index.html",
        "html",
        &pagina(
            "<link rel=\"stylesheet\" href=\"css/a.css\">",
            "<div class=\"w-10 fantasma-tw\"></div>",
        ),
    ));
    c.push(caso(
        "css_pagina_fora_da_raiz",
        p,
        &format!("{abs_fora}/pagina/p.html"),
        "html",
        &pagina(
            "<link rel=\"stylesheet\" href=\"../estilo.css\">",
            "<div class=\"so-no-fora fantasma-pf\"></div>",
        ),
    ));

    // ------------------------------------------------------------------ classe_css (jsx)
    let tsx =
        |id: &str, raiz: &Path, conteudo: &str| caso(id, raiz, "src/app.tsx", "tsx", conteudo);
    c.push(tsx(
        "jsx_folha_do_projeto",
        &m.jx.normal,
        "export const A = () => <div className=\"ui-a fantasma-j1\" />;\n",
    ));
    c.push(tsx(
        "jsx_folha_nao_utf8",
        &m.jx.nao_utf8,
        "export const A = () => <div className=\"ui-a fantasma-j2\" />;\n",
    ));
    c.push(tsx(
        "jsx_import_aponta_para_fora",
        &m.jx.import_fora,
        "export const A = () => <div className=\"ui-a so-no-fora fantasma-j3\" />;\n",
    ));
    c.push(tsx(
        "jsx_html_de_entrada_com_import",
        &m.jx.entrada_html,
        "export const A = () => <div className=\"in-b in-c fantasma-j4\" />;\n",
    ));
    c.push(tsx(
        "jsx_importa_folha_de_pacote",
        &m.jx.pacote,
        "import 'lib-ui/dist/ui.css';\nexport const A = () => <div className=\"ui-btn fantasma-j5\" />;\n",
    ));
    c.push(tsx(
        "jsx_importa_folha_por_alias",
        &m.jx.alias,
        "import '@/styles/x.css';\nexport const A = () => <div className=\"alias-a fantasma-j6\" />;\n",
    ));
    c.push(tsx(
        "jsx_alias_que_escapa_da_raiz",
        &m.jx.alias_fora,
        &format!(
            "import '@/{}/fora/estilo.css';\nexport const A = () => <div className=\"ok so-no-fora fantasma-j7\" />;\n",
            sobe(2)
        ),
    ));
    c.push(tsx(
        "jsx_pacote_que_escapa_da_raiz",
        &m.jx.pacote,
        &format!(
            "import 'lib-ui/{}/fora/estilo.css';\nexport const A = () => <div className=\"ui-btn fantasma-j8\" />;\n",
            sobe(4)
        ),
    ));
    c.push(tsx(
        "jsx_folha_acima_do_limite",
        &m.jx.folha_gigante,
        "export const A = () => <div className=\"qualquer fantasma-j9\" />;\n",
    ));
    c
}

fn overlays_de(c: &Caso) -> HashMap<String, String> {
    c.overlays
        .iter()
        .map(|(k, v)| (k.to_string_lossy().into_owned(), v.clone()))
        .collect()
}

fn rodar(c: &Caso) -> Value {
    let entrada = CheckInput {
        path: c.caminho.to_string_lossy().into_owned(),
        content: c.conteudo.clone(),
        project_root: c.raiz.clone(),
        disk: None,
        overlays: overlays_de(c),
        lang: c.lang.to_string(),
    };
    let saida = check_em(&entrada, Contexto::Escrita);
    let mut v = serde_json::to_value(&saida).expect("CheckOutput is serializable");
    // the wall time is the only field that is not a function of the input
    if let Some(o) = v.as_object_mut() {
        o.remove("elapsed_ms");
    }
    v
}

/// The answer as text, with the temp dir (raw and canonical) replaced by `<TMP>`.
fn normaliza(v: &Value, tmp: &Path) -> String {
    let mut texto = serde_json::to_string(v).expect("json");
    let canonico = tmp.canonicalize().unwrap_or_else(|_| tmp.to_path_buf());
    for prefixo in [tmp, canonico.as_path()] {
        texto = texto.replace(prefixo.to_string_lossy().as_ref(), "<TMP>");
    }
    texto
}

/// A directory for the tree that is NOT under `/tmp`: the TypeScript adapter reads a path under
/// `/tmp` as a scratch fragment (a flat neighbourhood scan only), so the same tree under `/tmp`
/// and elsewhere answers differently — the recording must not depend on where TMPDIR points.
fn diretorio_da_arvore() -> tempfile::TempDir {
    let preferido = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let base = if preferido.starts_with("/tmp") {
        // a CARGO_TARGET_DIR under /tmp: fall back to a directory inside the crate
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".vgp-replay-tmp")
    } else {
        preferido
    };
    fs::create_dir_all(&base).expect("base dir");
    tempfile::Builder::new()
        .prefix("vgp-replay-")
        .tempdir_in(&base)
        .expect("tempdir")
}

fn resposta_de_todos() -> BTreeMap<String, Value> {
    let dir = diretorio_da_arvore();
    let m = monta(dir.path());
    let mut out = BTreeMap::new();
    for c in casos(&m) {
        let v = rodar(&c);
        let norm: Value = serde_json::from_str(&normaliza(&v, dir.path())).expect("json");
        assert!(
            out.insert(c.id.clone(), norm).is_none(),
            "duplicate case id {}",
            c.id
        );
    }
    out
}

#[test]
fn o_replay_reproduz_a_gravacao_byte_a_byte() {
    let atual = resposta_de_todos();
    if let Ok(destino) = std::env::var("VGP_REPLAY_RECORD") {
        let texto = serde_json::to_string_pretty(&atual).expect("json") + "\n";
        fs::write(&destino, texto).expect("gravar");
        return;
    }
    let gravado: BTreeMap<String, Value> = serde_json::from_str(GRAVADO).expect("recording");
    let mut so_no_atual: Vec<&String> =
        atual.keys().filter(|k| !gravado.contains_key(*k)).collect();
    let mut so_no_gravado: Vec<&String> =
        gravado.keys().filter(|k| !atual.contains_key(*k)).collect();
    so_no_atual.sort();
    so_no_gravado.sort();
    assert!(
        so_no_atual.is_empty() && so_no_gravado.is_empty(),
        "cases only in the replay: {so_no_atual:?}; only in the recording: {so_no_gravado:?}"
    );
    let mut diferentes: Vec<String> = Vec::new();
    for (id, depois) in &atual {
        let antes = &gravado[id];
        if antes == depois {
            continue;
        }
        if let Some((_, _, fixada)) = JUSTIFIED_DIFFERENCES.iter().find(|(j, _, _)| j == id) {
            let esperada: Value = serde_json::from_str(fixada).expect("pinned answer is JSON");
            if *depois == esperada {
                continue;
            }
            diferentes.push(format!(
                "{id} (justified, but not the pinned answer)\n  pinned:   {fixada}\n  replayed: {}",
                serde_json::to_string(depois).expect("json")
            ));
            continue;
        }
        diferentes.push(format!(
            "{id}\n  recorded: {}\n  replayed: {}",
            serde_json::to_string(antes).expect("json"),
            serde_json::to_string(depois).expect("json")
        ));
    }
    assert!(
        diferentes.is_empty(),
        "{} case(s) differ from the recording and are not in JUSTIFIED_DIFFERENCES:\n{}",
        diferentes.len(),
        diferentes.join("\n")
    );
}

/// Every justified difference names a real case, carries a reason, and still differs: a stale
/// entry would hide a regression later.
#[test]
fn toda_diferenca_justificada_tem_razao_e_ainda_difere() {
    let atual = resposta_de_todos();
    let gravado: BTreeMap<String, Value> = serde_json::from_str(GRAVADO).expect("recording");
    for (id, razao, fixada) in JUSTIFIED_DIFFERENCES {
        serde_json::from_str::<Value>(fixada).expect("pinned answer is JSON");
        assert!(
            razao.trim().len() >= 20,
            "{id}: a justified difference needs a reason"
        );
        let (Some(a), Some(g)) = (atual.get(*id), gravado.get(*id)) else {
            panic!("{id}: not a case of the corpus");
        };
        assert_ne!(a, g, "{id}: no longer differs — remove it from the list");
    }
}

/// The corpus must exercise the adversarial shapes, not only parse: every adapter has cases, and
/// the tree really holds the escapes (a symlink out, a file over the cap, a non-UTF-8 file).
#[test]
fn o_corpus_cobre_cada_adaptador_e_a_arvore_tem_as_armadilhas() {
    let dir = diretorio_da_arvore();
    let m = monta(dir.path());
    let todos = casos(&m);
    for prefixo in [
        "rust_", "python_", "ts_", "js_", "tsx_", "html_", "css_", "jsx_",
    ] {
        let n = todos.iter().filter(|c| c.id.starts_with(prefixo)).count();
        assert!(n >= 1, "no case for {prefixo}");
    }
    assert!(todos.len() >= 60, "{} cases", todos.len());
    for elo in [
        "src/link_fora.rs",
        "pkg/link_fora.py",
        "web/src/link_fora.ts",
        "site/img/link.png",
    ] {
        let alvo = m.proj.join(elo);
        assert!(
            alvo.symlink_metadata()
                .expect("elo")
                .file_type()
                .is_symlink(),
            "{elo} must be a symlink"
        );
        let real = alvo.canonicalize().expect("canon");
        assert!(
            real.starts_with(m.fora.canonicalize().expect("fora")),
            "{elo} must point OUT of the project"
        );
    }
    let grande = m.proj.join("web/src/enorme.ts");
    assert!(grande.metadata().expect("meta").len() > 3 * 1024 * 1024);
}

/// The public repository holds no machine path: the recording and this source name no user.
#[test]
fn a_gravacao_e_o_codigo_nao_carregam_o_usuario_da_maquina() {
    let Some(usuario) = std::env::var("USER")
        .ok()
        .or_else(|| std::env::var("LOGNAME").ok())
        .filter(|u| !u.is_empty())
    else {
        return;
    };
    let casa = format!("/home/{usuario}");
    let fonte = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/vgp_contencao_replay.rs"),
    )
    .expect("source");
    for (nome, texto) in [("recording", GRAVADO), ("test source", fonte.as_str())] {
        assert!(!texto.contains(&casa), "{nome} carries {casa}");
        assert!(
            !texto.contains(&format!("{usuario}/")),
            "{nome} names the user"
        );
    }
}

// ---------------------------------------------------------------------------------------------
// latency probe
// ---------------------------------------------------------------------------------------------

fn percentil(ordenado: &[f64], p: f64) -> f64 {
    if ordenado.is_empty() {
        return 0.0;
    }
    let i = ((ordenado.len() as f64 - 1.0) * p).round() as usize;
    ordenado[i]
}

fn resumo(amostras: &mut [f64]) -> Value {
    amostras.sort_by(|a, b| a.partial_cmp(b).expect("finite"));
    serde_json::json!({
        "n": amostras.len(),
        "p50_ms": percentil(amostras, 0.50),
        "p95_ms": percentil(amostras, 0.95),
        "p99_ms": percentil(amostras, 0.99),
    })
}

/// VGP verify latency over the corpus tree: `VGP_LATENCY_N` iterations (default 1000) of every
/// case, the motor alone timed (the inputs are built outside the timed region). Writes one JSON
/// line to `VGP_LATENCY_OUT` (or stderr). Run with `--ignored`; the p95 of a round measures the
/// machine as much as the code, so compare binaries INTERLEAVED, never one after the other.
#[test]
#[ignore = "latency probe: run with --ignored"]
fn latencia_do_vgp_na_arvore() {
    let n: usize = std::env::var("VGP_LATENCY_N")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1000);
    let dir = diretorio_da_arvore();
    let m = monta(dir.path());
    let cs = casos(&m);
    let entradas: Vec<(String, CheckInput)> = cs
        .iter()
        .map(|c| {
            let grupo = c.id.split('_').next().unwrap_or("").to_string();
            let grupo = if matches!(grupo.as_str(), "tsx" | "js") {
                "ts".to_string()
            } else {
                grupo
            };
            let entrada = CheckInput {
                path: c.caminho.to_string_lossy().into_owned(),
                content: c.conteudo.clone(),
                project_root: c.raiz.clone(),
                disk: None,
                overlays: overlays_de(c),
                lang: c.lang.to_string(),
            };
            (grupo, entrada)
        })
        .collect();
    for (_, e) in &entradas {
        let _ = check_em(e, Contexto::Escrita); // warm-up: page cache, thread-local root cache
    }
    let mut por_grupo: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    let mut todas: Vec<f64> = Vec::new();
    for _ in 0..n {
        for (grupo, e) in &entradas {
            let t0 = Instant::now();
            let saida = check_em(e, Contexto::Escrita);
            let ms = t0.elapsed().as_secs_f64() * 1000.0;
            std::hint::black_box(&saida);
            por_grupo.entry(grupo.clone()).or_default().push(ms);
            todas.push(ms);
        }
    }
    let grupos: serde_json::Map<String, Value> = por_grupo
        .iter_mut()
        .map(|(k, v)| (k.clone(), resumo(v)))
        .collect();
    let linha = serde_json::json!({
        "iterations": n,
        "cases": entradas.len(),
        "overall": resumo(&mut todas),
        "groups": grupos,
    });
    let texto = serde_json::to_string(&linha).expect("json");
    match std::env::var("VGP_LATENCY_OUT") {
        Ok(destino) => {
            use std::io::Write;
            let mut f = fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(destino)
                .expect("open out");
            writeln!(f, "{texto}").expect("write");
        }
        Err(_) => eprintln!("{texto}"),
    }
}
