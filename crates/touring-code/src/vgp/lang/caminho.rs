//! V-4 (31/10/2026), deepened (02/11/2026) — the contained read: the ONE place where the
//! analyzer touches the filesystem of the project it judges.
//!
//! The PROPOSED content chooses paths: the `#[path = "…"]` of a Rust module, the
//! `href`/`src` of an HTML, the `@import` of a CSS, an import specifier. An ABSOLUTE
//! path does not concatenate in `Path::join` — it REPLACES the base — and a `../..`
//! escapes: without containment the analyzer read any file the user could read
//! (finding of 31/10/2026, in the Rust adapter).
//!
//! **Containment is the operation, not a precondition.** Until 02/11/2026 each adapter asked
//! `dentro_das_raizes(…)` and THEN called `std::fs::…`: an ordering rule that lived in the
//! caller — nineteen reads, nine separate checks, four normalizations that disagreed about
//! `..`, and every new read site a chance to forget the check (the `#[path]` escape of 31/10
//! was exactly that; the TypeScript `paths`/`extends`, the HTML `/../` and the symlinked
//! `.so` directory were the same shape). Now an adapter cannot read what it was not allowed
//! to: [`ler_contido`], [`listar_contido`] and the probes ([`sondar`], [`e_arquivo`],
//! [`e_diretorio`], [`existe`]) check and touch the disk in ONE call, and [`derivar`] builds a
//! path from a piece of the content only if it lands inside. The structural guard
//! `so_o_caminho_toca_o_disco` fails if another file under `vgp/lang/` touches the disk.
//!
//! **The roots are a SET, not one**: the project root AND the roots the RESOLVER
//! itself declares (the venv's `site-packages`, resolved through `pyvenv.cfg`; the
//! `node_modules` the module resolution walks). A path that only the CONTENT names never
//! enters the set. What is outside the set is refused with the reason, **without echoing the
//! path** ([`Recusa`] carries none).
//!
//! The rule: the canonical target must be inside SOME canonical root. A target that
//! does not exist yet is judged by its NEAREST EXISTING ANCESTOR — and it is the
//! canonicalization of that ancestor that resolves the `..` and exposes the symlink
//! that leaves the root (the lexical normalization does not catch symlinks).
//!
//! **Cost** (31/10): canonicalizing per read made the Rust adapter blow its p95 ceiling. The
//! canonical form of a root is remembered per thread, the target is canonicalized ONCE per
//! call (not once per root), and the nearest-existing-ancestor search uses `symlink_metadata`
//! (one syscall). The existence probes stat first and contain only on a hit: a missing
//! candidate — the common answer while probing extensions — costs what `is_file()` cost.

use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::OsString;
use std::fs::{self, FileType};
use std::io;
use std::path::{Component, Path, PathBuf};
use std::time::SystemTime;

thread_local! {
    /// What the roots are, per thread (the roots do not move).
    static RAIZES: RefCell<HashMap<PathBuf, Raiz>> = RefCell::new(HashMap::new());
}

/// A root as the comparison needs it.
enum Raiz {
    /// It exists on disk: its canonical form.
    Real(PathBuf),
    /// It does not exist (a virtual project — the adapter tests use `/home/u/proj`): its lexical
    /// normal form. Nothing under a root that does not exist exists either, so there is no
    /// symlink to escape and the comparison can be lexical.
    Virtual(PathBuf),
    /// Empty: `Path::starts_with("")` is true for EVERY path — it is not a root.
    Vazia,
}

/// Why a contained operation did not happen. It never carries the path: the refusal names the
/// reason, not what the content asked for.
#[derive(Debug)]
pub enum Recusa {
    /// The target is not inside any allowed root (an escape, a symlink that leaves, a relative
    /// target, no root at all).
    ForaDasRaizes,
    /// Inside the roots, but the filesystem said no (missing, not a file, not UTF-8…).
    Io(io::Error),
    /// A file over the cap the caller asked for.
    Grande {
        /// Its size, in bytes.
        tamanho: u64,
        /// The cap the caller asked for.
        limite: u64,
    },
    /// Inside the roots, but neither a file nor a directory (a FIFO, a device): reading it can
    /// block forever or never end.
    NaoERegular,
}

impl Recusa {
    /// Was it refused for being outside the roots (as opposed to missing or unreadable)?
    pub fn fora_das_raizes(&self) -> bool {
        matches!(self, Self::ForaDasRaizes)
    }
}

impl std::fmt::Display for Recusa {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ForaDasRaizes => f.write_str("caminho fora das raízes permitidas"),
            Self::Io(e) => write!(f, "{e}"),
            Self::Grande { tamanho, limite } => {
                write!(f, "{tamanho} bytes, acima do limite de {limite}")
            }
            Self::NaoERegular => f.write_str("não é um arquivo regular"),
        }
    }
}

/// What a path is on the disk (symlinks followed, like `is_file` / `is_dir`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sonda {
    /// A file inside the roots.
    Arquivo,
    /// A directory inside the roots.
    Diretorio,
    /// Exists inside the roots, but is neither (a FIFO, a device…).
    Outro,
    /// Does not exist (or cannot be stat'ed).
    Ausente,
    /// Exists, but OUTSIDE the roots: not looked at any further.
    Fora,
}

/// One entry of a listed directory. `caminho` is `dir` + the name, as `DirEntry::path` gives it
/// (never the canonical form).
#[derive(Debug)]
pub struct Entrada {
    /// The name of the entry.
    pub nome: OsString,
    /// `dir` + the name, as the caller named `dir`.
    pub caminho: PathBuf,
    /// `None` when the file type could not be read. A symlink is a symlink here (not followed).
    pub tipo: Option<FileType>,
}

// ---------------------------------------------------------------------------------------------
// normalization — the ONE rule for `..`
// ---------------------------------------------------------------------------------------------

/// The path with its `.` and `..` resolved **lexically** (no symlink is followed, the path may
/// not exist).
///
/// `..` climbs a normal component. At the filesystem root it stays at the root (`/..` is `/`).
/// In a RELATIVE path with nothing left to climb it is KEPT: dropping it would turn
/// `a/../../b` into `b`, an escape that reads as inside.
pub fn normaliza(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for comp in p.components() {
        match comp {
            Component::CurDir => {}
            Component::ParentDir => match out.components().next_back() {
                Some(Component::Normal(_)) => {
                    out.pop();
                }
                Some(Component::RootDir | Component::Prefix(_)) => {}
                Some(Component::ParentDir | Component::CurDir) | None => out.push(".."),
            },
            outro => out.push(outro.as_os_str()),
        }
    }
    out
}

// ---------------------------------------------------------------------------------------------
// the containment predicate (private: the operations below are the interface)
// ---------------------------------------------------------------------------------------------

/// The canonical target when it is inside SOME root; the refusal otherwise.
///
/// A root is not a FILE of the project: `admite_raiz` is `false` for a read and a path derived
/// from the content, `true` where a directory may be the root itself (listing the project).
///
/// Fail-closed: what cannot be proven inside is refused.
fn contido<P: AsRef<Path>>(
    raizes: &[P],
    alvo: &Path,
    admite_raiz: bool,
) -> Result<PathBuf, Recusa> {
    if raizes.is_empty() {
        return Err(Recusa::ForaDasRaizes);
    }
    if !alvo.is_absolute() {
        // A relative target would be resolved against the PROCESS's directory —
        // the analyzer's, not the project's. There is no honest answer.
        return Err(Recusa::ForaDasRaizes);
    }
    // the target is resolved at most ONCE per form, however many roots there are
    let mut real: Option<Option<PathBuf>> = None;
    let mut lexico: Option<PathBuf> = None;
    for raiz in raizes {
        let dentro = com_raiz(raiz.as_ref(), |r| match r {
            Raiz::Real(canon) => {
                let c = real
                    .get_or_insert_with(|| canonico_do_alvo(alvo, raiz.as_ref(), canon))
                    .as_ref()?;
                (c.starts_with(canon) && (admite_raiz || c != canon)).then(|| c.clone())
            }
            Raiz::Virtual(norm) => {
                let n = lexico.get_or_insert_with(|| normaliza(alvo));
                (n.starts_with(norm) && (admite_raiz || n != norm)).then(|| n.clone())
            }
            Raiz::Vazia => None,
        });
        if let Some(c) = dentro {
            return Ok(c);
        }
    }
    Err(Recusa::ForaDasRaizes)
}

/// Runs `f` on what `raiz` is, remembered: the same root is asked about on every read, and the
/// canonicalization walks the whole path. A hit allocates nothing.
fn com_raiz<R>(raiz: &Path, f: impl FnOnce(&Raiz) -> R) -> R {
    RAIZES.with(|c| {
        let mut mapa = c.borrow_mut();
        if let Some(conhecida) = mapa.get(raiz) {
            return f(conhecida);
        }
        let nova = match raiz.canonicalize() {
            Ok(canon) => Raiz::Real(canon),
            Err(_) => {
                let norm = normaliza(raiz);
                if norm.as_os_str().is_empty() {
                    Raiz::Vazia
                } else {
                    Raiz::Virtual(norm)
                }
            }
        };
        let saida = f(&nova);
        mapa.insert(raiz.to_path_buf(), nova);
        saida
    })
}

/// The canonical form of `alvo`, given a root `raiz` whose canonical form `canon` is known.
fn canonico_do_alvo(alvo: &Path, raiz: &Path, canon: &Path) -> Option<PathBuf> {
    canonico_sob_a_raiz(alvo, raiz, canon).or_else(|| canonico_do_prefixo_existente(alvo))
}

/// The fast path of the canonical form: `alvo` lies lexically under `raiz` and no component below
/// it is a symlink, so its canonical form is `canon` + the rest — one `lstat` per component
/// instead of walking the whole path (a deep project path costs a syscall per level in
/// `canonicalize`, on EVERY probe). `None` — not "outside": the caller takes the slow path — when
/// anything is not plain: a `.`/`..`, a symlink, a component that is not a directory.
fn canonico_sob_a_raiz(alvo: &Path, raiz: &Path, canon: &Path) -> Option<PathBuf> {
    let resto = alvo.strip_prefix(raiz).ok()?;
    let mut atual = raiz.to_path_buf();
    let mut saida = canon.to_path_buf();
    let mut componentes = resto.components();
    for comp in componentes.by_ref() {
        let Component::Normal(nome) = comp else {
            return None;
        };
        atual.push(nome);
        saida.push(nome);
        match atual.symlink_metadata() {
            Ok(meta) if meta.file_type().is_symlink() => return None,
            Ok(_) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                // nothing exists from here on: the rest is plain names
                for resto in componentes {
                    let Component::Normal(nome) = resto else {
                        return None;
                    };
                    saida.push(nome);
                }
                return Some(saida);
            }
            Err(_) => return None,
        }
    }
    Some(saida)
}

/// The canonical target: the nearest existing prefix, canonicalized, with the rest
/// re-applied (the `..` of the rest are resolved lexically over the already
/// canonicalized prefix).
fn canonico_do_prefixo_existente(alvo: &Path) -> Option<PathBuf> {
    let mut atual = alvo.to_path_buf();
    let mut resto: Vec<OsString> = Vec::new();
    loop {
        // `symlink_metadata` is ONE syscall; `canonicalize` walks the whole path
        // (and fails on the last component) — it is the walk up to the deepest
        // existing ancestor that costs.
        if atual.symlink_metadata().is_ok() {
            let mut saida = atual.canonicalize().ok()?;
            for comp in resto.iter().rev() {
                if comp == ".." {
                    saida.pop();
                } else {
                    saida.push(comp);
                }
            }
            return Some(saida);
        }
        let pai = atual.parent()?;
        let comp = atual
            .file_name()
            .map(|n| n.to_os_string())
            .unwrap_or_else(|| OsString::from(".."));
        resto.push(comp);
        atual = pai.to_path_buf();
    }
}

// ---------------------------------------------------------------------------------------------
// building a path from the content
// ---------------------------------------------------------------------------------------------

/// `base` + a piece the CONTENT wrote, only if the result lands inside the roots.
///
/// The piece must be relative and non-empty: an absolute one does not concatenate — in
/// `Path::join` it REPLACES the base, so `dir.join("/etc/passwd")` is `/etc/passwd`. The path
/// returned is the plain join (not normalized): what the caller does with it is its own.
///
/// `None` = refused (absolute, empty, or outside): the caller reports the reason, never the
/// path.
pub fn derivar<P: AsRef<Path>>(raizes: &[P], base: &Path, derivado: &str) -> Option<PathBuf> {
    if derivado.is_empty() || Path::new(derivado).is_absolute() {
        return None;
    }
    let alvo = base.join(derivado);
    contido(raizes, &alvo, false).ok().map(|_| alvo)
}

// ---------------------------------------------------------------------------------------------
// the operations that touch the disk
// ---------------------------------------------------------------------------------------------

/// The text of the file at `alvo`, if it is inside the roots.
pub fn ler_contido<P: AsRef<Path>>(raizes: &[P], alvo: &Path) -> Result<String, Recusa> {
    ler(raizes, alvo, None)
}

/// [`ler_contido`] refusing a file over `limite` bytes (a generated bundle is not a source).
pub fn ler_contido_ate<P: AsRef<Path>>(
    raizes: &[P],
    alvo: &Path,
    limite: u64,
) -> Result<String, Recusa> {
    ler(raizes, alvo, Some(limite))
}

fn ler<P: AsRef<Path>>(raizes: &[P], alvo: &Path, limite: Option<u64>) -> Result<String, Recusa> {
    let real = contido(raizes, alvo, false)?;
    let meta = fs::metadata(&real).map_err(Recusa::Io)?;
    // A directory goes on to `read_to_string`, which says why it cannot be read; a FIFO or a
    // device would block or never end.
    if !meta.is_file() && !meta.is_dir() {
        return Err(Recusa::NaoERegular);
    }
    if let Some(limite) = limite
        && meta.len() > limite
    {
        return Err(Recusa::Grande {
            tamanho: meta.len(),
            limite,
        });
    }
    fs::read_to_string(&real).map_err(Recusa::Io)
}

/// The entries of the directory `dir`, in the order the OS gives them, if it is inside the roots.
pub fn listar_contido<P: AsRef<Path>>(raizes: &[P], dir: &Path) -> Result<Vec<Entrada>, Recusa> {
    let real = contido(raizes, dir, true)?;
    let entradas = fs::read_dir(&real).map_err(Recusa::Io)?;
    Ok(entradas
        .flatten()
        .map(|e| Entrada {
            nome: e.file_name(),
            caminho: dir.join(e.file_name()),
            tipo: e.file_type().ok(),
        })
        .collect())
}

/// What `alvo` is. A missing path is [`Sonda::Ausente`] wherever it would be; one that EXISTS
/// outside the roots is [`Sonda::Fora`] and is not looked at any further.
pub fn sondar<P: AsRef<Path>>(raizes: &[P], alvo: &Path) -> Sonda {
    let Ok(meta) = fs::metadata(alvo) else {
        return Sonda::Ausente;
    };
    if contido(raizes, alvo, true).is_err() {
        return Sonda::Fora;
    }
    if meta.is_file() {
        Sonda::Arquivo
    } else if meta.is_dir() {
        Sonda::Diretorio
    } else {
        Sonda::Outro
    }
}

/// Is `alvo` a file inside the roots?
pub fn e_arquivo<P: AsRef<Path>>(raizes: &[P], alvo: &Path) -> bool {
    sondar(raizes, alvo) == Sonda::Arquivo
}

/// Is `alvo` a directory inside the roots?
pub fn e_diretorio<P: AsRef<Path>>(raizes: &[P], alvo: &Path) -> bool {
    sondar(raizes, alvo) == Sonda::Diretorio
}

/// Does `alvo` exist inside the roots (a file, a directory, anything)?
pub fn existe<P: AsRef<Path>>(raizes: &[P], alvo: &Path) -> bool {
    !matches!(sondar(raizes, alvo), Sonda::Ausente | Sonda::Fora)
}

/// (modification time, length) of the file at `alvo`, if it is inside the roots — the signature
/// a parse memo compares to know the file did not change.
pub fn assinatura_contida<P: AsRef<Path>>(raizes: &[P], alvo: &Path) -> Option<(SystemTime, u64)> {
    let real = contido(raizes, alvo, false).ok()?;
    let meta = fs::metadata(real).ok()?;
    Some((meta.modified().ok()?, meta.len()))
}

#[cfg(test)]
mod tests;
