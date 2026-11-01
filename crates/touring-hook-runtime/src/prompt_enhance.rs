//! Prompt Enhancement — native Rust replacement for prompt_enhancer.py.
//!
//! Classifies user prompts into 8 intent categories and applies
//! prompt engineering techniques (chain-of-thought, constitutional constraints,
//! structured output, few-shot reasoning, self-validation, precision hints).
//!
//! Performance: <1ms classification + composition (vs ~40ms Python).
//! Output: identical JSON contract to prompt_enhancer.py.
//!
//! ## TACO Integration (v6.2)
//!
//! This module is the **TACO Phase 0 Perception Layer** — the entry point for
//! all user prompts. It classifies intent and CILA level, then feeds the TACO
//! phase protocol:
//!
//! - L0-L1 (General/Code): Solo mode, TACO resolves directly
//! - L2 (Debug/Refactor/Test): Scout → Engineer → Validate
//! - L3 (Analysis/Plan): Scout → Architect → Engineer → Audit → Validate
//! - L4 (Creative): Full TACO pipeline (all phases)
//!
//! ## Touring CLI Command Ranks (v5.0)
//!
//! TIER 1 ★★★★★ (CRÍTICOS): touring ast meta, touring pre-edit, touring ast blast,
//!   touring index find, touring wiring orphans, touring e2e
//! TIER 2 ★★★★☆ (DIAGNÓSTICO): touring doctor, touring status, touring gate-metrics, touring learning status
//! TIER 3 ★★★★☆ (INTELLIGENCE): touring ast blast-cross-feature, touring wiring audit,
//!   touring wiring chains, touring file-knowledge extended, touring tantivy search
//! TIER 4 ★★★★☆ (SESSION): touring session start/assess, touring decompose create/add,
//!   touring memory store
//! TIER 5 ★★★★☆ (GENERATION): touring generate list-kinds/verify/render/plan-submit/plan-speculate

use std::collections::HashMap;

use crate::hook_response::HookResponse;
use crate::hook_runtime::HookRuntime;

/// Intent categories for prompt classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Intent {
    /// Writing or generating new code.
    Code,
    /// Diagnosing or fixing a failure.
    Debug,
    /// Restructuring existing code without changing behavior.
    Refactor,
    /// Writing or running tests.
    Test,
    /// Understanding or explaining code or data.
    Analysis,
    /// Open-ended or creative generation.
    Creative,
    /// Designing a multi-step implementation plan.
    Plan,
    /// Fallback when no specific intent keywords match.
    General,
}

impl Intent {
    /// Returns the uppercase string label for this intent (e.g. `"DEBUG"`).
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Code => "CODE",
            Self::Debug => "DEBUG",
            Self::Refactor => "REFACTOR",
            Self::Test => "TEST",
            Self::Analysis => "ANALYSIS",
            Self::Creative => "CREATIVE",
            Self::Plan => "PLAN",
            Self::General => "GENERAL",
        }
    }

    /// Priority for tiebreaking (higher = preferred).
    fn priority(&self) -> u8 {
        match self {
            Self::Debug => 7,
            Self::Test => 6,
            Self::Refactor => 5,
            Self::Plan => 4,
            Self::Analysis => 3,
            Self::Creative => 2,
            Self::Code => 1,
            Self::General => 0,
        }
    }

    /// All classifiable intents (excludes General — it has no keywords and
    /// serves only as the fallback when no keywords match).
    fn all() -> &'static [Intent] {
        &[
            Self::Code,
            Self::Debug,
            Self::Refactor,
            Self::Test,
            Self::Analysis,
            Self::Creative,
            Self::Plan,
        ]
    }
}

/// Prompt engineering technique.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Technique {
    /// Elicit step-by-step reasoning before the answer.
    ChainOfThought,
    /// Inject constitutional rules and constraints to follow.
    ConstitutionalConstraints,
    /// Request output in a fixed structured format.
    StructuredOutput,
    /// Provide few-shot reasoning exemplars.
    FewShotReasoning,
    /// Ask the model to validate its own answer.
    SelfValidation,
    /// Add precision hints to sharpen the response.
    PrecisionHints,
}

impl Technique {
    /// Returns the snake_case string label for this technique (e.g. `"chain_of_thought"`).
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ChainOfThought => "chain_of_thought",
            Self::ConstitutionalConstraints => "constitutional_constraints",
            Self::StructuredOutput => "structured_output",
            Self::FewShotReasoning => "few_shot_reasoning",
            Self::SelfValidation => "self_validation",
            Self::PrecisionHints => "precision_hints",
        }
    }
}

/// Classification result (basic).
#[derive(Debug, Clone)]
pub struct ClassifyResult {
    /// The classified intent category.
    pub intent: Intent,
    /// Confidence score for the classification, in `[0.0, 1.0]`.
    pub confidence: f64,
}

/// Full classification result with CILA level and applied techniques.
#[derive(Debug, Clone)]
pub struct ClassificationResult {
    /// The classified intent category.
    pub intent: Intent,
    /// CILA cognitive intensity level (0-4).
    pub cila_level: u8,
    /// Prompt engineering techniques selected for this intent.
    pub techniques: Vec<Technique>,
    /// Classification confidence (sum of matched keyword weights; with the
    /// Laya student path, the student's max probability).
    pub confidence: f64,
    /// Who decided: "keyword" (the deterministic default) or "laya" (the W4.2
    /// student, only above the calibrated τ — W4.4, flag TOURING_LAYA_INTENT).
    pub source: &'static str,
}

/// Map intent to CILA cognitive intensity level.
///
/// - General  -> L0 (direct response)
/// - Code     -> L1 (program-aided)
/// - Debug / Refactor / Test -> L2 (tool-augmented)
/// - Analysis / Plan -> L3 (pipeline)
/// - Creative -> L4 (agent loops)
///
/// **TACO Phase Routing**: CILA level determines which TACO phases execute:
/// - L0-L1: Solo mode (TACO resolves directly, no subagents)
/// - L2: Phase 1 (scout) → Phase 5 (engineer) → validate
/// - L3: Phase 1 → Phase 2 (architect) → Phase 5 → Phase 6 (audit) → validate
/// - L4+: All phases (0, 1, 2, 3, 4, 4.5, 5, 6, 7)
pub fn intent_to_cila(intent: &Intent) -> u8 {
    match intent {
        Intent::General => 0,
        Intent::Code => 1,
        Intent::Debug | Intent::Refactor | Intent::Test => 2,
        Intent::Analysis | Intent::Plan => 3,
        Intent::Creative => 4,
    }
}

/// Classify a prompt and return full details (intent, CILA level, techniques, confidence).
///
/// This is the primary classification entry point for consumers that need
/// all metadata in a single call — suitable for logging, debugging, and
/// downstream routing decisions.
pub fn classify_with_details(prompt: &str) -> ClassificationResult {
    details_from_claim(prompt, student_classify(prompt))
}

/// (Somente testes) O caminho completo para quem JÁ segura STUDENT_ENV_LOCK —
/// mesma decisão, sem adquirir o lock de novo (ver student_classify_inner).
#[cfg(test)]
pub(crate) fn classify_with_details_inner(prompt: &str) -> ClassificationResult {
    details_from_claim(prompt, student_classify_inner(prompt))
}

fn details_from_claim(prompt: &str, claim: Option<(Intent, f64)>) -> ClassificationResult {
    if let Some((intent, confidence)) = claim {
        return ClassificationResult {
            intent,
            cila_level: intent_to_cila(&intent),
            techniques: techniques_for(&intent),
            confidence,
            source: "laya",
        };
    }
    let result = classify(prompt);
    let techniques = techniques_for(&result.intent);
    let cila_level = intent_to_cila(&result.intent);

    ClassificationResult {
        intent: result.intent,
        cila_level,
        techniques,
        confidence: result.confidence,
        source: "keyword",
    }
}

/// S-3 (plan r2): the timed variant — the wrapper measures the whole decision
/// (student round trip included) so the decision journal carries `latency_ms`
/// without changing `classify_with_details`' signature (3 call sites + tests).
pub fn classify_with_details_timed(prompt: &str) -> (ClassificationResult, u64) {
    let started = std::time::Instant::now();
    let details = classify_with_details(prompt);
    (details, started.elapsed().as_millis() as u64)
}

/// Current time in epoch milliseconds for the journal (no new deps).
fn intent_journal_now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// The journal path: TOURING_INTENT_JOURNAL (override explícito) ou o default.
/// Sob `cfg(test)` o default é um arquivo de tmp do processo — a suíte NUNCA
/// escreve no journal de produção (a poluição foi medida: ~80 linhas de teste no
/// journal real em 01/10, encontrado pelo leitor da família decision).
fn intent_journal_path() -> std::path::PathBuf {
    if let Ok(p) = std::env::var("TOURING_INTENT_JOURNAL") {
        return std::path::PathBuf::from(p);
    }
    #[cfg(test)]
    {
        return std::env::temp_dir().join(format!("intent-journal-tests-{}", std::process::id()));
    }
    #[cfg(not(test))]
    {
        let home = std::env::var("HOME").unwrap_or_default();
        std::path::PathBuf::from(format!("{home}/.claude/touring/logs/intent_journal.jsonl"))
    }
}

/// Append one JSONL line; a failure is traced, never propagated (fail-open —
/// telemetry must never crash a hook; S-3.2, plan 2026-09-30 W3).
///
/// A linha inteira vai num ÚNICO `write_all` sobre O_APPEND: o POSIX garante o
/// offset atômico por syscall, então writers concorrentes (sessões, threads de
/// teste) nunca intercalam o conteúdo de uma linha. A forma anterior (`writeln!`)
/// passava por `write_fmt` em vários pedaços — 4,8% das linhas nasciam com dois
/// JSONs concatenados (medido: 4/84, touring-02, 01/10).
fn append_intent_journal(record: &serde_json::Value) {
    let path = intent_journal_path();
    // Delega na primitiva compartilhada (single write_all + O_APPEND); fail-open aqui.
    if let Err(err) = crate::hook_trace::append_json_line(&path, record) {
        tracing::debug!(?err, "prompt_enhance: intent journal write failed");
    }
}

/// S-3.2 (plan 2026-09-30, W3): decision journal for prompt classification —
/// one append-only JSONL line per classified prompt, written by the hook process
/// itself, never through the daemon queue (S32b). `path` marks which entry fired
/// (`compose_json` | `hook_entry`); readers dedup when both fire for one prompt.
/// This journal is the source the `decision` KPI family reads (S-3.1).
fn journal_intent_decision(details: &ClassificationResult, path: &str, latency_ms: u64) {
    append_intent_journal(&serde_json::json!({
        "ts_ms": intent_journal_now_ms(),
        "intent": details.intent.as_str(),
        "cila_level": details.cila_level,
        "confidence": details.confidence,
        "source": details.source,
        "path": path,
        "latency_ms": latency_ms,
        "skipped_automated": false,
    }));
}

/// The machine-payload counterpart of [`journal_intent_decision`]: proves the
/// G0 filter fired. Intent is null — nothing was classified.
fn journal_skip_automated(path: &str) {
    append_intent_journal(&serde_json::json!({
        "ts_ms": intent_journal_now_ms(),
        "intent": serde_json::Value::Null,
        "source": "keyword",
        "path": path,
        "skipped_automated": true,
    }));
}

// ── W4.4: Laya student intent classification (flag default-OFF) ─────────────
//
// The W4.2 student (laya-w42-intent, fine-tuned RLCD on the G1 corpus) answers
// intent as a typed Choice over the same 8 classes, served locally over the
// TypeSafe /v1/systemone wire (scripts/laya_student_server.py). Calibrated on
// the frozen test split (W4.3, 2026-10-01): max-prob ≥ STUDENT_TAU measures
// 0.926 accuracy at 22.5% support; everything below falls back to the keyword
// path — during transition, nothing gets worse. Latency budget: ~25 ms p99
// server-side; the hook spends < 5 ms on the wire (localhost, no TLS).

/// Measured on the frozen test split (W4.3): max-prob ≥ 0.6 → acc 0.926.
const STUDENT_TAU: f64 = 0.6;

/// The student server URL (scripts/laya_student_server.py). Tests point this at
/// an ephemeral TcpListener via LAYA_STUDENT_URL. In TEST BUILDS the default is
/// EMPTY (never call): env vars are process-global across test threads, and with
/// the shadow path every classify became a potential student client — a test
/// that never asked for the student must never become a client (race measured
/// in the suite 01/10: a parallel test consumed the one-shot stub's accept).
/// INVARIANT (2026-10-02): o texto do prompt só sai para LOOPBACK — a conexão
/// passa por student_endpoint, que recusa https (seria downgrade para texto
/// puro), DNS/remoto e tudo fora de 127.0.0.0/8 e ::1.
fn student_url() -> String {
    std::env::var("LAYA_STUDENT_URL").unwrap_or_else(|_| {
        #[cfg(test)]
        {
            String::new()
        }
        #[cfg(not(test))]
        {
            "http://127.0.0.1:8971".to_string()
        }
    })
}

/// Flag default-OFF, in the TOURING_PILLAR_INDUCTION_ARMED mold.
fn student_enabled() -> bool {
    std::env::var("TOURING_LAYA_INTENT").is_ok_and(|v| !v.is_empty() && v != "0" && v != "false")
}

/// (Somente testes) Trava do ambiente do aluno na suíte: LAYA_STUDENT_URL /
/// TOURING_LAYA_INTENT são process-wide. O teste que as modifica segura
/// JOURNAL_ENV_LOCK → STUDENT_ENV_LOCK (nesta ordem, sempre) e chama as
/// variantes `_inner`; qualquer outro classificador espera aqui até as envs
/// serem restauradas e então cai no keyword (student_url() vazio). Sem isso,
/// um classify concorrente decide com o stub one-shot de outro teste e o
/// consome (2 panics sob RUST_TEST_THREADS=12, medido 2026-10-02).
#[cfg(test)]
pub(crate) static STUDENT_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// The student's class name → our Intent (same 8 classes by construction).
fn intent_from_name(name: &str) -> Option<Intent> {
    match name.to_ascii_lowercase().as_str() {
        "general" => Some(Intent::General),
        "code" => Some(Intent::Code),
        "debug" => Some(Intent::Debug),
        "refactor" => Some(Intent::Refactor),
        "test" => Some(Intent::Test),
        "analysis" => Some(Intent::Analysis),
        "plan" => Some(Intent::Plan),
        "creative" => Some(Intent::Creative),
        _ => None,
    }
}

/// The JSON body for the student's /v1/systemone: the exact 8 trained classes.
fn student_request_body(prompt: &str) -> String {
    serde_json::json!({
        "state": {"prompt": prompt},
        "model": "laya-w42-intent",
        "questions": {
            "intent": {
                "type": "choice",
                "instructions": "What kind of work is the user asking for in `prompt`? Answer with the dominant intent.",
                // the exact 8 classes the student was trained on — no "other"
                "criteria": {
                    "general": null, "code": null, "debug": null, "refactor": null,
                    "test": null, "analysis": null, "plan": null, "creative": null
                }
            }
        }
    })
    .to_string()
}

/// One blocking POST to the student server; None on any wire failure.
fn student_http_post(body: &str) -> Option<serde_json::Value> {
    use std::io::{Read, Write};
    let url = student_url();
    if url.is_empty() {
        return None; // test builds sem LAYA_STUDENT_URL explícita: nunca chama
    }
    let addr = student_endpoint(&url)?;
    let request = format!(
        "POST /v1/systemone HTTP/1.1\r\nHost: {addr}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let timeout = std::time::Duration::from_millis(200);
    // connect_timeout: o default do connect é o retry do kernel (minutos em
    // host não roteável) — o teste remote_student_url_never_connects_and_fails_open_fast
    // prova o caminho recusado em < 100 ms. DNS nunca resolve aqui (só loopback).
    let mut stream = std::net::TcpStream::connect_timeout(&addr, timeout).ok()?;
    stream.set_read_timeout(Some(timeout)).ok()?;
    stream.set_write_timeout(Some(timeout)).ok()?;
    stream.write_all(request.as_bytes()).ok()?;
    let mut buf = String::new();
    stream.read_to_string(&mut buf).ok()?;
    serde_json::from_str(buf.get(buf.find("\r\n\r\n")? + 4..)?).ok()
}

/// Só `http://` + host loopback (127.0.0.0/8, [::1], `localhost`) + porta —
/// o texto do prompt nunca sai desta máquina: a shadow-matrix pergunta ao aluno
/// a CADA prompt, então uma LAYA_STUDENT_URL mal configurada mandaria o prompt
/// inteiro a um host remoto. None (fail-open: o caminho keyword decide) para
/// qualquer outra forma — https seria downgrade para TCP em texto puro, DNS
/// vazaria para remoto. A recusa deixa UM rastro por processo (um warn por
/// chamada inundaria o log do hook; zero esconderia a má configuração).
fn student_endpoint(url: &str) -> Option<std::net::SocketAddr> {
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
    let reject = |why: &str| {
        static WARNED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        if !WARNED.swap(true, std::sync::atomic::Ordering::Relaxed) {
            tracing::warn!(
                "prompt_enhance: LAYA_STUDENT_URL recusada ({why}); o texto do prompt só sai para loopback"
            );
        }
        None
    };
    let Some(rest) = url.strip_prefix("http://") else {
        return reject("só http:// — https seria downgrade para texto puro");
    };
    let Some(host_port) = rest.split('/').next() else {
        return reject("URL sem host");
    };
    let Some((host, port)) = host_port.rsplit_once(':') else {
        return reject("sem porta");
    };
    let Ok(port) = port.parse::<u16>() else {
        return reject("porta inválida");
    };
    let ip: IpAddr = match host {
        "localhost" => IpAddr::V4(Ipv4Addr::LOCALHOST),
        "::1" | "[::1]" => IpAddr::V6(Ipv6Addr::LOCALHOST),
        _ => match host.parse::<IpAddr>() {
            Ok(ip) => ip,
            Err(_) => return reject("host não é IP loopback (DNS/remoto)"),
        },
    };
    if !ip.is_loopback() {
        return reject("host fora de 127.0.0.0/8 e ::1");
    }
    Some(SocketAddr::new(ip, port))
}

/// Parse WITHOUT the τ gate — the shadow path (S-2, plan r2) needs the raw
/// answer: what the student WOULD have said is journal material even when it is
/// not a claim. The τ gate lives in the caller's decision, never in the parse.
fn parse_student_answer_raw(payload: &serde_json::Value) -> Option<(Intent, f64)> {
    let answer = payload.get("answers")?.get("intent")?;
    let choice = answer.get("choice")?.as_str()?;
    let max_prob = answer
        .get("probabilities")?
        .as_object()?
        .values()
        .filter_map(serde_json::Value::as_f64)
        .fold(0.0_f64, f64::max);
    Some((intent_from_name(choice)?, max_prob))
}

/// Parse + gate: the calibrated τ is the only gate, and it is read from the
/// student's own probabilities (never from a bare choice string).
fn parse_student_answer(payload: &serde_json::Value) -> Option<(Intent, f64)> {
    let (intent, max_prob) = parse_student_answer_raw(payload)?;
    if max_prob < STUDENT_TAU {
        return None;
    }
    Some((intent, max_prob))
}

/// S-2 (plan r2, shadow-matrix): the student's would-be answer, journaled with
/// `source: "laya_shadow"`. The agreement matrix is built while the keyword
/// path keeps deciding — a shadow line is NEVER a decision (the KPI reads it
/// as a separate source by construction).
fn journal_student_shadow(intent: Intent, max_prob: f64, latency_ms: u64) {
    append_intent_journal(&serde_json::json!({
        "ts_ms": intent_journal_now_ms(),
        "intent": intent.as_str(),
        "confidence": max_prob,
        "source": "laya_shadow",
        "path": "shadow",
        "latency_ms": latency_ms,
        "skipped_automated": false,
    }));
}

/// Ask the student; Some ONLY for a real claim (flag ON + max_prob ≥ τ).
/// Every answered-but-unclaimed evaluation is journaled as `laya_shadow` —
/// including with the flag OFF (shadow mode: the keyword path decides, the
/// journal still learns what the student would have said). None on any failure
/// (unreachable, slow, malformed) — every one is the keyword path, never a crash.
fn student_classify(prompt: &str) -> Option<(Intent, f64)> {
    // (Somente testes) serializa contra o modificador do ambiente do aluno
    // (STUDENT_ENV_LOCK); em produção não há lock — o hook decide por evento.
    #[cfg(test)]
    let _guard = STUDENT_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    student_classify_inner(prompt)
}

/// O corpo sem o lock — para quem JÁ segura STUDENT_ENV_LOCK (os testes que
/// modificam as envs do aluno); student_classify ali seria deadlock (o Mutex
/// do std não é reentrante).
fn student_classify_inner(prompt: &str) -> Option<(Intent, f64)> {
    let started = std::time::Instant::now();
    let payload = student_http_post(&student_request_body(prompt))?;
    let wire_ms = started.elapsed().as_millis() as u64;
    let (intent, max_prob) = parse_student_answer_raw(&payload)?;
    if student_enabled() {
        // o gate τ vive em parse_student_answer (fonte única do threshold)
        if let Some(claim) = parse_student_answer(&payload) {
            return Some(claim);
        }
    }
    journal_student_shadow(intent, max_prob, wire_ms);
    None
}

// ── S-8 (plan r2): semantic bash gate (CEG × aluno) ─────────────────────────
//
// O caso canônico da doc ("gate the bash tool"): as camadas determinísticas
// conhecem os padrões comuns — o buraco são os comandos que NINGUÉM viu. Para
// esses, um reversibility Score do aluno LOCAL (~10ms) vira advisory no
// pre-bash. ADVISORY SEMPRE: enriquece a decisão, nunca nega por palpite (um
// gate que nasce bloqueando vira gate que se contorna).

const SEMANTIC_GATE_ENV: &str = "TOURING_CEG_SEMANTIC_GATE";

/// Flag default-OFF, no molde TOURING_LAYA_INTENT.
fn semantic_gate_enabled() -> bool {
    std::env::var(SEMANTIC_GATE_ENV).is_ok_and(|v| !v.is_empty() && v != "0" && v != "false")
}

/// Programas cujo risco as camadas determinísticas já cobrem — o gate só paga o
/// aluno pelo DESCONHECIDO. Lista deliberadamente conservadora: na dúvida, NOVEL
/// (o custo de um falso "desconhecido" é um advisory inócuo, nunca um deny).
const KNOWN_PROGRAMS: &[&str] = &[
    "ls",
    "cat",
    "grep",
    "rg",
    "git",
    "cargo",
    "touring",
    "touring-hook",
    "touring-daemon",
    "python3",
    "python",
    "echo",
    "pwd",
    "cd",
    "wc",
    "head",
    "tail",
    "find",
    "sed",
    "awk",
    "jq",
    "curl",
    "bash",
    "sh",
    "mkdir",
    "cp",
    "mv",
    "rm",
    "touch",
    "chmod",
    "ps",
    "kill",
    "systemctl",
    "journalctl",
    "df",
    "du",
    "nvidia-smi",
    "pytest",
    "npm",
    "node",
    "pip",
    "pip3",
    "tar",
    "zip",
    "unzip",
    "make",
    "rustc",
    "rustup",
    "docker",
    "ssh",
    "scp",
    "rsync",
    "diff",
    "patch",
    "xargs",
    "tee",
    "sort",
    "uniq",
    "tr",
    "cut",
    "date",
    "sleep",
    "timeout",
    "nohup",
    "watch",
    "man",
    "which",
    "file",
    "stat",
    "ln",
    "readlink",
    "basename",
    "dirname",
    "env",
    "sudo",
    "apt",
    "pacman",
    "snapper",
    "gh",
    "tmux",
    "less",
    "strings",
    "nm",
    "ldd",
    "lsof",
    "dmesg",
    "mount",
    "dd",
    "sync",
    "hostname",
    "uname",
    "id",
    "whoami",
    "alias",
    "type",
    "exit",
    "true",
    "false",
    "test",
];

/// O short-program é conhecido (as camadas determinísticas já o cobrem)?
pub fn is_known_program(program: &str) -> bool {
    KNOWN_PROGRAMS.contains(&program)
}

/// O body /v1/systemone do gate: reversibilidade em 3 níveis (a forma da doc).
fn reversibility_request_body(command: &str) -> String {
    serde_json::json!({
        "state": {"command": command},
        "model": "laya-w42-intent",
        "questions": {
            "reversibility": {
                "type": "score",
                "instructions": {
                    "command": command,
                    "question": "Rate how reversible this shell command is if run by an agent: 0 = irreversible or destructive (data loss, force-push, drops state), 1 = reversible with cost (build artifacts, downloads, process spawns, edits), 2 = read-only or trivially reversible."
                },
                "criteria": [
                    "irreversible or destructive: loses data, rewrites shared state, or cannot be undone",
                    "reversible with cost: creates artifacts, downloads, spawns processes, or edits files that a later command can undo",
                    "read-only or trivially reversible: inspects, lists, prints, or computes without side effects"
                ]
            }
        }
    })
    .to_string()
}

/// (score 0–2, latency_ms) — None em qualquer falha ou com a flag OFF
/// (fail-open: o gate nunca atrapalha o caminho determinístico).
pub fn semantic_reversibility(command: &str) -> Option<(f64, u64)> {
    if !semantic_gate_enabled() {
        return None;
    }
    let started = std::time::Instant::now();
    let payload = student_http_post(&reversibility_request_body(command))?;
    let wire_ms = started.elapsed().as_millis() as u64;
    let score = payload
        .get("answers")?
        .get("reversibility")?
        .get("score")?
        .as_f64()?;
    Some((score, wire_ms))
}

/// Keyword entry: (keyword, exclusive_weight).
/// exclusive = 2.0 (only appears in one intent), shared = 1.0 (multiple intents).
/// Multi-word phrases use literal substring matching.
/// Single-word keywords use word-boundary matching (Python parity).
struct KeywordEntry {
    keyword: &'static str,
    weight: f64,
}

/// Check if `keyword` matches in `text` (already lowercased).
/// Single-word keywords use word-boundary matching (\b equivalent).
/// Multi-word phrases use literal substring matching.
fn keyword_matches(text: &str, keyword: &str) -> bool {
    if keyword.contains(' ') {
        // Multi-word phrase: literal substring match
        text.contains(keyword)
    } else {
        // Single word: word-boundary match
        // Find all occurrences and check boundaries
        let kw_bytes = keyword.as_bytes();
        let text_bytes = text.as_bytes();
        let kw_len = kw_bytes.len();

        let mut start = 0;
        while let Some(pos) = text[start..].find(keyword) {
            let abs_pos = start + pos;
            let end_pos = abs_pos + kw_len;

            // Check left boundary: start of string OR non-alphanumeric
            let left_ok = abs_pos == 0
                || text_bytes
                    .get(abs_pos - 1)
                    .map_or(true, |b| !b.is_ascii_alphanumeric());

            // Check right boundary: end of string OR non-alphanumeric
            let right_ok = end_pos >= text_bytes.len()
                || text_bytes
                    .get(end_pos)
                    .map_or(true, |b| !b.is_ascii_alphanumeric());

            if left_ok && right_ok {
                return true;
            }

            start = abs_pos + 1;
            if start >= text.len() {
                break;
            }
        }
        false
    }
}

/// Classify a prompt into one of 8 intent categories.
///
/// Uses weighted keyword scoring with tiebreaking by intent priority.
/// Bilingual: English + Portuguese (Brazilian).
pub fn classify(prompt: &str) -> ClassifyResult {
    let lower = prompt.to_lowercase();

    let mut scores: HashMap<Intent, f64> = HashMap::new();

    for intent in Intent::all() {
        let keywords = keywords_for(intent);
        let mut score = 0.0;
        for kw in keywords {
            if keyword_matches(&lower, kw.keyword) {
                score += kw.weight;
            }
        }
        if score > 0.0 {
            scores.insert(*intent, score);
        }
    }

    if scores.is_empty() {
        return ClassifyResult {
            intent: Intent::General,
            confidence: 0.0,
        };
    }

    let (best_intent, best_score) = scores
        .iter()
        .max_by(|(ia, sa), (ib, sb)| {
            sa.partial_cmp(sb)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| ia.priority().cmp(&ib.priority()))
        })
        .map(|(i, s)| (*i, *s))
        .expect("scores is non-empty — guaranteed by is_empty guard above");

    ClassifyResult {
        intent: best_intent,
        confidence: best_score,
    }
}
/// Returns the prompt-engineering techniques recommended for a given intent.
pub fn techniques_for(intent: &Intent) -> Vec<Technique> {
    match intent {
        Intent::Code => vec![
            Technique::ChainOfThought,
            Technique::ConstitutionalConstraints,
            Technique::StructuredOutput,
            Technique::PrecisionHints,
        ],
        Intent::Debug => vec![
            Technique::ChainOfThought,
            Technique::SelfValidation,
            Technique::ConstitutionalConstraints,
            Technique::FewShotReasoning,
        ],
        Intent::Refactor => vec![
            Technique::ChainOfThought,
            Technique::ConstitutionalConstraints,
            Technique::SelfValidation,
            Technique::PrecisionHints,
        ],
        Intent::Test => vec![
            Technique::ChainOfThought,
            Technique::StructuredOutput,
            Technique::ConstitutionalConstraints,
            Technique::FewShotReasoning,
        ],
        Intent::Analysis => vec![
            Technique::ChainOfThought,
            Technique::StructuredOutput,
            Technique::PrecisionHints,
        ],
        Intent::Creative => vec![
            Technique::ChainOfThought,
            Technique::FewShotReasoning,
            Technique::PrecisionHints,
        ],
        Intent::Plan => vec![
            Technique::ChainOfThought,
            Technique::StructuredOutput,
            Technique::SelfValidation,
            Technique::PrecisionHints,
        ],
        Intent::General => vec![Technique::ChainOfThought, Technique::PrecisionHints],
    }
}

/// Compose the full enhancement message for a classified prompt.
///
/// Includes:
/// 1. Prompt engineering techniques (CoT, constitutional, etc.)
/// 2. Action directives — concrete touring-cli/bash/AGP commands
///    that compile the user's intent into code-first actions.
pub fn compose(intent: &Intent, prompt: &str) -> String {
    let header = format!("[PROMPT ENHANCEMENT -- {} MODE]", intent.as_str());
    let techniques = techniques_for(intent);

    let mut sections = Vec::with_capacity(techniques.len() + 1);
    for tech in &techniques {
        let section = template_for(tech, intent);
        sections.push(format!("## {}\n{}", tech_heading(tech), section));
    }

    // Action directives — translate intent into touring ecosystem commands
    let actions = action_directives(intent, prompt);
    if !actions.is_empty() {
        sections.push(actions);
    }

    format!("{}\n\n{}", header, sections.join("\n\n"))
}

/// Prefixes that mark a payload the user never typed.
///
/// Claude Code delivers background events on the same channel as a prompt: agent
/// task-notifications, Stop-hook feedback, system notices, cross-session messages,
/// artifact comments and the output of `!` commands. They arrive as the "prompt" this
/// hook classifies, so on 2026-09-22 a session watched its own agent notifications be
/// enhanced as TEST, CODE and DEBUG. Such a payload has no intent to read, and reading
/// one costs the turn an injected mode plus a CILA level that routes nothing.
///
/// Slash commands are deliberately absent: `<command-args>` carries the user's own
/// words, so those payloads stay classifiable.
/// The list is measured, not guessed: a sample of 800 prompts drawn from 667 real
/// transcripts on 2026-09-22 still carried 148 machine payloads (18.5%) after the first
/// eight prefixes — 134 of them peer messages from another Claude session.
pub const AUTOMATED_PAYLOAD_PREFIXES: &[&str] = &[
    "<task-notification>",
    "[SYSTEM NOTIFICATION",
    "Stop hook feedback",
    "<system-reminder>",
    "<cross-session-message",
    "Another Claude session sent a message",
    "<agent-message",
    "[Cross-session idle notice]",
    "[Cross-session delivery notice]",
    "[Artifact comment sent to Claude]",
    "<local-command-stdout>",
    "<bash-input>",
    "<bash-stdout>",
    "Caveat: The messages below were generated by the user while running local commands",
];

/// True when the payload is machine-generated and carries no user intent.
///
/// Deterministic by design — a prefix check, not a judgement. See
/// [`AUTOMATED_PAYLOAD_PREFIXES`] for what counts and why.
pub fn is_automated_payload(prompt: &str) -> bool {
    let head = prompt.trim_start();
    AUTOMATED_PAYLOAD_PREFIXES
        .iter()
        .any(|prefix| head.starts_with(prefix))
}

/// The response for a payload that is not a prompt: contract preserved, nothing injected.
fn skipped_automated_payload() -> serde_json::Value {
    // S27: no invented field — the reason goes to the trace.
    tracing::debug!(reason = "automated_payload", "prompt_enhance: skipped");
    serde_json::json!({
        "hookSpecificOutput": { "hookEventName": "UserPromptSubmit" },
        "suppressOutput": true,
    })
}

/// S27 (plan 2026-09-25) — the density invariant at the ONE exit of this module: every
/// `<placeholder>` in the context gets the value the prompt yields, or its line is dropped.
/// A template shipped to the model (`touring tantivy search "<topic>"`) is debt, not help
/// (rules/touring-4-pillars.md). Output-structure tags (`<analysis>…</analysis>`) are
/// recognised by their closing tag on the same line and kept.
pub fn densify(context: &str, prompt: &str) -> String {
    context
        .lines()
        .filter_map(|line| fill_placeholders(line, prompt))
        .collect::<Vec<_>>()
        .join("\n")
}

fn fill_placeholders(line: &str, prompt: &str) -> Option<String> {
    let mut out = String::with_capacity(line.len());
    let mut rest = line;
    while let Some(i) = rest.find('<') {
        let tail = &rest[i + 1..];
        let Some(j) = tail.find('>') else { break };
        let name = &tail[..j];
        let is_word = !name.is_empty() && name.chars().all(|c| c.is_ascii_lowercase() || c == '_');
        out.push_str(&rest[..i]);
        if is_word && !line.contains(&format!("</{name}>")) {
            out.push_str(&derive_placeholder(name, prompt)?);
        } else {
            out.push('<');
            out.push_str(name);
            out.push('>');
        }
        rest = &tail[j + 1..];
    }
    out.push_str(rest);
    Some(out)
}

fn derive_placeholder(name: &str, prompt: &str) -> Option<String> {
    match name {
        "topic" | "feature_area" | "keyword_from_task" | "error_keyword" | "relevant_keyword"
        | "query" | "task_keywords" => prompt_keywords(prompt),
        "symbol" | "symbol_name" => prompt_symbol(prompt),
        "file" => prompt_path(prompt),
        _ => None,
    }
}

const STOPWORDS: &[&str] = &[
    "the",
    "and",
    "for",
    "with",
    "that",
    "this",
    "from",
    "into",
    "have",
    "make",
    "please",
    "como",
    "para",
    "com",
    "uma",
    "que",
    "dos",
    "das",
    "nos",
    "nas",
    "pelo",
    "pela",
    "este",
    "esta",
    "isso",
    "fix",
    "add",
    "implement",
    "write",
    "create",
    "explain",
    "plan",
    "refactor",
    "test",
    "tests",
    "corrija",
    "implemente",
    "escreva",
    "crie",
    "explique",
    "planeje",
    "refatore",
];

fn prompt_keywords(prompt: &str) -> Option<String> {
    let words: Vec<String> = prompt
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|w| w.chars().count() >= 4 && !STOPWORDS.contains(&w.to_lowercase().as_str()))
        .take(3)
        .map(str::to_string)
        .collect();
    (!words.is_empty()).then(|| words.join(" "))
}

fn prompt_symbol(prompt: &str) -> Option<String> {
    prompt
        .split(|c: char| !c.is_alphanumeric() && c != '_' && c != ':')
        .find_map(|w| {
            let w = w.trim_matches(':');
            let camel = w.chars().next().is_some_and(char::is_uppercase)
                && w.chars().skip(1).any(char::is_uppercase)
                && w.chars().any(char::is_lowercase);
            let snake = w.contains('_')
                && w.chars()
                    .all(|c| c.is_ascii_lowercase() || c == '_' || c.is_ascii_digit());
            (w.len() >= 3 && (camel || snake || w.contains("::"))).then(|| w.to_string())
        })
}

fn prompt_path(prompt: &str) -> Option<String> {
    const EXT: &[&str] = &[
        "rs", "py", "ts", "tsx", "js", "go", "md", "toml", "json", "yaml", "yml", "sh", "sql",
    ];
    prompt.split_whitespace().find_map(|w| {
        let w = w.trim_matches(|c: char| "\"'`,;()[]".contains(c));
        let ext = w.rsplit_once('.').map(|(_, e)| e)?;
        (EXT.contains(&ext) && w.len() > ext.len() + 1).then(|| w.to_string())
    })
}

/// Compose and return as Claude Code JSON output.
///
/// The output JSON is backward-compatible with the Python `prompt_enhancer.py`
/// contract (`hookSpecificOutput.hookEventName` + `hookSpecificOutput.additionalContext`).
///
/// The classification (`cila_level`, `intent`, `techniques`, `confidence`,
/// `taco_phase_protocol`, `touring_cli_hints`) goes to the trace and to
/// [`compose_details`], never onto the wire (S27): UserPromptSubmit's
/// `hookSpecificOutput` accepts `additionalContext`/`sessionTitle`/`suppressOriginalPrompt` only.
///
/// A machine payload ([`is_automated_payload`]) is answered with the event name, a
/// `skipped` reason and `suppressOutput`, and none of the fields above.
pub fn compose_json(prompt: &str) -> serde_json::Value {
    if is_automated_payload(prompt) {
        journal_skip_automated("compose_json");
        return skipped_automated_payload();
    }

    let (details, latency_ms) = classify_with_details_timed(prompt);
    journal_intent_decision(&details, "compose_json", latency_ms);
    let context = compose(&details.intent, prompt);

    let technique_names: Vec<&str> = details.techniques.iter().map(|t| t.as_str()).collect();

    // TACO Phase Protocol routing based on CILA level
    let taco_phase = taco_phase_for_cila(details.cila_level);
    let touring_hints = touring_cli_hints_for_intent(&details.intent);

    // S27: the metadata goes to the trace, never onto the wire — UserPromptSubmit's
    // hookSpecificOutput accepts additionalContext/sessionTitle/suppressOriginalPrompt only.
    tracing::info!(
        cila_level = details.cila_level,
        intent = details.intent.as_str(),
        techniques = ?technique_names,
        confidence = details.confidence,
        taco_phase = %taco_phase,
        touring_hints = %touring_hints,
        "prompt_enhance: classified"
    );
    serde_json::json!({
        "hookSpecificOutput": {
            "hookEventName": "UserPromptSubmit",
            "additionalContext": densify(&context, prompt),
        }
    })
}

/// The classification behind [`compose_json`] — for diagnostics and tests, never the wire.
/// `Null` for a machine payload (nothing was classified).
/// `pub`, não `#[cfg(test)] pub(crate)`: o consumidor é o `integration_e2e` do
/// crate `touring-hooks` (tests/integration_e2e.rs:534,585) — `cfg(test)` não
/// atravessa a fronteira de crate e `pub(crate)` é invisível fora do runtime
/// (2026-10-02: a medição de "zero consumers" olhou só este crate; E0425 ×2 no
/// workspace test).
pub fn compose_details(prompt: &str) -> serde_json::Value {
    if is_automated_payload(prompt) {
        return serde_json::Value::Null;
    }
    let details = classify_with_details(prompt);
    let technique_names: Vec<&str> = details.techniques.iter().map(|t| t.as_str()).collect();
    serde_json::json!({
        "cila_level": details.cila_level,
        "intent": details.intent.as_str(),
        "techniques": technique_names,
        "confidence": details.confidence,
        "taco_phase_protocol": taco_phase_for_cila(details.cila_level),
        "touring_cli_hints": touring_cli_hints_for_intent(&details.intent),
    })
}

/// Entry point for the `user_prompt_submit` lifecycle hook.
///
/// Receives the raw user prompt (from Claude Code's `hookEventName: "UserPromptSubmit"`
/// payload via `prompt_enhance.rs` compose_json output), enriches it with project
/// attribution and TACO phase routing, and returns a `HookResponse::Context` that
/// injects the enhanced context into the session.
///
/// # Hook Protocol
/// - Event name: `user_prompt_submit` (registered in `hook_registry.rs`)
/// - Emitted by: `prompt_enhance::compose_json` → `cli_prompt_enhance` in `cli_handlers.rs`
/// - Payload: `{"prompt": "...", "session_id": "..."}` (or nested `/input/message`)
/// - Returns: `HookResponse::Context` with enhanced context + project attribution
///
/// # Project Attribution
/// Project attribution is inferred from `HookRuntime` project root, using the same
/// `workspace_roots` pattern as `cli_handlers_index::resolve_workspace`. This enables
/// cross-project memory and ensures context is tagged to the active project.
pub fn run_user_prompt_submit(runtime: &HookRuntime, input: &serde_json::Value) -> HookResponse {
    // Extract prompt from payload (supports nested /input/message format)
    let prompt = input
        .get("prompt")
        .or_else(|| input.pointer("/input/message"))
        .and_then(|v| v.as_str())
        .unwrap_or("");

    if prompt.is_empty() {
        return HookResponse::Allow;
    }

    // A background event is not a prompt: nothing to classify, nothing to inject.
    if is_automated_payload(prompt) {
        journal_skip_automated("hook_entry");
        return HookResponse::Allow;
    }

    // Classify intent + CILA level
    let (details, latency_ms) = classify_with_details_timed(prompt);
    journal_intent_decision(&details, "hook_entry", latency_ms);

    // Build enriched context via compose
    let context = compose(&details.intent, prompt);

    // Project attribution — infer project_dir from runtime's project root
    let project_dir = runtime
        .project_root
        .to_str()
        .map(|s| s.to_string())
        .unwrap_or_default();

    // Build the full enrichment output (same structure as compose_json but HookResponse)
    let additional_context = format!(
        "[PROJECT: {}]\n[PHASE: {} | INTENT: {} | CILA: {}]\n{}",
        project_dir,
        match details.cila_level {
            0 | 1 => "SOLO",
            2 => "L2: Scout → Engineer",
            3 => "L3: Scout → Architect → Engineer → Audit",
            _ => "L4+: Full TACO pipeline",
        },
        details.intent.as_str(),
        details.cila_level,
        context
    );

    HookResponse::Context {
        context: additional_context,
        event_name: Some("user_prompt_submit".to_string()),
    }
}

/// Returns the TACO phase protocol description for a given CILA level.
/// Used by compose_json to embed phase routing information.
fn taco_phase_for_cila(cila_level: u8) -> serde_json::Value {
    match cila_level {
        0 | 1 => serde_json::json!({
            "level": cila_level,
            "mode": "SOLO",
            "description": "TACO resolves directly, no subagents",
            "phases": []
        }),
        2 => serde_json::json!({
            "level": cila_level,
            "mode": "L2",
            "description": "Scout → Engineer → Validate",
            "phases": ["phase_1_scout", "phase_5_engineer", "validate"]
        }),
        3 => serde_json::json!({
            "level": cila_level,
            "mode": "L3",
            "description": "Scout → Architect → Engineer → Audit → Validate",
            "phases": ["phase_1_scout", "phase_2_architect", "phase_5_engineer", "phase_6_audit", "validate"]
        }),
        _ => serde_json::json!({
            "level": cila_level,
            "mode": "L4+",
            "description": "Full TACO pipeline (all phases)",
            "phases": ["phase_0_health_gate", "phase_1_scout", "phase_2_architect", "phase_3_context7", "phase_4_decompose", "phase_4_5_audit", "phase_5_engineer", "phase_6_post_audit", "phase_7_docs"]
        }),
    }
}

/// Returns relevant Touring CLI commands (TIER 1-5) for the given intent.
/// Based on CLI COMMAND RANKS v5.0.
fn touring_cli_hints_for_intent(intent: &Intent) -> serde_json::Value {
    let hints = match intent {
        Intent::Code | Intent::Refactor => vec![
            "touring ast meta <file> --depth summary -j",
            "touring ast blast <file>",
            "touring index find <symbol>",
            "touring wiring audit -j",
            "touring generate verify --symbol <name>",
        ],
        Intent::Debug => vec![
            "touring doctor -j",
            "touring status -j",
            "touring memory recall \"<query>\"",
            "touring e2e -j",
            "touring wiring orphans -j",
        ],
        Intent::Test => vec![
            "cargo test -p <crate>",
            "touring ast meta <file> --depth summary -j",
            "touring wiring audit -j",
        ],
        Intent::Analysis => vec![
            "touring file-knowledge extended <file>",
            "touring tantivy search \"<query>\"",
            "touring ast blast-cross-feature <file>",
            "touring cognitive metrics",
        ],
        Intent::Plan => vec![
            "touring decompose create <type> \"<desc>\"",
            "touring decompose add <task> <sub> [deps]",
            "touring session start <id> type \"<obj>\"",
        ],
        Intent::Creative => vec![
            "touring evolution insights -j",
            "touring evolution drift -j",
            "touring memory recall \"<query>\"",
        ],
        Intent::General => vec!["touring doctor -j", "touring status -j"],
    };

    serde_json::json!({ "tier_1_commands": hints })
}

/// Generate action directives that compile user intent into touring ecosystem commands.
fn action_directives(intent: &Intent, prompt: &str) -> String {
    let lower = prompt.to_lowercase();

    let mut directives = Vec::new();

    // Code-First DISCOVER — always search before acting
    directives.push("## Code-First Directives");
    match intent {
        Intent::Code | Intent::Refactor => {
            directives.push(
                "Before writing any code, DISCOVER existing implementations \
                 (touring is the single source of truth — no external MCP servers):\n\
                 ```bash\n\
                 # 1. BM25/semantic search over the symbol index (<10ms, in-process)\n\
                 touring tantivy search \"<keyword_from_task>\"\n\
                 # 2. Who imports/uses a symbol (transitive consumers, BFS)\n\
                 touring wiring impact <symbol> --depth 2\n\
                 # 3. Real signature + module path + line (zero-hallucination)\n\
                 touring ast find <symbol> -j\n\
                 ```\n\
                 VIOLATION = writing code without DISCOVER first.",
            );
        }
        Intent::Debug => {
            directives.push(
                "Before proposing a fix, trace the actual code path (touring-native):\n\
                 ```bash\n\
                 # 1. Find the symbol where the error occurs\n\
                 touring tantivy search \"<error_keyword>\"\n\
                 # 2. Real signature + the file:line to Read (never guess)\n\
                 touring ast find <symbol> -j\n\
                 # 3. Check who calls this (blast radius, transitive)\n\
                 touring wiring impact <symbol> --depth 2\n\
                 ```\n\
                 VIOLATION = proposing fix without reading the actual code.",
            );
        }
        Intent::Test => {
            directives.push(
                "Before writing tests, discover existing test patterns:\n\
                 ```bash\n\
                 touring tantivy search \"test_<module>\"\n\
                 # Check existing fixtures and helpers in a test file\n\
                 touring ast overview tests/<file> -j\n\
                 ```",
            );
        }
        Intent::Analysis => {
            directives.push(
                "Before analyzing, gather ground truth:\n\
                 ```bash\n\
                 touring tantivy search \"<topic>\"\n\
                 touring wiring chains   # architecture: source→sink module relationships\n\
                 ```\n\
                 Tag claims: FACT [1.0] | INFERENCE [0.7-0.9] | SPECULATION [<0.7]",
            );
        }
        Intent::Plan => {
            directives.push(
                "Before planning, map the terrain:\n\
                 ```bash\n\
                 # 1. Discover what exists\n\
                 touring tantivy search \"<feature_area>\"\n\
                 # 2. Check blast radius of proposed changes\n\
                 touring ast blast <file>   # or: touring wiring impact <symbol> --depth 2\n\
                 # 3. Enter plan mode\n\
                 # Use EnterPlanMode tool for non-trivial tasks (3+ steps)\n\
                 ```",
            );
        }
        _ => {
            directives.push(
                "Verify before asserting:\n\
                 ```bash\n\
                 touring tantivy search \"<relevant_keyword>\"\n\
                 ```",
            );
        }
    }

    // Detect specific touring-cli opportunities from prompt content
    let mut cli_hints = Vec::new();

    // CLI hints via const lookup table — O(n*m) vs 17 sequential if-chains (CC 70→12)
    // Each entry: (keywords_slice, hint_text). If ANY keyword matches → push hint.
    const CLI_HINTS: &[(&[&str], &str)] = &[
        (
            &["symbol", "function", "class"],
            "- `touring index find <symbol_name> -j` — exact symbol lookup (<10ms, in-process)",
        ),
        (
            &["blast", "impact", "refactor"],
            "- `touring wiring impact <symbol> --depth 2` — blast radius before any change",
        ),
        (
            &["drift", "compliance"],
            "- `touring cortex PreToolUse` — drift/compliance check",
        ),
        (
            &["performance", "benchmark"],
            "- `cargo bench -p <crate>` — Criterion benchmarks",
        ),
        (
            &["test", "pytest", "coverage"],
            "- `cargo test -p <crate>` / `pytest --cov` — validate before claiming done",
        ),
        (
            &["deploy", "release", "build"],
            "- `cargo build --release -p touring-server` — rebuild touring binary",
        ),
        (
            &["hook", "pre-read", "post-edit"],
            "- `touring <hook-name>` — native hook execution (<5ms)",
        ),
        (
            &["file metadata", "quality", "complexity"],
            "- `touring ast meta <file> --depth summary` — file metadata first (blast + quality)",
        ),
        (
            &["wiring", "orphan", "integration"],
            "- `touring wiring audit` — orphans + low-score modules\n- `touring wiring orphans` — find orphan pub symbols",
        ),
        (
            &["memory", "lesson", "pattern"],
            "- `touring memory recall \"<query>\"` — search lessons/patterns\n- `touring memory store <key> <value> --tier semantic --type lesson`",
        ),
        (
            &["decompose", "task", "dag", "subtask"],
            "- `touring decompose create <type> <desc>` — create task DAG\n- `touring decompose add <task_id> <subtask_id> \"<desc>\"`",
        ),
        (
            &["generate", "create module", "scaffold"],
            "- `touring generate list-kinds` — list 30 generator kinds\n- `touring generate verify --symbol <name>` — VGP symbol verification\n- `touring generate schema-dump` — GeneratorPlan JSON Schema",
        ),
        (
            &["e2e", "end-to-end", "health"],
            "- `touring e2e --depth quick` — index + wiring only (~50ms)\n- `touring e2e --depth standard` — + AST + quality (~500ms)\n- `touring doctor -j` — daemon health check",
        ),
        (
            &["status", "dashboard"],
            "- `touring status -j` — unified dashboard",
        ),
        (
            &["search"],
            "- `touring tantivy search \"<query>\"` — BM25 ranked search\n- `touring tantivy fuzzy \"<query>\" 2` — fuzzy with edit distance\n- `touring query \"lang = rust AND loc > 100\"` — DSL query",
        ),
        (
            &["mcts", "planning", "multi-path"],
            "- `touring mcts search [root_state]` — Monte Carlo Tree Search",
        ),
        (
            &["cognitive", "metrics"],
            "- `touring cognitive metrics` — engine metrics",
        ),
        (
            &["scip", "export symbol"],
            "- `touring scip emit <file>` — SCIP-compatible export",
        ),
        (
            &["skill"],
            "- `Skill(\"Touring\")` — invoke Touring skill for code intelligence",
        ),
    ];

    for (keywords, hint) in CLI_HINTS {
        if keywords.iter().any(|kw| keyword_matches(&lower, kw)) {
            cli_hints.push(*hint);
        }
    }

    // Touring skill activation — hint when touring keywords detected
    let touring_keywords = [
        "touring",
        "vgp",
        "wiring audit",
        "orphan",
        "blast radius",
        "file metadata",
        "tantivy",
        "decompose",
        "memory recall",
        "evolution drift",
        "cognitive",
        "mcts",
        "diary",
        "gotcha",
        "inferlet",
        "session",
        "checkpoint",
    ];
    let touring_kw_detected = touring_keywords.iter().any(|kw| lower.contains(kw));
    if touring_kw_detected {
        directives.push("\n## Touring Skill Activation");
        directives.push(
            "Invoke the **Touring skill** (`Skill(\"Touring\")`) for:\n\
             - Code intelligence (AST/index/wiring analysis)\n\
             - File metadata with blast radius + quality scores\n\
             - VGP symbol verification before code generation\n\
             - Memory persistence (lessons, patterns)\n\
             - Task decomposition into validated DAGs\n\
             - RL-guided suggestions + evolution drift detection\n\
             - BM25/Tantivy full-text search over symbols\n\
             - 120+ touring CLI commands + 85 MCP tools",
        );
    }

    if !cli_hints.is_empty() {
        directives.push("\n## Touring CLI Hints");
        for hint in &cli_hints {
            directives.push(hint);
        }
    }

    // VGP (Verified Generation Protocol) — verify before generating
    directives.push("\n## VGP — Verified Generation Protocol");
    match intent {
        Intent::Code | Intent::Refactor => {
            directives.push(
                "V1(extract): `touring_ast_find` before referencing any `struct.field`\n\
                 V2(verify): Read real source — never infer signatures\n\
                 V3(blast_radius): `touring wiring impact <symbol> --depth 2` before editing symbols with ≥2 callers\n\
                 V4(cache): Reuse DISCOVER results within session",
            );
        }
        Intent::Debug => {
            directives.push(
                "V1(extract): Read the actual stack trace / error message\n\
                 V2(verify): `find_symbol` with `include_body=True` on the failing symbol\n\
                 V3(blast_radius): Check who calls the failing function before fixing\n\
                 V4(cache): Record fix pattern for future error prediction",
            );
        }
        _ => {
            directives.push(
                "V1(extract): Gather ground truth before analysis\n\
                 V2(verify): Cross-reference claims with source code",
            );
        }
    }

    directives.join("\n")
}

// ---------------------------------------------------------------------------
// Templates — const table indexed by (tech_idx, intent_idx)
// ---------------------------------------------------------------------------

/// Number of techniques and intents (for table indexing).
const N_TECHNIQUES: usize = 6;
const N_INTENTS: usize = 8;

/// TECH_HEADINGS[i] = heading for technique at index i.
const TECH_HEADINGS: [&str; N_TECHNIQUES] = [
    "Chain Of Thought",
    "Constitutional Constraints",
    "Structured Output",
    "Few Shot Reasoning",
    "Self Validation",
    "Precision Hints",
];

/// TECH_IDX[t] = index of technique t in the template table.
fn tech_idx(tech: &Technique) -> usize {
    match tech {
        Technique::ChainOfThought => 0,
        Technique::ConstitutionalConstraints => 1,
        Technique::StructuredOutput => 2,
        Technique::FewShotReasoning => 3,
        Technique::SelfValidation => 4,
        Technique::PrecisionHints => 5,
    }
}

/// INTENT_IDX[i] = index of intent i in the template table.
fn intent_idx(intent: &Intent) -> usize {
    match intent {
        Intent::General => 0,
        Intent::Code => 1,
        Intent::Debug => 2,
        Intent::Refactor => 3,
        Intent::Test => 4,
        Intent::Analysis => 5,
        Intent::Creative => 6,
        Intent::Plan => 7,
    }
}

/// Template table: TEMPLATES[tech_idx][intent_idx] = template string.
/// Unused combinations fall back to the wildcard "_" entry for that technique.
const TEMPLATES: [[&str; N_INTENTS]; N_TECHNIQUES] = [
    /* ChainOfThought [0] */
    [
        // General [0]
        "Think step-by-step before producing output. Decompose the problem into \
         sub-problems, solve each one explicitly, then synthesize the final answer. \
         Show your reasoning chain so the user can follow and verify each step.",
        // Code [1]
        "Think step-by-step before producing output. Decompose the problem into \
         sub-problems, solve each one explicitly, then synthesize the final answer. \
         Show your reasoning chain so the user can follow and verify each step.",
        // Debug [2]
        "Diagnose step-by-step: (1) reproduce the symptom mentally from the \
         description, (2) enumerate possible root causes ranked by likelihood, \
         (3) for the top candidate, trace the code path that triggers the fault, \
         (4) propose the minimal fix and explain why it resolves the root cause \
         without side effects.",
        // Refactor [3]
        "Analyze step-by-step: (1) identify the specific code smell or violation, \
         (2) state which SOLID principle or design pattern applies, (3) describe \
         the target structure after refactoring, (4) list the exact transformations \
         required in order, (5) confirm behavior equivalence.",
        // Test [4]
        "Think step-by-step before producing output. Decompose the problem into \
         sub-problems, solve each one explicitly, then synthesize the final answer. \
         Show your reasoning chain so the user can follow and verify each step.",
        // Analysis [5]
        "Think step-by-step before producing output. Decompose the problem into \
         sub-problems, solve each one explicitly, then synthesize the final answer. \
         Show your reasoning chain so the user can follow and verify each step.",
        // Creative [6]
        "Think step-by-step before producing output. Decompose the problem into \
         sub-problems, solve each one explicitly, then synthesize the final answer. \
         Show your reasoning chain so the user can follow and verify each step.",
        // Plan [7]
        "Plan step-by-step: (1) clarify scope and success criteria, (2) identify \
         dependencies and blockers, (3) break work into atomic deliverables, \
         (4) estimate effort per deliverable using T-shirt sizing (S/M/L/XL), \
         (5) sequence deliverables respecting dependencies.",
    ],
    /* ConstitutionalConstraints [1] */
    [
        // General [0]
        "Constraints: never introduce secrets or credentials in code. Validate all \
         inputs. Prefer standard library solutions before external dependencies. \
         Follow PEP 8 and project conventions. Keep changes minimal in scope.",
        // Code [1]
        "CODE constraints: add type hints to every function signature. Include \
         docstrings on all public functions. Handle errors with specific exception \
         types, never bare except. Never hardcode secrets, paths, or credentials. \
         Follow Single Responsibility Principle per function.",
        // Debug [2]
        "DEBUG constraints: never modify code you have not fully read and \
         understood. Preserve existing tests. Fix the root cause, not symptoms. \
         If the fix is uncertain, add a regression test first. Never silence \
         errors without explicit justification.",
        // Refactor [3]
        "REFACTOR constraints: full test suite must pass before AND after. Never \
         change behavior — only structure. Preserve the public API contract. \
         Each commit should be a single, reviewable transformation.",
        // Test [4]
        "TEST constraints: each test must be independent and idempotent. Use \
         descriptive test names that state the expected behavior. Mock external \
         dependencies, never hit real APIs. Cover happy path, edge cases, and \
         error paths. Target >90% branch coverage for new code.",
        // Analysis [5]
        "Constraints: never introduce secrets or credentials in code. Validate all \
         inputs. Prefer standard library solutions before external dependencies. \
         Follow PEP 8 and project conventions. Keep changes minimal in scope.",
        // Creative [6]
        "Constraints: never introduce secrets or credentials in code. Validate all \
         inputs. Prefer standard library solutions before external dependencies. \
         Follow PEP 8 and project conventions. Keep changes minimal in scope.",
        // Plan [7]
        "Constraints: never introduce secrets or credentials in code. Validate all \
         inputs. Prefer standard library solutions before external dependencies. \
         Follow PEP 8 and project conventions. Keep changes minimal in scope.",
    ],
    /* StructuredOutput [2] */
    [
        // General [0]
        "When producing structured artifacts, use clear delimiters. For code blocks \
         use language-tagged fences. For multi-part responses use XML-style tags: \
         <analysis>...</analysis>, <implementation>...</implementation>, \
         <verification>...</verification>.",
        // Code [1]
        "Structure your output as: <design> architectural decisions and trade-offs \
         </design>, <implementation> the complete code </implementation>, \
         <tests> key test cases </tests>, <usage> example usage </usage>.",
        // Debug [2]
        "When producing structured artifacts, use clear delimiters. For code blocks \
         use language-tagged fences. For multi-part responses use XML-style tags: \
         <analysis>...</analysis>, <implementation>...</implementation>, \
         <verification>...</verification>.",
        // Refactor [3]
        "When producing structured artifacts, use clear delimiters. For code blocks \
         use language-tagged fences. For multi-part responses use XML-style tags: \
         <analysis>...</analysis>, <implementation>...</implementation>, \
         <verification>...</verification>.",
        // Test [4]
        "Structure your test output as: <strategy> describe what to test and why \
         </strategy>, <fixtures> any shared setup or factories </fixtures>, \
         <tests> the complete test code </tests>, <coverage> list which branches \
         and edge cases are covered </coverage>.",
        // Analysis [5]
        "When producing structured artifacts, use clear delimiters. For code blocks \
         use language-tagged fences. For multi-part responses use XML-style tags: \
         <analysis>...</analysis>, <implementation>...</implementation>, \
         <verification>...</verification>.",
        // Creative [6]
        "When producing structured artifacts, use clear delimiters. For code blocks \
         use language-tagged fences. For multi-part responses use XML-style tags: \
         <analysis>...</analysis>, <implementation>...</implementation>, \
         <verification>...</verification>.",
        // Plan [7]
        "Structure your plan as: <objective> what and why </objective>, \
         <deliverables> numbered list </deliverables>, <timeline> sequenced with \
         dependencies </timeline>, <risks> what could go wrong and mitigations \
         </risks>.",
    ],
    /* FewShotReasoning [3] */
    [
        // General [0]
        "Include a worked example showing INPUT -> REASONING -> OUTPUT to demonstrate \
         the expected approach before solving the actual problem.",
        // Code [1]
        "Include a worked example showing INPUT -> REASONING -> OUTPUT to demonstrate \
         the expected approach before solving the actual problem.",
        // Debug [2]
        "Include a diagnostic trace example: SYMPTOM (what the user sees) -> \
         HYPOTHESIS (ranked possible causes) -> INVESTIGATION (code path traced) -> \
         ROOT CAUSE (specific line/condition) -> FIX (minimal change). Example: \
         SYMPTOM \"KeyError on line 42\" -> HYPOTHESIS \"dict key missing because \
         upstream filter dropped the record\" -> FIX \"add .get() with default and \
         log warning\".",
        // Refactor [3]
        "Include a worked example showing INPUT -> REASONING -> OUTPUT to demonstrate \
         the expected approach before solving the actual problem.",
        // Test [4]
        "Include a test design trace: REQUIREMENT (what behavior to verify) -> \
         BOUNDARY (edge conditions identified) -> TEST CASE (specific input -> \
         expected output with assertion). Example: REQUIREMENT \"parse_date handles \
         ISO 8601\" -> BOUNDARY \"empty string, None, timezone offset\" -> TEST CASE \
         \"parse_date('') raises ValueError with message containing 'empty'\".",
        // Analysis [5]
        "Include a worked example showing INPUT -> REASONING -> OUTPUT to demonstrate \
         the expected approach before solving the actual problem.",
        // Creative [6]
        "Include a worked example showing INPUT -> REASONING -> OUTPUT to demonstrate \
         the expected approach before solving the actual problem.",
        // Plan [7]
        "Include a worked example showing INPUT -> REASONING -> OUTPUT to demonstrate \
         the expected approach before solving the actual problem.",
    ],
    /* SelfValidation [4] */
    [
        // General [0]
        "Before finalizing, re-read the requirements, verify your output matches, \
         check for logical errors, and trace execution with a concrete example.",
        // Code [1]
        "Before finalizing, re-read the requirements, verify your output matches, \
         check for logical errors, and trace execution with a concrete example.",
        // Debug [2]
        "After proposing a fix, validate it: (1) does the fix address the root cause \
         or just a symptom? (2) mentally execute the fixed code path with the failing \
         input, (3) check for regressions — does the fix break any other code path? \
         (4) confirm the fix is minimal and does not introduce new dependencies.",
        // Refactor [3]
        "After refactoring, validate: (1) public API is unchanged, (2) test suite \
         passes, (3) complexity is reduced (measured), (4) no new dependencies \
         introduced.",
        // Test [4]
        "Before finalizing, re-read the requirements, verify your output matches, \
         check for logical errors, and trace execution with a concrete example.",
        // Analysis [5]
        "Before finalizing, re-read the requirements, verify your output matches, \
         check for logical errors, and trace execution with a concrete example.",
        // Creative [6]
        "Before finalizing, re-read the requirements, verify your output matches, \
         check for logical errors, and trace execution with a concrete example.",
        // Plan [7]
        "After creating the plan, validate: (1) each deliverable is atomic and \
         independently shippable, (2) dependencies are explicit and acyclic, \
         (3) estimates are realistic, (4) risks have mitigations.",
    ],
    /* PrecisionHints [5] */
    [
        // General [0]
        "Calibrate output precision to the task. Use confidence indicators on a \
         0.0-1.0 scale when making uncertain claims. For code, target precision \
         level 0.95+ (production-ready). For analysis, clearly separate facts (1.0) \
         from inferences (0.7-0.9) from speculation (< 0.7).",
        // Code [1]
        "Calibrate output precision to the task. Use confidence indicators on a \
         0.0-1.0 scale when making uncertain claims. For code, target precision \
         level 0.95+ (production-ready). For analysis, clearly separate facts (1.0) \
         from inferences (0.7-0.9) from speculation (< 0.7).",
        // Debug [2]
        "Calibrate output precision to the task. Use confidence indicators on a \
         0.0-1.0 scale when making uncertain claims. For code, target precision \
         level 0.95+ (production-ready). For analysis, clearly separate facts (1.0) \
         from inferences (0.7-0.9) from speculation (< 0.7).",
        // Refactor [3]
        "Calibrate output precision to the task. Use confidence indicators on a \
         0.0-1.0 scale when making uncertain claims. For code, target precision \
         level 0.95+ (production-ready). For analysis, clearly separate facts (1.0) \
         from inferences (0.7-0.9) from speculation (< 0.7).",
        // Test [4]
        "Calibrate output precision to the task. Use confidence indicators on a \
         0.0-1.0 scale when making uncertain claims. For code, target precision \
         level 0.95+ (production-ready). For analysis, clearly facts (1.0) \
         from inferences (0.7-0.9) from speculation (< 0.7).",
        // Analysis [5]
        "Analysis precision: tag each claim with confidence. FACT [1.0] for verified \
         information. INFERENCE [0.7-0.9] for conclusions drawn from evidence. \
         SPECULATION [<0.7] for hypotheses needing verification. Always state what \
         evidence would change your assessment.",
        // Creative [6]
        "Creative precision: produce at least 3 distinct alternatives ranked by \
         feasibility. For each, state trade-offs and confidence in success.",
        // Plan [7]
        "Planning precision: use T-shirt sizing (S/M/L/XL) for estimates. Tag \
         risks as LOW/MEDIUM/HIGH with probability and impact.",
    ],
];

/// Heading for a technique.
fn tech_heading(tech: &Technique) -> &'static str {
    TECH_HEADINGS.get(tech_idx(tech)).copied().unwrap_or("")
}

/// Get the template string for a technique + intent combination.
/// Uses the const TEMPLATES table for O(1) lookup instead of 48 match arms.
fn template_for(tech: &Technique, intent: &Intent) -> &'static str {
    TEMPLATES
        .get(tech_idx(tech))
        .and_then(|row| row.get(intent_idx(intent)))
        .copied()
        .unwrap_or("")
}

// ---------------------------------------------------------------------------
// Keywords (bilingual EN + PT-BR)
// ---------------------------------------------------------------------------

fn keywords_for(intent: &Intent) -> &'static [KeywordEntry] {
    match intent {
        Intent::Code => &[
            KeywordEntry {
                keyword: "function",
                weight: 2.0,
            },
            KeywordEntry {
                keyword: "class",
                weight: 2.0,
            },
            KeywordEntry {
                keyword: "implement",
                weight: 2.0,
            },
            KeywordEntry {
                keyword: "build",
                weight: 1.0,
            },
            KeywordEntry {
                keyword: "add",
                weight: 1.0,
            },
            KeywordEntry {
                keyword: "endpoint",
                weight: 2.0,
            },
            KeywordEntry {
                keyword: "api",
                weight: 1.0,
            },
            KeywordEntry {
                keyword: "module",
                weight: 1.0,
            },
            KeywordEntry {
                keyword: "component",
                weight: 1.0,
            },
            KeywordEntry {
                keyword: "script",
                weight: 1.0,
            },
            KeywordEntry {
                keyword: "programa",
                weight: 2.0,
            },
            KeywordEntry {
                keyword: "implemente",
                weight: 2.0,
            },
            KeywordEntry {
                keyword: "desenvolva",
                weight: 2.0,
            },
            KeywordEntry {
                keyword: "construa",
                weight: 2.0,
            },
            KeywordEntry {
                keyword: "escreva",
                weight: 1.0,
            },
            KeywordEntry {
                keyword: "crie",
                weight: 1.0,
            },
            KeywordEntry {
                keyword: "adicione",
                weight: 1.0,
            },
            KeywordEntry {
                keyword: "handler",
                weight: 2.0,
            },
            KeywordEntry {
                keyword: "middleware",
                weight: 2.0,
            },
            KeywordEntry {
                keyword: "decorator",
                weight: 2.0,
            },
            KeywordEntry {
                keyword: "cli",
                weight: 2.0,
            },
        ],
        Intent::Debug => &[
            KeywordEntry {
                keyword: "error",
                weight: 2.0,
            },
            KeywordEntry {
                keyword: "bug",
                weight: 2.0,
            },
            KeywordEntry {
                keyword: "fix",
                weight: 2.0,
            },
            KeywordEntry {
                keyword: "issue",
                weight: 1.0,
            },
            KeywordEntry {
                keyword: "broken",
                weight: 2.0,
            },
            KeywordEntry {
                keyword: "fails",
                weight: 1.0,
            },
            KeywordEntry {
                keyword: "crash",
                weight: 2.0,
            },
            KeywordEntry {
                keyword: "crashes",
                weight: 2.0,
            },
            KeywordEntry {
                keyword: "failing",
                weight: 2.0,
            },
            KeywordEntry {
                keyword: "exception",
                weight: 2.0,
            },
            KeywordEntry {
                keyword: "traceback",
                weight: 2.0,
            },
            KeywordEntry {
                keyword: "stack",
                weight: 1.0,
            },
            KeywordEntry {
                keyword: "debug",
                weight: 2.0,
            },
            KeywordEntry {
                keyword: "not working",
                weight: 3.0,
            },
            KeywordEntry {
                keyword: "corrija",
                weight: 2.0,
            },
            KeywordEntry {
                keyword: "conserte",
                weight: 2.0,
            },
            KeywordEntry {
                keyword: "erro",
                weight: 1.0,
            },
            KeywordEntry {
                keyword: "falha",
                weight: 2.0,
            },
            KeywordEntry {
                keyword: "quebrado",
                weight: 2.0,
            },
            KeywordEntry {
                keyword: "problema",
                weight: 1.0,
            },
            KeywordEntry {
                keyword: "typeerror",
                weight: 2.0,
            },
            KeywordEntry {
                keyword: "keyerror",
                weight: 2.0,
            },
            KeywordEntry {
                keyword: "valueerror",
                weight: 2.0,
            },
            KeywordEntry {
                keyword: "attributeerror",
                weight: 2.0,
            },
            KeywordEntry {
                keyword: "runtimeerror",
                weight: 2.0,
            },
            KeywordEntry {
                keyword: "importerror",
                weight: 2.0,
            },
            KeywordEntry {
                keyword: "returns none",
                weight: 3.0,
            },
            KeywordEntry {
                keyword: "retornando none",
                weight: 3.0,
            },
            KeywordEntry {
                keyword: "memory leak",
                weight: 3.0,
            },
            KeywordEntry {
                keyword: "track down",
                weight: 3.0,
            },
            KeywordEntry {
                keyword: "null pointer",
                weight: 3.0,
            },
            KeywordEntry {
                keyword: "undefined",
                weight: 1.0,
            },
            KeywordEntry {
                keyword: "unexpected",
                weight: 1.0,
            },
            KeywordEntry {
                keyword: "inesperado",
                weight: 1.0,
            },
        ],
        Intent::Refactor => &[
            KeywordEntry {
                keyword: "refactor",
                weight: 2.0,
            },
            KeywordEntry {
                keyword: "clean",
                weight: 1.0,
            },
            KeywordEntry {
                keyword: "simplify",
                weight: 2.0,
            },
            KeywordEntry {
                keyword: "restructure",
                weight: 2.0,
            },
            KeywordEntry {
                keyword: "improve",
                weight: 1.0,
            },
            KeywordEntry {
                keyword: "optimize",
                weight: 1.0,
            },
            KeywordEntry {
                keyword: "rename",
                weight: 2.0,
            },
            KeywordEntry {
                keyword: "extract",
                weight: 2.0,
            },
            KeywordEntry {
                keyword: "move",
                weight: 1.0,
            },
            KeywordEntry {
                keyword: "decouple",
                weight: 2.0,
            },
            KeywordEntry {
                keyword: "solid",
                weight: 2.0,
            },
            KeywordEntry {
                keyword: "refatore",
                weight: 2.0,
            },
            KeywordEntry {
                keyword: "limpe",
                weight: 2.0,
            },
            KeywordEntry {
                keyword: "simplifique",
                weight: 2.0,
            },
            KeywordEntry {
                keyword: "melhore",
                weight: 2.0,
            },
            KeywordEntry {
                keyword: "otimize",
                weight: 2.0,
            },
            KeywordEntry {
                keyword: "reorganize",
                weight: 2.0,
            },
            KeywordEntry {
                keyword: "decompose",
                weight: 2.0,
            },
        ],
        Intent::Test => &[
            KeywordEntry {
                keyword: "test",
                weight: 2.0,
            },
            KeywordEntry {
                keyword: "tests",
                weight: 2.0,
            },
            KeywordEntry {
                keyword: "spec",
                weight: 1.0,
            },
            KeywordEntry {
                keyword: "pytest",
                weight: 2.0,
            },
            KeywordEntry {
                keyword: "coverage",
                weight: 2.0,
            },
            KeywordEntry {
                keyword: "assert",
                weight: 1.0,
            },
            KeywordEntry {
                keyword: "mock",
                weight: 1.0,
            },
            KeywordEntry {
                keyword: "fixture",
                weight: 2.0,
            },
            KeywordEntry {
                keyword: "unittest",
                weight: 2.0,
            },
            KeywordEntry {
                keyword: "tdd",
                weight: 2.0,
            },
            KeywordEntry {
                keyword: "teste",
                weight: 2.0,
            },
            KeywordEntry {
                keyword: "testes",
                weight: 2.0,
            },
            KeywordEntry {
                keyword: "cobertura",
                weight: 2.0,
            },
            KeywordEntry {
                keyword: "unit test",
                weight: 3.0,
            },
            KeywordEntry {
                keyword: "unit tests",
                weight: 3.0,
            },
            KeywordEntry {
                keyword: "integration test",
                weight: 3.0,
            },
            KeywordEntry {
                keyword: "test suite",
                weight: 3.0,
            },
            KeywordEntry {
                keyword: "test cases",
                weight: 3.0,
            },
        ],
        Intent::Analysis => &[
            KeywordEntry {
                keyword: "explain",
                weight: 2.0,
            },
            KeywordEntry {
                keyword: "analyze",
                weight: 2.0,
            },
            KeywordEntry {
                keyword: "review",
                weight: 1.0,
            },
            KeywordEntry {
                keyword: "understand",
                weight: 1.0,
            },
            KeywordEntry {
                keyword: "how does",
                weight: 3.0,
            },
            KeywordEntry {
                keyword: "how is",
                weight: 3.0,
            },
            KeywordEntry {
                keyword: "how do",
                weight: 3.0,
            },
            KeywordEntry {
                keyword: "what is",
                weight: 1.0,
            },
            KeywordEntry {
                keyword: "what does",
                weight: 3.0,
            },
            KeywordEntry {
                keyword: "why does",
                weight: 3.0,
            },
            KeywordEntry {
                keyword: "describe",
                weight: 2.0,
            },
            KeywordEntry {
                keyword: "diagram",
                weight: 2.0,
            },
            KeywordEntry {
                keyword: "explique",
                weight: 2.0,
            },
            KeywordEntry {
                keyword: "analise",
                weight: 2.0,
            },
            KeywordEntry {
                keyword: "revise",
                weight: 2.0,
            },
            KeywordEntry {
                keyword: "entenda",
                weight: 2.0,
            },
            KeywordEntry {
                keyword: "como funciona",
                weight: 3.0,
            },
            KeywordEntry {
                keyword: "walk through",
                weight: 3.0,
            },
            KeywordEntry {
                keyword: "implications",
                weight: 2.0,
            },
            KeywordEntry {
                keyword: "bottleneck",
                weight: 2.0,
            },
            KeywordEntry {
                keyword: "gargalo",
                weight: 2.0,
            },
        ],
        Intent::Creative => &[
            KeywordEntry {
                keyword: "brainstorm",
                weight: 2.0,
            },
            KeywordEntry {
                keyword: "suggest",
                weight: 1.0,
            },
            KeywordEntry {
                keyword: "idea",
                weight: 2.0,
            },
            KeywordEntry {
                keyword: "approach",
                weight: 1.0,
            },
            KeywordEntry {
                keyword: "alternative",
                weight: 1.0,
            },
            KeywordEntry {
                keyword: "propose",
                weight: 2.0,
            },
            KeywordEntry {
                keyword: "proponha",
                weight: 2.0,
            },
            KeywordEntry {
                keyword: "sugira",
                weight: 2.0,
            },
            KeywordEntry {
                keyword: "ideia",
                weight: 2.0,
            },
            KeywordEntry {
                keyword: "alternativa",
                weight: 2.0,
            },
            KeywordEntry {
                keyword: "estrategia",
                weight: 2.0,
            },
            KeywordEntry {
                keyword: "blog post",
                weight: 3.0,
            },
            KeywordEntry {
                keyword: "documentation",
                weight: 1.0,
            },
            KeywordEntry {
                keyword: "draft",
                weight: 2.0,
            },
        ],
        Intent::Plan => &[
            KeywordEntry {
                keyword: "plan",
                weight: 2.0,
            },
            KeywordEntry {
                keyword: "roadmap",
                weight: 2.0,
            },
            KeywordEntry {
                keyword: "steps",
                weight: 1.0,
            },
            KeywordEntry {
                keyword: "milestone",
                weight: 2.0,
            },
            KeywordEntry {
                keyword: "phase",
                weight: 1.0,
            },
            KeywordEntry {
                keyword: "breakdown",
                weight: 2.0,
            },
            KeywordEntry {
                keyword: "estimate",
                weight: 1.0,
            },
            KeywordEntry {
                keyword: "planeje",
                weight: 2.0,
            },
            KeywordEntry {
                keyword: "plano",
                weight: 2.0,
            },
            KeywordEntry {
                keyword: "etapas",
                weight: 2.0,
            },
            KeywordEntry {
                keyword: "fases",
                weight: 2.0,
            },
            KeywordEntry {
                keyword: "migration",
                weight: 1.0,
            },
            KeywordEntry {
                keyword: "projete",
                weight: 2.0,
            },
            KeywordEntry {
                keyword: "estruturar",
                weight: 2.0,
            },
            KeywordEntry {
                keyword: "architect",
                weight: 2.0,
            },
            KeywordEntry {
                keyword: "design the system",
                weight: 3.0,
            },
            KeywordEntry {
                keyword: "design system",
                weight: 3.0,
            },
            KeywordEntry {
                keyword: "implementation plan",
                weight: 3.0,
            },
            KeywordEntry {
                keyword: "technical roadmap",
                weight: 3.0,
            },
        ],
        Intent::General => &[],
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[path = "prompt_enhance_tests.rs"]
mod tests;
