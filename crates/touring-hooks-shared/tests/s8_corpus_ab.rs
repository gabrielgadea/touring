//! S8 (2026-10-08) — A/B da dedupe de injeção sobre corpus REAL de payloads de hook.
//!
//! O corpus (`S8_CORPUS_JSONL`, gerado por `docs/plans/2026-10-08-compactacao-65/
//! diagnostics/` + extrator `/tmp/s8_extrair_corpus.py`) carrega o `stdout` bruto que
//! cada hook imprimiu nos transcripts reais do projeto analise — exatamente a string
//! que `dedup_hook_output` recebe no daemon. O "antes" é o payload tal qual emitido
//! (a era pré-dedupe); o "depois" é o mesmo payload pelo código novo. Mesmo
//! session_id, mesma ordem; nas linhas `compact_boundary` a época avança — o que o
//! deploy real faz no handler post-compact.
//!
//! Sem a env, roda um mini-corpus embutido (prova de mecanismo, sempre verde no CI).
//! Com a env, exige as metas do aceite A2: N ≥ 200 payloads, queda ≥ 40 % e o
//! controle positivo — nenhuma diretiva MUST perdida na 1ª emissão, e o primeiro
//! payload de cada sessão passando intacto.

use std::collections::HashMap;
use std::collections::HashSet;
use std::path::Path;

use touring_hooks_shared::injection_dedup::{advance_epoch, dedup_hook_output};

#[derive(serde::Deserialize)]
struct CorpusLine {
    session_id: Option<String>,
    payload: Option<String>,
    command: Option<String>,
    hook_name: Option<String>,
    compact_boundary: Option<bool>,
    turn_boundary: Option<bool>,
}

/// O que o daemon do touring processa no deploy real (o resto é hook Python do
/// projeto — processo separado, nunca passa pelo `run_hook_command`).
fn e_touring(command: &str) -> bool {
    let c = command.to_lowercase();
    c.contains("touring")
        || c.starts_with("taco:")
        || c.starts_with("code mode:")
        || c.starts_with("scout perp")
}

/// As linhas de diretiva de um payload (`hookSpecificOutput.additionalContext`).
fn directive_lines(payload: &str) -> Vec<String> {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(payload) else {
        return Vec::new();
    };
    let Some(ctx) = v
        .get("hookSpecificOutput")
        .and_then(|h| h.get("additionalContext"))
        .and_then(|c| c.as_str())
    else {
        return Vec::new();
    };
    ctx.lines()
        .map(str::trim_start)
        .filter(|l| l.starts_with("MUST") || l.starts_with("SHOULD"))
        .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
        .collect()
}

/// O additionalContext de saída (para o controle positivo de presença).
fn output_context(output: &str) -> String {
    serde_json::from_str::<serde_json::Value>(output)
        .ok()
        .and_then(|v| {
            v.get("hookSpecificOutput")
                .and_then(|h| h.get("additionalContext"))
                .and_then(|c| c.as_str())
                .map(str::to_string)
        })
        .unwrap_or_default()
}

struct Ab {
    n_payloads: usize,
    bytes_antes: usize,
    bytes_depois: usize,
    /// (sessão, época) → diretivas já vistas nos INPUTS (o controle de 1ª emissão).
    seen: HashMap<(String, u64), HashSet<String>>,
    primeiro_payload_intacto: bool,
    /// Diagnóstico por camada: (intactos, viraram_referência, reduzidos, esvaziados)
    /// e os bytes que cada camada economizou.
    intactos: (usize, usize),
    referencia: (usize, usize),
    reduzidos: (usize, usize),
    esvaziados: (usize, usize),
    /// Potencial da política de MOLDE: payloads que o Rust deixou intactos mas cujo
    /// molde (números→N, paths→P) já saiu nesta época — (quantidade, bytes do ctx).
    intactos_molde_repetido: (usize, usize),
    moldes_vistos: HashMap<(String, u64), HashSet<String>>,
    /// Gasto de turno por sessão (só o cli-suggest cobra — cli_suggester.rs:6205).
    gasto_turno: HashMap<String, usize>,
    /// (n, antes, depois) por hookName — a tabela do relatório do brief.
    por_hook: HashMap<String, (usize, usize, usize)>,
}

/// Modo do A/B: A = tudo pela dedupe · B = deploy-fiel (só o que o daemon touring
/// processa) · C = combinado (B + session-start:compact fora [settings da 6b] +
/// turn_budget 2000 no cli-suggest — o binário novo completo, o que o juiz mede
/// ao vivo).
#[derive(Clone, Copy, PartialEq)]
enum Modo {
    Total,
    DeployFiel,
    Combinado,
}

/// O molde de um ctx: dígitos → `N`, segmentos com `/` → `P`. Diretivas
/// (MUST/SHOULD/MAY) NÃO são normalizadas: um path novo numa diretiva é diretiva
/// nova ("olhe ESTE arquivo"), e o controle positivo as protege.
fn molde_ctx(ctx: &str) -> String {
    let mut out = String::with_capacity(ctx.len());
    for line in ctx.lines() {
        let t = line.trim_start();
        if t.starts_with("MUST") || t.starts_with("SHOULD") || t.starts_with("MAY") {
            out.push_str(t);
        } else {
            let mut prev_slash = false;
            for c in t.chars() {
                if c.is_ascii_digit() {
                    if !out.ends_with('N') {
                        out.push('N');
                    }
                    prev_slash = false;
                } else if c == '/' {
                    if !prev_slash {
                        out.push('P');
                        prev_slash = true;
                    }
                } else {
                    prev_slash = false;
                    out.push(c);
                }
            }
        }
        out.push('\n');
    }
    out
}

/// O teto novo (2000) aplicado ao cli-suggest como o código real faz
/// (cli_suggester.rs:6199): decide com spent+len, cobra o que emite.
fn apply_turn_budget(payload: &str, spent: usize) -> (String, usize) {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(payload) else {
        return (payload.to_string(), spent);
    };
    let Some(ctx) = v
        .get("hookSpecificOutput")
        .and_then(|h| h.get("additionalContext"))
        .and_then(|c| c.as_str())
    else {
        return (payload.to_string(), spent);
    };
    let mut ctx_eff = ctx.to_string();
    if spent + ctx.len() > touring_hooks_shared::turn_budget::TURN_BUDGET_DEFAULT
        && let Some(cut) = touring_hooks_shared::turn_budget::directives_only(ctx)
    {
        ctx_eff = cut;
    }
    let novo_spent = spent + ctx_eff.len();
    if ctx_eff == ctx {
        return (payload.to_string(), novo_spent);
    }
    let mut v2 = v;
    v2["hookSpecificOutput"]["additionalContext"] = serde_json::Value::String(ctx_eff);
    (
        serde_json::to_string(&v2).unwrap_or_else(|_| payload.to_string()),
        novo_spent,
    )
}

impl Ab {
    fn new() -> Self {
        Self {
            n_payloads: 0,
            bytes_antes: 0,
            bytes_depois: 0,
            seen: HashMap::new(),
            primeiro_payload_intacto: true,
            intactos: (0, 0),
            referencia: (0, 0),
            reduzidos: (0, 0),
            esvaziados: (0, 0),
            intactos_molde_repetido: (0, 0),
            moldes_vistos: HashMap::new(),
            gasto_turno: HashMap::new(),
            por_hook: HashMap::new(),
        }
    }

    fn registra_hook(&mut self, hook: &str, antes: usize, depois: usize) {
        let e = self.por_hook.entry(hook.to_string()).or_default();
        e.0 += 1;
        e.1 += antes;
        e.2 += depois;
    }

    fn process(&mut self, root: &Path, line: &CorpusLine, modo: Modo) {
        let Some(sid) = line.session_id.as_deref() else {
            return;
        };
        if line.turn_boundary == Some(true) {
            self.gasto_turno.insert(sid.to_string(), 0);
            return;
        }
        if line.compact_boundary == Some(true) {
            advance_epoch(root, sid);
            return;
        }
        let Some(payload) = line.payload.as_deref() else {
            return;
        };
        let command = line.command.as_deref().unwrap_or("");
        let hook = line.hook_name.as_deref().unwrap_or("");
        self.n_payloads += 1;
        self.bytes_antes += payload.len();

        // Modo B/C: o daemon só processa o touring-hook — o resto passa intacto
        // e nunca toca o estado de dedupe.
        if modo != Modo::Total && !e_touring(command) {
            self.bytes_depois += payload.len();
            self.intactos.0 += 1;
            self.registra_hook(hook, payload.len(), payload.len());
            return;
        }
        // Modo C: `touring-hook session-start` não roda mais com source=compact
        // (mudança de settings da 6b, 08/10) — o payload deixa de existir.
        if modo == Modo::Combinado && hook == "SessionStart:compact" {
            self.registra_hook(hook, payload.len(), 0);
            return;
        }
        // Modo C: o teto novo (2000) no cli-suggest — a única emissora que
        // consulta o turn_budget. Replica a ordem real via `apply_turn_budget`.
        let mut payload_eff = payload.to_string();
        if modo == Modo::Combinado && command.contains("cli-suggest") {
            let spent = *self.gasto_turno.get(sid).unwrap_or(&0);
            let (novo, novo_spent) = apply_turn_budget(payload, spent);
            self.gasto_turno.insert(sid.to_string(), novo_spent);
            payload_eff = novo;
        }
        let payload = payload_eff.as_str();

        let epoch = touring_hooks_shared::injection_dedup::current_epoch(root, sid);
        let out = dedup_hook_output(root, sid, payload);
        self.bytes_depois += out.len();
        self.registra_hook(hook, payload.len(), out.len());
        self.classificar(sid, epoch, payload, &out);
        self.controle_positivo(sid, epoch, payload, &out);
    }

    /// Diagnóstico por camada: intacto / omitido / reduzido (+ potencial de molde).
    fn classificar(&mut self, sid: &str, epoch: u64, payload: &str, out: &str) {
        let ctx_out = output_context(out);
        let economia = payload.len().saturating_sub(out.len());
        if out == payload {
            self.intactos.0 += 1;
            let ctx_in = output_context(payload);
            let m = molde_ctx(&ctx_in);
            let key = (sid.to_string(), epoch);
            let moldes = self.moldes_vistos.entry(key).or_default();
            if !moldes.insert(m) {
                self.intactos_molde_repetido.0 += 1;
                self.intactos_molde_repetido.1 += ctx_in.len();
            }
        } else if ctx_out.is_empty() {
            self.esvaziados.0 += 1;
            self.esvaziados.1 += economia;
        } else if ctx_out.trim_start().starts_with('↑') {
            self.referencia.0 += 1;
            self.referencia.1 += economia;
        } else {
            self.reduzidos.0 += 1;
            self.reduzidos.1 += economia;
        }
    }

    /// Os dois controles positivos do aceite: o primeiro payload de cada sessão
    /// passa intacto, e nenhuma diretiva se perde na 1ª emissão.
    fn controle_positivo(&mut self, sid: &str, epoch: u64, payload: &str, out: &str) {
        if epoch == 0 && !self.seen.contains_key(&(sid.to_string(), 0)) && out != payload {
            self.primeiro_payload_intacto = false;
        }
        let seen = self.seen.entry((sid.to_string(), epoch)).or_default();
        let ctx_out = output_context(out);
        let out_norm: String = ctx_out.split_whitespace().collect::<Vec<_>>().join(" ");
        for d in directive_lines(payload) {
            if !seen.contains(&d) {
                assert!(
                    out_norm.contains(&d),
                    "diretiva perdida na 1ª emissão (sessão {sid}, época {epoch}): {d}"
                );
                seen.insert(d);
            }
        }
    }
}

/// Mini-corpus embutido: prova de mecanismo sempre verde (sem a env).
fn mini_corpus() -> Vec<CorpusLine> {
    let bloco = |n: u8| {
        format!(
            "{{\"hookSpecificOutput\": {{\"additionalContext\": \
             \"Pre-edit prevention for foo.rs ({n} signal):\\n  \
             MUST    touring ast meta foo.rs --depth summary -j\\n  \
             MUST    touring ast blast foo.rs\\n  \
             SHOULD  touring pre-edit\\n  \
             Reason: arquivo quente da sessão {n}\"}}}}"
        )
    };
    let mk = |payload: String| CorpusLine {
        session_id: Some("sessao-mini".into()),
        payload: Some(payload),
        command: Some("touring cli-suggest: classifying operation".into()),
        hook_name: Some("PreToolUse:Bash".into()),
        compact_boundary: None,
        turn_boundary: None,
    };
    vec![
        mk(bloco(1)),
        mk(bloco(1)), // repetição exata → omitida
        mk(bloco(2)), // seção alterada → emitida (diretiva nova não se perde)
        mk(bloco(2)),
        CorpusLine {
            session_id: Some("sessao-mini".into()),
            payload: None,
            command: None,
            hook_name: None,
            compact_boundary: Some(true),
            turn_boundary: None,
        },
        mk(bloco(2)), // época nova → reemite
    ]
}

fn run_modo(lines: &[CorpusLine], modo: Modo) -> Ab {
    let tmp = tempfile::tempdir().expect("tempdir");
    let mut ab = Ab::new();
    for line in lines {
        ab.process(tmp.path(), line, modo);
    }
    ab
}

fn resumo(ab: &Ab, modo: &str, real: bool) -> String {
    let queda = 1.0 - (ab.bytes_depois as f64 / ab.bytes_antes.max(1) as f64);
    format!(
        "{{\"modo\": \"{modo}\", \"n\": {}, \"bytes_antes\": {}, \"bytes_depois\": {}, \
         \"queda\": {:.4}, \"real\": {real}, \"intactos\": [{}, {}], \"referencia\": [{}, {}], \
         \"reduzidos\": [{}, {}], \"esvaziados\": [{}, {}], \"molde_repetido_intacto\": [{}, {}]}}",
        ab.n_payloads,
        ab.bytes_antes,
        ab.bytes_depois,
        queda,
        ab.intactos.0,
        ab.intactos.1,
        ab.referencia.0,
        ab.referencia.1,
        ab.reduzidos.0,
        ab.reduzidos.1,
        ab.esvaziados.0,
        ab.esvaziados.1,
        ab.intactos_molde_repetido.0,
        ab.intactos_molde_repetido.1
    )
}

#[test]
fn corpus_ab_queda_e_controle_positivo() {
    assert!(
        std::env::var("TOURING_DEDUP_DISABLED").is_err(),
        "o kill switch não pode estar armado no A/B"
    );

    let (lines, real) = match std::env::var("S8_CORPUS_JSONL") {
        Ok(path) => {
            let text = std::fs::read_to_string(&path).expect("corpus legível");
            let lines: Vec<CorpusLine> = text
                .lines()
                .filter(|l| !l.trim().is_empty())
                .map(|l| serde_json::from_str(l).expect("linha JSONL válida"))
                .collect();
            (lines, true)
        }
        Err(_) => (mini_corpus(), false),
    };

    // O aceite A2 mede o COMBINADO (o binário novo completo: dedupe + teto 2000
    // + session-start:compact fora) — é o que o juiz mede ao vivo. Os modos
    // Total e DeployFiel são a decomposição por alavanca, impressa no relatório.
    let combinado = run_modo(&lines, Modo::Combinado);
    if real {
        let total = run_modo(&lines, Modo::Total);
        let fiel = run_modo(&lines, Modo::DeployFiel);
        println!("{}", resumo(&total, "A-total", real));
        println!("{}", resumo(&fiel, "B-deploy-fiel", real));
    }
    println!("{}", resumo(&combinado, "C-combinado", real));

    // A tabela por hookName do modo combinado (o relatório do brief a pede):
    // ordenada pela economia, em bytes/chamada antes→depois.
    let mut hooks: Vec<_> = combinado.por_hook.iter().collect();
    hooks.sort_by_key(|(_, (_, a, d))| std::cmp::Reverse(a.saturating_sub(*d)));
    for (hook, (n, antes, depois)) in hooks.iter().take(20) {
        println!(
            "POR_HOOK {n:>6} {:>7.1}→{:>7.1} B/chamada  {}",
            *antes as f64 / *n.max(&1) as f64,
            *depois as f64 / *n.max(&1) as f64,
            hook
        );
    }

    let queda = 1.0 - (combinado.bytes_depois as f64 / combinado.bytes_antes.max(1) as f64);
    assert!(
        combinado.primeiro_payload_intacto,
        "o primeiro payload de cada sessão passa intacto"
    );
    if real {
        assert!(
            combinado.n_payloads >= 200,
            "corpus degenerado: {} < 200",
            combinado.n_payloads
        );
        assert!(
            queda >= 0.40,
            "aceite A2 (combinado): queda {:.1}% < 40% (antes={} depois={})",
            queda * 100.0,
            combinado.bytes_antes,
            combinado.bytes_depois
        );
    } else {
        assert!(
            combinado.bytes_depois < combinado.bytes_antes,
            "mecanismo: repetição encolhe"
        );
    }
}
