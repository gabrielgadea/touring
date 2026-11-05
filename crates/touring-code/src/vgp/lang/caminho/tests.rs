//! Tests of the contained read (`caminho.rs`): the table of path cases is asserted ONCE here;
//! the adapters are tested through their verdicts (`tests/vgp_contencao_replay.rs`).

use super::*;
use std::fs;
use std::path::PathBuf;

/// A path that climbs `n` levels, built by COMPONENT (the `..` literal in a
/// `join` is what the F2.1/CWE-22 rule flags — and the test needs exactly the
/// dangerous path to prove that it is refused).
fn sobe(n: usize) -> PathBuf {
    let mut p = PathBuf::new();
    for _ in 0..n {
        p.push(Component::ParentDir);
    }
    p
}

/// What a contained read answered, as a comparable tag.
#[derive(Debug, PartialEq, Eq)]
enum Tag {
    Ok,
    Fora,
    Io(io::ErrorKind),
    Grande,
    NaoERegular,
}

fn tag<T>(r: Result<T, Recusa>) -> Tag {
    match r {
        Ok(_) => Tag::Ok,
        Err(Recusa::ForaDasRaizes) => Tag::Fora,
        Err(Recusa::Io(e)) => Tag::Io(e.kind()),
        Err(Recusa::Grande { .. }) => Tag::Grande,
        Err(Recusa::NaoERegular) => Tag::NaoERegular,
    }
}

/// The tree every table below runs on:
///
/// ```text
/// tmp/proj/sub/x.rs            tmp/fora.rs            tmp/venv/lib/pkg.py
/// tmp/proj/link.rs  -> ../fora.rs      tmp/fora_dir/y.rs
/// tmp/proj/linkdir  -> ../fora_dir     tmp/proj2/vizinho.rs   (shares the textual prefix)
/// tmp/proj/ruim.bin (not UTF-8)        tmp/proj/grande.txt    (over a 16-byte cap)
/// ```
struct Arvore {
    tmp: PathBuf,
    raiz: PathBuf,
    venv: PathBuf,
}

impl Arvore {
    fn nova(tag: &str) -> Self {
        let tmp = std::env::temp_dir().join(format!("vgp-caminho-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        let raiz = tmp.join("proj");
        let venv = tmp.join("venv");
        fs::create_dir_all(raiz.join("sub")).expect("mkdir");
        fs::create_dir_all(venv.join("lib")).expect("mkdir");
        fs::create_dir_all(tmp.join("fora_dir")).expect("mkdir");
        fs::create_dir_all(tmp.join("proj2")).expect("mkdir");
        fs::write(raiz.join("sub/x.rs"), "// x").expect("write");
        fs::write(tmp.join("fora.rs"), "// fora").expect("write");
        fs::write(tmp.join("fora_dir/y.rs"), "// y").expect("write");
        fs::write(tmp.join("proj2/vizinho.rs"), "// vizinho").expect("write");
        fs::write(venv.join("lib/pkg.py"), "# pkg").expect("write");
        fs::write(raiz.join("ruim.bin"), [0xffu8, 0xfe, 0xfa]).expect("write");
        fs::write(raiz.join("grande.txt"), "a".repeat(64)).expect("write");
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(tmp.join("fora.rs"), raiz.join("link.rs")).expect("ln");
            std::os::unix::fs::symlink(tmp.join("fora_dir"), raiz.join("linkdir")).expect("ln");
        }
        Self { tmp, raiz, venv }
    }

    fn raizes(&self) -> [&Path; 2] {
        [self.raiz.as_path(), self.venv.as_path()]
    }
}

impl Drop for Arvore {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.tmp);
    }
}

// ------------------------------------------------------------------ the one table

/// V-4 (31/10/2026) — the four cases the coordination fixed, and the rest of the ways a path
/// can leave the roots, asserted ONCE here for the contained read. The adapters are tested
/// through their verdicts (`tests/vgp_contencao_replay.rs`).
#[test]
fn a_leitura_contida_so_le_o_que_esta_dentro_das_raizes() {
    let a = Arvore::nova("tabela");
    let r = a.raiz.clone();
    let fora_absoluto = Path::new("/etc").join("hostname");
    let casos: Vec<(&str, PathBuf, Tag)> = vec![
        ("arquivo do projeto", r.join("sub/x.rs"), Tag::Ok),
        (
            "`..` que fica dentro",
            r.join("sub").join(sobe(1)).join("sub/x.rs"),
            Tag::Ok,
        ),
        ("raiz declarada (venv)", a.venv.join("lib/pkg.py"), Tag::Ok),
        (
            "`..` que escapa",
            r.join(sobe(4)).join("etc").join("hostname"),
            Tag::Fora,
        ),
        (
            "`..` que escapa até um arquivo real",
            r.join(sobe(1)).join("fora.rs"),
            Tag::Fora,
        ),
        ("absoluto fora", fora_absoluto, Tag::Fora),
        ("symlink de arquivo para fora", r.join("link.rs"), Tag::Fora),
        (
            "arquivo atras de symlink de diretorio",
            r.join("linkdir/y.rs"),
            Tag::Fora,
        ),
        (
            "irmao que compartilha o prefixo textual",
            a.tmp.join("proj2/vizinho.rs"),
            Tag::Fora,
        ),
        ("a raiz nao e um arquivo do projeto", r.clone(), Tag::Fora),
        ("relativo", PathBuf::from("sub/x.rs"), Tag::Fora),
        (
            "ausente dentro",
            r.join("sub/nao_existe.rs"),
            Tag::Io(io::ErrorKind::NotFound),
        ),
        (
            "ausente fora: fora vence",
            a.tmp.join("nao_existe.rs"),
            Tag::Fora,
        ),
        (
            "diretorio dentro",
            r.join("sub"),
            Tag::Io(io::ErrorKind::IsADirectory),
        ),
        (
            "nao UTF-8",
            r.join("ruim.bin"),
            Tag::Io(io::ErrorKind::InvalidData),
        ),
    ];
    for (nome, alvo, esperado) in casos {
        assert_eq!(
            tag(ler_contido(&a.raizes(), &alvo)),
            esperado,
            "ler: {nome}"
        );
    }
    // sem raizes, nada e lido (fail-closed)
    let nenhuma: [&Path; 0] = [];
    assert_eq!(tag(ler_contido(&nenhuma, &r.join("sub/x.rs"))), Tag::Fora);
}

/// The text comes back, and the cap is a refusal that carries the size, not a truncation.
#[test]
fn o_limite_de_tamanho_recusa_em_vez_de_truncar() {
    let a = Arvore::nova("limite");
    let g = a.raiz.join("grande.txt");
    assert_eq!(
        ler_contido(&a.raizes(), &a.raiz.join("sub/x.rs")).expect("le"),
        "// x"
    );
    assert_eq!(
        ler_contido_ate(&a.raizes(), &g, 64)
            .expect("no limite")
            .len(),
        64
    );
    match ler_contido_ate(&a.raizes(), &g, 63) {
        Err(Recusa::Grande { tamanho, limite }) => {
            assert_eq!((tamanho, limite), (64, 63));
        }
        outro => panic!("expected Grande, got {outro:?}"),
    }
    // the cap does not turn an escape into a size problem
    assert_eq!(
        tag(ler_contido_ate(
            &a.raizes(),
            &a.raiz.join("link.rs"),
            1 << 20
        )),
        Tag::Fora
    );
}

/// A FIFO would block the read forever: refused, not opened.
#[cfg(unix)]
#[test]
fn um_arquivo_especial_nao_e_aberto() {
    let a = Arvore::nova("fifo");
    let fifo = a.raiz.join("canal");
    let feito = std::process::Command::new("mkfifo")
        .arg(&fifo)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !feito {
        return; // no mkfifo on this machine: the case cannot be built
    }
    // Opening a FIFO for reading with no writer blocks FOREVER. This thread holds the other end
    // open if (and only if) somebody tries to read it: a regression — the special-file guard gone —
    // then fails the assertion below instead of hanging the whole suite. With the guard in place
    // nobody reads, the thread stays parked in `open` and dies with the process.
    let escritor = fifo.clone();
    std::thread::spawn(move || {
        let _ = fs::OpenOptions::new().write(true).open(&escritor);
    });
    assert_eq!(tag(ler_contido(&a.raizes(), &fifo)), Tag::NaoERegular);
    assert_eq!(sondar(&a.raizes(), &fifo), Sonda::Outro);
}

/// A root that does not exist on disk (a virtual project — the adapter tests use `/home/u/proj`)
/// is compared lexically; the same rules hold.
#[test]
fn raiz_virtual_compara_lexicamente() {
    let raiz = Path::new("/home/u/proj");
    let casos = [
        (
            "dentro",
            "/home/u/proj/src/a.rs",
            Tag::Io(io::ErrorKind::NotFound),
        ),
        ("a propria raiz", "/home/u/proj", Tag::Fora),
        (
            "irmao com o mesmo prefixo textual",
            "/home/u/proj2/a.rs",
            Tag::Fora,
        ),
        ("fora", "/home/u/outro/a.rs", Tag::Fora),
    ];
    for (nome, alvo, esperado) in casos {
        assert_eq!(
            tag(ler_contido(&[raiz], Path::new(alvo))),
            esperado,
            "{nome}"
        );
    }
    let escapa = raiz.join("src").join(sobe(2)).join("outro/a.rs");
    assert_eq!(
        tag(ler_contido(&[raiz], &escapa)),
        Tag::Fora,
        "`..` que escapa"
    );
    let fica = raiz.join("src").join(sobe(1)).join("lib/a.rs");
    assert_eq!(
        tag(ler_contido(&[raiz], &fica)),
        Tag::Io(io::ErrorKind::NotFound),
        "`..` que fica"
    );
    // an empty root is not a root: `starts_with("")` is true for every path
    assert_eq!(
        tag(ler_contido(&[Path::new("")], Path::new("/etc/hostname"))),
        Tag::Fora
    );
}

// ------------------------------------------------------------------ the other operations

/// The probes answer what `is_file` / `is_dir` / `exists` answered — for what is INSIDE — and
/// name what exists outside instead of reporting it.
#[test]
fn as_sondas_dizem_o_que_e_e_nomeiam_o_que_esta_fora() {
    let a = Arvore::nova("sondas");
    let r = &a.raiz;
    let casos = [
        ("arquivo", r.join("sub/x.rs"), Sonda::Arquivo),
        ("diretorio", r.join("sub"), Sonda::Diretorio),
        ("ausente dentro", r.join("sub/nada.rs"), Sonda::Ausente),
        (
            "ausente fora e so ausente",
            a.tmp.join("nada.rs"),
            Sonda::Ausente,
        ),
        ("existe fora", a.tmp.join("fora.rs"), Sonda::Fora),
        ("symlink para fora", r.join("link.rs"), Sonda::Fora),
        (
            "diretorio symlink para fora",
            r.join("linkdir"),
            Sonda::Fora,
        ),
        (
            "o `..` que sai",
            r.join(sobe(1)).join("fora.rs"),
            Sonda::Fora,
        ),
        (
            "o irmao textual",
            a.tmp.join("proj2/vizinho.rs"),
            Sonda::Fora,
        ),
        ("raiz declarada", a.venv.join("lib/pkg.py"), Sonda::Arquivo),
    ];
    for (nome, alvo, esperado) in casos {
        assert_eq!(sondar(&a.raizes(), &alvo), esperado, "{nome}");
    }
    assert!(e_arquivo(&a.raizes(), &r.join("sub/x.rs")));
    assert!(!e_arquivo(&a.raizes(), &r.join("sub")));
    assert!(e_diretorio(&a.raizes(), &r.join("sub")));
    assert!(!e_diretorio(&a.raizes(), &r.join("sub/x.rs")));
    assert!(existe(&a.raizes(), &r.join("sub")));
    assert!(existe(&a.raizes(), &r.join("sub/x.rs")));
    assert!(!existe(&a.raizes(), &r.join("sub/nada.rs")));
    assert!(
        !existe(&a.raizes(), &r.join("link.rs")),
        "symlink out is not 'there'"
    );
    assert!(!e_arquivo(&a.raizes(), Path::new("sub/x.rs")), "relative");
    // the root is a directory of the project (it can be listed), but not a file to read
    assert_eq!(sondar(&a.raizes(), r), Sonda::Diretorio);
    assert!(e_diretorio(&a.raizes(), r));
    assert!(existe(&a.raizes(), r));
}

/// A listing is a READ: only a directory inside the roots is listed, the entries keep the
/// directory the caller named, and a symlink is a symlink (not followed).
#[test]
fn a_listagem_e_contida_e_nao_segue_symlink() {
    let a = Arvore::nova("lista");
    let r = &a.raiz;
    let mut nomes: Vec<String> = listar_contido(&a.raizes(), r)
        .expect("lista")
        .into_iter()
        .map(|e| e.nome.to_string_lossy().into_owned())
        .collect();
    nomes.sort();
    assert_eq!(
        nomes,
        ["grande.txt", "link.rs", "linkdir", "ruim.bin", "sub"]
    );
    let entradas = listar_contido(&a.raizes(), r).expect("lista");
    let link = entradas
        .iter()
        .find(|e| e.nome == "linkdir")
        .expect("linkdir");
    assert_eq!(
        link.caminho,
        r.join("linkdir"),
        "the path the caller named, not canonical"
    );
    assert!(link.tipo.expect("tipo").is_symlink());
    let sub = entradas.iter().find(|e| e.nome == "sub").expect("sub");
    assert!(sub.tipo.expect("tipo").is_dir());

    assert_eq!(
        tag(listar_contido(&a.raizes(), &r.join("linkdir"))),
        Tag::Fora
    );
    assert_eq!(tag(listar_contido(&a.raizes(), &a.tmp)), Tag::Fora);
    assert_eq!(
        tag(listar_contido(&a.raizes(), &r.join("sub/x.rs"))),
        Tag::Io(io::ErrorKind::NotADirectory),
        "a file is not a directory"
    );
    assert_eq!(
        tag(listar_contido(&a.raizes(), &r.join("nao_existe"))),
        Tag::Io(io::ErrorKind::NotFound)
    );
}

/// The signature of a parse memo: only for what is inside.
#[test]
fn a_assinatura_so_existe_dentro() {
    let a = Arvore::nova("assinatura");
    let (_, tamanho) = assinatura_contida(&a.raizes(), &a.raiz.join("sub/x.rs")).expect("sig");
    assert_eq!(tamanho, 4);
    assert!(assinatura_contida(&a.raizes(), &a.raiz.join("link.rs")).is_none());
    assert!(assinatura_contida(&a.raizes(), &a.raiz.join("sub/nada.rs")).is_none());
}

/// The piece the CONTENT wrote: relative and inside, or refused.
#[test]
fn o_pedaco_derivado_do_conteudo_fica_dentro_ou_e_recusado() {
    let a = Arvore::nova("derivar");
    let base = a.raiz.join("sub");
    let abs = a.tmp.join("fora.rs");
    let abs_dentro = a.raiz.join("sub/x.rs");
    let sobe_um = format!("{}/sub/x.rs", sobe(1).to_string_lossy());
    let sobe_tres = format!("{}/fora.rs", sobe(3).to_string_lossy());
    let casos: Vec<(&str, &str, bool)> = vec![
        ("relativo", "x.rs", true),
        ("relativo fundo", "a/b/c.rs", true),
        ("`..` que fica", &sobe_um, true),
        ("`..` que sai", &sobe_tres, false),
        ("absoluto fora", abs.to_str().expect("utf8"), false),
        (
            "absoluto DENTRO tambem (o join substituiria a base)",
            abs_dentro.to_str().expect("utf8"),
            false,
        ),
        ("vazio", "", false),
    ];
    for (nome, derivado, esperado) in casos {
        let got = derivar(&a.raizes(), &base, derivado);
        assert_eq!(got.is_some(), esperado, "{nome}");
        if let Some(p) = got {
            assert_eq!(
                p,
                base.join(derivado),
                "{nome}: the plain join, not normalized"
            );
        }
    }
}

/// A relative target that EXISTS from the process's directory is still refused: it would be
/// resolved against the analyzer's cwd, not against the project (a mutant that dropped the
/// `is_absolute` guard would canonicalize it from the cwd and find it inside the root).
#[test]
fn um_alvo_relativo_que_existe_a_partir_do_cwd_continua_recusado() {
    let cwd = std::env::current_dir().expect("cwd");
    let Some(nome) = fs::read_dir(&cwd)
        .expect("ls cwd")
        .flatten()
        .find(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .map(|e| e.file_name())
    else {
        return; // an empty cwd: the case cannot be built
    };
    let relativo = Path::new(&nome);
    assert!(
        cwd.join(relativo).is_file(),
        "precondition: it exists from the cwd"
    );
    assert_eq!(tag(ler_contido(&[cwd.as_path()], relativo)), Tag::Fora);
    assert!(!e_arquivo(&[cwd.as_path()], relativo));
    assert!(derivar(&[cwd.as_path()], Path::new(""), &nome.to_string_lossy()).is_none());
}

/// A listing returns the paths as the caller NAMED the directory, never the canonical form: the
/// callers compare them with overlay keys and print them in findings.
#[cfg(unix)]
#[test]
fn a_listagem_devolve_o_caminho_que_o_chamador_nomeou_nao_o_canonico() {
    let a = Arvore::nova("alias");
    let alias = a.tmp.join("alias");
    std::os::unix::fs::symlink(&a.raiz, &alias).expect("ln");
    let entradas = listar_contido(&[alias.as_path()], &alias).expect("lista");
    let sub = entradas.iter().find(|e| e.nome == "sub").expect("sub");
    assert_eq!(sub.caminho, alias.join("sub"));
    assert!(!sub.caminho.starts_with(&a.raiz), "not the canonical form");
}

// ------------------------------------------------------------------ the fast path

/// The fast canonical form (one `lstat` per component) agrees with the slow one (`canonicalize` of
/// the nearest existing prefix) wherever it answers — and it does NOT answer through a symlink, a
/// `..`, or a path that is not under the root.
#[test]
fn o_caminho_rapido_concorda_com_o_lento_e_recusa_o_que_nao_e_simples() {
    let a = Arvore::nova("rapido");
    let raiz = a.raiz.clone();
    let canon = raiz.canonicalize().expect("canon");
    let r = &raiz;
    let simples = [
        r.join("sub/x.rs"),
        r.join("sub"),
        r.join("sub/nao_existe.rs"),
        r.join("nao/existe/nada.rs"),
        r.join("grande.txt"),
        r.join("sub/./x.rs"), // `Path::components` drops a `.` in the middle: still plain
    ];
    for alvo in &simples {
        let rapido = canonico_sob_a_raiz(alvo, r, &canon);
        assert!(rapido.is_some(), "{alvo:?}: a plain path is answered");
        assert_eq!(rapido, canonico_do_prefixo_existente(alvo), "{alvo:?}");
    }
    let nao_simples = [
        r.join("link.rs"),                            // a symlink
        r.join("linkdir/y.rs"),                       // below a symlinked directory
        r.join("sub").join(sobe(1)).join("sub/x.rs"), // a `..`
        a.tmp.join("fora.rs"),                        // not under the root
        r.join("sub/x.rs/abaixo.rs"),                 // through a file
    ];
    for alvo in &nao_simples {
        assert_eq!(canonico_sob_a_raiz(alvo, r, &canon), None, "{alvo:?}");
    }
    // and the verdict is the same either way: the containment answers for every one of them
    for alvo in simples.iter().chain(&nao_simples) {
        let lento = canonico_do_prefixo_existente(alvo).is_some_and(|c| c.starts_with(&canon));
        assert_eq!(
            contido(&[r.as_path()], alvo, true).is_ok(),
            lento,
            "{alvo:?}: the fast path must not change the verdict"
        );
    }
}

// ------------------------------------------------------------------ normalization

/// Lexical normalization — the ONE rule for `..`. In absolute paths `..` stops at the root
/// (`/..` is `/`); in relative paths it is kept when nothing is left to climb.
#[test]
fn a_normalizacao_tem_uma_regra_so_para_dois_pontos() {
    let casos = [
        ("/a/b/../c", "/a/c"),
        ("/a/./b", "/a/b"),
        ("/a/b/..", "/a"),
        ("/a/../../x", "/x"),
        ("/..", "/"),
        ("/../..", "/"),
        ("/../a", "/a"),
        ("a/b/..", "a"),
        ("a/../../b", "../b"),
        ("../a", "../a"),
        ("../../a/..", "../.."),
        ("./a", "a"),
        ("a/..", ""),
        (".", ""),
        ("", ""),
    ];
    for (entrada, esperado) in casos {
        assert_eq!(
            normaliza(Path::new(entrada)),
            PathBuf::from(esperado),
            "{entrada}"
        );
    }
}

/// The behaviour of `caminho.rs` and `classe_css` before 02/11/2026: the excess `..` is DROPPED.
fn descarta(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            outro => out.push(outro.as_os_str()),
        }
    }
    out
}

/// The behaviour of `html` and `typescript` before 02/11/2026: the excess `..` is KEPT.
fn preserva(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    out.push("..");
                }
            }
            outro => out.push(outro.as_os_str()),
        }
    }
    out
}

/// The unified rule agrees with each old behaviour exactly where that one was right, and the
/// two old ones disagreed with each other on five shapes: for an ABSOLUTE path `..` stops at
/// the root (as the OS does — the DROP result), for a RELATIVE one the escape stays visible
/// (the KEEP result).
#[test]
fn a_regra_unica_concorda_com_cada_comportamento_antigo_onde_ele_acertava() {
    // (input, drop, keep): the two shapes where the old behaviours disagreed with each other
    let mut divergentes: Vec<(String, PathBuf, PathBuf)> = Vec::new();
    for entrada in [
        "/a/b/c",
        "/a/../b",
        "/a/b/../c",
        "/a/./b",
        "a/b/../c",
        "a/./b",
        "/a/..",
        // excess `..`
        "/a/../../x",
        "/..",
        "a/../../b",
        "../a",
        "/../a",
    ] {
        let p = Path::new(entrada);
        let novo = normaliza(p);
        let (d, k) = (descarta(p), preserva(p));
        if d == k {
            assert_eq!(
                novo, d,
                "{entrada}: where the old rules agree, so does the new one"
            );
        } else {
            // absolute: `..` stops at the root, as the OS does -> the DROP result; relative:
            // the escape stays visible -> the KEEP result
            let esperado = if p.is_absolute() { &d } else { &k };
            assert_eq!(&novo, esperado, "{entrada}");
            divergentes.push((entrada.to_string(), d, k));
        }
    }
    assert_eq!(
        divergentes.len(),
        5,
        "the shapes where DROP and KEEP disagreed: {divergentes:?}"
    );
}
