//! Severity policy and delta of the VGP motor (VL0 §3 steps 4-5, §5) — language-agnostic.
//!
//! The adapters MEASURE (they report findings with the [`Alvo`] they are about, or why they
//! could not measure); [`super::check`] DECIDES, here, once, for every language. The severity an
//! adapter writes into a [`Finding`] is overwritten by [`aplicar`]: enforcement lives in the
//! executor, never in the adapter's good manners (D8) — a new adapter cannot invent a `deny`.
//!
//! Policy (§5), in order of precedence:
//!
//! | rule | finding | severity |
//! |---|---|---|
//! | P4 | class in [`CLASSES_CONSULTIVAS`] (`cdn_pin`, `classe_css`) | `warn`, whatever the target |
//! | P1 | target [`Alvo::Externo`] (stdlib, closed builtin, installed third party, a CDN 404) | `deny`, in every bucket |
//! | P2 | target [`Alvo::Projeto`] where the code runs now ([`roda_agora`]) | `deny` |
//! | P3 | target [`Alvo::Projeto`] elsewhere, or [`Alvo::Incerto`] / [`Alvo::Fechado`] anywhere | `warn` |
//!
//! [`Alvo::Fechado`] never denies ALONE: layer 1 proves the receiver's type, not that the value
//! reaching the attribute has that type (it follows no flow). The deny is the merge's, when layer 2
//! independently names the SAME receiver type ([`super::sombra::fundir_findings`]).
//!
//! Delta (§3 step 4): a `deny` already present in the disk content is downgraded to `warn` —
//! the hook denies what the edit INTRODUCES, never what pre-existed ([`aplicar_delta`]).

use std::collections::HashSet;
use std::path::Path;

use super::{Finding, is_inline};

/// Whom a finding is about — what decides how loud the motor may be (§5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Alvo {
    /// A target outside the project that the motor proved absent: stdlib / closed builtin table,
    /// an installed third-party package, a CDN version answered 404. No later edit of the
    /// project can make the code right → `deny` anywhere.
    Externo,
    /// A symbol of the PROJECT. The edit that follows may still create it (consumer before
    /// provider, the RED of TDD) → `deny` only where the code runs now.
    Projeto,
    /// The receiver or the binding is dynamic or of unknown type (`import *`, `__getattr__`,
    /// a builtin "form", a class hook): the motor knows it may be wrong → never `deny`.
    ///
    /// Layer 2 never escalates it either: the doubt is a name that `exec`/`eval`/`globals()` may have
    /// bound, which the checker cannot see.
    Incerto,
    /// A member missing from a type whose attribute set is CLOSED at run time (`os.stat_result`:
    /// not subclassable, no `__dict__`, no `__getattr__` — `stdlib_model::TIPOS_FECHADOS`). The
    /// ABSENCE is certain for an instance of that type; what layer 1 cannot prove (it follows no flow)
    /// is that the value reaching the attribute IS one → `warn` alone, `deny` only when layer 2
    /// names the same receiver type. A different doubt from [`Alvo::Incerto`]'s, hence a different
    /// target.
    Fechado,
}

/// Classes that only ever advise (§5.2): a class used by JS hooks is legitimate, a floating CDN
/// pin is a recommendation — neither is an error the motor may deny.
pub const CLASSES_CONSULTIVAS: &[&str] = &["cdn_pin", "classe_css"];

/// WHEN the verdict is asked — the moment decides whether a file "runs now" (FRAG, 10/10/2026).
///
/// A file being WRITTEN is not running: the edit that follows may still complete it (a fragment in
/// `/tmp` is a snippet to paste — case `9a8eed8a7502` of the P0M), and spec §5.1 only lets the
/// motor `deny` where "no later edit can make the code right". The same file being EXECUTED
/// (`python3 X.py`) is the moment that holds. The default is the write: every hook that judges a
/// proposed file (`Write`/`Edit`/`MultiEdit`) and `touring vgp check` answer in it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Contexto {
    /// A proposed file (`Write`/`Edit`/`MultiEdit`): only an inline program runs now.
    #[default]
    Escrita,
    /// A file about to be run by an interpreter (`python3 X.py`, `node X.js`): the scratch
    /// buckets (`/tmp/`, the project's `.cache/`) run now, as before.
    Execucao,
}

impl Contexto {
    /// The CLI's `--contexto` spelling → the context (`None`: not a context).
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "escrita" => Some(Self::Escrita),
            "execucao" | "execução" => Some(Self::Execucao),
            _ => None,
        }
    }
}

/// Does the code at `path` run right now, so that nothing can fix it before it breaks?
///
/// An inline program does, in every context. A scratch path — `/tmp/` or the project's `.cache/`
/// (agent scratch scripts, §5.1) — only does when the verdict is asked at EXECUTION: written
/// there, it is a fragment the next edit may still complete ([`Contexto`]).
pub fn roda_agora(path: &str, project_root: &Path, contexto: Contexto) -> bool {
    if is_inline(path) {
        return true;
    }
    if contexto == Contexto::Escrita {
        return false;
    }
    if path.starts_with("/tmp/") {
        return true;
    }
    let p = Path::new(path);
    let relativo = p.strip_prefix(project_root).unwrap_or(p);
    relativo
        .components()
        .next()
        .is_some_and(|c| c.as_os_str() == ".cache")
}

/// The severity of one finding — the whole §5 table as a pure function.
pub fn severidade(classe: &str, alvo: Alvo, roda_agora: bool) -> &'static str {
    if CLASSES_CONSULTIVAS.contains(&classe) {
        return "warn";
    }
    match alvo {
        Alvo::Externo => "deny",
        Alvo::Projeto if roda_agora => "deny",
        Alvo::Projeto | Alvo::Incerto | Alvo::Fechado => "warn",
    }
}

/// Overwrite every finding's severity with the policy's answer for (class, target, where, when).
pub fn aplicar(findings: &mut [Finding], path: &str, project_root: &Path, contexto: Contexto) {
    let agora = roda_agora(path, project_root, contexto);
    for f in findings.iter_mut() {
        f.severidade = severidade(&f.classe, f.alvo, agora).to_string();
    }
}

/// Downgrade to `warn` every `deny` whose (class, name) the disk content already had.
/// `no_disco` is the adapter's answer on the disk content; its severities are irrelevant.
pub fn aplicar_delta(findings: &mut [Finding], no_disco: &[Finding]) {
    let ja: HashSet<(&str, &str)> = no_disco
        .iter()
        .map(|f| (f.classe.as_str(), f.nome.as_str()))
        .collect();
    for f in findings.iter_mut() {
        if f.severidade == "deny" && ja.contains(&(f.classe.as_str(), f.nome.as_str())) {
            f.severidade = "warn".into();
            f.sugestao = format!("pré-existente (já estava no disco): {}", f.sugestao);
        }
    }
}

/// Aggregate a verdict from findings, falling back to `base` when there are none — the base
/// carries an earlier "unknown" (a merged layer must never turn a could-not-measure into allow).
pub fn verdict_dos(findings: &[Finding], base: &str) -> String {
    if findings.iter().any(|f| f.severidade == "deny") {
        "deny".to_string()
    } else if findings.is_empty() {
        base.to_string()
    } else {
        "warn".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn achado(classe: &str, nome: &str, alvo: Alvo) -> Finding {
        Finding::candidato(classe, nome, 1, alvo, "s".into(), Vec::new())
    }

    fn sev(classe: &str, alvo: Alvo, agora: bool) -> &'static str {
        severidade(classe, alvo, agora)
    }

    #[test]
    fn roda_agora_na_execucao_cobre_inline_tmp_e_cache_do_projeto() {
        let raiz = PathBuf::from("/home/u/proj");
        let roda = |p: &str| roda_agora(p, &raiz, Contexto::Execucao);
        assert!(roda("<string>"));
        assert!(roda("<stdin>"));
        assert!(roda("/tmp/x.py"));
        assert!(roda("/home/u/proj/.cache/scratch/a.py"));
        assert!(roda(".cache/a.py"), "caminho relativo ao projeto");
        assert!(!roda("/home/u/proj/src/a.py"));
        assert!(
            !roda("/home/u/outro/.cache/a.py"),
            "o .cache de OUTRO projeto não é scratch deste"
        );
        assert!(
            !roda("/home/u/proj/src/.cache/a.py"),
            "só o .cache da RAIZ do projeto"
        );
        assert!(!roda("/tmpx/a.py"), "prefixo /tmp/ exige a barra");
    }

    #[test]
    fn na_escrita_so_o_inline_roda_agora() {
        // FRAG: escrever em /tmp ou no .cache não executa nada — a edição seguinte ainda completa
        let raiz = PathBuf::from("/home/u/proj");
        let roda = |p: &str| roda_agora(p, &raiz, Contexto::Escrita);
        assert!(roda("<string>"));
        assert!(roda("<stdin>"));
        assert!(!roda("/tmp/x.py"), "fragmento em /tmp");
        assert!(
            !roda("/home/u/proj/.cache/scratch/a.py"),
            "scratch do projeto"
        );
        assert!(!roda(".cache/a.py"));
        assert!(!roda("/home/u/proj/src/a.py"));
    }

    #[test]
    fn o_contexto_padrao_e_a_escrita_e_o_cli_o_le_pelo_nome() {
        assert_eq!(Contexto::default(), Contexto::Escrita);
        assert_eq!(Contexto::parse("escrita"), Some(Contexto::Escrita));
        assert_eq!(Contexto::parse("execucao"), Some(Contexto::Execucao));
        assert_eq!(Contexto::parse("execução"), Some(Contexto::Execucao));
        for ruim in ["", "Escrita", "run", "exec"] {
            assert_eq!(Contexto::parse(ruim), None, "{ruim:?}");
        }
    }

    #[test]
    fn p1_externo_nega_em_qualquer_balde() {
        assert_eq!(sev("atributo", Alvo::Externo, false), "deny");
        assert_eq!(sev("atributo", Alvo::Externo, true), "deny");
        assert_eq!(sev("cdn", Alvo::Externo, false), "deny", "404 confirmado");
    }

    #[test]
    fn p2_projeto_nega_so_onde_roda_agora() {
        assert_eq!(sev("nome", Alvo::Projeto, true), "deny");
        assert_eq!(sev("nome", Alvo::Projeto, false), "warn");
    }

    #[test]
    fn p3_fechado_sozinho_so_avisa_em_todo_balde() {
        // P0B-d: layer 1 proves the TYPE closed, not that the value reaching the attribute has it —
        // alone it warns where the code runs now and where it does not; only the merge with layer 2
        // (same receiver type) denies it
        for agora in [true, false] {
            assert_eq!(
                sev("atributo", Alvo::Fechado, agora),
                "warn",
                "agora={agora}"
            );
            assert_eq!(sev("membro", Alvo::Fechado, agora), "warn", "agora={agora}");
        }
    }

    #[test]
    fn p3_incerto_nunca_nega() {
        assert_eq!(sev("atributo", Alvo::Incerto, false), "warn");
        assert_eq!(sev("atributo", Alvo::Incerto, true), "warn");
    }

    #[test]
    fn p4_classes_consultivas_nunca_negam_nem_com_alvo_externo() {
        for classe in CLASSES_CONSULTIVAS {
            assert_eq!(sev(classe, Alvo::Externo, true), "warn", "{classe}");
            assert_eq!(sev(classe, Alvo::Projeto, true), "warn", "{classe}");
        }
    }

    #[test]
    fn aplicar_sobrescreve_a_severidade_que_o_adaptador_escreveu() {
        let mut fs = vec![
            achado("atributo", "a", Alvo::Incerto),
            achado("nome", "b", Alvo::Projeto),
        ];
        fs[0].severidade = "deny".into(); // o adaptador tentou negar um alvo incerto
        fs[1].severidade = "deny".into(); // ... e um símbolo do projeto num arquivo comum
        let raiz = Path::new("/home/u/proj");
        aplicar(&mut fs, "/home/u/proj/a.py", raiz, Contexto::Escrita);
        assert_eq!(fs[0].severidade, "warn");
        assert_eq!(fs[1].severidade, "warn");
        aplicar(&mut fs, "/tmp/a.py", raiz, Contexto::Escrita);
        assert_eq!(
            fs[1].severidade, "warn",
            "escrita em /tmp: ainda pode nascer"
        );
        aplicar(&mut fs, "/tmp/a.py", raiz, Contexto::Execucao);
        assert_eq!(fs[0].severidade, "warn", "incerto não nega nem em /tmp");
        assert_eq!(fs[1].severidade, "deny", "projeto nega onde roda agora");
        aplicar(&mut fs, "<string>", raiz, Contexto::Escrita);
        assert_eq!(
            fs[1].severidade, "deny",
            "o inline roda agora em qualquer contexto"
        );
    }

    #[test]
    fn delta_rebaixa_so_o_deny_que_o_disco_ja_tinha() {
        let mut fs = vec![
            achado("atributo", "velho", Alvo::Externo),
            achado("atributo", "novo", Alvo::Externo),
            achado("nome", "velho", Alvo::Externo),
            achado("membro", "aviso", Alvo::Incerto),
        ];
        aplicar(
            &mut fs,
            "/home/u/proj/a.py",
            Path::new("/home/u/proj"),
            Contexto::Escrita,
        );
        let disco = vec![
            achado("atributo", "velho", Alvo::Externo),
            achado("membro", "aviso", Alvo::Incerto),
        ];
        aplicar_delta(&mut fs, &disco);
        assert_eq!(fs[3].severidade, "warn");
        assert_eq!(
            fs[3].sugestao, "s",
            "o delta só reescreve o que rebaixa (deny)"
        );
        assert_eq!(fs[0].severidade, "warn");
        assert!(fs[0].sugestao.starts_with("pré-existente"));
        assert_eq!(fs[1].severidade, "deny", "o introduzido segue negado");
        assert_eq!(
            fs[2].severidade, "deny",
            "a chave é (classe, nome): o mesmo nome em outra classe não é pré-existente"
        );
    }

    #[test]
    fn delta_ignora_a_severidade_do_achado_no_disco() {
        let mut fs = vec![achado("atributo", "x", Alvo::Externo)];
        aplicar(
            &mut fs,
            "/home/u/proj/a.py",
            Path::new("/home/u/proj"),
            Contexto::Escrita,
        );
        let mut disco = achado("atributo", "x", Alvo::Externo);
        disco.severidade = "warn".into();
        aplicar_delta(&mut fs, &[disco]);
        assert_eq!(fs[0].severidade, "warn");
    }

    #[test]
    fn verdict_dos_deny_domina_e_vazio_devolve_a_base() {
        let mut deny = achado("a", "x", Alvo::Externo);
        deny.severidade = "deny".into();
        let mut warn = achado("a", "y", Alvo::Incerto);
        warn.severidade = "warn".into();
        assert_eq!(verdict_dos(&[warn.clone(), deny], "allow"), "deny");
        assert_eq!(verdict_dos(&[warn], "allow"), "warn");
        assert_eq!(verdict_dos(&[], "unknown"), "unknown");
        assert_eq!(verdict_dos(&[], "allow"), "allow");
    }
}
