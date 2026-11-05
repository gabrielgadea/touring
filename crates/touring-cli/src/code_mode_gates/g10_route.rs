//! G10ROTA (30.4.97) — qual rota do `touring run` EXECUTA o programa que um gate está para nomear.
//!
//! Caso real (analise-78, 09/10/2026 22:39 BRT, transcrição `05166acc-…`, linhas 5786-5798): a 5ª
//! `python-inline` seriada ESCREVIA `RETOMADA.md` (`Path.write_text`). O G10 nomeou
//! `touring run --lang python --code …`; o executor nega a escrita no perfil Sandboxed (X6
//! `file-write`) e a rota nomeada falhava. A que funcionou foi
//! `TOURING_TRUSTED_OK=1 touring run --allow-forbidden …`, válida só dentro do workspace porque o
//! Landlock concede escrita apenas a ele.
//!
//! D8 — o deny não tem predicado próprio. Quem decide se uma rota executa é o EXECUTOR:
//! [`touring_ceg::gateway::run_admission`] (a mesma composição que o `touring run` aplica) e
//! [`touring_ceg::gateway::sandbox_executor::sandbox_can_write`] (as raízes de escrita que o
//! Landlock concede). Este módulo só traduz o veredito deles em texto.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use regex::Regex;
use touring_ceg::gateway::run_admission::{RunRoute, run_route};
use touring_ceg::gateway::sandbox_executor::sandbox_can_write;

/// A rota que executa um programa, com o destino da escrita já confrontado com o Landlock.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Route {
    /// `touring run` como está: o perfil Sandboxed admite o programa.
    Sandboxed,
    /// O Sandboxed recusa, o Trusted admite: `TOURING_TRUSTED_OK=1 touring run --allow-forbidden`.
    Trusted {
        /// A frase do executor que recusou no Sandboxed.
        sandbox_denial: String,
    },
    /// O programa escreve em `dest`, fora do que o Landlock concede: nenhuma rota escreve ali.
    NoWritableDest {
        /// O destino resolvido (absoluto, sem `..`).
        dest: PathBuf,
    },
    /// O executor recusa o programa nos DOIS perfis (rede, padrão destrutivo…).
    Unrunnable {
        /// A recusa do perfil Trusted.
        reason: String,
    },
}

/// A rota do programa `code` (`lang`: `python`, `bash`…) a partir do projeto `project_root`.
pub(super) fn route_for(project_root: &Path, lang: &str, code: &str) -> Route {
    match run_route(lang, code, project_root) {
        RunRoute::Sandboxed => Route::Sandboxed,
        RunRoute::Trusted { sandbox_denial } => {
            match unwritable_destination(project_root, lang, code) {
                Some(dest) => Route::NoWritableDest { dest },
                None => Route::Trusted { sandbox_denial },
            }
        }
        RunRoute::Unrunnable { reason } => Route::Unrunnable { reason },
    }
}

impl Route {
    /// Algum `touring run` executa este programa?
    pub(super) fn runs(&self) -> bool {
        matches!(self, Self::Sandboxed | Self::Trusted { .. })
    }

    /// O prefixo de ambiente do comando: a elevação é POR COMANDO, nunca um estado exportado.
    pub(super) fn env_prefix(&self) -> &'static str {
        match self {
            Self::Trusted { .. } => "TOURING_TRUSTED_OK=1 ",
            _ => "",
        }
    }

    /// A flag de `touring run` que seleciona o perfil.
    pub(super) fn run_flags(&self) -> &'static str {
        match self {
            Self::Trusted { .. } => "--allow-forbidden ",
            _ => "",
        }
    }

    /// A frase que explica por que a rota difere do `touring run` simples — vazia no Sandboxed.
    pub(super) fn explanation(&self, project_root: &Path) -> String {
        let root = project_root.display();
        match self {
            Self::Sandboxed => String::new(),
            Self::Trusted { sandbox_denial } => format!(
                " Este corpo ESCREVE: o executor o nega no perfil Sandboxed ({sandbox_denial}), \
                 então `touring run` sem `--allow-forbidden` falharia. A rota que executa é a \
                 Trusted, decidida POR COMANDO (TOURING_TRUSTED_OK=1 no próprio comando) e válida \
                 só dentro do workspace {root}: o Landlock concede escrita apenas a ele e aos \
                 temporários do run."
            ),
            Self::NoWritableDest { dest } => format!(
                " Este corpo escreve em `{}`, fora do que o sandbox pode escrever (workspace \
                 {root}, ~/.cache, ~/.cargo e o tmp privado do run): nenhuma rota de `touring run` \
                 escreve ali — a Trusted também passa pelo Landlock. Rode nativo (o MESMO \
                 comando com o prefixo TOURING_GATE_OK=1: bypass por-comando, contado como \
                 bypassed) ou aponte o destino para dentro do workspace.",
                dest.display()
            ),
            Self::Unrunnable { reason } => format!(
                " O executor nega este corpo nos DOIS perfis (Sandboxed e Trusted): {reason}. \
                 Nenhuma rota de `touring run` o executa — rode nativo (o MESMO comando com o \
                 prefixo TOURING_GATE_OK=1: bypass por-comando, contado como bypassed)."
            ),
        }
    }
}

// ── o destino da escrita ────────────────────────────────────────────────────

/// As marcas python que escrevem em UM caminho. Subconjunto de `PY_INLINE_WRITER_MARKS` (o teste
/// `as_marcas_de_destino_sao_subconjunto_das_de_escritor` impede a deriva): a lista larga inclui
/// rede/subprocesso/DB, cujos literais NÃO são destinos de escrita; e `shutil.`/`os.rename`
/// carregam dois caminhos (origem lida, destino escrito), que um léxico não distingue.
pub(super) const PY_PATH_WRITE_MARKS: &[&str] = &[
    ".write(",
    ".writelines(",
    "write_text(",
    "write_bytes(",
    ".unlink(",
    ".touch(",
    ".chmod(",
    ".rmdir(",
    "os.remove",
    "os.rmdir",
    "os.removedirs",
    "os.mkdir",
    "os.makedirs",
    "os.truncate",
];

fn literal_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"["']([^"'\n]+)["']"#).expect("literal regex compila"))
}

fn binding_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        // A atribuição TERMINA no literal (ou no `)` do `Path(...)`): `t = Path("x").read_text()`
        // vincula o CONTEÚDO lido, não o caminho — e `write_text(t)` não escreve em "x".
        Regex::new(
            r#"^\s*([A-Za-z_]\w*)\s*=\s*(?:(?:pathlib\.)?Path\(\s*)?["']([^"'\n]+)["']\s*\)?\s*(?:#.*)?$"#,
        )
        .expect("binding regex compila")
    })
}

fn ident_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"[A-Za-z_]\w*").expect("ident regex compila"))
}

/// Um literal que PODE sair do workspace: absoluto, `~/…` ou com `..`. Um nome relativo simples
/// (`saida.md`) escreve no cwd — dentro do workspace por definição.
fn may_leave_workspace(lit: &str) -> bool {
    lit.starts_with('/') || lit.starts_with("~/") || lit.contains("../") || lit.ends_with("/..")
}

/// Os literais de caminho que o corpo python ESCREVE: os literais da linha que traz uma marca de
/// escrita, mais os literais vinculados (`p = Path("…")`, `p = "…"`) aos nomes que aparecem nela.
/// Heurística léxica — não resolve `Path.home() / …`, f-strings nem `os.path.join`; um destino que
/// ela não vê é um destino que o executor ainda recusa com uma mensagem que ensina.
pub(super) fn python_write_destinations(body: &str) -> Vec<String> {
    let mut bound: HashMap<String, String> = HashMap::new();
    let mut out: Vec<String> = Vec::new();
    for line in body.lines() {
        if let Some(c) = binding_re().captures(line) {
            bound.insert(c[1].to_string(), c[2].to_string());
        }
        let writes = PY_PATH_WRITE_MARKS.iter().any(|m| line.contains(m))
            || super::py_open_mutating_mode(line);
        if !writes {
            continue;
        }
        let from_literals = literal_re().captures_iter(line).map(|c| c[1].to_string());
        let from_bindings = ident_re()
            .find_iter(line)
            .filter_map(|m| bound.get(m.as_str()).cloned());
        for lit in from_literals.chain(from_bindings) {
            if may_leave_workspace(&lit) && !out.contains(&lit) {
                out.push(lit);
            }
        }
    }
    out
}

fn resolve_literal(project_root: &Path, lit: &str) -> PathBuf {
    let raw = match (lit.strip_prefix("~/"), std::env::var_os("HOME")) {
        (Some(rest), Some(home)) => PathBuf::from(home).join(rest),
        _ => PathBuf::from(lit),
    };
    // Sem normalizar `..`: `sandbox_can_write` resolve o caminho (lexical + symlinks) antes de
    // julgar, e o destino citado no deny sai como o corpo o escreveu.
    if raw.is_absolute() {
        raw
    } else {
        project_root.join(raw)
    }
}

/// O primeiro destino de escrita que o Landlock NÃO concede (só python: é onde o corpo é inspecionável).
fn unwritable_destination(project_root: &Path, lang: &str, code: &str) -> Option<PathBuf> {
    if !matches!(lang.to_ascii_lowercase().as_str(), "python" | "py") {
        return None;
    }
    python_write_destinations(code)
        .iter()
        .map(|lit| resolve_literal(project_root, lit))
        .find(|dest| !sandbox_can_write(project_root, dest))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn as_marcas_de_destino_sao_subconjunto_das_de_escritor() {
        for m in PY_PATH_WRITE_MARKS {
            assert!(
                super::super::PY_INLINE_WRITER_MARKS.contains(m),
                "`{m}` está na lista de destinos mas não na de escritores — a deriva que a regra D8 proíbe"
            );
        }
    }

    /// O caso real: o caminho vem de um vínculo (`p = pathlib.Path("…")`) e o `write_text` está
    /// em outra linha — e ele é RELATIVO, logo dentro do workspace: nenhum destino a vigiar.
    #[test]
    fn o_caso_real_nao_tem_destino_fora_do_workspace() {
        let body = "import pathlib\np = pathlib.Path(\"docs/plans/2026-10-06-geopolitica-logistica-eua/RETOMADA.md\")\nt = p.read_text(encoding=\"utf-8\")\nvelho = \"> Estado 10/10 ~00:30:\"\np.write_text(t.replace(velho, \"> Estado 09/10 22:38\"), encoding=\"utf-8\")";
        assert!(python_write_destinations(body).is_empty());
    }

    #[test]
    fn destino_absoluto_vinculado_e_visto_na_linha_do_write() {
        let body = "import pathlib\nalvo = pathlib.Path(\"/etc/touring-g97/saida.md\")\nalvo.write_text(\"x\")";
        assert_eq!(
            python_write_destinations(body),
            vec!["/etc/touring-g97/saida.md".to_string()]
        );
    }

    #[test]
    fn destino_literal_direto_no_open_de_escrita() {
        let body = "with open(\"/var/tmp/g97/x.txt\", \"w\") as f:\n    f.write(\"x\")";
        assert_eq!(
            python_write_destinations(body),
            vec!["/var/tmp/g97/x.txt".to_string()]
        );
    }

    /// Controle: um caminho absoluto só LIDO não é destino — a rota Trusted segue prometida.
    #[test]
    fn caminho_absoluto_apenas_lido_nao_e_destino() {
        let body = "import pathlib\nt = pathlib.Path(\"/etc/hostname\").read_text()\npathlib.Path(\"saida.md\").write_text(t)";
        assert!(python_write_destinations(body).is_empty());
    }

    #[test]
    fn home_e_pai_contam_como_saida_possivel() {
        let body = "p = \"~/relatorio.md\"\nopen(p, \"w\").write(\"x\")\nq = \"../fora/y.md\"\nopen(q, \"w\").write(\"y\")";
        assert_eq!(
            python_write_destinations(body),
            vec!["~/relatorio.md".to_string(), "../fora/y.md".to_string()]
        );
    }
}
