//! A taxonomia canônica de famílias de injeção de contexto — a REGUA.
//!
//! Migrada de `crates/touring-cli/src/cli/context_budget.rs` na S-4
//! (26/10/2026), pelo mesmo movimento do `cila.rs`: a régua (que publica
//! `context_budget.by_family` no KPI) e a política nativa
//! ([`crate::injection_policy`]) respondem a MESMA pergunta, e quando o
//! predicado é escrito em dois lugares os dois divergem — já custou nove
//! famílias de divergência entre a régua e o espelho Python
//! (`client/hooks/injection_policy.py`). Os dois crates dependem deste, então
//! a resposta é dada uma vez; `context_budget.rs` re-exporta
//! [`injection_family`] e [`attributed_family`], o que mantém os call sites
//! intactos.
//!
//! # Duas camadas, e por que duas
//!
//! 1. **Marcadores** ([`FAMILY_MARKERS`]) — um literal que o payload CARREGA
//!    (em qualquer posição). É o que a régua já usava; continua valendo, e
//!    agora também nomeia as famílias que só a marca do emissor alcançava.
//! 2. **Padrões** ([`FAMILY_PATTERNS`]) — o CONTEÚDO das formas que não têm
//!    marcador próprio: o payload que o orçamento do turno stripped (3.048
//!    blocos no corpus, §2.7 do `scout-corpus-qualitativo`), a razão de deny do
//!    gate (`[G2 … → REESCRITO]`) e o programa fundido.
//!
//! Medido sobre os 40 transcripts mais recentes (43.852 blocos, 6,39 MB): a
//! camada de texto sozinha deixava **77,2 %** dos bytes no catch-all `other`;
//! com os padrões, **11,5 %** — e o que era catch-all ganhou nome
//! (`code-mode-deny` +160 kB, que é a razão de deny do gate,
//! `compaction-gaps` +84 kB, `code-mode-contract` +69 kB,
//! `pipefail-evidence` +44 kB).

use regex::Regex;
use std::sync::OnceLock;

/// A família PRIMÁRIA de um texto injetado: o primeiro marcador que casa,
/// depois o primeiro padrão, senão `other`.
///
/// A política (`crate::injection_policy::allows_payload`) precisa do CONJUNTO
/// de famílias — para essa pergunta use [`families`]; julgar o payload inteiro
/// pela família primária descartaria um fato que veio de carona num aviso.
#[must_use]
pub fn injection_family(text: &str) -> &'static str {
    for (family, needles) in FAMILY_MARKERS {
        if needles.iter().any(|n| text.contains(n)) {
            return family;
        }
    }
    for (family, pattern) in family_patterns() {
        if pattern.is_match(text) {
            return family;
        }
    }
    "other"
}

/// TODAS as famílias presentes num texto, na ordem canônica (marcadores na
/// ordem da tabela, depois padrões), sem repetição.
///
/// Um payload pode misturar advisory e fato: `verdict.rs` (`context_with`)
/// compõe os avisos primeiro e o contexto do veredito depois, num texto só. A
/// política decide pelo CONJUNTO — e `no-advisory` só suprime o que é
/// **exclusivamente** sermão.
#[must_use]
pub fn families(text: &str) -> Vec<&'static str> {
    let mut found: Vec<&'static str> = Vec::new();
    for (family, needles) in FAMILY_MARKERS {
        if needles.iter().any(|n| text.contains(n)) && !found.contains(family) {
            found.push(family);
        }
    }
    for (family, pattern) in family_patterns() {
        if pattern.is_match(text) && !found.contains(family) {
            found.push(family);
        }
    }
    found
}

/// Marcadores literais: o payload que CARREGA um destes NOMEIA a sua família.
///
/// A ordem é de especificidade: o contrato do code mode vem antes do deny
/// porque o cabeçalho dele começa com as mesmas duas palavras do marcador do
/// deny (`CODE MODE — contrato` ⊃ `CODE MODE`).
///
/// O espelho Python (`client/hooks/injection_policy.py`) carrega as MESMAS
/// linhas, na MESMA ordem, com os MESMOS nomes — o teste
/// `the_python_policy_mirrors_this_taxonomy` (na régua) reprova quando os dois
/// divergem.
pub const FAMILY_MARKERS: &[(&str, &[&str])] = &[
    ("code-mode-contract", &["CODE MODE — contrato"]),
    (
        "code-mode-deny",
        &[
            "CODE MODE",
            "G7 re-inspe",
            "G10 write",
            "G1 rajada",
            "G11 ",
            "G3 edit",
        ],
    ),
    ("touring-suggest", &["TOURING SUGGEST"]),
    (
        "loop-outer",
        &["LOOP RESUME", "work-outer", "explore-ledger", "OUTER ·"],
    ),
    (
        "session-start",
        &[
            "Touring Knowledge",
            "ATZILUTH",
            "SCOUT PERP",
            "TOURING (slim)",
            "Painel",
            "Session intelligence",
            "Touring: collecting session intelligence",
            "Touring: initializing cognitive engine",
        ],
    ),
    ("prompt-enhance", &["PROMPT ENHANCEMENT"]),
    (
        "past-lessons",
        &["lições de erros passados", "Similar command failed"],
    ),
    (
        "post-tool-echo",
        &["temporal: reliability", "last fail:", "post-tool-batch"],
    ),
    ("evidence-guard", &["EVIDENCE GUARD"]),
    (
        "debug-stdout",
        &[
            "[EVENT_READER]",
            "[POST_QUALITY_GATE",
            "Hook execution started",
        ],
    ),
    ("plugin-hook", &["[SUGESTÃO DE SKILL"]),
    ("pre-write", &["code-workflow", "rust-workflow"]),
    (
        "pre-edit",
        &["HOTSPOT:", "wiring_predict:", "callers:", "related docs:"],
    ),
    ("pre-read", &["defs[", "📌 defs", "🪝 defs"]),
    ("post-edit", &["post-edit verification:", "post-read:"]),
    ("post-write", &["post-write:"]),
    ("pre-edit-prevention", &["Pre-edit prevention"]),
    ("delegation-nudge", &["[delegation-nudge]"]),
];

/// Padrões de conteúdo: as formas que não têm marcador próprio.
///
/// A ordem é de especificidade: o deny (`[G2 …]`) antes do programa fundido, e
/// o programa fundido antes da diretiva genérica — que é o catch-all das
/// formas que são só uma ordem.
pub const FAMILY_PATTERNS: &[(&str, &[&str])] = &[
    ("code-mode-deny", &[r"\[G\d+\s"]),
    ("session-contract", &[r"MUST\s+touring doctor -j"]),
    (
        "pipefail-evidence",
        &["exited 0, but its output shows errors after a pipe"],
    ),
    ("generic-noise", &[r"^\[generic\]"]),
    ("context-monitor", &[r"Contexto em \d+%"]),
    ("compaction-gaps", &[r"\[COMPACTA[ÇC][ÃA]O"]),
    ("delegation-nudge", &[r"\[M3 delega"]),
    ("failure-echo", &[r"⚡ last fail:", r"failed \(exit \d+\):"]),
    ("quality-gate", &[r"\[E\d+\s"]),
    ("touring-suggest", &[r"(?m)^\s*(?:MUST|SHOULD|MAY)\s+\S"]),
];

/// Os padrões compilados, uma vez: a régua roda dentro de `touring kpi` e
/// varre todo bloco do transcript.
#[must_use]
pub fn family_patterns() -> &'static [(&'static str, Regex)] {
    static COMPILED: OnceLock<Vec<(&'static str, Regex)>> = OnceLock::new();
    COMPILED.get_or_init(|| {
        FAMILY_PATTERNS
            .iter()
            .flat_map(|(family, patterns)| {
                patterns.iter().map(move |pattern| {
                    (
                        *family,
                        // Padrão de constante: se compila mal, é bug de código — e a
                        // régua não pode silenciar uma família por isso.
                        Regex::new(pattern).expect("padrão de família constante deve compilar"),
                    )
                })
            })
            .collect()
    })
}

/// How an emitter's mark claims the output it produced (C10, 2026-10-17).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Claim {
    /// A single-purpose hook: whatever it injects belongs to ITS family, whatever the text says.
    Owns,
    /// A multi-purpose emitter: used only when the text names no family.
    Fallback,
}

/// Emitter mark → family. The mark is the label Claude Code records on every `hook_success`
/// attachment (`command`): the hook's `statusMessage` when it has one, else its command line.
/// It is the emitter's OWN identity, so attributing by it infers nothing from the text and
/// stamps no byte into the injection.
///
/// Measured 17/10/2026 over the 3 newest transcripts (13,741 `hook_success`, 23 distinct marks):
/// text needles alone left 82.8 % of the injected bytes in `other`. The first match wins, and
/// every needle is checked against the versioned hook registry
/// (`the_emitter_table_has_no_stale_needle`).
pub const EMITTERS: &[(&str, &str, Claim)] = &[
    ("analyzing blast radius", "pre-edit", Claim::Owns),
    ("speculative validation", "pre-write", Claim::Owns),
    ("analyzing file context", "pre-read", Claim::Owns),
    ("checking command history", "pre-bash", Claim::Owns),
    ("syntax pre-check", "pre-edit-prevention", Claim::Owns),
    ("verifying edit quality", "post-edit", Claim::Owns),
    ("registering new file", "post-write", Claim::Owns),
    ("evidence_guard.py", "evidence-guard", Claim::Owns),
    // Hooks that ship with a plugin carry an unexpanded `${CLAUDE_PLUGIN_ROOT}` in the label.
    ("CLAUDE_PLUGIN_ROOT", "plugin-hook", Claim::Owns),
    ("prose_integrity_gate.py", "prose-integrity", Claim::Owns),
    ("check_context.sh", "context-monitor", Claim::Owns),
    ("delegation-nudge", "delegation-nudge", Claim::Owns),
    // Multi-purpose emitters: the text decides first (a `cli-suggest` block can be a code-mode
    // deny, a suggestion or a past lesson); the mark only rescues what the text leaves in `other`.
    ("touring cli-suggest", "touring-suggest", Claim::Fallback),
    ("loop_resume.py", "loop-outer", Claim::Fallback),
    ("touring-hook post-bash", "post-tool-echo", Claim::Fallback),
];

/// The family an emitter mark claims, and how.
fn emitter_claim(emitter: &str) -> Option<(&'static str, Claim)> {
    EMITTERS
        .iter()
        .find(|(needle, _, _)| emitter.contains(needle))
        .map(|(_, family, claim)| (*family, *claim))
}

/// [`injection_family`] with the emitter's own mark applied — the taxonomy every accounting
/// path uses (C10). Order: a dedicated emitter owns its output; else the text needles; else the
/// emitter's fallback; else `other`. A transcript without the mark (older Claude Code) falls
/// straight to the text, exactly as before.
#[must_use]
pub fn attributed_family(text: &str, emitter: Option<&str>) -> &'static str {
    let claimed = emitter.and_then(emitter_claim);
    if let Some((family, Claim::Owns)) = claimed {
        return family;
    }
    match injection_family(text) {
        "other" => claimed.map_or("other", |(family, _)| family),
        family => family,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `families` responde o CONJUNTO: um payload que mistura sermão e fato
    /// carrega as DUAS famílias, na ordem canônica, sem repetição — é o que
    /// permite à política preservar o fato que veio de carona num aviso.
    #[test]
    fn families_devolve_o_conjunto_misto_sem_repeticao() {
        let mixed = "[TOURING SUGGEST]\n  MUST x\nHOTSPOT: a.rs blast 12";
        let found = families(mixed);
        assert!(found.contains(&"touring-suggest"), "{found:?}");
        assert!(found.contains(&"pre-edit"), "{found:?}");
        assert_eq!(
            found.iter().filter(|f| **f == "touring-suggest").count(),
            1,
            "uma família com marcador E padrão casando entra uma vez só"
        );
        // A primeira família do conjunto é a resposta da régua (primária).
        assert_eq!(found.first().copied(), Some(injection_family(mixed)));
    }

    #[test]
    fn families_do_desconhecido_e_vazio() {
        assert!(families("[gabrielgadea]").is_empty());
        assert!(families("").is_empty());
    }

    /// Toda família da tabela de emissores precisa ser nomeável pelo texto ou
    /// pela marca — `pre-bash` e `prose-integrity` só existem pela marca, e
    /// é por isso que a tabela de emissores NÃO pode perder essas linhas.
    #[test]
    fn as_familias_so_de_emissor_sao_alcancaveis_pela_marca() {
        assert_eq!(
            attributed_family("texto sem marcador", Some("checking command history")),
            "pre-bash"
        );
        assert_eq!(
            attributed_family("texto sem marcador", Some("prose_integrity_gate.py")),
            "prose-integrity"
        );
    }
}
