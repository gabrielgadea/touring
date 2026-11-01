//! VGP layer 2 for Rust — the project's `rust-analyzer`, native diagnostics only.
//!
//! **Which binary.** The one the project's toolchain resolves: `rustup which rust-analyzer` run at the
//! project's Cargo root, so a `rust-toolchain.toml` pin wins over the machine default. Nothing is ever
//! installed: the lookup runs with `RUSTUP_AUTO_INSTALL=0`, a missing toolchain or component is an
//! `unknown` that says so, and the server itself is started with `CARGO_NET_OFFLINE=true` (a workspace
//! whose dependencies are not on disk fails to load — an `unknown` — instead of downloading them).
//!
//! **Which configuration.** `checkOnSave` is off — the flycheck (`cargo check`) is a build, minutes of
//! CPU on a big workspace, and the question layer 2 asks ("does the name you used exist?") is answered by
//! the server's own analysis. `diagnostics.experimental.enable` is ON: without it E0425 (unknown value)
//! and E0599 (unknown method) are not reported at all (measured on rust-analyzer 1.98: 1 of 3 invented
//! names). Build scripts, proc macros and cache priming are off by default (a loaded workspace costs
//! seconds instead of a `cargo check` of every dependency); `TOURING_VGP_L2_RA_BUILD=1` turns the first
//! two back on.
//!
//! **How it is asked.** Pull (`textDocument/diagnostic`) after the server reports `quiescent: true` — push
//! publishes the syntax diagnostics first and the semantic ones later, and a pull sent while the workspace
//! is loading is answered with an EMPTY report, so the first publication/answer proves nothing (see
//! [`crate::lsp_nucleo`]). A file the server says is not part of any crate (`unlinked-file`) and a
//! workspace whose health is `error` are `unknown`, never `allow`.
//!
//! **What it cannot see** (measured, rust-analyzer 1.98, native diagnostics): a missing function in an
//! existing module (`util::inventada()`), an unresolved type in a `let` annotation, an unresolved path
//! in an expression with several segments. `cargo check` finds them; this layer does not run it.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::{Map, Value, json};

use crate::achado::{Escopo, L2Achado, severidade};
use crate::lsp_nucleo::{EscolhaDeModo, Inicio, Modo, Normalizador, NucleoLsp, Pedido, Perfil};
use crate::servidor::{Alvo, Ambiente, Edicao, Sessao, Verificado, dirs_ate_a_raiz};

/// The cold start (cargo metadata + workspace load) is paid once, in the warm-up thread.
const STARTUP_BUDGET: Duration = Duration::from_secs(120);
/// Per-check budget of a warm session (a pull on a loaded workspace answers in tens of ms).
const DEFAULT_BUDGET: Duration = Duration::from_millis(2000);
const ENV_BUILD: &str = "TOURING_VGP_L2_RA_BUILD";

static PERFIL: Perfil = Perfil {
    nome: "rust-analyzer",
    extensao_inline: "rs",
    exige_quiescencia: true,
    language_id: |_| "rust",
};

/// Types of `std`/`core`/`alloc` whose missing members are a fact about the standard library (sorted
/// for `binary_search`; the head identifier of the type in the diagnostic, references stripped).
const TIPOS_STD: [&str; 51] = [
    "Arc",
    "BTreeMap",
    "BTreeSet",
    "BinaryHeap",
    "Box",
    "Cell",
    "Cow",
    "Duration",
    "HashMap",
    "HashSet",
    "Instant",
    "LinkedList",
    "Mutex",
    "Option",
    "OsStr",
    "OsString",
    "Path",
    "PathBuf",
    "Rc",
    "RefCell",
    "Result",
    "RwLock",
    "String",
    "Vec",
    "VecDeque",
    "bool",
    "char",
    "f32",
    "f64",
    "i128",
    "i16",
    "i32",
    "i64",
    "i8",
    "isize",
    "str",
    "u128",
    "u16",
    "u32",
    "u64",
    "u8",
    "usize",
    "Ordering",
    "Range",
    "RangeInclusive",
    "Reverse",
    "SystemTime",
    "Weak",
    "Wrapping",
    "Once",
    "Pin",
];

// ── discovery ──────────────────────────────────────────────────────────────

/// A `rust-analyzer` found for a project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Localizado {
    /// The server executable (the real binary of the toolchain, not the rustup proxy).
    pub(crate) binario: PathBuf,
    /// The Cargo root the server is rooted at: the highest `Cargo.toml` that declares a `[workspace]`
    /// between the file and the project root, else the nearest `Cargo.toml`.
    pub(crate) cargo: PathBuf,
}

fn declara_workspace(manifesto: &Path) -> bool {
    std::fs::read_to_string(manifesto)
        .is_ok_and(|texto| texto.lines().any(|l| l.trim() == "[workspace]"))
}

/// The Cargo root for an edit of `arquivo` in the project at `raiz` — `None` when no `Cargo.toml` (nor a
/// `rust-project.json` at the root) exists between them: rust-analyzer would load no project and answer
/// every file with an empty report.
pub(crate) fn raiz_cargo(raiz: &Path, arquivo: &str) -> Option<PathBuf> {
    let dirs = dirs_ate_a_raiz(raiz, arquivo);
    let com_manifesto: Vec<&PathBuf> = dirs
        .iter()
        .filter(|dir| dir.join("Cargo.toml").is_file())
        .collect();
    let workspace = com_manifesto
        .iter()
        .rev()
        .find(|dir| declara_workspace(&dir.join("Cargo.toml")));
    workspace
        .or(com_manifesto.first())
        .map(|dir| (*dir).clone())
        .or_else(|| {
            raiz.join("rust-project.json")
                .is_file()
                .then(|| raiz.to_path_buf())
        })
}

/// The registry sub-key of an edit: the Cargo root (empty when there is none).
pub(crate) fn subchave(raiz: &Path, arquivo: &str) -> PathBuf {
    raiz_cargo(raiz, arquivo).unwrap_or_default()
}

/// The pure half of discovery: a Cargo root exists and `rustup` can be run. Answered before any
/// thread or process is launched.
pub(crate) fn pre_checar(alvo: &Alvo<'_>, amb: &Ambiente) -> Result<(), String> {
    pre_localizar(alvo.raiz, alvo.arquivo, amb).map(|_| ())
}

fn pre_localizar(raiz: &Path, arquivo: &str, amb: &Ambiente) -> Result<(PathBuf, PathBuf), String> {
    let cargo = raiz_cargo(raiz, arquivo).ok_or_else(|| {
        format!(
            "nenhum Cargo.toml (nem rust-project.json) entre {arquivo} e a raiz {}: o rust-analyzer não carregaria projeto algum",
            raiz.display()
        )
    })?;
    let rustup = amb.achar("rustup").ok_or_else(|| {
        "rustup não está no PATH: o toolchain do projeto não pode ser resolvido e o Touring nunca instala o rust-analyzer"
            .to_string()
    })?;
    Ok((cargo, rustup))
}

/// Finds the `rust-analyzer` of the project's toolchain — or says why there is none.
pub(crate) fn localizar(raiz: &Path, arquivo: &str, amb: &Ambiente) -> Result<Localizado, String> {
    let (cargo, rustup) = pre_localizar(raiz, arquivo, amb)?;
    let saida = Command::new(&rustup)
        .args(["which", "rust-analyzer"])
        .current_dir(&cargo)
        .env("RUSTUP_AUTO_INSTALL", "0")
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("não foi possível executar `rustup which rust-analyzer`: {e}"))?;
    if !saida.status.success() {
        let erro = String::from_utf8_lossy(&saida.stderr);
        return Err(format!(
            "o toolchain de {} não tem rust-analyzer (`rustup which rust-analyzer`: {}); o Touring nunca instala",
            cargo.display(),
            erro.lines().next().unwrap_or("sem mensagem").trim()
        ));
    }
    let impresso = String::from_utf8_lossy(&saida.stdout);
    let binario = PathBuf::from(impresso.trim());
    if !binario.is_file() {
        return Err(format!(
            "`rustup which rust-analyzer` apontou {}, que não existe",
            binario.display()
        ));
    }
    Ok(Localizado { binario, cargo })
}

// ── configuration ──────────────────────────────────────────────────────────

fn capabilities() -> Value {
    json!({
        "experimental": {"serverStatusNotification": true},
        "textDocument": {
            "publishDiagnostics": {"versionSupport": true},
            "diagnostic": {"dynamicRegistration": false},
        },
    })
}

/// `initializationOptions`: native diagnostics only, the cheap workspace load.
fn opcoes(com_build: bool) -> Value {
    json!({
        "checkOnSave": false,
        "cachePriming": {"enable": false},
        "cargo": {"buildScripts": {"enable": com_build}},
        "procMacro": {"enable": com_build},
        "diagnostics": {"enable": true, "experimental": {"enable": true}},
    })
}

// ── diagnostic → finding ───────────────────────────────────────────────────

/// The text of a single-line LSP `range` of `texto` (columns are UTF-16 code units).
pub(crate) fn trecho(texto: &str, d: &Value) -> Option<String> {
    let numero = |p: &str| d.pointer(p).and_then(Value::as_u64);
    let linha = numero("/range/start/line")?;
    if linha != numero("/range/end/line")? {
        return None;
    }
    let texto_da_linha = texto.lines().nth(usize::try_from(linha).ok()?)?;
    let inicio = byte_da_coluna(texto_da_linha, numero("/range/start/character")?)?;
    let fim = byte_da_coluna(texto_da_linha, numero("/range/end/character")?)?;
    texto_da_linha.get(inicio..fim).map(str::to_string)
}

/// The byte offset of UTF-16 column `coluna` in `linha`.
fn byte_da_coluna(linha: &str, coluna: u64) -> Option<usize> {
    let mut unidades = 0u64;
    for (byte, c) in linha.char_indices() {
        if unidades == coluna {
            return Some(byte);
        }
        unidades += c.len_utf16() as u64;
    }
    (unidades == coluna).then_some(linha.len())
}

fn e_ident(c: char) -> bool {
    c == '_' || c.is_alphanumeric()
}

/// The identifier a text ends with (`std::collections::HashMapz` → `HashMapz`).
fn ident_final(s: &str) -> Option<&str> {
    let s = s.trim().trim_end_matches([';', ',', '}', ')']);
    let inicio = s.rfind(|c: char| !e_ident(c)).map_or(0, |i| i + 1);
    (inicio < s.len()).then(|| &s[inicio..])
}

/// The identifier a text starts with (`y: 2` → `y`).
fn ident_inicial(s: &str) -> Option<&str> {
    let s = s.trim();
    let fim = s.find(|c: char| !e_ident(c)).unwrap_or(s.len());
    (fim > 0).then(|| &s[..fim])
}

/// The first `` `…` `` run of a message.
fn entre_crases(msg: &str) -> Option<&str> {
    let resto = &msg[msg.find('`')? + 1..];
    let fim = resto.find('`')?;
    (fim > 0).then(|| &resto[..fim])
}

/// The type in `… on type `Vec<i32, Global>``.
fn tipo_da_mensagem(msg: &str) -> Option<&str> {
    let depois = &msg[msg.find("on type `")? + "on type `".len()..];
    Some(&depois[..depois.find('`')?])
}

/// Is `tipo` (as rust-analyzer prints it) a type of the standard library?
pub(crate) fn tipo_std(tipo: &str) -> bool {
    let sem_ref = tipo
        .trim()
        .trim_start_matches('&')
        .trim_start_matches("mut ");
    let cabeca = sem_ref
        .split(|c: char| !(e_ident(c) || c == ':'))
        .next()
        .unwrap_or(sem_ref);
    let cabeca = cabeca.rsplit("::").next().unwrap_or(cabeca);
    TIPOS_STD.contains(&cabeca)
}

/// Crate names declared as dependencies by the `Cargo.toml` at `manifesto` (hyphens as underscores).
pub(crate) fn dependencias(manifesto: &Path) -> HashSet<String> {
    let Ok(texto) = std::fs::read_to_string(manifesto) else {
        return HashSet::new();
    };
    let mut nomes = HashSet::new();
    let mut em_deps = false;
    for linha in texto.lines().map(str::trim) {
        if let Some(secao) = linha.strip_prefix('[') {
            let secao = secao.trim_end_matches(']');
            let (cabeca, filho) = secao.rsplit_once('.').unwrap_or((secao, ""));
            em_deps = secao.ends_with("dependencies");
            if cabeca.ends_with("dependencies") && !filho.is_empty() {
                nomes.insert(filho.trim_matches('"').replace('-', "_"));
                em_deps = false;
            }
        } else if em_deps && let Some((nome, _)) = linha.split_once('=') {
            let nome = nome
                .trim()
                .split('.')
                .next()
                .unwrap_or("")
                .trim_matches('"');
            if !nome.is_empty() {
                nomes.insert(nome.replace('-', "_"));
            }
        }
    }
    nomes
}

/// The `Cargo.toml` of the crate the file belongs to: the nearest above it (not above `limite`).
fn manifesto_do_crate(origem: &Path, limite: &Path) -> Option<PathBuf> {
    origem
        .ancestors()
        .skip(1)
        .take_while(|dir| dir.starts_with(limite))
        .map(|dir| dir.join("Cargo.toml"))
        .find(|manifesto| manifesto.is_file())
}

/// Whose symbol is behind an unresolved import path (`std::collections::HashMapz`).
fn escopo_do_import(caminho: &str, origem: &Path, cargo: &Path) -> Escopo {
    let primeiro = caminho
        .trim()
        .trim_start_matches("::")
        .split("::")
        .next()
        .unwrap_or("")
        .trim();
    match primeiro {
        "std" | "core" | "alloc" => Escopo::Externo,
        "crate" | "self" | "super" => Escopo::Projeto,
        outro => {
            let declarada = manifesto_do_crate(origem, cargo)
                .is_some_and(|m| dependencias(&m).contains(&outro.replace('-', "_")));
            if declarada {
                Escopo::Externo
            } else {
                Escopo::Projeto
            }
        }
    }
}

/// Rule of a rust-analyzer diagnostic code (rustc codes for its native analysis).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Regra {
    Nome,
    Import,
    Modulo,
    Membro,
    Campo,
}

fn regra_do_codigo(code: &str, msg: &str) -> Option<(Regra, &'static str)> {
    match code {
        "E0425" | "E0412" | "E0433" => Some((Regra::Nome, "nome")),
        "E0432" => Some((Regra::Import, "import")),
        "E0583" => Some((Regra::Modulo, "import")),
        "E0599" | "E0609" => Some((Regra::Membro, "atributo")),
        // rust-analyzer labels both the struct-literal field and the field access E0559/E0560
        "E0559" | "E0560" if msg.starts_with("no field `") => Some((Regra::Membro, "atributo")),
        "E0559" | "E0560" => Some((Regra::Campo, "kwarg")),
        _ => None,
    }
}

fn codigo(d: &Value) -> &str {
    d.get("code").and_then(Value::as_str).unwrap_or("")
}

/// The finding of one native diagnostic, or `None` when it is not a name class.
fn achado_de(d: &Value, pedido: &Pedido<'_>, cargo: &Path) -> Option<L2Achado> {
    let msg = d.get("message").and_then(Value::as_str).unwrap_or("");
    let (regra, classe) = regra_do_codigo(codigo(d), msg)?;
    let texto = trecho(pedido.content, d).unwrap_or_default();
    let (nome, escopo) = identificar(regra, msg, &texto, pedido, cargo)?;
    let linha0 = d.pointer("/range/start/line").and_then(Value::as_u64)?;
    Some(L2Achado {
        classe: classe.to_string(),
        nome,
        linha: u32::try_from(linha0.saturating_add(1)).unwrap_or(u32::MAX),
        escopo,
        severidade: severidade(pedido.balde, escopo),
        regra: codigo(d).to_string(),
        mensagem: msg.to_string(),
    })
}

/// The invented name and whose it is. rust-analyzer's messages often carry no name (`no such value in
/// this scope`), so the name is read from the source under the diagnostic's range (`texto`).
fn identificar(
    regra: Regra,
    msg: &str,
    texto: &str,
    pedido: &Pedido<'_>,
    cargo: &Path,
) -> Option<(String, Escopo)> {
    Some(match regra {
        Regra::Nome => (ident_final(texto)?.to_string(), Escopo::Proprio),
        Regra::Import => (
            ident_final(texto)?.to_string(),
            escopo_do_import(texto, &pedido.origem, cargo),
        ),
        Regra::Modulo => (
            ident_final(texto.trim_start_matches("mod "))?.to_string(),
            Escopo::Projeto,
        ),
        Regra::Campo => (ident_inicial(texto)?.to_string(), Escopo::Projeto),
        Regra::Membro => {
            let nome = entre_crases(msg)
                .map(str::to_string)
                .or_else(|| ident_final(texto).map(str::to_string))?;
            (nome, escopo_do_membro(msg, texto))
        }
    })
}

/// A member of a std type is a fact about the standard library (`externo`); of anything else, the
/// project's (`projeto`). The type comes from the message (`on type `Vec<i32, Global>``) or, for an
/// associated item, from the path before `::` in the source (`String::inventada`).
fn escopo_do_membro(msg: &str, texto: &str) -> Escopo {
    let tipo = tipo_da_mensagem(msg).or_else(|| texto.split_once("::").map(|(cabeca, _)| cabeca));
    if tipo.is_some_and(tipo_std) {
        Escopo::Externo
    } else {
        Escopo::Projeto
    }
}

/// The normalizer of rust-analyzer's native diagnostics.
pub(crate) struct NormRa {
    cargo: PathBuf,
}

impl Normalizador for NormRa {
    fn achados(
        &self,
        diags: &[Value],
        pedido: &Pedido<'_>,
        _raiz: &Path,
    ) -> Result<Vec<L2Achado>, String> {
        if diags.iter().any(|d| codigo(d) == "unlinked-file") {
            return Err(format!(
                "camada 2 (rust-analyzer): {} não faz parte de nenhum crate carregado (unlinked-file: nenhum `mod` o declara) — não medido",
                pedido.origem.display()
            ));
        }
        Ok(diags
            .iter()
            .filter(|d| d.get("source").and_then(Value::as_str) == Some("rust-analyzer"))
            .filter_map(|d| achado_de(d, pedido, &self.cargo))
            .collect())
    }
}

// ── the session ────────────────────────────────────────────────────────────

/// A warm `rust-analyzer` rooted at a Cargo root.
pub(crate) struct SessaoRa {
    nucleo: NucleoLsp,
    norm: NormRa,
}

/// Starts the project's `rust-analyzer` (the spawner of [`crate::servidor::Servidor::RustAnalyzer`]).
pub(crate) fn abrir(alvo: &Alvo<'_>, amb: &Ambiente) -> Result<Box<dyn Sessao>, String> {
    let achado = localizar(alvo.raiz, alvo.arquivo, amb)?;
    let com_build = std::env::var(ENV_BUILD).is_ok_and(|v| v.trim() == "1");
    let mut comando = Command::new(&achado.binario);
    comando
        .current_dir(&achado.cargo)
        .env("RUSTUP_AUTO_INSTALL", "0")
        .env("CARGO_NET_OFFLINE", "true");
    let inicio = Inicio {
        comando,
        sections: Map::new(),
        raiz_lsp: &achado.cargo,
        capabilities: capabilities(),
        init_options: opcoes(com_build),
        startup: STARTUP_BUDGET,
        modo: EscolhaDeModo::Exigido(Modo::Pull),
    };
    let nucleo = NucleoLsp::iniciar(inicio, alvo.raiz.to_path_buf(), DEFAULT_BUDGET, &PERFIL)
        .map_err(|e| {
            format!(
                "rust-analyzer não iniciou em {}: {e}",
                achado.cargo.display()
            )
        })?;
    Ok(Box::new(SessaoRa {
        nucleo,
        norm: NormRa {
            cargo: achado.cargo,
        },
    }))
}

impl Sessao for SessaoRa {
    fn verificar(&mut self, edicao: &Edicao<'_>, prazo: Option<Duration>) -> Verificado {
        match prazo {
            None => self.nucleo.verificar(&self.norm, edicao),
            Some(prazo) => self.nucleo.verificar_ate(&self.norm, edicao, prazo),
        }
    }

    /// The first pull of a workspace pays the load: wait for `quiescent`, then prime the analysis with
    /// the very file that asked for the session.
    fn aquecer(&mut self, edicao: &Edicao<'_>, prazo: Duration) -> Result<(), String> {
        let limite = Instant::now() + prazo;
        self.nucleo
            .esperar_pronto(limite)
            .map_err(|e| format!("rust-analyzer não terminou de carregar o workspace: {e}"))?;
        let restante = limite.saturating_duration_since(Instant::now());
        let verificado = self.nucleo.verificar_ate(&self.norm, edicao, restante);
        if verificado.saudavel {
            Ok(())
        } else {
            Err(verificado
                .resultado
                .motivo
                .unwrap_or_else(|| "rust-analyzer não ficou utilizável".to_string()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::achado::{Balde, Severidade};
    use crate::testutil::Arvore;

    // ── the diagnostics rust-analyzer 1.98 really sent (captured 09/10/2026 on a 2-file crate) ──

    /// The source the diagnostics below were captured on (one line per element, columns exact).
    fn fonte() -> String {
        [
            "use std::collections::HashMapz;",
            "use crate::inexistente::Coisa;",
            "use serde::Serialize;",
            "mod fantasma;",
            "pub struct Ponto { pub x: i32 }",
            "pub enum E { A { f: i32 } }",
            "pub fn a() {",
            "    let p = Ponto { x: 1, y: 2 };",
            "    let _a = p.z;",
            "    let v: Vec<i32> = Vec::new();",
            "    v.pusha(3);",
            "    let _c = nome_solto + 1;",
            "    let _e = Ponto::novo();",
            "    let _g = E::A { g: 1 };",
            "    let _s = String::new();",
            "    _s.nao_existe();",
            "}",
            "",
        ]
        .join("\n")
    }

    fn d(codigo: &str, msg: &str, l: u64, c0: u64, c1: u64) -> Value {
        json!({"code": codigo, "source": "rust-analyzer", "severity": 1, "message": msg,
            "range": {"start": {"line": l, "character": c0}, "end": {"line": l, "character": c1}}})
    }

    fn pedido<'a>(arvore: &Arvore, balde: Balde, content: &'a str) -> Pedido<'a> {
        Pedido {
            uri: String::new(),
            origem: arvore.raiz.join("src/lib.rs"),
            balde,
            content,
            disk: None,
            overlays: &[],
            deadline: Instant::now(),
        }
    }

    fn achar(t: &Arvore, diag: &Value) -> L2Achado {
        let fonte = fonte();
        let p = pedido(t, Balde::Arquivo, &fonte);
        achado_de(diag, &p, &t.raiz).expect("a name-class diagnostic must be a finding")
    }

    #[test]
    fn e0425_unknown_value_is_a_nome_read_from_the_source_range() {
        let t = Arvore::nova("ra_e0425");
        let a = achar(&t, &d("E0425", "no such value in this scope", 11, 13, 23));
        assert_eq!(
            (a.classe.as_str(), a.nome.as_str(), a.linha, a.escopo),
            ("nome", "nome_solto", 12, Escopo::Proprio)
        );
        assert_eq!(
            a.severidade,
            Severidade::Warn,
            "an own name in a project file only warns"
        );
    }

    #[test]
    fn e0560_unknown_struct_literal_field_is_a_kwarg_named_by_its_range() {
        let t = Arvore::nova("ra_e0560");
        let a = achar(&t, &d("E0560", "no such field", 7, 26, 30));
        assert_eq!(
            (a.classe.as_str(), a.nome.as_str(), a.escopo),
            ("kwarg", "y", Escopo::Projeto)
        );
        let variante = achar(&t, &d("E0559", "no such field", 13, 20, 24));
        assert_eq!(
            (variante.classe.as_str(), variante.nome.as_str()),
            ("kwarg", "g")
        );
    }

    #[test]
    fn e0599_unknown_method_of_a_std_type_is_externo_and_denies() {
        let t = Arvore::nova("ra_e0599");
        let vec = achar(
            &t,
            &d(
                "E0599",
                "no method `pusha` on type `Vec<i32, Global>`",
                10,
                6,
                11,
            ),
        );
        assert_eq!(
            (vec.classe.as_str(), vec.nome.as_str(), vec.escopo),
            ("atributo", "pusha", Escopo::Externo)
        );
        assert_eq!(vec.severidade, Severidade::Deny);
        let string = achar(
            &t,
            &d(
                "E0599",
                "no method `nao_existe` on type `String`",
                15,
                8,
                18,
            ),
        );
        assert_eq!(
            (string.nome.as_str(), string.escopo),
            ("nao_existe", Escopo::Externo)
        );
    }

    #[test]
    fn e0599_assoc_item_has_no_name_in_the_message_so_it_comes_from_the_path() {
        let t = Arvore::nova("ra_assoc");
        let p = achar(&t, &d("E0599", "no such associated item", 12, 13, 24));
        assert_eq!(
            (p.nome.as_str(), p.escopo),
            ("novo", Escopo::Projeto),
            "Ponto is the project's"
        );
        assert_eq!(p.severidade, Severidade::Warn);
        let std = {
            let pe = pedido(
                &t,
                Balde::Arquivo,
                "fn f() { let _ = String::inventada(); }\n",
            );
            achado_de(
                &d("E0599", "no such associated item", 0, 17, 34),
                &pe,
                &t.raiz,
            )
            .unwrap()
        };
        assert_eq!(
            (std.nome.as_str(), std.escopo),
            ("inventada", Escopo::Externo),
            "String is std's"
        );
    }

    #[test]
    fn a_field_access_that_rust_analyzer_labels_e0559_is_an_atributo() {
        let t = Arvore::nova("ra_field");
        let a = achar(&t, &d("E0559", "no field `z` on type `Ponto`", 8, 15, 16));
        assert_eq!(
            (a.classe.as_str(), a.nome.as_str(), a.escopo),
            ("atributo", "z", Escopo::Projeto)
        );
    }

    #[test]
    fn e0432_unresolved_import_is_scoped_by_the_root_of_the_path() {
        let t = Arvore::nova("ra_e0432");
        t.arquivo(
            "Cargo.toml",
            "[package]\nname = \"x\"\n[dependencies]\nserde = \"1\"\nserde_json.workspace = true\n[dev-dependencies]\ntokio-util = \"0.7\"\n",
        );
        let std = achar(&t, &d("E0432", "unresolved import", 0, 4, 30));
        assert_eq!(
            (std.classe.as_str(), std.nome.as_str(), std.escopo),
            ("import", "HashMapz", Escopo::Externo)
        );
        assert_eq!(
            std.severidade,
            Severidade::Deny,
            "std lacks that item: no edit of the project fixes it"
        );
        let local = achar(&t, &d("E0432", "unresolved import", 1, 4, 29));
        assert_eq!(
            (local.nome.as_str(), local.escopo),
            ("Coisa", Escopo::Projeto)
        );
        let declarada = achar(&t, &d("E0432", "unresolved import", 2, 4, 20));
        assert_eq!(
            (declarada.nome.as_str(), declarada.escopo),
            ("Serialize", Escopo::Externo),
            "serde IS a declared dependency, so a missing item is a fact"
        );
    }

    #[test]
    fn an_import_from_an_undeclared_crate_only_warns_because_the_dependency_may_come_next() {
        let t = Arvore::nova("ra_undeclared");
        t.arquivo("Cargo.toml", "[package]\nname = \"x\"\n[dependencies]\n");
        let a = achar(&t, &d("E0432", "unresolved import", 2, 4, 20));
        assert_eq!(a.escopo, Escopo::Projeto);
        assert_eq!(a.severidade, Severidade::Warn);
    }

    #[test]
    fn e0583_unresolved_module_is_an_import_that_warns() {
        let t = Arvore::nova("ra_e0583");
        let a = achar(
            &t,
            &d(
                "E0583",
                "unresolved module, can't find module file: fantasma.rs, or fantasma/mod.rs",
                3,
                0,
                13,
            ),
        );
        assert_eq!(
            (a.classe.as_str(), a.nome.as_str(), a.escopo),
            ("import", "fantasma", Escopo::Projeto)
        );
    }

    #[test]
    fn diagnostics_that_are_not_about_invented_names_are_not_findings() {
        let t = Arvore::nova("ra_outros");
        let fonte = fonte();
        let p = pedido(&t, Balde::Arquivo, &fonte);
        for (code, msg) in [
            ("E0308", "expected i32, found &'static str"),
            ("E0107", "expected 0 arguments, found 2"),
            ("E0282", "type annotations needed"),
            ("E0063", "missing structure fields:\n- f\n"),
            ("unused-variables", "unused variable"),
        ] {
            assert!(
                achado_de(&d(code, msg, 0, 0, 3), &p, &t.raiz).is_none(),
                "{code}"
            );
        }
    }

    #[test]
    fn only_rust_analyzers_own_diagnostics_count() {
        let t = Arvore::nova("ra_fonte");
        let fonte = fonte();
        let p = pedido(&t, Balde::Arquivo, &fonte);
        let mut de_outro = d("E0425", "x", 11, 13, 23);
        de_outro["source"] = json!("rustc");
        let norm = NormRa {
            cargo: t.raiz.clone(),
        };
        assert!(norm.achados(&[de_outro], &p, &t.raiz).unwrap().is_empty());
    }

    #[test]
    fn an_unlinked_file_is_unknown_never_an_allow() {
        let t = Arvore::nova("ra_unlinked");
        let p = pedido(&t, Balde::Arquivo, "pub fn f() { nome_solto }\n");
        let unlinked = json!({"code": "unlinked-file", "source": "rust-analyzer", "severity": 4,
            "message": "This file is not included anywhere in the module tree",
            "range": {"start": {"line": 0, "character": 0}, "end": {"line": 1, "character": 0}}});
        let norm = NormRa {
            cargo: t.raiz.clone(),
        };
        let motivo = norm.achados(&[unlinked], &p, &t.raiz).unwrap_err();
        assert!(
            motivo.contains("unlinked-file") && motivo.contains("rust-analyzer"),
            "{motivo}"
        );
    }

    // ── range slicing (UTF-16) and the small parsers ───────────────────────

    #[test]
    fn range_columns_are_utf16_units_not_bytes_or_chars() {
        // `é` is 1 UTF-16 unit and 2 bytes; `𝄞` is 2 UTF-16 units and 4 bytes
        let texto = "let é𝄞 = nome_solto;\n";
        let r = |c0: u64, c1: u64| json!({"range": {"start": {"line": 0, "character": c0}, "end": {"line": 0, "character": c1}}});
        assert_eq!(trecho(texto, &r(10, 20)).as_deref(), Some("nome_solto"));
        assert_eq!(trecho(texto, &r(4, 5)).as_deref(), Some("é"));
        assert_eq!(trecho(texto, &r(5, 7)).as_deref(), Some("𝄞"));
        assert_eq!(
            trecho(texto, &r(5, 6)),
            None,
            "a range inside a surrogate pair is not a slice"
        );
        let multi = json!({"range": {"start": {"line": 0, "character": 0}, "end": {"line": 1, "character": 0}}});
        assert_eq!(
            trecho(texto, &multi),
            None,
            "multi-line ranges are not names"
        );
    }

    #[test]
    fn identifier_helpers_take_the_right_end() {
        assert_eq!(ident_final("std::collections::HashMapz"), Some("HashMapz"));
        assert_eq!(ident_final("Ponto::novo;"), Some("novo"));
        assert_eq!(ident_final("  x  "), Some("x"));
        assert_eq!(ident_final("a::"), None);
        assert_eq!(ident_inicial("y: 2"), Some("y"));
        assert_eq!(ident_inicial(": 2"), None);
        assert_eq!(
            entre_crases("no method `pusha` on type `Vec`"),
            Some("pusha")
        );
        assert_eq!(
            tipo_da_mensagem("no method `pusha` on type `Vec<i32, Global>`"),
            Some("Vec<i32, Global>")
        );
        assert_eq!(tipo_da_mensagem("no such associated item"), None);
    }

    #[test]
    fn std_types_are_recognised_through_references_generics_and_paths() {
        for tipo in [
            "Vec<i32, Global>",
            "&String",
            "&mut Vec<u8>",
            "HashMap<i32, i32, RandomState, Global>",
            "std::string::String",
            "Option<Box<u8>>",
            "i32",
            "&str",
        ] {
            assert!(tipo_std(tipo), "{tipo}");
        }
        for nao in ["Ponto", "&Ponto", "crate::util::Ponto", "MeuVec<i32>", ""] {
            assert!(!tipo_std(nao), "{nao}");
        }
        let mut ordenado = TIPOS_STD.to_vec();
        ordenado.sort_unstable();
        ordenado.dedup();
        assert_eq!(
            ordenado.len(),
            TIPOS_STD.len(),
            "no duplicate in the std type table"
        );
    }

    #[test]
    fn dependencies_are_read_from_every_dependency_table_and_normalised() {
        let t = Arvore::nova("ra_deps");
        let m = t.arquivo(
            "Cargo.toml",
            "[package]\nname = \"x\"\n\n[dependencies]\nserde = { version = \"1\" }\nserde_json.workspace = true\nrand-core = \"0.6\"\n\n[dev-dependencies]\ncriterion = \"0.5\"\n\n[build-dependencies.cc]\nversion = \"1\"\n\n[target.'cfg(unix)'.dependencies]\nlibc = \"0.2\"\n\n[features]\nfast = []\n",
        );
        let deps = dependencias(&m);
        for esperado in [
            "serde",
            "serde_json",
            "rand_core",
            "criterion",
            "cc",
            "libc",
        ] {
            assert!(deps.contains(esperado), "{esperado} in {deps:?}");
        }
        assert!(!deps.contains("fast"), "a feature is not a dependency");
        assert!(!deps.contains("x"), "the package name is not a dependency");
        assert!(dependencias(&t.raiz.join("nao-existe.toml")).is_empty());
    }

    // ── the Cargo root ─────────────────────────────────────────────────────

    #[test]
    fn the_cargo_root_is_the_highest_workspace_manifest_else_the_nearest_crate() {
        let t = Arvore::nova("ra_root");
        t.arquivo("Cargo.toml", "[workspace]\nmembers = [\"crates/a\"]\n");
        t.arquivo("crates/a/Cargo.toml", "[package]\nname = \"a\"\n");
        t.arquivo("solto/Cargo.toml", "[package]\nname = \"solto\"\n");
        assert_eq!(
            raiz_cargo(&t.raiz, "crates/a/src/lib.rs"),
            Some(t.raiz.clone()),
            "member → its workspace"
        );
        assert_eq!(
            raiz_cargo(&t.raiz, "solto/src/lib.rs"),
            Some(t.raiz.clone()),
            "the workspace root is the highest"
        );
        let t2 = Arvore::nova("ra_root2");
        t2.arquivo("a/Cargo.toml", "[package]\nname = \"a\"\n");
        assert_eq!(
            raiz_cargo(&t2.raiz, "a/src/x.rs"),
            Some(t2.raiz.join("a")),
            "no workspace: the nearest crate"
        );
        assert_eq!(
            raiz_cargo(&t2.raiz, "b/y.rs"),
            None,
            "no Cargo.toml between the file and the root"
        );
        t2.arquivo("rust-project.json", "{}");
        assert_eq!(
            raiz_cargo(&t2.raiz, "b/y.rs"),
            Some(t2.raiz.clone()),
            "rust-project.json at the root"
        );
    }

    // ── discovery: the toolchain's binary, never a global one, never an install ──

    #[cfg(unix)]
    mod descoberta {
        use super::*;

        /// A fake `rustup` that records its argv and the auto-install variable, then prints `binario`.
        fn rustup_falso(t: &Arvore, binario: &Path, sai: i32) -> PathBuf {
            t.executavel(
                "fakebin/rustup",
                &format!(
                    "#!/bin/sh\necho \"$@ AUTO=$RUSTUP_AUTO_INSTALL\" >> '{}'\nif [ {sai} -ne 0 ]; then echo \"error: toolchain 'x' does not have the binary rust-analyzer\" >&2; exit {sai}; fi\necho '{}'\n",
                    t.raiz.join("WITNESS").display(),
                    binario.display()
                ),
            )
        }

        fn projeto(t: &Arvore) {
            t.arquivo("Cargo.toml", "[package]\nname = \"x\"\n");
            t.arquivo("src/lib.rs", "");
        }

        fn amb(t: &Arvore) -> Ambiente {
            Ambiente::com_path(vec![t.raiz.join("fakebin")])
        }

        #[test]
        fn the_binary_is_whatever_rustup_resolves_for_the_project() {
            let t = Arvore::nova("ra_disc_ok");
            projeto(&t);
            let ra = t.executavel("toolchain/bin/rust-analyzer", "#!/bin/sh\n");
            rustup_falso(&t, &ra, 0);
            let achado = localizar(&t.raiz, "src/lib.rs", &amb(&t)).unwrap();
            assert_eq!(
                achado,
                Localizado {
                    binario: ra,
                    cargo: t.raiz.clone()
                }
            );
            let testemunha = std::fs::read_to_string(t.raiz.join("WITNESS")).unwrap();
            assert_eq!(
                testemunha.trim(),
                "which rust-analyzer AUTO=0",
                "only `which`, never an install, auto-install off"
            );
        }

        #[test]
        fn abrir_starts_the_binary_rustup_resolves_and_checks_end_to_end() {
            use crate::testutil::FAKE_PULL;
            let t = Arvore::nova("ra_e2e");
            projeto(&t);
            let script = t.arquivo("fake_ra.py", FAKE_PULL);
            let ra = t.executavel(
                "toolchain/bin/rust-analyzer",
                &format!("#!/bin/sh\nexec python3 '{}' ok\n", script.display()),
            );
            rustup_falso(&t, &ra, 0);
            let alvo = Alvo {
                servidor: crate::servidor::Servidor::RustAnalyzer,
                raiz: &t.raiz,
                arquivo: "src/lib.rs",
            };
            let mut sessao = abrir(&alvo, &amb(&t)).expect("the fake server starts");
            let edicao = Edicao {
                path: "src/lib.rs",
                content: "# undef fantasma\n",
                disk: None,
                overlays: &[],
            };
            sessao
                .aquecer(&edicao, Duration::from_secs(20))
                .expect("warm-up waits for quiescence");
            let v = sessao.verificar(&edicao, None);
            assert!(v.saudavel, "{:?}", v.resultado);
            assert_eq!(v.resultado.achados.len(), 1, "{:?}", v.resultado);
            let a = &v.resultado.achados[0];
            assert_eq!(
                (
                    a.classe.as_str(),
                    a.nome.as_str(),
                    a.linha,
                    a.regra.as_str()
                ),
                ("nome", "fantasma", 1, "E0425")
            );
        }

        #[test]
        fn a_global_rust_analyzer_on_path_is_never_a_substitute() {
            let t = Arvore::nova("ra_disc_global");
            projeto(&t);
            // a rust-analyzer sits on PATH, but there is no rustup to resolve the project's toolchain
            t.executavel("fakebin/rust-analyzer", "#!/bin/sh\n");
            let motivo = localizar(&t.raiz, "src/lib.rs", &amb(&t)).unwrap_err();
            assert!(
                motivo.contains("rustup") && motivo.contains("nunca instala"),
                "{motivo}"
            );
        }

        #[test]
        fn a_toolchain_without_the_component_is_an_absent_verifier_with_the_rustup_reason() {
            let t = Arvore::nova("ra_disc_nocomp");
            projeto(&t);
            rustup_falso(&t, Path::new("/x"), 1);
            let motivo = localizar(&t.raiz, "src/lib.rs", &amb(&t)).unwrap_err();
            assert!(motivo.contains("does not have the binary"), "{motivo}");
            let testemunha = std::fs::read_to_string(t.raiz.join("WITNESS")).unwrap();
            assert!(
                !testemunha.contains("install") && !testemunha.contains("component add"),
                "{testemunha}"
            );
        }

        #[test]
        fn a_path_that_does_not_exist_is_refused() {
            let t = Arvore::nova("ra_disc_ghost");
            projeto(&t);
            rustup_falso(&t, Path::new("/nao/existe/rust-analyzer"), 0);
            assert!(
                localizar(&t.raiz, "src/lib.rs", &amb(&t))
                    .unwrap_err()
                    .contains("não existe")
            );
        }

        #[test]
        fn a_project_without_cargo_is_unknown_before_any_process_runs() {
            let t = Arvore::nova("ra_disc_nocargo");
            rustup_falso(&t, Path::new("/x"), 0);
            let motivo = localizar(&t.raiz, "src/lib.rs", &amb(&t)).unwrap_err();
            assert!(motivo.contains("Cargo.toml"), "{motivo}");
            assert!(
                !t.raiz.join("WITNESS").exists(),
                "rustup must not even be asked"
            );
        }
    }
}
