//! Structural guard (deepening #8, 02/11/2026): only `vgp/lang/caminho.rs` touches the
//! filesystem of the project the motor judges.
//!
//! Containment is the OPERATION (`caminho::ler_contido`, `listar_contido`, `sondar`…), not a
//! precondition each adapter has to remember. This test is what keeps that true: it reads the
//! source of every adapter and fails if one calls `std::fs`, `File::open`, `read_to_string`,
//! `read_dir`, `canonicalize`, or probes a `Path` (`.exists()`, `.is_file()`, `.is_dir()`)
//! itself — or asks the old public predicate `dentro_das_raizes`.
//!
//! Comments, string literals and every `#[cfg(test)]` item (a `mod tests;` declaration, an inline
//! `mod tests { … }`, a gated `use` or `fn`) are not code that ships, and are not scanned.
//! Anything legitimate goes in [`PERMITIDOS`], with the reason.

use std::fs;
use std::path::{Path, PathBuf};

/// What touches the disk (or decides containment by hand). `caminho.rs` owns every one.
const PROIBIDOS: &[&str] = &[
    "std::fs",
    "fs::read",
    "fs::File",
    "fs::metadata",
    "File::open",
    "OpenOptions",
    "read_to_string",
    "read_dir",
    ".canonicalize(",
    "symlink_metadata",
    ".exists()",
    "dentro_das_raizes",
];

/// `.is_file()` / `.is_dir()` on one of these is a `FileType` ALREADY read (a listing entry), not
/// a probe of the filesystem.
const RECEPTORES_DE_TIPO: &[&str] = &["tipo", "t", "ft"];

/// (file relative to `src/vgp/lang`, token, why it is not a project path).
const PERMITIDOS: &[(&str, &str, &str)] = &[
    ("cdn/cache.rs", "std::fs", CACHE_DO_CDN),
    ("cdn/cache.rs", "fs::read", CACHE_DO_CDN),
    ("cdn/cache.rs", "read_to_string", CACHE_DO_CDN),
    ("cdn/cache.rs", "fs::metadata", CACHE_DO_CDN),
];

/// Why the CDN cache may use `std::fs` itself.
const CACHE_DO_CDN: &str = "the CDN cache is OUR file under the user's cache directory, written by this module — not a path the analyzed content names";

/// The source as the compiler sees the SHIPPED code: comments and literals blanked and every
/// `#[cfg(test)]` item removed (newlines kept, so the line numbers stay).
fn codigo_de_producao(fonte: &str) -> String {
    sem_itens_de_teste(&sem_comentarios_e_literais(fonte))
}

/// Removes each item an `#[cfg(test)]` attribute gates. The attribute may come BEFORE the code (a
/// file that declares `#[cfg(test)] mod tests;` in its header), so nothing is cut "from here on".
fn sem_itens_de_teste(codigo: &str) -> String {
    const MARCA: &str = "#[cfg(test)]";
    let mut out = String::with_capacity(codigo.len());
    let mut resto = codigo;
    while let Some(i) = resto.find(MARCA) {
        out.push_str(&resto[..i]);
        let depois = &resto[i + MARCA.len()..];
        let fim = fim_do_item(depois);
        out.extend(depois[..fim].chars().filter(|c| *c == '\n'));
        resto = &depois[fim..];
    }
    out.push_str(resto);
    out
}

/// Where the item that follows an attribute ends: at its `;` when that comes before any `{`
/// (`mod tests;`, `use …;`), else at the `}` that closes the first `{`.
fn fim_do_item(texto: &str) -> usize {
    let mut profundidade = 0usize;
    for (i, c) in texto.char_indices() {
        match c {
            ';' if profundidade == 0 => return i + 1,
            '{' => profundidade += 1,
            '}' => {
                profundidade = profundidade.saturating_sub(1);
                if profundidade == 0 {
                    return i + 1;
                }
            }
            _ => {}
        }
    }
    texto.len()
}

/// Comments and literals blanked (newlines kept).
fn sem_comentarios_e_literais(fonte: &str) -> String {
    let c: Vec<char> = fonte.chars().collect();
    let mut out = String::with_capacity(c.len());
    let mut i = 0;
    while i < c.len() {
        let atual = c[i];
        let proximo = c.get(i + 1).copied();
        if atual == '/' && proximo == Some('/') {
            while i < c.len() && c[i] != '\n' {
                i += 1;
            }
        } else if atual == '/' && proximo == Some('*') {
            i += 2;
            while i + 1 < c.len() && !(c[i] == '*' && c[i + 1] == '/') {
                out.extend(c[i].eq(&'\n').then_some('\n'));
                i += 1;
            }
            i += 2;
        } else if let Some(fim) = fim_de_string_crua(&c, i) {
            out.extend(c[i..fim].iter().filter(|x| **x == '\n'));
            out.push(' ');
            i = fim;
        } else if atual == '"' {
            i += 1;
            while i < c.len() && c[i] != '"' {
                if c[i] == '\\' {
                    i += 1;
                }
                out.extend(c.get(i).filter(|x| **x == '\n'));
                i += 1;
            }
            i += 1;
            out.push(' ');
        } else if atual == '\'' {
            i = fim_de_literal_de_char(&c, i, &mut out);
        } else {
            out.push(atual);
            i += 1;
        }
    }
    out
}

/// `r"…"` / `r#"…"#` starting at `i`: the index just past it.
fn fim_de_string_crua(c: &[char], i: usize) -> Option<usize> {
    if c[i] != 'r' || (i > 0 && (c[i - 1].is_alphanumeric() || c[i - 1] == '_')) {
        return None;
    }
    let mut j = i + 1;
    let mut hashes = 0;
    while c.get(j) == Some(&'#') {
        hashes += 1;
        j += 1;
    }
    if c.get(j) != Some(&'"') {
        return None;
    }
    j += 1;
    while j < c.len() {
        if c[j] == '"'
            && c[j + 1..]
                .iter()
                .take(hashes)
                .filter(|x| **x == '#')
                .count()
                == hashes
        {
            return Some(j + 1 + hashes);
        }
        j += 1;
    }
    Some(c.len())
}

/// `'x'`, `'\n'` (a char literal) or `'a` (a lifetime) at `i`: the index to resume from.
fn fim_de_literal_de_char(c: &[char], i: usize, out: &mut String) -> usize {
    if c.get(i + 1) == Some(&'\\') {
        let mut j = i + 2;
        while j < c.len() && c[j] != '\'' {
            j += 1;
        }
        out.push(' ');
        return j + 1;
    }
    if c.get(i + 2) == Some(&'\'') {
        out.push(' ');
        return i + 3;
    }
    out.push('\'');
    i + 1
}

/// The identifier right before the `.` at `ponto`.
fn receptor(codigo: &str, ponto: usize) -> &str {
    let antes = &codigo[..ponto];
    let inicio = antes
        .rfind(|ch: char| !(ch.is_alphanumeric() || ch == '_'))
        .map_or(0, |p| p + 1);
    &antes[inicio..]
}

/// (line, token) of every disk access in the shipped code of `fonte`.
fn acessos(fonte: &str) -> Vec<(usize, String)> {
    let codigo = codigo_de_producao(fonte);
    let linha_de = |pos: usize| codigo[..pos].matches('\n').count() + 1;
    let mut out = Vec::new();
    for token in PROIBIDOS {
        for (pos, _) in codigo.match_indices(token) {
            out.push((linha_de(pos), (*token).to_string()));
        }
    }
    for sonda in [".is_file()", ".is_dir()"] {
        for (pos, _) in codigo.match_indices(sonda) {
            if !RECEPTORES_DE_TIPO.contains(&receptor(&codigo, pos)) {
                out.push((linha_de(pos), sonda.to_string()));
            }
        }
    }
    out.sort();
    out
}

fn fontes_de(dir: &Path, out: &mut Vec<PathBuf>) {
    for entrada in fs::read_dir(dir).expect("read_dir").flatten() {
        let caminho = entrada.path();
        let nome = entrada.file_name().to_string_lossy().into_owned();
        if caminho.is_dir() {
            if nome != "tests" {
                fontes_de(&caminho, out);
            }
        } else if nome.ends_with(".rs") && nome != "tests.rs" && nome != "testes.rs" {
            out.push(caminho);
        }
    }
}

fn raiz_de_lang() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src/vgp/lang")
}

fn relativo(arquivo: &Path) -> String {
    arquivo
        .strip_prefix(raiz_de_lang())
        .expect("under lang")
        .to_string_lossy()
        .replace('\\', "/")
}

#[test]
fn so_o_caminho_toca_o_disco() {
    let mut arquivos = Vec::new();
    fontes_de(&raiz_de_lang(), &mut arquivos);
    // not vacuous: the walk found the adapters
    assert!(
        arquivos.len() >= 30,
        "only {} sources found",
        arquivos.len()
    );
    let rel: Vec<String> = arquivos.iter().map(|a| relativo(a)).collect();
    for esperado in [
        "rust.rs",
        "python/modules.rs",
        "typescript/projeto.rs",
        "html/recurso.rs",
        "classe_css/folhas.rs",
        "classe_css/projeto.rs",
        "caminho.rs",
    ] {
        assert!(rel.iter().any(|r| r == esperado), "{esperado} not scanned");
    }
    let mut violacoes: Vec<String> = Vec::new();
    for arquivo in &arquivos {
        let r = relativo(arquivo);
        if r == "caminho.rs" {
            continue;
        }
        let fonte = fs::read_to_string(arquivo).expect("read source");
        for (linha, token) in acessos(&fonte) {
            let permitido = PERMITIDOS
                .iter()
                .any(|(f, t, motivo)| *f == r && *t == token && !motivo.is_empty());
            if !permitido {
                violacoes.push(format!("{r}:{linha}: `{token}`"));
            }
        }
    }
    assert!(
        violacoes.is_empty(),
        "these adapters touch the disk themselves — route them through `caminho` \
         (ler_contido / listar_contido / sondar / e_arquivo…):\n{}",
        violacoes.join("\n")
    );
}

/// Every allowance still earns its place: the file exists and still has the token.
#[test]
fn nenhuma_permissao_ficou_velha() {
    for (arquivo, token, motivo) in PERMITIDOS {
        assert!(motivo.len() >= 20, "{arquivo}: a reason is required");
        let fonte = fs::read_to_string(raiz_de_lang().join(arquivo)).expect("allowed file exists");
        assert!(
            acessos(&fonte).iter().any(|(_, t)| t == token),
            "{arquivo}: `{token}` is no longer there — remove the allowance"
        );
    }
}

/// The owner is the one place that has them (so the scanner is not blind), and the scanner
/// itself: it catches what it must and ignores what is not code.
#[test]
fn o_scanner_acha_o_que_deve_e_ignora_o_que_nao_e_codigo() {
    let dono = fs::read_to_string(raiz_de_lang().join("caminho.rs")).expect("caminho.rs");
    let nos_acessos: Vec<String> = acessos(&dono).into_iter().map(|(_, t)| t).collect();
    for token in [
        "std::fs",
        "read_to_string",
        "read_dir",
        ".canonicalize(",
        "symlink_metadata",
    ] {
        assert!(
            nos_acessos.iter().any(|t| t == token),
            "caminho.rs lacks `{token}`?"
        );
    }

    let achado = |src: &str| acessos(src).into_iter().map(|(_, t)| t).collect::<Vec<_>>();
    let apanha = |src: &str, token: &str| achado(src).iter().any(|t| t == token);
    // caught
    assert!(apanha(
        "fn f(p: &Path) { std::fs::read_to_string(p); }",
        "read_to_string"
    ));
    assert!(apanha(
        "fn f(p: &Path) { std::fs::read_to_string(p); }",
        "std::fs"
    ));
    assert!(apanha(
        "fn f(p: &Path) -> bool { p.is_file() }",
        ".is_file()"
    ));
    assert!(apanha("fn f(p: &Path) -> bool { p.is_dir() }", ".is_dir()"));
    assert!(apanha("fn f(p: &Path) -> bool { p.exists() }", ".exists()"));
    assert!(apanha("fn f() { File::open(p) }", "File::open"));
    assert!(apanha(
        "fn f() { dentro_das_raizes(&r, p) }",
        "dentro_das_raizes"
    ));
    assert!(apanha(
        "fn f(p: &Path) { for e in p.read_dir() {} }",
        "read_dir"
    ));
    assert!(apanha(
        "fn f(p: &Path) { p.canonicalize(); }",
        ".canonicalize("
    ));
    assert!(apanha(
        "fn f(p: &Path) { p.symlink_metadata(); }",
        "symlink_metadata"
    ));
    assert!(apanha("use std::fs::OpenOptions;", "OpenOptions"));
    // not code that ships
    assert!(achado("// std::fs::read_to_string(p)\nfn f() {}").is_empty());
    assert!(achado("/* p.exists() */ fn f() {}").is_empty());
    assert!(achado("fn f() { let s = \"std::fs::read_dir\"; }").is_empty());
    assert!(achado("fn f() { let s = r#\"p.is_file()\"#; }").is_empty());
    assert!(achado("fn f() { let c = '\"'; let s = \"x\"; }").is_empty());
    assert!(
        achado("fn f<'a>(x: &'a str) {}\n#[cfg(test)]\nmod tests { fn t() { std::fs::read(p); } }")
            .is_empty()
    );
    // a test module declared in the HEADER does not hide the code that comes after it
    assert!(apanha(
        "#[cfg(test)]\nmod tests;\n\nfn f(p: &Path) { std::fs::read_to_string(p); }",
        "read_to_string"
    ));
    // ... nor does an inline test module hide the code after IT, while what is inside it is ignored
    let depois_do_modulo = "#[cfg(test)]\nmod tests {\n    fn t() { std::fs::write(a, b); }\n}\n\nfn f(p: &Path) -> bool { p.exists() }";
    assert_eq!(achado(depois_do_modulo), [".exists()"]);
    assert!(achado("#[cfg(test)]\nuse std::fs;\nfn f() {}").is_empty());
    assert!(
        achado("#[cfg(test)]\n#[allow(dead_code)]\nfn t(p: &Path) { p.exists(); }\nfn f() {}")
            .is_empty()
    );
    // a FileType already read is not a probe
    assert!(achado("fn f(e: Entrada) -> bool { e.tipo.is_some_and(|t| t.is_dir()) }").is_empty());
    assert!(achado("fn f(tipo: FileType) -> bool { tipo.is_file() }").is_empty());
    // line numbers survive the blanking
    assert_eq!(
        acessos("// c\n/* a\nb */\nfn f(p: &Path) { p.exists(); }"),
        [(4, ".exists()".to_string())]
    );
}
