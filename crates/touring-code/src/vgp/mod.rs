//! VGP motor — layer 1: deny invented API BEFORE the edit, pure and daemonless.
//!
//! One [`check`] call per proposed file. The language adapters ([`lang`], one file each) MEASURE
//! — Python today: static scope analysis (`nome`), from-import resolution (`import`), project/venv
//! module attributes (`atributo`), concrete-class instance members (`membro`/`atributo`) and
//! keyword arguments (`kwarg`) — and [`check`] DECIDES for every language the same way
//! ([`politica`]): severity by target × where the code runs, and the delta against the disk
//! content. Fail-open by construction: a parse error or any unresolvable link yields `unknown`
//! (with the reason), never `deny` — the corpus ruler is falso deny ≤ 1 % (V1 spec,
//! 2026-10-08-vgp-motor-base). A language with no adapter is `unknown`, never `allow`: `allow`
//! claims a verification that did not happen (absence is not zero).
//!
//! Delta mode: with `disk` content, a finding is denied only when it is NOT already present in
//! the disk content — a pre-edit hook denies what the edit INTRODUCES, never what pre-existed.

pub mod lang;
pub mod politica;
pub mod sombra;

use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

pub use lang::lang_do_caminho;
pub use politica::{Alvo, Contexto};

use lang::{Adaptador, Transversal};

/// One deny/warn finding — the JSON shape the judge and the hooks read.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Finding {
    /// Check family (VL0 §4): `nome` | `atributo` | `import` | `kwarg` | `membro` | …
    pub classe: String,
    /// The provably-missing name.
    pub nome: String,
    /// 1-based line in the proposed content.
    pub linha: usize,
    /// `deny` | `warn` — decided by [`politica`], never by the adapter.
    pub severidade: String,
    /// The teaching refusal — why, plus the closest real name when one exists.
    pub sugestao: String,
    /// Real names that do exist (member set / module top names, capped).
    pub reais: Vec<String>,
    /// Whom the finding is about (the input of the severity policy). In-process only: the
    /// JSON the judge reads is unchanged.
    #[serde(skip)]
    pub alvo: Alvo,
    /// The type of the receiver a member finding is about, as the layer that found it names it
    /// (layer 1: `stat_result`; layer 2: the checker's class, possibly qualified —
    /// `os.stat_result`). [`sombra::fundir_findings`] compares the two for an [`Alvo::Fechado`]
    /// finding. In-process only, like `alvo`: the JSON the judge reads is unchanged.
    #[serde(skip)]
    pub receptor: Option<String>,
}

impl Finding {
    /// A finding whose severity is ALREADY decided (layer-2 merge, CLI errors). Its target is
    /// irrelevant: [`check`] never re-judges it. Adapters use [`Finding::candidato`].
    pub fn new(
        classe: &str,
        nome: &str,
        linha: usize,
        severidade: &str,
        sugestao: String,
        reais: Vec<String>,
    ) -> Self {
        Self {
            classe: classe.to_string(),
            nome: nome.to_string(),
            linha,
            severidade: severidade.to_string(),
            sugestao,
            reais,
            alvo: Alvo::Projeto,
            receptor: None,
        }
    }

    /// This finding with the type of its receiver ([`Finding::receptor`]).
    pub fn com_receptor(mut self, receptor: Option<String>) -> Self {
        self.receptor = receptor;
        self
    }

    /// An adapter's finding: what it is about ([`Alvo`]) instead of how loud it should be —
    /// [`politica::aplicar`] fills `severidade`. Until then it reads `warn`, the safe default.
    pub fn candidato(
        classe: &str,
        nome: &str,
        linha: usize,
        alvo: Alvo,
        sugestao: String,
        reais: Vec<String>,
    ) -> Self {
        Self {
            alvo,
            ..Self::new(classe, nome, linha, "warn", sugestao, reais)
        }
    }
}

/// Everything a check needs; the CLI handler resolves files into strings first.
pub struct CheckInput {
    /// Virtual path of the proposed file; `<string>`/`<stdin>` means inline code.
    pub path: String,
    /// Proposed content (the post-edit state the motor judges).
    pub content: String,
    /// Project root the module names resolve against.
    pub project_root: PathBuf,
    /// Disk content for delta mode (deny only what the edit introduces).
    pub disk: Option<String>,
    /// Overlay path → content: module files at the entry's instant, laid over today's disk.
    pub overlays: HashMap<String, String>,
    /// A `lang` of VL0 §2 (`python` · `rust` · `typescript` · `tsx` · `javascript` · `html` ·
    /// `css`); derive it with [`lang_do_caminho`]. Anything else answers `unknown`.
    pub lang: String,
}

/// The JSON the CLI prints and the judge reads — always present, even on failure.
#[derive(Debug, Serialize)]
pub struct CheckOutput {
    /// `deny` | `warn` | `allow` | `unknown` (could not measure — fail-open).
    pub verdict: String,
    /// Motor layer (1 = static, pure, daemonless).
    pub layer: u8,
    /// Language the check ran against.
    pub lang: String,
    /// Wall time of the motor itself (the hook's budget is p95 ≤ 150 ms).
    pub elapsed_ms: f64,
    /// Findings ordered by discovery; deny first is not guaranteed.
    pub findings: Vec<Finding>,
    /// Camada 2 status when `--camadas 1,2` was asked and pyright was unavailable —
    /// `"unknown: <motivo>"` (the L1 findings still stand). Absent when L2 ran or was not asked.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub camada2: Option<String>,
    /// Why the motor could not measure (parse failure, adapter not implemented, no adapter for
    /// the language). Present whenever something went unmeasured, even beside findings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub motivo: Option<String>,
}

/// The one public entry. `verdict`: deny (≥1 deny finding) · warn (≥1 warn finding, no deny) ·
/// allow · unknown (nothing could be measured — fail-open, never deny).
///
/// Pipeline, the same for every language (VL0 §3): pick the adapter by `input.lang` →
/// adapter (+ transversal checks) on the PROPOSED content → severity policy → delta against
/// the disk content → verdict.
///
/// This is the verdict of a file being WRITTEN ([`Contexto::Escrita`]): the hooks of
/// `Write`/`Edit`/`MultiEdit` and `touring vgp check` without `--contexto`. A file about to be
/// EXECUTED asks [`check_em`] with [`Contexto::Execucao`].
pub fn check(input: &CheckInput) -> CheckOutput {
    check_em(input, Contexto::Escrita)
}

/// [`check`] at an explicit moment: the same pipeline, the severity policy asked WHEN the code
/// is judged (FRAG, 10/10/2026 — a fragment in `/tmp` is no error until something runs it).
pub fn check_em(input: &CheckInput, contexto: Contexto) -> CheckOutput {
    let t0 = Instant::now();
    let (verdict, findings, motivo) = match lang::adaptador_de(&input.lang) {
        Some(adaptador) => julgar(
            input,
            adaptador,
            &lang::transversais_de(&input.lang),
            contexto,
        ),
        None => (
            "unknown".to_string(),
            Vec::new(),
            Some(format!(
                "linguagem `{}` sem adaptador registrado",
                input.lang
            )),
        ),
    };
    CheckOutput {
        verdict,
        layer: 1,
        lang: input.lang.clone(),
        elapsed_ms: t0.elapsed().as_secs_f64() * 1000.0,
        findings,
        camada2: None,
        motivo,
    }
}

/// [`check`] with the adapter and the transversal checks injected — the seam that lets the
/// policy and the delta be proven for ANY language, not only for the registered ones.
pub(crate) fn julgar(
    input: &CheckInput,
    adaptador: &dyn Adaptador,
    transversais: &[&dyn Transversal],
    contexto: Contexto,
) -> (String, Vec<Finding>, Option<String>) {
    let mut medido = medir(input, adaptador, transversais);
    politica::aplicar(
        &mut medido.achados,
        &input.path,
        &input.project_root,
        contexto,
    );

    // delta: the disk run only matters when there is a deny to downgrade
    if let Some(disco) = &input.disk
        && medido.achados.iter().any(|f| f.severidade == "deny")
    {
        let do_disco = CheckInput {
            path: input.path.clone(),
            content: disco.clone(),
            project_root: input.project_root.clone(),
            disk: None,
            overlays: input.overlays.clone(),
            lang: input.lang.clone(),
        };
        let no_disco = medir(&do_disco, adaptador, transversais);
        politica::aplicar_delta(&mut medido.achados, &no_disco.achados);
    }

    let base = if medido.nao_medido.is_some() {
        "unknown"
    } else {
        "allow"
    };
    let verdict = politica::verdict_dos(&medido.achados, base);
    (verdict, medido.achados, medido.nao_medido)
}

/// What the adapter and the transversal checks found on ONE content, severities still undecided.
struct Medido {
    achados: Vec<Finding>,
    nao_medido: Option<String>,
}

fn medir(
    input: &CheckInput,
    adaptador: &dyn Adaptador,
    transversais: &[&dyn Transversal],
) -> Medido {
    let v = adaptador.verificar(input);
    let mut achados = v.achados;
    // the adapter's reason first, then each transversal's in registry order — none is dropped
    let mut motivos: Vec<String> = v.nao_medido.into_iter().collect();
    for t in transversais {
        let r = t.verificar(input);
        achados.extend(r.achados);
        motivos.extend(r.nao_medido);
    }
    Medido {
        achados,
        nao_medido: (!motivos.is_empty()).then(|| motivos.join("; ")),
    }
}

/// Is this virtual path an inline program (`python3 -c` / heredoc / `touring run --code`)?
pub fn is_inline(path: &str) -> bool {
    path == "<string>" || path == "<stdin>"
}
