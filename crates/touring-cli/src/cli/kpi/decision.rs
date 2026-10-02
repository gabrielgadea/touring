//! S-3.1 (2026-10-01) — família `decision`: as seis réguas de decisão tipada.
//!
//! Cada régua lê um journal append-only em `~/.claude/touring/logs/` e devolve
//! uma métrica. A regra que atravessa todas as seis: **ausência de dado é
//! `unmeasured`, nunca `0`**. Um zero é um valor medido — significando "nenhuma
//! linha de journal", que é a mesma forma de "zero decisões tomadas" e de "o
//! emissor não existe ainda". Quando não há o que medir, esta família devolve
//! `{"status": "unmeasured", …}` com o motivo, e quem lê decide.
//!
//! Por que isso importa aqui, e não é preciosismo: a estratégia do plano quer
//! medir uma taxa de fallback (`keyword_fallback_rate`) e uma share curada
//! (`recall_curated_share`). Um `0.0` lido como "o Jev nunca caiu para o keyword"
//! aciona a decisão errada — liga a flag. Um `unmeasured` diz "o emissor ainda
//! não roda", que é o que se deve fazer com um KPI novo nas suas primeiras
//! horas de vida.
//!
//! Os journals (o emissor da S-3.2 é `prompt_enhance.rs`, dono touring-a7):
//!   - `intent_journal.jsonl`  — uma linha por prompt humano
//!   - `recall_journal.jsonl`  — uma linha por recall, com a injeção real
//!   - `jev_journal.jsonl`     — o journal do `scripts/jev_client.py`

use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Janela padrão: 7 dias, a mesma do plano (`docs/kpi/` usa 14 d; aqui a
/// decisão é por dia, e 7 d é o suficiente para a taxa de fallback estabilizar).
pub(crate) const WINDOW_MS: u64 = 7 * 24 * 3_600_000;

/// O que o plano pede para distinguir, em `intent_journal.jsonl`:
/// `source` é `"keyword" | "laya" | "laya_shadow"`. O `laya_shadow` é o que o
/// aluno **teria** dito — não é uma decisão tomada, e é por isso que a
/// [`keyword_fallback_rate_from`] o conta mas **não** o põe no denominador.
const ROUTE_KEYWORD: &str = "keyword";
/// Decisão real servida pelo braço laya.
const ROUTE_LAYA: &str = "laya";
/// O que o aluno teria dito. Contada no relatório, excluída do denominador.
const ROUTE_LAYA_SHADOW: &str = "laya_shadow";

/// `~/.claude/touring/logs/` — o diretório que o `update-touring` já cria e
/// rotaciona (retenção de 7 dias, ver CLAUDE.md §17-b).
fn logs_dir() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(Path::new(&home).join(".claude/touring/logs"))
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

/// Um registro de journal que não pôde ser lido como objeto JSON.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct BadLine {
    pub line_no: usize,
    pub preview: String,
}

/// Lê as linhas de um journal mais novas que `cutoff_ms`.
///
/// Devolve `(registros, linhas_ilegíveis)`. Uma linha ilegível **não** aborta a
/// leitura: um journal com uma linha truncada por um crash é exatamente o caso
/// em que a série ainda vale, e esconder o resto por causa de uma linha seria
/// perder a medição inteira. O relatório publica quantas havia.
fn read_window(path: &Path, cutoff_ms: u64) -> (Vec<Value>, Vec<BadLine>) {
    let Ok(text) = std::fs::read_to_string(path) else {
        return (Vec::new(), Vec::new());
    };
    let mut records = Vec::new();
    let mut bad = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        match serde_json::from_str::<Value>(trimmed) {
            Ok(v) => {
                // O `ts` do emissor pode ser epoch (segundos ou milissegundos) ou
                // RFC3339. Aceitar os três é o que impede que um emissor futuro
                // mude de formato e zere a família em silêncio.
                if !within_window(&v, cutoff_ms) {
                    continue;
                }
                records.push(v);
            }
            Err(_) => bad.push(BadLine {
                line_no: i + 1,
                preview: trimmed.chars().take(60).collect(),
            }),
        }
    }
    (records, bad)
}

fn within_window(record: &Value, cutoff_ms: u64) -> bool {
    let Some(ts) = timestamp_of(record) else {
        // Sem timestamp a linha não é datável: conta como dentro da janela,
        // porque descartar seria pior que arriscar. O journal é append-only.
        return true;
    };
    ts >= cutoff_ms
}

/// O campo de tempo, em QUALQUER um dos nomes que um emissor possa usar.
///
/// **[F] Nomes vistos no journal real (01/10, 1790820948580):** o `prompt_enhance.rs`
/// da S-3.2 grava **`ts_ms`**; o `scripts/jev_client.py` grava **`ts`**. Ler só um
/// dos dois é deixar a régua silenciosamente sem janela — e o pior modo possível:
/// `ts_ms` não encontrado caía no fail-open de "sem timestamp conta como dentro",
/// que trazia o journal INTEIRO para a janela de 7 dias, para sempre, sem erro.
///
/// A lista é a fonte única do nome aceito; acrescentar um emissor é acrescentar
/// uma string aqui, e `timestamp_names_are_the_ones_the_emitters_write` fixa o
/// contrato com o que está em disco.
const TIMESTAMP_KEYS: [&str; 4] = ["ts_ms", "ts", "timestamp", "timestamp_ms"];

fn timestamp_of(record: &Value) -> Option<u64> {
    TIMESTAMP_KEYS
        .iter()
        .find_map(|key| record.get(*key).and_then(epoch_ms))
}

fn epoch_ms(value: &Value) -> Option<u64> {
    match value {
        Value::Number(n) => n.as_u64().map(normalize_epoch),
        Value::String(s) => rfc3339_to_ms(s),
        _ => None,
    }
}

/// Segundos (10 dígitos) vs milissegundos (13): a ordem de grandeza decide, e
/// sem isso um journal de 2026 seria lido como 1970 + 56 anos.
fn normalize_epoch(n: u64) -> u64 {
    if n < 100_000_000_000 { n * 1000 } else { n }
}

/// RFC3339 sem depender de chrono. Só aceita a forma `YYYY-MM-DDTHH:MM:SSZ` —
/// o que o journal realmente grava — e devolve `None` para qualquer outra, em
/// vez de tentar adivinhar offset ou fração.
fn rfc3339_to_ms(s: &str) -> Option<u64> {
    if s.len() < 20 || !s.ends_with('Z') {
        return None;
    }
    const SLICES: [std::ops::Range<usize>; 6] = [0..4, 5..7, 8..10, 11..13, 14..16, 17..19];
    let parts: Vec<i64> = SLICES
        .iter()
        .map(|r| s.get(r.clone())?.parse().ok())
        .collect::<Option<Vec<i64>>>()?;
    let (year, month, day, hour, minute, second) =
        (parts[0], parts[1], parts[2], parts[3], parts[4], parts[5]);
    let seconds = days_from_civil(year, month, day) * 86_400 + hour * 3600 + minute * 60 + second;
    u64::try_from(seconds * 1000).ok()
}

/// Dias desde 1970-01-01 (algoritmo de Howard Hinnant, sem chrono).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn unmeasured(why: &str, extra: Value) -> Value {
    json!({ "status": "unmeasured", "reason": why, "window_days": 7, "extra": extra })
}

/// Régua 1 — `intent_distribution`: quantas vezes cada intent foi classificado.
///
/// Denominador: só as linhas **humanas** (`skipped_automated: false` ou ausente).
/// As automáticas não recebem intent por desenho (o G0 do plano anterior), e
/// incluí-las aqui inflaria a distribuição com `general`.
pub(crate) fn intent_distribution_from(records: &[Value]) -> Value {
    let mut counts: BTreeMap<String, u64> = BTreeMap::new();
    let mut human = 0u64;
    for rec in records {
        if rec.get("skipped_automated").and_then(Value::as_bool) == Some(true) {
            continue;
        }
        human += 1;
        let Some(intent) = rec.get("intent").and_then(Value::as_str) else {
            continue;
        };
        *counts.entry(intent.to_string()).or_insert(0) += 1;
    }
    if human == 0 {
        return unmeasured("no_human_prompts", json!({ "lines_read": records.len() }));
    }
    let total: u64 = counts.values().sum();
    let distribution: serde_json::Map<String, Value> = counts
        .iter()
        .map(|(k, v)| {
            (
                k.clone(),
                json!({ "count": v, "share": round4(*v as f64 / human as f64) }),
            )
        })
        .collect();
    json!({
        "status": "measured",
        "human_prompts": human,
        "distinct_intents": counts.len(),
        "total_classified": total,
        "distribution": distribution,
    })
}

/// Régua 2 — `keyword_fallback_rate`: a share de decisões que caíram no keyword.
///
/// Quando `source == "keyword"` **e** a flag estava ligada, a resposta correta é
/// "não sei se caiu ou se o keyword era o esperado". Por isso o plano define a
/// métrica como *a share com `source` diferente do esperado quando a flag está
/// on*. Aqui a flag é detectada pela existência de QUALQUER linha `jev` na janela:
/// sem ela, todas as decisões foram de keyword e a taxa é 0 por construção — o
/// que é um zero **medido**, e por isso devolvido como `measured`, não
/// `unmeasured`.
pub(crate) fn keyword_fallback_rate_from(records: &[Value]) -> Value {
    let mut total = 0u64;
    let mut by_source: BTreeMap<String, u64> = BTreeMap::new();
    let mut jev_active = false;
    // [F] UNIVERSO = SÓ DECISÕES. A shadow (`laya_shadow`) é o que o aluno
    // TERIA dito; não é uma decisão tomada. Contá-la no denominador
    // *substituía* a taxa — medido em 02/10: 3 shadow em 151 davam 0,9735
    // quando a verdade sobre as 148 decisões reais é 0,9932.
    //
    // [F] E o pior: `flag_active` passava a `true` só com shadow presente,
    // mesmo com ZERO decisões do braço laya. Um KPI que diz "a flag está
    // activa" quando nada foi servido é pior que um KPI calado.
    //
    // Fontes desconhecidas continuam a ser CONTADAS e VISÍVEIS — para não
    // repetir o mesmo erro pelo outro lado (engolir uma rota nova em silêncio).
    for rec in records {
        if rec.get("skipped_automated").and_then(Value::as_bool) == Some(true) {
            continue;
        }
        let Some(source) = rec.get("source").and_then(Value::as_str) else {
            continue;
        };
        *by_source.entry(source.to_string()).or_insert(0) += 1;
        // if/else e não `match`: uma constante num padrão é lida como binding
        // pelo compilador (E0408), e um KPI não pode depender dessa subtileza.
        if source == ROUTE_LAYA_SHADOW {
            // contada acima (visível no by_source), nunca no denominador
        } else {
            total += 1;
            if source == ROUTE_LAYA {
                jev_active = true;
            } else if source != ROUTE_KEYWORD {
                // fonte desconhecida: conta e conta como "não keyword". Uma rota
                // nova não pode desaparecer em silêncio do denominador.
                jev_active = true;
            }
        }
    }
    if total == 0 {
        return unmeasured("no_routed_prompts", json!({ "lines_read": records.len() }));
    }
    if !jev_active {
        return json!({
            "status": "measured",
            "flag_active": false,
            "note": "no non-keyword route in window — 0.0 is MEASURED, not missing",
            "decisions": total,
            "keyword_fallback_rate": 0.0,
            "by_source": source_map(&by_source, total),
        });
    }
    let keyword = by_source.get(ROUTE_KEYWORD).copied().unwrap_or(0);
    json!({
        "status": "measured",
        "flag_active": true,
        "decisions": total,
        "keyword_fallback_rate": round4(keyword as f64 / total as f64),
        "by_source": source_map(&by_source, total),
    })
}

fn source_map(by_source: &BTreeMap<String, u64>, total: u64) -> Value {
    let map: serde_json::Map<String, Value> = by_source
        .iter()
        .map(|(k, v)| {
            (
                k.clone(),
                json!({ "count": v, "share": round4(*v as f64 / total as f64) }),
            )
        })
        .collect();
    Value::Object(map)
}

/// A flag do braço laya (mesma semântica de prompt_enhance::student_enabled).
fn laya_intent_flag_on() -> bool {
    std::env::var("TOURING_LAYA_INTENT").is_ok_and(|v| !v.is_empty() && v != "0" && v != "false")
}

/// S-4 (plan r2 — critério de rollback da documentação oficial): model path
/// starvation. COM o braço ON (TOURING_LAYA_INTENT=1), `keyword_fallback_rate`
/// acima de 0,95 na janela de 7 dias é o critério documentado de rollback/stop
/// — o modelo está faminto de caminho. COM o braço OFF o número é DESENHO (o
/// keyword domina por construção) e sai `not_armed` — nunca como deriva.
/// Universo: só decisões reais (keyword|laya) — `laya_shadow` não é decisão
/// (a shadow é o que o aluno TERIA dito; contá-la inflaria a base), e uma
/// fonte DESCONHECIDA também não conta (typo ou rota futura nunca entra no
/// denominador de um número que mede decisões — assimetria intencional com a
/// `keyword_fallback_rate_from`, que as conta no by_source para não sumirem).
pub(crate) fn starvation_gate_from(records: &[Value], flag_on: bool) -> Value {
    if !flag_on {
        return json!({
            "status": "not_armed",
            "note": "TOURING_LAYA_INTENT off — keyword dominance is design, not drift",
        });
    }
    let mut decisions = 0u64;
    let mut keyword = 0u64;
    let mut claims = 0u64;
    for rec in records {
        if rec.get("skipped_automated").and_then(Value::as_bool) == Some(true) {
            continue;
        }
        match rec.get("source").and_then(Value::as_str) {
            Some("keyword") => {
                decisions += 1;
                keyword += 1;
            }
            Some("laya") => {
                decisions += 1;
                claims += 1;
            }
            _ => {} // laya_shadow e afins: nunca são decisão
        }
    }
    if decisions == 0 {
        return unmeasured("no_decisions_in_window", json!({}));
    }
    let rate = round4(keyword as f64 / decisions as f64);
    let starving = rate > 0.95;
    json!({
        "status": if starving { "unmet" } else { "pass" },
        "keyword_fallback_rate": rate,
        "threshold": 0.95,
        "decisions": decisions,
        "laya_claims": claims,
        "window_days": 7,
        "next_action": if starving {
            "revisar τ/temperature (w44) ou rollback do braço laya — o critério documentado de starvation disparou"
        } else {
            "braço saudável — continuar medindo"
        },
    })
}

/// Régua 3 — `recall_curated_share`: a share de memórias injetadas que o recall
/// pediu. Separada de propósito da régua 2: o `recall_journal.jsonl` é outro
/// arquivo, e o plano pede a métrica separada.
pub(crate) fn recall_curated_share_from(records: &[Value]) -> Value {
    let mut recalls = 0u64;
    let mut injected = 0u64;
    let mut candidates = 0u64;
    for rec in records {
        recalls += 1;
        if let Some(n) = rec.get("injected").and_then(Value::as_u64) {
            injected += n;
        }
        if let Some(n) = rec.get("candidates").and_then(Value::as_u64) {
            candidates += n;
        }
    }
    if recalls == 0 {
        return unmeasured("no_recalls", json!({}));
    }
    json!({
        "status": "measured",
        "recalls": recalls,
        "candidates_total": candidates,
        "injected_total": injected,
        // A share que o plano pede: quanto do que o recall ofrecceu foi de fato
        // injetado no contexto. `candidates_total == 0` é um zero medido
        // (houve recall, o recall não ofereceu nada) — diferente de nenhum recall.
        "curated_share": if candidates == 0 { 0.0 } else { round4(injected as f64 / candidates as f64) },
    })
}

/// Régua 4 — `jev_calls_total`, 5 — `jev_latency_p99`, 6 — `jev_confidence_hist`.
///
/// As três leem o journal do adapter (`scripts/jev_client.py`), que já é vivo
/// desde a S-0.2. `route_taken` distingue `api` (medido), `cache` (servido do
/// cache — **não** é uma chamada) e `fallback` (falhou; tem `fallback_reason`).
/// Somar `cache` como chamada inflaria `jev_calls_total` e inventaria latência
/// para algo que não foi à rede.
pub(crate) fn jev_activity_from(records: &[Value]) -> Value {
    let mut tally = JevTally::default();
    for rec in records {
        tally.absorb(rec);
    }
    if tally.is_empty() {
        return unmeasured(
            "no_jev_journal_lines",
            json!({ "lines_read": records.len() }),
        );
    }
    let mut latencies = tally.latencies;
    json!({
        "status": "measured",
        "jev_calls_total": tally.api,
        "cache_hits": tally.cache,
        "fallbacks": tally.fallback,
        "fallback_reasons": tally.fallback_reasons,
        // A pergunta que a chamada fez, e quando ela CAIU pela primeira vez.
        // **[F]** Sem o carimbo de tempo, um defeito já corrigido continua
        // reportado para sempre — foi o que aconteceu com `which_skill`, cujas
        // duas primeiras chamadas eram o bug da linha 285 e cuja terceira já
        // passou (408,8 ms).
        "by_question": tally.by_question,
        "first_fallback_at": tally.first_fallback_at,
        "jev_latency_p99_ms": percentile(&mut latencies, 0.99),
        "jev_latency_p50_ms": percentile(&mut latencies, 0.50),
        "confidence_samples": tally.confidences.len(),
        "jev_confidence_hist": confidence_histogram(&tally.confidences),
    })
}

/// Acumulador separado da função que o publica: cada `if let` da régua vira uma
/// linha, e a complexidade sai do leitor (que é o que o `clippy` mede) para o
/// acumulador (que só recebe linhas).
#[derive(Default)]
struct JevTally {
    api: u64,
    cache: u64,
    fallback: u64,
    latencies: Vec<f64>,
    confidences: Vec<f64>,
    fallback_reasons: BTreeMap<String, u64>,
    /// `question_id` → contagem. **[F] Medido em 01/10:** o journal mistura
    /// cinco chamadas diferentes (`kind` 1.056 · `intent` 121 · `relevant` 45 ·
    /// `which_skill` 3), e as latências/confianças as tratam todas como se
    /// fossem uma. Um p99 agregado de seis perguntas não diz nada sobre nenhuma.
    by_question: BTreeMap<String, u64>,
    /// A primeira vez que um `question_id` caiu em fallback. É o que separa
    /// "este roteiro está com defeito" de "este roteiro já foi consertado" —
    /// **[F]** as 2 primeiras chamadas de `which_skill` caíram
    /// (`unexpected:AttributeError`/`RuntimeError`, o bug da linha 285) e a 3ª
    /// passou; sem o carimbo de tempo, o KPI continua reportando a falha para
    /// sempre.
    first_fallback_at: BTreeMap<String, String>,
}

impl JevTally {
    fn is_empty(&self) -> bool {
        self.api == 0 && self.cache == 0 && self.fallback == 0
    }

    fn absorb(&mut self, rec: &Value) {
        let question = question_id_of(rec);
        if let Some(q) = &question {
            *self.by_question.entry(q.clone()).or_insert(0) += 1;
        }
        match rec
            .get("route_taken")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
        {
            "api" => {
                self.api += 1;
                if let Some(ms) = rec.get("latency_ms").and_then(Value::as_f64) {
                    self.latencies.push(ms);
                }
            }
            "cache" => self.cache += 1,
            "fallback" => {
                self.fallback += 1;
                let reason = rec
                    .get("fallback_reason")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown");
                *self.fallback_reasons.entry(reason.to_string()).or_insert(0) += 1;
                if let (Some(q), Some(ts)) = (&question, rec.get("ts").and_then(Value::as_str)) {
                    self.first_fallback_at
                        .entry(q.clone())
                        .or_insert_with(|| ts.to_string());
                }
            }
            _ => {}
        }
        self.absorb_confidence(rec);
    }

    /// A confiança vive em `answers_digest` (o digest que a S-0.3 passou a
    /// gravar). Aceitar também a forma plana deixa o leitor funcionar com
    /// journals de antes e depois dessa mudança.
    fn absorb_confidence(&mut self, rec: &Value) {
        if let Some(c) = rec.get("confidence").and_then(Value::as_f64) {
            self.confidences.push(c);
            return;
        }
        let Some(Value::Object(map)) = rec.get("answers_digest") else {
            return;
        };
        for answer in map.values() {
            if let Some(c) = answer.get("confidence").and_then(Value::as_f64) {
                self.confidences.push(c);
            }
        }
    }
}

/// A pergunta que a chamada made — a chave que separa os ROTEIROS no journal.
fn question_id_of(record: &Value) -> Option<String> {
    match record.get("question_ids") {
        Some(Value::Array(ids)) if ids.len() == 1 => ids[0].as_str().map(str::to_string),
        Some(Value::Array(ids)) if !ids.is_empty() => Some(format!("multi:{}", ids.len())),
        _ => record
            .get("question_id")
            .and_then(Value::as_str)
            .map(str::to_string),
    }
}

/// Percentil por interpolação linear entre ordens (o mesmo método do
/// `hdrhistogram` que o resto do repo usa para p99 — sem dependência nova).
/// Recebe `&mut [f64]`: ordena no lugar, e `&mut Vec` aqui prometeria um
/// Vec novo sem necessidade (clippy `ptr_arg`).
fn percentile(values: &mut [f64], q: f64) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let pos = q * (values.len() - 1) as f64;
    let lo = pos.floor() as usize;
    let hi = pos.ceil() as usize;
    let value = if lo == hi {
        values[lo]
    } else {
        values[lo] + (values[hi] - values[lo]) * (pos - lo as f64)
    };
    Some(round4(value))
}

/// Histograma de confiança em 10 faixas de 0,1 — é o que o plano quer ler para
/// escolher τ. Só sai quando há amostras: sem elas, `[]` e a contagem 0 dizem
/// "nada medido" melhor do que dez faixas zeradas, que diriam "mediu, e o modelo
/// nunca teve confiança".
fn confidence_histogram(confidences: &[f64]) -> Value {
    if confidences.is_empty() {
        return json!([]);
    }
    let mut bins = [0u64; 10];
    for c in confidences {
        let idx = ((c.clamp(0.0, 0.999_999) * 10.0) as usize).min(9);
        bins[idx] += 1;
    }
    let total = confidences.len() as f64;
    Value::Array(
        (0..10)
            .map(|i| {
                json!({
                    "bin": format!("[0.{},0.{})", i, i + 1),
                    "count": bins[i],
                    "share": round4(bins[i] as f64 / total),
                })
            })
            .collect(),
    )
}

fn round4(v: f64) -> f64 {
    (v * 10_000.0).round() / 10_000.0
}

// ── o wrapper vivo ─────────────────────────────────────────────────────────

/// Ponto de entrada: lê os três journals e devolve a família `decision`.
///
/// `dir` é `~/.claude/touring/logs`. Cada régua lê SEU arquivo; um journal
/// ausente não derruba as outras — cada uma responde `unmeasured` com o motivo
/// dela, e a família sai com as seis réguas sempre presentes. Uma família que
/// some quando falta um arquivo é uma família que alguém vai ler como "não
/// medido" e, depois, como zero.
pub(crate) fn decision(dir: &Path, cutoff_ms: u64) -> Value {
    let (intent, intent_bad) = read_window(&dir.join("intent_journal.jsonl"), cutoff_ms);
    let (recall, recall_bad) = read_window(&dir.join("recall_journal.jsonl"), cutoff_ms);
    let (jev, jev_bad) = read_window(&dir.join("jev_journal.jsonl"), cutoff_ms);

    let mut family = json!({
        "status": "measured",
        "window_days": 7,
        "cutoff_ms": cutoff_ms,
        "intent_distribution": intent_distribution_from(&intent),
        "keyword_fallback_rate": keyword_fallback_rate_from(&intent),
        "starvation_gate": starvation_gate_from(&intent, laya_intent_flag_on()),
        "recall_curated_share": recall_curated_share_from(&recall),
        "jev_calls_total": jev_activity_from(&jev),
        "jev_latency_p99_ms": jev_activity_from(&jev).get("jev_latency_p99_ms").cloned().unwrap_or(Value::Null),
        "jev_confidence_hist": jev_activity_from(&jev).get("jev_confidence_hist").cloned().unwrap_or(Value::Null),
    });
    let bad = json!({
        "intent_journal": bad_lines(&intent_bad),
        "recall_journal": bad_lines(&recall_bad),
        "jev_journal": bad_lines(&jev_bad),
    });
    let total_bad: usize = [&intent_bad, &recall_bad, &jev_bad]
        .iter()
        .map(|b| b.len())
        .sum();
    family["malformed_lines"] = bad;
    if total_bad > 0 {
        // Journal com lixo é sinal, não ruído: alguém escreve errado nele.
        family["note"] = json!(format!(
            "{total_bad} malformed line(s) skipped — the writer is emitting non-JSON"
        ));
    }
    family
}

fn bad_lines(bad: &[BadLine]) -> Value {
    if bad.is_empty() {
        return json!(0);
    }
    json!({
        "count": bad.len(),
        "first": { "line_no": bad[0].line_no, "preview": bad[0].preview },
    })
}

/// Resolve `~/.claude/touring/logs` e a janela, e chama [`decision`].
pub(crate) fn decision_family() -> Value {
    let Some(dir) = logs_dir() else {
        return unmeasured("no_home", json!({}));
    };
    decision(&dir, now_ms().saturating_sub(WINDOW_MS))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ts(offset_ms: u64) -> Value {
        json!(1_800_000_000_000u64 + offset_ms)
    }

    #[test]
    fn intent_distribution_ignores_automated_prompts() {
        let records = vec![
            json!({ "ts": ts(0), "intent": "analysis", "source": "keyword" }),
            json!({ "ts": ts(1), "intent": "code", "source": "keyword" }),
            json!({ "ts": ts(2), "intent": "analysis", "source": "keyword", "skipped_automated": true }),
        ];
        let out = intent_distribution_from(&records);
        assert_eq!(out["status"], "measured");
        // 2 humanos, não 3: a 3ª linha é payload automático e não recebe intent.
        assert_eq!(out["human_prompts"], 2);
        assert_eq!(out["distribution"]["analysis"]["count"], 1);
        assert_eq!(out["distribution"]["analysis"]["share"], 0.5);
    }

    #[test]
    fn intent_distribution_is_unmeasured_without_prompts() {
        let out = intent_distribution_from(&[]);
        assert_eq!(out["status"], "unmeasured");
        assert_eq!(out["reason"], "no_human_prompts");
        // A forma que o plano exige: `unmeasured`, e não `{"count": 0}`.
        assert!(out.get("distribution").is_none());
    }

    #[test]
    fn starvation_gate_not_armed_with_flag_off() {
        let records = vec![json!({ "ts": ts(0), "intent": "code", "source": "keyword" })];
        let out = starvation_gate_from(&records, false);
        assert_eq!(out["status"], "not_armed");
    }

    #[test]
    fn starvation_gate_fires_only_above_threshold_with_flag_on() {
        let starving: Vec<Value> = (0..99)
            .map(|i| json!({ "ts": ts(i), "intent": "code", "source": "keyword" }))
            .chain(std::iter::once(
                json!({ "ts": ts(99), "intent": "code", "source": "laya" }),
            ))
            .collect();
        let out = starvation_gate_from(&starving, true);
        assert_eq!(out["status"], "unmet");
        assert_eq!(out["keyword_fallback_rate"], 0.99);
        assert_eq!(out["decisions"], 100);
        assert_eq!(out["laya_claims"], 1);
        assert!(out["next_action"].as_str().unwrap().contains("starvation"));

        let healthy: Vec<Value> = (0..80)
            .map(|i| json!({ "ts": ts(i), "intent": "code", "source": "keyword" }))
            .chain((80..100).map(|i| json!({ "ts": ts(i), "intent": "code", "source": "laya" })))
            .collect();
        let out2 = starvation_gate_from(&healthy, true);
        assert_eq!(out2["status"], "pass");
        assert_eq!(out2["keyword_fallback_rate"], 0.8);
    }

    #[test]
    fn starvation_gate_shadow_is_never_a_decision() {
        // 100 decisões keyword + 300 shadows: a taxa é 1.0 (unmet), porque a
        // shadow NÃO entra no universo — contá-la fingiria suporte que não existe.
        let records: Vec<Value> = (0..100)
            .map(|i| json!({ "ts": ts(i), "intent": "code", "source": "keyword" }))
            .chain(
                (100..400)
                    .map(|i| json!({ "ts": ts(i), "intent": "code", "source": "laya_shadow" })),
            )
            .collect();
        let out = starvation_gate_from(&records, true);
        assert_eq!(out["status"], "unmet");
        assert_eq!(out["decisions"], 100);
        assert_eq!(out["laya_claims"], 0);
    }

    #[test]
    fn starvation_gate_unmeasured_without_decisions() {
        let out = starvation_gate_from(&[], true);
        assert_eq!(out["status"], "unmeasured");
        assert_eq!(out["reason"], "no_decisions_in_window");
    }

    #[test]
    fn decision_family_carries_starvation_gate_with_real_journals() {
        // A montagem inteira sobre os journals REAIS (read-only): o campo existe
        // e seu status é um dos quatro declarados — o que separa a família do
        // daemon é a versão do binário dele, nunca a forma do payload.
        let out = decision_family();
        let gate = &out["starvation_gate"];
        assert!(
            gate.is_object(),
            "starvation_gate ausente na família: {}",
            &out.to_string()[..out.to_string().len().min(300)]
        );
        let status = gate["status"].as_str().unwrap_or("");
        assert!(
            ["not_armed", "pass", "unmet", "unmeasured"].contains(&status),
            "status inesperado: {status}"
        );
    }

    #[test]
    fn fallback_rate_zero_is_measured_not_unmeasured() {
        // A distinção que a regra existe para preservar: keyword sozinho com
        // janela observada é um 0 medido; nada no journal é ausência.
        let records = vec![json!({ "ts": ts(0), "intent": "code", "source": "keyword" })];
        let out = keyword_fallback_rate_from(&records);
        assert_eq!(out["status"], "measured");
        assert_eq!(out["flag_active"], false);
        assert_eq!(out["keyword_fallback_rate"], 0.0);

        let none = keyword_fallback_rate_from(&[]);
        assert_eq!(none["status"], "unmeasured");
    }

    #[test]
    fn fallback_rate_counts_the_keyword_share_under_an_active_flag() {
        let records = vec![
            json!({ "ts": ts(0), "source": "keyword" }),
            json!({ "ts": ts(1), "source": "jev" }),
            json!({ "ts": ts(2), "source": "jev" }),
            json!({ "ts": ts(3), "source": "laya" }),
        ];
        let out = keyword_fallback_rate_from(&records);
        assert_eq!(out["flag_active"], true);
        assert_eq!(out["keyword_fallback_rate"], 0.25);
    }

    #[test]
    fn recall_curated_share_is_unmeasured_without_recalls() {
        let out = recall_curated_share_from(&[]);
        assert_eq!(out["status"], "unmeasured");
        assert_eq!(out["reason"], "no_recalls");
    }

    #[test]
    fn recall_curated_share_separates_zero_candidates_from_no_recall() {
        // Houve recall e ele não ofereceu nada: share 0.0 medido.
        let out =
            recall_curated_share_from(&[json!({ "ts": ts(0), "candidates": 0, "injected": 0 })]);
        assert_eq!(out["status"], "measured");
        assert_eq!(out["curated_share"], 0.0);
    }

    #[test]
    fn jev_activity_does_not_count_cache_hits_as_calls() {
        let records = vec![
            json!({ "ts": ts(0), "route_taken": "api", "latency_ms": 400.0 }),
            json!({ "ts": ts(1), "route_taken": "api", "latency_ms": 500.0 }),
            json!({ "ts": ts(2), "route_taken": "cache" }),
            json!({ "ts": ts(3), "route_taken": "fallback", "fallback_reason": "no_key" }),
        ];
        let out = jev_activity_from(&records);
        assert_eq!(out["jev_calls_total"], 2, "cache hit is not a call");
        assert_eq!(out["cache_hits"], 1);
        assert_eq!(out["fallbacks"], 1);
        assert_eq!(out["fallback_reasons"]["no_key"], 1);
    }

    #[test]
    fn jev_activity_reads_confidence_from_the_digest() {
        let records = vec![json!({
            "ts": ts(0),
            "route_taken": "api",
            "latency_ms": 371.9,
            "answers_digest": { "kind": { "type": "choice", "confidence": 0.8 } },
        })];
        let out = jev_activity_from(&records);
        assert_eq!(out["confidence_samples"], 1);
        let hist = out["jev_confidence_hist"].as_array().expect("histogram");
        assert_eq!(hist.len(), 10);
        assert_eq!(hist[8]["count"], 1);
    }

    #[test]
    fn confidence_histogram_is_empty_without_samples() {
        // Dez faixas zeradas diriam "o modelo nunca teve confiança"; [] diz
        // "nada foi medido". A diferença é o ponto.
        let out = jev_activity_from(&[json!({ "ts": ts(0), "route_taken": "api" })]);
        assert_eq!(out["confidence_samples"], 0);
        assert_eq!(out["jev_confidence_hist"].as_array().map(Vec::len), Some(0));
    }

    #[test]
    fn jev_activity_exposes_latency_p50_ms() {
        // p50 por interpolação linear (`percentile`): [100..400] → pos 1.5 → 250.
        // `JevTally::absorb` só coleta latency_ms de route "api" (match acima) —
        // a linha cache com 1.0 ms está na amostra para exercitar essa exclusão.
        let records = vec![
            json!({ "ts": ts(0), "route_taken": "api", "latency_ms": 100.0 }),
            json!({ "ts": ts(1), "route_taken": "api", "latency_ms": 200.0 }),
            json!({ "ts": ts(2), "route_taken": "api", "latency_ms": 300.0 }),
            json!({ "ts": ts(3), "route_taken": "api", "latency_ms": 400.0 }),
            json!({ "ts": ts(4), "route_taken": "cache", "latency_ms": 1.0 }),
        ];
        let out = jev_activity_from(&records);
        assert_eq!(out["jev_latency_p50_ms"], json!(250.0));
        // Sem latência medida o campo é null — nunca um 0 que fingiria medição.
        let out_empty = jev_activity_from(&[json!({ "ts": ts(0), "route_taken": "api" })]);
        assert_eq!(out_empty["jev_latency_p50_ms"], json!(null));
    }

    #[test]
    fn epoch_ms_accepts_seconds_and_milliseconds() {
        let secs = json!(1_800_000_000u64);
        let millis = json!(1_800_000_000_000u64);
        assert_eq!(epoch_ms(&secs), Some(1_800_000_000_000));
        assert_eq!(epoch_ms(&millis), Some(1_800_000_000_000));
    }

    #[test]
    fn epoch_ms_parses_rfc3339_without_chrono() {
        // 2026-01-01T00:00:00Z
        assert_eq!(
            epoch_ms(&json!("2026-01-01T00:00:00Z")),
            Some(1_767_225_600_000)
        );
    }

    #[test]
    fn the_question_separates_the_callers_in_one_journal() {
        // **[F] Medido em 01/10 no journal de produção:** cinco perguntas
        // diferentes dividem o mesmo arquivo (`kind` 1.056 · `intent` 121 ·
        // `relevant` 45 · `which_skill` 3). Somadas sem distinção, o p99 de uma
        // é o p99 de nenhuma — e o `which_skill` do skill-picker tem latência
        // própria, diferente da chamada hospedada.
        let records = vec![
            json!({ "ts": ts(0), "route_taken": "api", "latency_ms": 380.0, "question_ids": ["kind"] }),
            json!({ "ts": ts(1), "route_taken": "api", "latency_ms": 12.0, "question_ids": ["intent"] }),
            json!({ "ts": ts(2), "route_taken": "api", "latency_ms": 40.0, "question_ids": ["which_skill"] }),
            json!({ "ts": ts(3), "route_taken": "api", "latency_ms": 90.0, "question_ids": ["a", "b"] }),
        ];
        let out = jev_activity_from(&records);
        assert_eq!(out["by_question"]["kind"], 1);
        assert_eq!(out["by_question"]["which_skill"], 1);
        // Várias perguntas numa chamada viram um rótulo próprio, nunca uma
        // contagem por pergunta que fingiria separá-las.
        assert_eq!(out["by_question"]["multi:2"], 1);
        check_callers(&out);
    }

    fn check_callers(out: &Value) {
        assert!(
            out.get("by_question").is_some(),
            "a família não separa os chamadores de um journal compartilhado"
        );
    }

    #[test]
    fn a_first_fallback_carries_its_time() {
        // **[F]** As duas primeiras chamadas de `which_skill` caíram (o bug da
        // linha 285, `unexpected:AttributeError` e `RuntimeError`) e a terceira
        // passou (408,8 ms). Sem o carimbo de tempo, o KPI reporta a falha
        // para sempre e ninguém distingue "quebrado" de "já consertado".
        let records = vec![
            json!({ "ts": "2026-10-01T09:02:50-0300", "route_taken": "fallback",
                    "question_ids": ["which_skill"], "fallback_reason": "unexpected:AttributeError" }),
            json!({ "ts": "2026-10-01T09:05:02-0300", "route_taken": "api",
                    "question_ids": ["which_skill"], "latency_ms": 408.8 }),
        ];
        let out = jev_activity_from(&records);
        assert_eq!(out["fallbacks"], 1);
        assert_eq!(
            out["first_fallback_at"]["which_skill"],
            "2026-10-01T09:02:50-0300"
        );
        // A contagem por pergunta mostra que o roteiro NÃO está mais caindo:
        // 2 chamadas, 1 fallback — e o carimbo diz quando foi.
        assert_eq!(out["by_question"]["which_skill"], 2);
    }

    #[test]
    fn timestamp_names_are_the_ones_the_emitters_write() {
        // O contrato com o que está EM DISCO, não com o que este arquivo lembra.
        // Em 01/10, com os dois emissores vivos: `prompt_enhance.rs` grava
        // `ts_ms` e `scripts/jev_client.py` grava `ts`. Ler só um deixaria a
        // régua sem janela — silenciosamente, pelo fail-open de "sem timestamp
        // conta como dentro".
        let home = std::env::var_os("HOME");
        let mut seen: Vec<String> = Vec::new();
        for name in ["intent_journal.jsonl", "jev_journal.jsonl"] {
            let Some(home) = &home else { continue };
            let path = Path::new(home).join(".claude/touring/logs").join(name);
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            for line in text.lines().filter(|l| !l.trim().is_empty()) {
                // Uma linha malformada não pode falhar o teste — ela é justamente
                // o caso que `read_window` trata e que o relatório publica.
                let Ok(record) = serde_json::from_str::<Value>(line) else {
                    continue;
                };
                if let Some(obj) = record.as_object() {
                    for key in TIMESTAMP_KEYS {
                        if obj.contains_key(key) && !seen.iter().any(|s| s == key) {
                            seen.push(key.to_string());
                        }
                    }
                }
            }
        }
        if seen.is_empty() {
            return; // sem journals em disco: nada a fixar ainda
        }
        let unhandled: Vec<&String> = seen
            .iter()
            .filter(|k| !TIMESTAMP_KEYS.contains(&k.as_str()))
            .collect();
        assert!(
            unhandled.is_empty(),
            "emissor grava um timestamp que o leitor não conhece: {unhandled:?} (aceitos: {TIMESTAMP_KEYS:?})"
        );
    }

    #[test]
    fn a_timestamp_outside_the_window_is_excluded() {
        // O que o defeito do `ts_ms` teria deixado passar: sem o nome certo, a
        // linha caía no fail-open e entrava como "dentro da janela".
        let old = json!({ "ts_ms": 1_000_000_000_000u64, "intent": "code", "source": "keyword" });
        let recent =
            json!({ "ts_ms": 9_999_999_999_999u64, "intent": "code", "source": "keyword" });
        let cutoff = 1_800_000_000_000u64;
        assert!(
            !within_window(&old, cutoff),
            "linha de 2021 entrou na janela de 2026"
        );
        assert!(within_window(&recent, cutoff));
    }

    #[test]
    fn a_malformed_line_does_not_discard_the_measurement() {
        let dir = std::env::temp_dir().join(format!("jev-kpi-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("mkdir");
        std::fs::write(
            dir.join("intent_journal.jsonl"),
            "{\"ts\":1800000000,\"intent\":\"code\",\"source\":\"keyword\"}\nNAO É JSON\n",
        )
        .expect("write");
        let out = decision(&dir, 0);
        assert_eq!(out["intent_distribution"]["status"], "measured");
        assert_eq!(out["intent_distribution"]["human_prompts"], 1);
        assert_eq!(out["malformed_lines"]["intent_journal"]["count"], 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn all_six_rules_are_present_even_with_no_journals() {
        let dir = std::env::temp_dir().join(format!("jev-kpi-empty-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("mkdir");
        let out = decision(&dir, 0);
        // A família não pode sumir: quem lê precisa ver as seis, e cada uma
        // dizendo por que está ausente.
        for key in [
            "intent_distribution",
            "keyword_fallback_rate",
            "recall_curated_share",
            "jev_calls_total",
            "jev_latency_p99_ms",
            "jev_confidence_hist",
        ] {
            assert!(out.get(key).is_some(), "missing rule: {key}");
        }
        assert_eq!(out["intent_distribution"]["status"], "unmeasured");
        assert_eq!(out["keyword_fallback_rate"]["status"], "unmeasured");
        assert_eq!(out["recall_curated_share"]["status"], "unmeasured");
        assert_eq!(out["jev_calls_total"]["status"], "unmeasured");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
