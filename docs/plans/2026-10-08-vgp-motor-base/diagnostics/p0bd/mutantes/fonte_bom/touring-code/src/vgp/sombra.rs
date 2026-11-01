//! VGP shadow glue — the sombra.jsonl emitter every pre-hook shares (V2/V5).
//!
//! Contract (coordenadora, adendo 09/10 — what the V4 judge reads):
//! `~/.claude/touring/vgp/sombra.jsonl`, append + fsync, one line per hook check:
//! {"ts": ISO-Z ms, "session_id", "tool_use_id", "hook", "project_root", "path", "lang",
//!  "balde": "arquivo|tmp|inline", "camadas": \[1\], "modo": "sombra|ativo", "verdict",
//!  "elapsed_ms", "findings": [{classe,nome,linha,severidade,escopo,sugestao,reais≤20}],
//!  "motivo": só quando unknown (o do motor: parse falhou, adaptador ausente, ...)}
//! No file content ever (size + privacy). Mode resolution: `TOURING_VGP_DISABLED=1` kills it;
//! `TOURING_VGP_MODE` beats the file; `.touring/touring.toml [vgp] modo = "sombra"|"ativo"|"off"`;
//! code default is **sombra** — "ativo" is only ever a project's choice (V4), never a code default.
//! Fail-open end to end: any internal error degrades to verdict "unknown" with `motivo`, and the
//! hook's own answer is NEVER touched.

use std::cell::RefCell;
use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use serde::Serialize;

use super::{Alvo, CheckInput, CheckOutput, Contexto, Finding, check_em};

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

/// The raw value of `[vgp] <chave> = …` in a toml text (inline comment stripped, trimmed) — line
/// scan with section state (same stance as r3_shadow: neither side pulls a TOML parser for a few
/// keys). The first line that declares the key wins; a value that spans lines is not read.
fn toml_vgp_value<'a>(text: &'a str, chave: &str) -> Option<&'a str> {
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
        if key.trim() == chave {
            return Some(value.split('#').next().unwrap_or("").trim());
        }
    }
    None
}

/// `[vgp] modo = "…"` from a toml text.
fn toml_declares_mode(text: &str) -> Option<Mode> {
    toml_vgp_value(text, "modo").and_then(parse_mode)
}

/// The language list when layer 2 is not declared: only Python, the one verifier measured against
/// a corpus (V3). rust-analyzer, tsserver and deno cost 0.3-1.5 GiB resident per live session
/// (RELATORIO-VL0-camada2-generica §5), so they are a project's opt-in, never a code default.
const CAMADA2_LANGS_PADRAO: &[&str] = &["python"];

/// Languages whose layer 2 (a language-server verifier, `touring-pyright`) runs for `project_root`:
/// `[vgp] camada2_langs = ["python", "rust"]` in `.touring/touring.toml` — the declared list
/// REPLACES the default (`[]` turns layer 2 off), names are the `lang` of VL0 §2 — else only
/// `["python"]`. A value that is not a one-line `[...]` list is ignored (the default stands).
pub fn camada2_langs(project_root: &Path) -> Vec<String> {
    std::fs::read_to_string(project_root.join(".touring/touring.toml"))
        .ok()
        .as_deref()
        .and_then(toml_declares_camada2_langs)
        .unwrap_or_else(|| CAMADA2_LANGS_PADRAO.iter().map(|l| l.to_string()).collect())
}

/// Whether layer 2 runs for `lang` in `project_root` ([`camada2_langs`]).
pub fn camada2_ligada(lang: &str, project_root: &Path) -> bool {
    camada2_langs(project_root).iter().any(|l| l == lang)
}

/// `[vgp] camada2_langs = ["…", …]` from a toml text: `None` when absent or not a one-line list.
fn toml_declares_camada2_langs(text: &str) -> Option<Vec<String>> {
    let lista = toml_vgp_value(text, "camada2_langs")?
        .strip_prefix('[')?
        .strip_suffix(']')?;
    Some(
        lista
            .split(',')
            .map(|l| l.trim().trim_matches(['"', '\'']).to_ascii_lowercase())
            .filter(|l| !l.is_empty())
            .collect(),
    )
}

thread_local! {
    /// Per-thread redirect of the shadow log ([`com_log_em`]); `None` in production.
    static LOG_REDIRECIONADO: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
}

/// Runs `f` with THIS thread's shadow log redirected to `log` — for tests of the hooks (other
/// crates) that must read what the shadow wrote without mutating the process environment, which
/// parallel tests share. Restored when `f` returns or panics.
pub fn com_log_em<R>(log: &Path, f: impl FnOnce() -> R) -> R {
    struct Restaura(Option<PathBuf>);
    impl Drop for Restaura {
        fn drop(&mut self) {
            let anterior = self.0.take();
            LOG_REDIRECIONADO.with(|o| *o.borrow_mut() = anterior);
        }
    }
    let anterior = LOG_REDIRECIONADO.with(|o| o.borrow_mut().replace(log.to_path_buf()));
    let _restaura = Restaura(anterior);
    f()
}

/// The shadow log path: a [`com_log_em`] redirect, else `TOURING_VGP_SOMBRA_FILE`, else
/// `~/.claude/touring/vgp/sombra.jsonl`.
pub fn sombra_path() -> PathBuf {
    if let Some(p) = LOG_REDIRECIONADO.with(|o| o.borrow().clone()) {
        return p;
    }
    if let Some(p) = std::env::var_os("TOURING_VGP_SOMBRA_FILE").filter(|v| !v.is_empty()) {
        return PathBuf::from(p);
    }
    let home = std::env::var("HOME").unwrap_or_default();
    PathBuf::from(format!("{home}/.claude/touring/vgp/sombra.jsonl"))
}

/// The reason logged when the motor panicked or an `unknown` came with no reason of its own.
const MOTIVO_FALHA_INTERNA: &str = "falha interna do motor (fail-open)";

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
    lang: &'a str,
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
///
/// The verdict of a file being WRITTEN ([`Contexto::Escrita`]); a file about to be run asks
/// [`sombra_check_em`].
pub fn sombra_check(
    hook: &str,
    input: &CheckInput,
    session_id: &str,
    tool_use_id: &str,
    mode: Mode,
    extra: Option<SombraExtra>,
) -> CheckOutput {
    sombra_check_em(
        hook,
        input,
        session_id,
        tool_use_id,
        mode,
        extra,
        Contexto::Escrita,
    )
}

/// [`sombra_check`] at an explicit moment ([`Contexto`]) — the execution hooks (`pre-bash-exec`)
/// judge the file that is about to run with the scratch buckets live.
pub fn sombra_check_em(
    hook: &str,
    input: &CheckInput,
    session_id: &str,
    tool_use_id: &str,
    mode: Mode,
    extra: Option<SombraExtra>,
    contexto: Contexto,
) -> CheckOutput {
    let mut out =
        std::panic::catch_unwind(|| check_em(input, contexto)).unwrap_or_else(|_| CheckOutput {
            verdict: "unknown".into(),
            layer: 1,
            lang: input.lang.clone(),
            elapsed_ms: 0.0,
            findings: Vec::new(),
            camada2: None,
            motivo: Some(MOTIVO_FALHA_INTERNA.to_string()),
        });
    let mut camadas = vec![1u8];
    if let Some(extra) = extra {
        camadas.push(2);
        out.elapsed_ms += extra.elapsed_ms;
        fundir_findings(&mut out.findings, extra.findings);
        out.camada2 = extra.camada2_motivo.map(|m| format!("unknown: {m}"));
        // o veredito agrega as duas camadas: deny > warn > allow (unknown da L2 não derruba a L1)
        out.verdict = super::politica::verdict_dos(&out.findings, &out.verdict);
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
        lang: &input.lang,
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
        motivo: (out.verdict == "unknown").then(|| {
            out.motivo
                .clone()
                .unwrap_or_else(|| MOTIVO_FALHA_INTERNA.to_string())
        }),
    };
    append_line(&line);
    out
}

/// Merge extra-layer findings into a findings vec: dedupe by (nome, linha), the STRONGER
/// severity wins. Shared by the shadow line and by the CLI's `--camadas 1,2` merge.
///
/// The one exception (P0B (b)): a finding layer 1 declared UNCERTAIN ([`Alvo::Incerto`] — a name that
/// `exec`/`eval`/`globals()`/`import *` may have bound) is never escalated by the extra layer. Layer 1
/// reads the program's own scopes; pyright does not see the `exec`, so its `deny` over the same
/// (nome, linha) is the very false deny layer 1 abstained from (`cr` after `exec(open(…).read())`).
///
/// The other doubt, [`Alvo::Fechado`] (P0B-d): layer 1 proved the receiver's type has a CLOSED member
/// set and the member is not in it, but it follows no flow, so alone it warns. When layer 2 names the
/// SAME receiver type for the same (nome, linha) — whatever severity it gave, `Escopo::Tipo` warns by
/// design — two independent readings agree on both the type and the absence, and the finding is
/// `deny` ([`corrobora_tipo_fechado`]). A different class, or one layer only, stays a warning.
pub fn fundir_findings(base: &mut Vec<Finding>, novos: Vec<Finding>) {
    for f in novos {
        match base
            .iter_mut()
            .find(|b| b.nome == f.nome && b.linha == f.linha)
        {
            Some(existente) => {
                if existente.severidade != "deny"
                    && f.severidade == "deny"
                    && existente.alvo != Alvo::Incerto
                {
                    *existente = f;
                } else if existente.severidade != "deny" && corrobora_tipo_fechado(existente, &f) {
                    existente.severidade = "deny".into();
                }
            }
            None => base.push(f),
        }
    }
}

/// Does layer 2 corroborate layer 1's closed-type finding — the same receiver type on both sides?
/// A finding that is not [`Alvo::Fechado`], or a side that names no receiver, never qualifies.
fn corrobora_tipo_fechado(camada1: &Finding, camada2: &Finding) -> bool {
    camada1.alvo == Alvo::Fechado
        && match (camada1.receptor.as_deref(), camada2.receptor.as_deref()) {
            (Some(a), Some(b)) => mesmo_tipo(a, b),
            _ => false,
        }
}

/// The same type as the two layers spell it: layer 1 uses the model's name (`stat_result`), the
/// checker may qualify it (`os.stat_result`) — only the last `.` segment is compared, whole (a class
/// that merely ENDS like the type, `meu_stat_result`, is another type).
fn mesmo_tipo(a: &str, b: &str) -> bool {
    fn ultimo(nome: &str) -> &str {
        nome.rsplit('.').next().unwrap_or(nome)
    }
    !ultimo(a).is_empty() && ultimo(a) == ultimo(b)
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
    // The P0B tests ask the write-time verdict; since FRAG the module body only calls `check_em`.
    use crate::vgp::check;

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

    fn langs(text: &str) -> Option<Vec<String>> {
        toml_declares_camada2_langs(text)
    }

    #[test]
    fn toml_camada2_langs_le_a_lista_de_uma_linha() {
        let l = |v: &[&str]| Some(v.iter().map(|s| s.to_string()).collect::<Vec<_>>());
        assert_eq!(
            langs("[vgp]\ncamada2_langs = [\"python\", \"rust\"]\n"),
            l(&["python", "rust"])
        );
        assert_eq!(
            langs(
                "[outra]\nx = 1\n[vgp]\nmodo = \"ativo\"\ncamada2_langs=['TSX',\"typescript\"] # só TS\n"
            ),
            l(&["tsx", "typescript"]),
            "aspas simples, caixa e comentário"
        );
        assert_eq!(
            langs("[vgp]\ncamada2_langs = []\n"),
            l(&[]),
            "lista vazia = camada 2 desligada, não ausência"
        );
        assert_eq!(
            langs("[vgp]\ncamada2_langs = [ \"rust\" , ]\n"),
            l(&["rust"]),
            "vírgula final e espaços"
        );
    }

    #[test]
    fn toml_camada2_langs_ausente_ou_malformada_nao_declara() {
        assert_eq!(langs("[vgp]\nmodo = \"ativo\"\n"), None, "sem a chave");
        assert_eq!(
            langs("camada2_langs = [\"rust\"]\n"),
            None,
            "fora da seção [vgp] não conta"
        );
        assert_eq!(
            langs("[outra]\ncamada2_langs = [\"rust\"]\n[vgp]\n"),
            None,
            "em outra seção não conta"
        );
        assert_eq!(
            langs("[vgp]\ncamada2_langs = \"rust\"\n"),
            None,
            "não é lista"
        );
        assert_eq!(
            langs("[vgp]\ncamada2_langs = [\"rust\",\n  \"tsx\"]\n"),
            None,
            "lista em várias linhas não é lida: o padrão vale, nunca metade da lista"
        );
    }

    #[test]
    fn camada2_ligada_padrao_so_python_e_a_declaracao_substitui() {
        let raiz = std::env::temp_dir().join(format!("vgp-camada2-ligada-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&raiz);
        std::fs::create_dir_all(raiz.join(".touring")).unwrap();
        // sem toml: só Python
        assert_eq!(camada2_langs(&raiz), vec!["python".to_string()]);
        for (lang, esperado) in [
            ("python", true),
            ("rust", false),
            ("typescript", false),
            ("tsx", false),
            ("javascript", false),
            ("html", false),
            ("css", false),
        ] {
            assert_eq!(camada2_ligada(lang, &raiz), esperado, "padrão: {lang}");
        }
        // toml com [vgp] mas sem a chave: ainda o padrão
        std::fs::write(
            raiz.join(".touring/touring.toml"),
            "[vgp]\nmodo = \"sombra\"\n",
        )
        .unwrap();
        assert!(camada2_ligada("python", &raiz) && !camada2_ligada("rust", &raiz));
        // a declaração SUBSTITUI o padrão
        std::fs::write(
            raiz.join(".touring/touring.toml"),
            "[vgp]\ncamada2_langs = [\"rust\", \"tsx\"]\n",
        )
        .unwrap();
        assert!(camada2_ligada("rust", &raiz) && camada2_ligada("tsx", &raiz));
        assert!(
            !camada2_ligada("python", &raiz),
            "quem declara a lista é dono dela inteira: python fora da lista = desligado"
        );
        assert!(
            !camada2_ligada("typescript", &raiz),
            "tsx não liga typescript"
        );
        // lista vazia desliga tudo
        std::fs::write(
            raiz.join(".touring/touring.toml"),
            "[vgp]\ncamada2_langs = []\n",
        )
        .unwrap();
        assert!(!camada2_ligada("python", &raiz));
        let _ = std::fs::remove_dir_all(&raiz);
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
        let input = CheckInput {
            path: "/tmp/x.py".into(),
            content: "print(cp.PASTA)\n".into(),
            project_root: dir.clone(),
            disk: None,
            overlays: HashMap::new(),
            lang: "python".into(),
        };
        let out = com_log_em(&log, || {
            sombra_check("pre-edit", &input, "s1", "tu1", Mode::Sombra, None)
        });
        // FRAG: escrever em /tmp é um fragmento — o nome ausente é do projeto e só avisa
        assert_eq!(out.verdict, "warn");
        let linha: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&log).unwrap()).unwrap();
        assert_eq!(linha["hook"], "pre-edit");
        assert_eq!(linha["lang"], "python");
        assert_eq!(linha["balde"], "tmp");
        assert_eq!(linha["camadas"], serde_json::json!([1]));
        assert_eq!(linha["modo"], "sombra");
        assert_eq!(linha["verdict"], "warn");
        assert!(linha["findings"][0]["escopo"].is_string());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn sombra_check_em_execucao_grava_o_hook_de_execucao_e_nega_o_scratch() {
        let dir = std::env::temp_dir().join("vgp-sombra-t-exec");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let log = dir.join("sombra.jsonl");
        let input = CheckInput {
            path: "/tmp/x.py".into(),
            content: "print(cp.PASTA)\n".into(),
            project_root: dir.clone(),
            disk: None,
            overlays: HashMap::new(),
            lang: "python".into(),
        };
        let out = com_log_em(&log, || {
            sombra_check_em(
                "pre-bash-exec",
                &input,
                "s1",
                "tu3",
                Mode::Sombra,
                None,
                Contexto::Execucao,
            )
        });
        assert_eq!(
            out.verdict, "deny",
            "o mesmo arquivo, agora pedido para rodar"
        );
        let linha: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&log).unwrap()).unwrap();
        assert_eq!(linha["hook"], "pre-bash-exec");
        assert_eq!(linha["verdict"], "deny");
        assert_eq!(linha["balde"], "tmp");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn linha_de_linguagem_sem_adaptador_carrega_lang_e_o_motivo_do_motor() {
        let dir = std::env::temp_dir().join("vgp-sombra-t-lang");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let log = dir.join("sombra.jsonl");
        let input = CheckInput {
            path: "/tmp/a.go".into(),
            content: "package main\n".into(),
            project_root: dir.clone(),
            disk: None,
            overlays: HashMap::new(),
            lang: "go".into(),
        };
        let out = com_log_em(&log, || {
            sombra_check("pre-write", &input, "s1", "tu2", Mode::Sombra, None)
        });
        assert_eq!(out.verdict, "unknown");
        let linha: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&log).unwrap()).unwrap();
        assert_eq!(linha["lang"], "go");
        assert_eq!(linha["verdict"], "unknown");
        assert!(
            linha["motivo"].as_str().unwrap().contains("sem adaptador"),
            "o motivo é o do motor, não o genérico: {linha}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn redirecionamento_e_por_thread_e_restaura_no_fim() {
        let a = std::env::temp_dir().join("vgp-sombra-redir-a.jsonl");
        let antes = sombra_path();
        com_log_em(&a, || assert_eq!(sombra_path(), a));
        assert_eq!(sombra_path(), antes, "restaurado ao sair");
        let r = std::panic::catch_unwind(|| com_log_em(&a, || panic!("falha no meio")));
        assert!(r.is_err());
        assert_eq!(sombra_path(), antes, "restaurado também no pânico");
    }

    // ── P0B (b3): a camada 2 não escala o que a camada 1 declarou incerto ─────────

    fn nome_l2_deny(nome: &str, linha: usize) -> Finding {
        Finding::new(
            "nome",
            nome,
            linha,
            "deny",
            format!("\"{nome}\" is not defined"),
            Vec::new(),
        )
    }

    fn entrada_py(path: &str, content: &str, raiz: &Path) -> CheckInput {
        CheckInput {
            path: path.into(),
            content: content.into(),
            project_root: raiz.to_path_buf(),
            disk: None,
            overlays: HashMap::new(),
            lang: "python".into(),
        }
    }

    fn resumo(fs: &[Finding]) -> Vec<(String, usize, String)> {
        fs.iter()
            .map(|f| (f.nome.clone(), f.linha, f.severidade.clone()))
            .collect()
    }

    #[test]
    fn p0b_b_fusao_nao_deixa_a_camada2_negar_por_cima_do_dinamico_da_camada_1() {
        // forma de eb898e2aa211: `cr` ligado por `exec(open('contraste.py').read())` — a camada 1
        // ABSTEVE (warn) e a camada 2 negou por cima; quem declara incerteza não é desmentido por quem
        // não vê o `exec`
        let raiz = std::env::temp_dir();
        for conteudo in [
            "exec(open('contraste.py').read())\nprint(cr)\n",
            "eval('cr = 1')\nprint(cr)\n",
            "globals().update(cr=1)\nprint(cr)\n",
        ] {
            let mut out = check(&entrada_py("<string>", conteudo, &raiz));
            assert_eq!(out.verdict, "warn", "{conteudo:?}: {:?}", out.findings);
            fundir_findings(&mut out.findings, vec![nome_l2_deny("cr", 2)]);
            assert_eq!(
                resumo(&out.findings),
                [("cr".to_string(), 2, "warn".to_string())],
                "{conteudo:?}"
            );
            assert_eq!(
                crate::vgp::politica::verdict_dos(&out.findings, &out.verdict),
                "warn",
                "{conteudo:?}"
            );
        }
    }

    #[test]
    fn p0b_b_fusao_controles_sem_sinal_dinamico_a_camada2_segue_escalando() {
        // `fantasma` em arquivo do projeto: a camada 1 só AVISA (P3, alvo do projeto) e a camada 2, que
        // vê o nome como do programa, continua podendo escalar — o veto é só ao alvo INCERTO
        let raiz = Path::new("/home/u/proj");
        let mut out = check(&entrada_py("/home/u/proj/a.py", "print(fantasma)\n", raiz));
        assert_eq!(
            resumo(&out.findings),
            [("fantasma".to_string(), 1, "warn".to_string())]
        );
        fundir_findings(&mut out.findings, vec![nome_l2_deny("fantasma", 1)]);
        assert_eq!(
            resumo(&out.findings),
            [("fantasma".to_string(), 1, "deny".to_string())]
        );
        // o veto vale para o MESMO (nome, linha): outro nome, ou a mesma linha de outro nome, entra
        let mut out = check(&entrada_py(
            "<string>",
            "exec('x = 1')\nprint(cr)\nprint(outro)\n",
            &std::env::temp_dir(),
        ));
        fundir_findings(
            &mut out.findings,
            vec![nome_l2_deny("cr", 2), nome_l2_deny("novo", 3)],
        );
        let por_nome = |n: &str| {
            out.findings
                .iter()
                .find(|f| f.nome == n)
                .map(|f| f.severidade.as_str())
        };
        assert_eq!(por_nome("cr"), Some("warn"), "veto ao (cr, 2)");
        assert_eq!(
            por_nome("novo"),
            Some("deny"),
            "nome que a camada 1 não declarou incerto"
        );
    }

    // ---- P0B-d: "tipo fechado" corroborado pelas duas camadas ------------------------------------
    //
    // `d94f8090d150`: `st = <path>.stat(); st.st_m_size` raises at run time. Layer 1 knows
    // `stat_result` has a CLOSED member set (warns: it follows no flow) and layer 2 names the same
    // receiver class (warns: `Escopo::Tipo`). Together they deny; either alone, or two different
    // classes, do not.

    const STAT_L3: &str = "from pathlib import Path\nst = Path('x').stat()\nprint(st.st_m_size)\n";

    /// A layer-2 `Cannot access attribute` as the mapper builds it: the checker's message, the
    /// receiver class it named, and the `warn` that `Escopo::Tipo` gives it.
    fn l2_membro(nome: &str, linha: usize, classe: &str, severidade: &str) -> Finding {
        Finding::new(
            "membro",
            nome,
            linha,
            severidade,
            format!("Cannot access attribute \"{nome}\" for class \"{classe}\"\n  Attribute \"{nome}\" is unknown"),
            Vec::new(),
        )
        .com_receptor(Some(classe.to_string()))
    }

    fn fundido(fonte: &str, l2: Vec<Finding>) -> CheckOutput {
        let mut out = check(&entrada_py(
            "/home/u/proj/a.py",
            fonte,
            Path::new("/home/u/proj"),
        ));
        fundir_findings(&mut out.findings, l2);
        out
    }

    #[test]
    fn p0bd_fechado_e_a_mesma_classe_na_camada_2_vira_deny() {
        let sozinho = fundido(STAT_L3, Vec::new());
        assert_eq!(
            resumo(&sozinho.findings),
            [("st_m_size".to_string(), 3, "warn".to_string())],
            "a camada 1 sozinha só avisa"
        );
        let out = fundido(
            STAT_L3,
            vec![l2_membro("st_m_size", 3, "stat_result", "warn")],
        );
        assert_eq!(
            resumo(&out.findings),
            [("st_m_size".to_string(), 3, "deny".to_string())],
            "{:?}",
            out.findings
        );
        // the refusal keeps teaching what layer 1 knows: the real members
        assert!(out.findings[0].sugestao.contains("st_size"));
        assert!(!out.findings[0].reais.is_empty());
        assert_eq!(
            crate::vgp::politica::verdict_dos(&out.findings, &out.verdict),
            "deny"
        );
    }

    #[test]
    fn p0bd_o_pyright_pode_qualificar_a_classe_so_o_ultimo_segmento_conta() {
        for classe in ["os.stat_result", "posix.stat_result"] {
            let out = fundido(STAT_L3, vec![l2_membro("st_m_size", 3, classe, "warn")]);
            assert_eq!(
                resumo(&out.findings),
                [("st_m_size".to_string(), 3, "deny".to_string())],
                "{classe}"
            );
        }
    }

    #[test]
    fn p0bd_classe_diferente_ou_sem_classe_ou_so_uma_camada_segue_warn() {
        let aviso = [("st_m_size".to_string(), 3, "warn".to_string())];
        for l2 in [
            // another receiver: the checker is talking about an object layer 1 never typed
            l2_membro("st_m_size", 3, "Foo", "warn"),
            // a class that merely ENDS like the closed type is not the type
            l2_membro("st_m_size", 3, "meu_stat_result", "warn"),
            // the checker named no class (module attribute, kwarg…): nothing to corroborate with
            Finding::new("membro", "st_m_size", 3, "warn", "m".into(), Vec::new()),
        ] {
            let out = fundido(STAT_L3, vec![l2.clone()]);
            assert_eq!(resumo(&out.findings), aviso, "{l2:?}");
        }
        // the same class on ANOTHER line is another finding: layer 1's stays a warning
        let out = fundido(
            STAT_L3,
            vec![l2_membro("st_m_size", 2, "stat_result", "warn")],
        );
        assert_eq!(
            resumo(&out.findings),
            [
                ("st_m_size".to_string(), 3, "warn".to_string()),
                ("st_m_size".to_string(), 2, "warn".to_string())
            ]
        );
    }

    #[test]
    fn p0bd_receptor_str_nao_e_fechado_e_segue_warn() {
        // forma de 0893b1f5187f: `x.setdefault` com o pyright dizendo `for class "str"` — um `str`
        // pode ser outra coisa (`dict`) que chega ali; nem a camada 1 o tem por fechado
        let fonte = "x = 'abc'\nx.setdefault(1)\n";
        let out = fundido(fonte, vec![l2_membro("setdefault", 2, "str", "warn")]);
        assert_eq!(
            resumo(&out.findings),
            [("setdefault".to_string(), 2, "warn".to_string())],
            "{:?}",
            out.findings
        );
        assert_eq!(out.findings[0].alvo, Alvo::Incerto);
    }

    #[test]
    fn p0bd_incerto_segue_sem_ser_escalado_nem_pela_mesma_classe() {
        // o `Incerto` do sentido exec/eval (o nome que o `exec` pode ter ligado) e o do builtin
        // "forma": a camada 2 não os desmente — nem negando por cima, nem concordando na classe
        let fonte = "x = 'abc'\nx.setdefault(1)\n";
        for l2 in [
            l2_membro("setdefault", 2, "str", "deny"),
            l2_membro("setdefault", 2, "str", "warn"),
        ] {
            let out = fundido(fonte, vec![l2]);
            assert_eq!(
                resumo(&out.findings),
                [("setdefault".to_string(), 2, "warn".to_string())]
            );
        }
        let raiz = std::env::temp_dir();
        let mut out = check(&entrada_py("<string>", "exec('x = 1')\nprint(cr)\n", &raiz));
        fundir_findings(
            &mut out.findings,
            vec![nome_l2_deny("cr", 2).com_receptor(Some("stat_result".into()))],
        );
        assert_eq!(
            resumo(&out.findings),
            [("cr".to_string(), 2, "warn".to_string())]
        );
    }
}
