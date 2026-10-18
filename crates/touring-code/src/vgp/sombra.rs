//! VGP shadow glue — the sombra.jsonl emitter every pre-hook shares (V2/V5).
//!
//! Contract (coordenadora, adendo 09/10 — what the V4 judge reads):
//! `~/.claude/touring/vgp/sombra.jsonl`, append + fsync, one line per hook check:
//! {"ts": ISO-Z ms, "session_id", "tool_use_id", "hook", "project_root", "path",
//!  "balde": "arquivo|tmp|inline", "camadas": [1], "modo": "sombra|ativo", "verdict",
//!  "elapsed_ms", "findings": [{classe,nome,linha,severidade,escopo,sugestao,reais≤20}],
//!  "motivo": só quando unknown}
//! No file content ever (size + privacy). Mode resolution: `TOURING_VGP_DISABLED=1` kills it;
//! `TOURING_VGP_MODE` beats the file; `.touring/touring.toml [vgp] modo = "sombra"|"ativo"|"off"`;
//! code default is **sombra** — "ativo" is only ever a project's choice (V4), never a code default.
//! Fail-open end to end: any internal error degrades to verdict "unknown" with `motivo`, and the
//! hook's own answer is NEVER touched.

use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use serde::Serialize;

use super::{CheckInput, CheckOutput, Finding, check};

/// The resolved mode for a project.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Disabled (kill switch or `[vgp] modo = "off"`).
    Off,
    /// Log only — the hook's response never changes (the code default, V2/V3).
    Sombra,
    /// Log + the response follows the motor (`[VGP · deny] ` / `[VGP · aviso] `) — V4, project's choice.
    Ativo,
}

impl Mode {
    fn as_str(self) -> &'static str {
        match self {
            Mode::Off => "off",
            Mode::Sombra => "sombra",
            Mode::Ativo => "ativo",
        }
    }
}

/// Kill switch humano + override de env + `[vgp] modo` do projeto, env > toml > default sombra.
pub fn vgp_mode(project_root: &Path) -> Mode {
    if std::env::var_os("TOURING_VGP_DISABLED")
        .map(|v| !v.is_empty() && v != "0")
        .unwrap_or(false)
    {
        return Mode::Off;
    }
    if let Some(m) = std::env::var("TOURING_VGP_MODE")
        .ok()
        .and_then(|v| parse_mode(&v))
    {
        return m;
    }
    static VERDICT: OnceLock<Mutex<HashMap<PathBuf, Mode>>> = OnceLock::new();
    let cache = VERDICT.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(m) = cache
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(project_root)
    {
        return *m;
    }
    let text = std::fs::read_to_string(project_root.join(".touring/touring.toml")).ok();
    let mode = text
        .as_deref()
        .and_then(toml_declares_mode)
        .unwrap_or(Mode::Sombra);
    cache
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(project_root.to_path_buf(), mode);
    mode
}

fn parse_mode(raw: &str) -> Option<Mode> {
    match raw.trim().trim_matches('"') {
        "off" | "0" | "false" => Some(Mode::Off),
        "sombra" | "shadow" => Some(Mode::Sombra),
        "ativo" | "active" | "deny" => Some(Mode::Ativo),
        _ => None,
    }
}

/// `[vgp] modo = "…"` from a toml text — line scan with section state (same stance as r3_shadow:
/// neither side pulls a TOML parser for one key).
fn toml_declares_mode(text: &str) -> Option<Mode> {
    let mut in_vgp = false;
    for line in text.lines() {
        let l = line.trim();
        if l.starts_with('[') {
            in_vgp = l == "[vgp]";
            continue;
        }
        if !in_vgp {
            continue;
        }
        let Some((key, value)) = l.split_once('=') else {
            continue;
        };
        if key.trim() != "modo" {
            continue;
        }
        return parse_mode(value.split('#').next().unwrap_or(""));
    }
    None
}

/// The shadow log path: `TOURING_VGP_SOMBRA_FILE`, else `~/.claude/touring/vgp/sombra.jsonl`.
pub fn sombra_path() -> PathBuf {
    if let Some(p) = std::env::var_os("TOURING_VGP_SOMBRA_FILE").filter(|v| !v.is_empty()) {
        return PathBuf::from(p);
    }
    let home = std::env::var("HOME").unwrap_or_default();
    PathBuf::from(format!("{home}/.claude/touring/vgp/sombra.jsonl"))
}

/// `arquivo|tmp|inline` — the balde the judge buckets false-deny by.
pub fn balde_de(path: &str) -> &'static str {
    if super::is_inline(path) {
        "inline"
    } else if path.starts_with("/tmp/") {
        "tmp"
    } else {
        "arquivo"
    }
}

/// One finding in the shadow line — adds `escopo` (the P2 target scope) and caps `reais` at 20.
#[derive(Debug, Serialize)]
struct SombraFinding {
    classe: String,
    nome: String,
    linha: usize,
    severidade: String,
    escopo: String,
    sugestao: String,
    reais: Vec<String>,
}

impl SombraFinding {
    fn de(f: &Finding, escopo: &str) -> Self {
        Self {
            classe: f.classe.clone(),
            nome: f.nome.clone(),
            linha: f.linha,
            severidade: f.severidade.clone(),
            escopo: escopo.to_string(),
            sugestao: f.sugestao.chars().take(300).collect(),
            reais: f.reais.iter().take(20).cloned().collect(),
        }
    }
}

#[derive(Debug, Serialize)]
struct SombraLine<'a> {
    ts: String,
    session_id: &'a str,
    tool_use_id: &'a str,
    hook: &'a str,
    project_root: &'a str,
    path: &'a str,
    balde: &'a str,
    camadas: Vec<u8>,
    modo: &'a str,
    verdict: &'a str,
    elapsed_ms: f64,
    findings: Vec<SombraFinding>,
    #[serde(skip_serializing_if = "Option::is_none")]
    camada2: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    motivo: Option<String>,
}

/// What a layer-2 check adds to the shadow line, in vgp-local types (the handler computes it in
/// its own crate — touring-code never names L2 types, so the dependency graph stays acyclic).
#[derive(Debug, Default)]
pub struct SombraExtra {
    /// Extra layer ran (2) and its findings, already mapped to vgp terms.
    pub findings: Vec<Finding>,
    /// The extra layer's wall time (added to the line's).
    pub elapsed_ms: f64,
    /// Set when the extra layer could not measure: logged as motivo, L1 verdict untouched.
    pub camada2_motivo: Option<String>,
}

/// The whole shadow operation for one hook check: run the motor, append the line, return the
/// motor's output (the hook may use it in ATIVO mode). NEVER touches the hook's own response;
/// every internal error degrades to a logged "unknown" and to a fail-open output.
pub fn sombra_check(
    hook: &str,
    input: &CheckInput,
    session_id: &str,
    tool_use_id: &str,
    mode: Mode,
    extra: Option<SombraExtra>,
) -> CheckOutput {
    let mut out = std::panic::catch_unwind(|| check(input)).unwrap_or_else(|_| CheckOutput {
        verdict: "unknown".into(),
        layer: 1,
        lang: input.lang.clone(),
        elapsed_ms: 0.0,
        findings: Vec::new(),
        camada2: None,
    });
    let mut camadas = vec![1u8];
    if let Some(extra) = extra {
        camadas.push(2);
        out.elapsed_ms += extra.elapsed_ms;
        fundir_findings(&mut out.findings, extra.findings);
        out.camada2 = extra.camada2_motivo.map(|m| format!("unknown: {m}"));
        // o veredito agrega as duas camadas: deny > warn > allow (unknown da L2 não derruba a L1)
        out.verdict = super::checks::verdict_dos(&out.findings, &out.verdict);
    }
    out.layer = *camadas.iter().max().unwrap_or(&1);
    let out = out;
    let line = SombraLine {
        ts: iso_z_now(),
        session_id,
        tool_use_id,
        hook,
        project_root: &input.project_root.to_string_lossy(),
        path: &input.path,
        balde: balde_de(&input.path),
        camadas,
        modo: mode.as_str(),
        verdict: &out.verdict,
        elapsed_ms: out.elapsed_ms,
        findings: out
            .findings
            .iter()
            .map(|f| {
                SombraFinding::de(
                    f,
                    if f.severidade == "deny" {
                        "externo"
                    } else {
                        "projeto"
                    },
                )
            })
            .collect(),
        camada2: out.camada2.clone(),
        motivo: (out.verdict == "unknown")
            .then(|| "falha interna do motor (fail-open)".to_string()),
    };
    append_line(&line);
    out
}

/// Merge extra-layer findings into a findings vec: dedupe by (nome, linha), the STRONGER
/// severity wins. Shared by the shadow line and by the CLI's `--camadas 1,2` merge.
pub fn fundir_findings(base: &mut Vec<Finding>, novos: Vec<Finding>) {
    for f in novos {
        match base
            .iter_mut()
            .find(|b| b.nome == f.nome && b.linha == f.linha)
        {
            Some(existente) => {
                if existente.severidade != "deny" && f.severidade == "deny" {
                    *existente = f;
                }
            }
            None => base.push(f),
        }
    }
}

/// Append one JSON line with fsync. Never panics, never propagates: I/O trouble goes to stderr.
fn append_line(line: &impl Serialize) {
    let r = (|| -> std::io::Result<()> {
        let path = sombra_path();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;
        let mut js = serde_json::to_string(line).unwrap_or_else(|_| "{}".into());
        js.push('\n');
        f.write_all(js.as_bytes())?;
        f.sync_all()
    })();
    if let Err(e) = r {
        eprintln!("[vgp · sombra] falha ao gravar a linha (a chamada segue intacta): {e}");
    }
}

/// ISO-8601 UTC with milliseconds, Z-suffixed — stdlib only (no chrono in this crate).
fn iso_z_now() -> String {
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let (s, msec) = (ms / 1000, ms % 1000);
    let days = s / 86_400;
    let tod = s % 86_400;
    let (h, m, sec) = (tod / 3600, tod % 3600 / 60, tod % 60);
    let (y, mo, d) = civil_from_days(days as i64);
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{m:02}:{sec:02}.{msec:03}Z")
}

/// Howard Hinnant's civil-from-days (public-domain algorithm).
fn civil_from_days(z: i64) -> (i64, u64, u64) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toml_modo_e_precedencia() {
        let t = "[outra]\nx = 1\n[vgp]\nmodo = \"ativo\" # comentário\n";
        assert_eq!(toml_declares_mode(t), Some(Mode::Ativo));
        assert_eq!(
            toml_declares_mode("[vgp]\nmodo = \"off\"\n"),
            Some(Mode::Off)
        );
        assert_eq!(toml_declares_mode("[vgp]\noutra = 1\n"), None);
        assert_eq!(
            toml_declares_mode("modo = \"ativo\"\n"),
            None,
            "fora da seção não conta"
        );
    }

    #[test]
    fn balde_por_path() {
        assert_eq!(balde_de("<string>"), "inline");
        assert_eq!(balde_de("/tmp/x.py"), "tmp");
        assert_eq!(balde_de("/home/u/p/a.py"), "arquivo");
    }

    #[test]
    fn iso_z_tem_forma_do_contrato() {
        let ts = iso_z_now();
        assert!(
            ts.ends_with('Z') && ts.len() == 24 && &ts[10..11] == "T",
            "ts: {ts}"
        );
    }

    #[test]
    fn sombra_check_grava_a_linha_e_nao_toca_a_resposta() {
        let dir = std::env::temp_dir().join("vgp-sombra-t");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let log = dir.join("sombra.jsonl");
        unsafe { std::env::set_var("TOURING_VGP_SOMBRA_FILE", &log) };
        let input = CheckInput {
            path: "/tmp/x.py".into(),
            content: "print(cp.PASTA)\n".into(),
            project_root: dir.clone(),
            disk: None,
            overlays: HashMap::new(),
            lang: "python".into(),
        };
        let out = sombra_check("pre-edit", &input, "s1", "tu1", Mode::Sombra, None);
        assert_eq!(out.verdict, "deny");
        let linha: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&log).unwrap()).unwrap();
        assert_eq!(linha["hook"], "pre-edit");
        assert_eq!(linha["balde"], "tmp");
        assert_eq!(linha["camadas"], serde_json::json!([1]));
        assert_eq!(linha["modo"], "sombra");
        assert_eq!(linha["verdict"], "deny");
        assert!(linha["findings"][0]["escopo"].is_string());
        unsafe { std::env::remove_var("TOURING_VGP_SOMBRA_FILE") };
        let _ = std::fs::remove_dir_all(&dir);
    }
}
