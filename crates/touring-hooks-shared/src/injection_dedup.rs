//! `injection_dedup` — S8 (08/10/2026): dedupe por SEÇÃO cuja validade é a ÉPOCA
//! de compactação, não um TTL de 300 s.
//!
//! Origem: medido em 12 transcripts do analise (8.864 chamadas, 08/10): o conselho
//! injetado custa 894 B por chamada, e o bloco quase nunca é byte-idêntico porque
//! carrega o arquivo/comando da vez — então as SEÇÕES estáticas ("(cached 300s…)",
//! "Reason: …", linhas de lições) viajam de novo a cada chamada, e a dedupe por
//! bloco do `cli_suggester` (moka, TTL 300 s) as deixava passar: repetiam-se
//! inteiras a cada 5 minutos embora a primeira cópia continuasse no contexto até a
//! compactação (defeitos (d) e (e) do brief).
//!
//! Política (emitir só quando algo muda):
//! - **Bloco inteiro** repetido na época — por texto EXATO ou por MOLDE
//!   (diretivas exatas + prosa normalizada) → **omitido** por inteiro (o brief
//!   S8: "repetição exata omitida ou referência curta quando menor"; medido
//!   08/10 no corpus real: a referência curta custava ~0,9 MB de envelope —
//!   o rastro era mais caro que o conselho; a auditoria fica no arquivo de
//!   estado, não na janela).
//! - **Linha não-estrutural** repetida na época (exata para diretivas, por
//!   molde para prosa/telemetria) → omitida. Linhas estruturais (vazias, code
//!   fences, linhas de tabela `|`, headings `#`, separadores `---`/`===`, e
//!   qualquer linha < 24 chars) NUNCA são suprimidas: suprimir um `|---|---|`
//!   destruiria a segunda tabela de um bloco legítimo.
//!
//! Estado: arquivo por sessão em `<root>/.claude/touring/dedup-{session}.txt` —
//! a mesma topologia do `turn_budget.rs` (o daemon é long-lived e os handlers
//! efêmeros; um cache de processo daria a cada um a sua cópia e a época seria um
//! fio que parece ligado). O arquivo reinicia no avanço da época (post-compact).
//! Tudo é fail-open: erro de I/O devolve o texto intacto.
//!
//! Kill switch HUMANO: `TOURING_DEDUP_DISABLED=1` (o modelo nunca o liga).
//!
//! Retro 11/11/2026 (item b): um 2º estágio, por (emissor, arquivo-alvo) numa janela de tempo,
//! roda ANTES deste — ver [`dedup_hook_output_for_target`]. Este módulo mede por LINHA; o bloco do
//! pre-edit/pre-read é uma linha só de segmentos ` | `, e o estágio novo mede por SEGMENTO.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

/// Linhas abaixo deste tamanho nunca são suprimidas: separadores e marcadores
/// curtos carregam estrutura desproporcional ao tamanho. (Medido 08/10 no corpus
/// real: baixar o piso para 12 cortou 3 KB em 8,2 MB — 0,04 p.p.; a proteção
/// estrutural vale mais que o ganho.)
const MIN_LINE_BYTES: usize = 24;

fn dedup_file(root: &Path, session: &str) -> PathBuf {
    let safe: String = session
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    root.join(".claude/touring")
        .join(format!("dedup-{safe}.txt"))
}

/// A decisão pura do kill switch — testável sem tocar o env do processo.
/// (Race medida pela analise-6b em 08/10: `set_var` num teste desligava a
/// dedupe dos OUTROS 12 testes `dedupe_` em threads paralelas do mesmo
/// processo — 41/150 falhas. Por isso o Rust 2024 marcou `set_var` unsafe:
/// o env é global ao processo, e o cargo test roda os testes em paralelo.)
fn disabled_from(v: Option<&str>) -> bool {
    v.is_some_and(|v| v != "0")
}

fn dedup_disabled() -> bool {
    disabled_from(std::env::var("TOURING_DEDUP_DISABLED").ok().as_deref())
}

fn hash_line(normalized: &str) -> u64 {
    let mut h = DefaultHasher::new();
    normalized.hash(&mut h);
    h.finish()
}

/// Lê (época, hashes vistos). Arquivo ausente/corrompido = época 0 sem hashes.
fn read_state(root: &Path, session: &str) -> (u64, Vec<u64>) {
    let Ok(text) = std::fs::read_to_string(dedup_file(root, session)) else {
        return (0, Vec::new());
    };
    let mut lines = text.lines();
    let epoch = lines
        .next()
        .and_then(|l| l.strip_prefix("epoch="))
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(0);
    let hashes = lines
        .filter_map(|l| l.strip_prefix("h="))
        .filter_map(|v| u64::from_str_radix(v.trim(), 16).ok())
        .collect();
    (epoch, hashes)
}

fn write_state(root: &Path, session: &str, epoch: u64, hashes: &[u64]) {
    let path = dedup_file(root, session);
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let mut text = format!("epoch={epoch}\n");
    for h in hashes {
        text.push_str(&format!("h={h:016x}\n"));
    }
    let _ = std::fs::write(&path, text);
}

/// A época corrente da sessão (0 se nunca avançou).
#[must_use]
pub fn current_epoch(root: &Path, session: &str) -> u64 {
    read_state(root, session).0
}

/// A sessão compactou: a época avança e o conjunto de seções vistas reabre —
/// o contexto foi podado, então a próxima cópia de uma seção é de fato a primeira
/// que o modelo vê. Chamado pelo handler `post-compact` (a época NÃO avança mais
/// no session-start: ele não roda com source=compact desde 08/10).
pub fn advance_epoch(root: &Path, session: &str) -> u64 {
    let (epoch, _) = read_state(root, session);
    let next = epoch + 1;
    write_state(root, session, next, &[]);
    next
}

/// Uma linha é suprimível se não é estrutural e tem conteúdo de sobra.
fn is_suppressible(line: &str, in_fence: bool, in_table: bool) -> bool {
    let t = line.trim();
    if t.is_empty() || in_fence || in_table {
        return false;
    }
    if t.len() < MIN_LINE_BYTES {
        return false;
    }
    if t.starts_with('#') || t.starts_with("```") {
        return false;
    }
    // separadores puros
    if t.chars().all(|c| c == '-' || c == '=' || c == ' ') {
        return false;
    }
    true
}

/// A chave de repetição de uma linha — o "hash da seção NORMALIZADA" do brief S8.
///
/// Linhas de DIRETIVA (MUST/SHOULD/MAY) chaveiam no texto exato: `ast meta a.rs`
/// e `ast meta b.rs` são diretivas diferentes ("olhe ESTE arquivo") e a 1ª
/// emissão de cada uma é protegida pelo controle positivo do aceite A2.
///
/// Linhas de PROSA/telemetria chaveiam no MOLDE, por token: um token com `/` (um
/// path) vira `P` INTEIRO e sequências de dígitos viram `N` (`orphans=8931` →
/// `orphans=N`). `HEALTH HEALTHY 0.88 … 8931 orphan(s)` e a mesma linha com
/// números novos são a MESMA seção repetida — o valor novo não muda a ação de
/// quem lê, e é exatamente essa variação que fazia o conselho "quase igual"
/// escapar da dedupe por texto exato (medido 08/10: a exata corta 19,4 % do
/// corpus real; o teto pedido é 40 %).
fn line_key(t: &str) -> String {
    if t.starts_with("MUST") || t.starts_with("SHOULD") || t.starts_with("MAY") {
        return t.to_string();
    }
    let mut out: Vec<String> = Vec::new();
    for tok in t.split_whitespace() {
        if tok.contains('/') {
            out.push("P".to_string());
            continue;
        }
        let mut buf = String::with_capacity(tok.len());
        let mut in_num = false;
        for c in tok.chars() {
            if c.is_ascii_digit() {
                if !in_num {
                    buf.push('N');
                    in_num = true;
                }
            } else {
                in_num = false;
                buf.push(c);
            }
        }
        out.push(buf);
    }
    out.join(" ")
}

/// Dedupe de um bloco de contexto para (sessão, época corrente).
///
/// Retorna o texto a emitir: intacto na primeira vez, VAZIO quando o bloco
/// INTEIRO se repete (exato ou por molde — omissão, ver o topo do módulo), e
/// com as linhas repetidas omitidas quando o bloco mudou. Fail-open: qualquer
/// erro de I/O devolve `text` intacto.
#[must_use]
pub fn dedup_sections(root: &Path, session: &str, text: &str) -> String {
    dedup_sections_with(root, session, text, dedup_disabled())
}

/// O corpo, com o kill switch como PARÂMETRO (testes exercem `disabled` sem
/// mutar o env global do processo — ver `disabled_from`).
fn dedup_sections_with(root: &Path, session: &str, text: &str, disabled: bool) -> String {
    if disabled || text.is_empty() {
        return text.to_string();
    }
    let (epoch, mut seen) = read_state(root, session);

    // Camada 1 — o bloco inteiro: casa pelo texto EXATO ou pelo MOLDE (diretivas
    // exatas + prosa normalizada por `line_key`): se o molde casa, as diretivas
    // são idênticas e só a telemetria variou. Repetição é OMITIDA (o brief S8:
    // "repetição exata omitida ou referência curta quando menor" — para conselho
    // a omissão é segura: a 1ª cópia segue na janela até a compactação, e o
    // rastro de auditoria fica no arquivo de estado, não na janela; medido 08/10:
    // 7 465 referências × ~120 B de envelope ≈ 0,9 MB de rastro, ~11 p.p. do
    // corpus — o rastro era mais caro que o conselho que ele anunciava).
    let block_hash = hash_line(text);
    let mold_hash = hash_line(
        &text
            .lines()
            .map(|l| line_key(l.trim_start()))
            .collect::<Vec<_>>()
            .join("\n"),
    );
    if seen.contains(&block_hash) || seen.contains(&mold_hash) {
        return String::new();
    }

    // Camada 2 — linhas não-estruturais repetidas na época.
    let mut out: Vec<&str> = Vec::with_capacity(text.lines().count());
    let mut in_fence = false;
    let mut in_table = false;
    let mut changed = false;
    for line in text.lines() {
        let t = line.trim_start();
        if t.starts_with("```") {
            in_fence = !in_fence;
            out.push(line);
            continue;
        }
        let is_table_line = t.starts_with('|');
        if is_table_line != in_table {
            in_table = is_table_line;
        }
        if !is_suppressible(line, in_fence, in_table) {
            out.push(line);
            continue;
        }
        let h = hash_line(&line_key(t));
        if seen.contains(&h) {
            changed = true; // omitida — já entregue nesta época
            continue;
        }
        seen.push(h);
        out.push(line);
    }

    // Persiste: o bloco inteiro (exato e molde) + as linhas novas na época.
    seen.push(block_hash);
    seen.push(mold_hash);
    write_state(root, session, epoch, &seen);

    if !changed {
        return text.to_string();
    }
    let joined = out.join("\n");
    // Se a dedupe removeu tudo que havia de novo, o bloco vira a referência.
    if joined.trim().is_empty() {
        return String::new();
    }
    joined
}

/// Aplica a dedupe ao **output JSON final** de um hook — o ponto central do daemon
/// (`run_hook_command`), por onde TODOS os handlers da dispatch table saem, e o
/// subcomando stateless `dedup-stdin` do `touring-hook` (S-4p), por onde os hooks
/// Python passam a mesma dedupe com a mesma chave (root, sessão).
///
/// Extrai os campos de CONTEXTO — `hookSpecificOutput.additionalContext`,
/// `hookSpecificOutput.systemMessage` e o `systemMessage` da raiz (S-4p,
/// 26/10/2026: o corpus mediu o Touring injetando pelos três) — passa cada um
/// por [`dedup_sections`] e re-serializa. `permissionDecisionReason` fica FORA
/// por contrato: é o canal da DECISÃO (a separação está declarada em
/// `client/hooks/injection_policy.py`), e suprimir a razão repetida de um deny
/// esconderia do operador por que a 2ª negação aconteceu.
///
/// Qualquer outra forma (não-JSON, sem os campos, contexto vazio,
/// re-serialização falha) devolve o output intacto — fail-open por construção.
/// Quando nada muda, os bytes originais voltam sem re-serialização (a ordem de
/// chaves do JSON de outros campos nunca é tocada).
#[must_use]
pub fn dedup_hook_output(root: &Path, session: &str, output: &str) -> String {
    dedup_hook_output_with(root, session, output, dedup_disabled())
}

/// O corpo, com o kill switch como parâmetro (ver `dedup_sections_with`).
fn dedup_hook_output_with(root: &Path, session: &str, output: &str, disabled: bool) -> String {
    if disabled {
        return output.to_string();
    }
    let Ok(mut v) = serde_json::from_str::<serde_json::Value>(output) else {
        return output.to_string();
    };
    let mut changed = false;
    // A ordem é a ordem de prioridade de retenção: uma linha partilhada entre
    // campos fica no PRIMEIRO (o additionalContext é o canal principal).
    changed |= dedup_json_field(
        &mut v,
        root,
        session,
        Some("hookSpecificOutput"),
        "additionalContext",
    );
    changed |= dedup_json_field(
        &mut v,
        root,
        session,
        Some("hookSpecificOutput"),
        "systemMessage",
    );
    changed |= dedup_json_field(&mut v, root, session, None, "systemMessage");
    if !changed {
        return output.to_string();
    }
    finish_envelope(&mut v);
    serde_json::to_string(&v).unwrap_or_else(|_| output.to_string())
}

/// Omissão total em todos os campos esvazia o hookSpecificOutput? O envelope
/// vazio sai junto (~2 B é o menor rastro possível, e o CC não injeta nada).
fn finish_envelope(v: &mut serde_json::Value) {
    if v.get("hookSpecificOutput")
        .and_then(serde_json::Value::as_object)
        .is_some_and(serde_json::Map::is_empty)
    {
        v.as_object_mut().map(|m| m.remove("hookSpecificOutput"));
    }
}

/// Dedupa UM campo-string de contexto do JSON de saída (`parent=None` = raiz).
///
/// Retorna `true` quando o campo mudou — reduzido ou REMOVIDO (omissão total:
/// o campo repetido sai do JSON em vez de viajar vazio). Campo ausente, vazio
/// ou não-string: inalterado, `false`. A dedupe do conteúdo é a de
/// [`dedup_sections`], com o mesmo arquivo de estado por sessão.
fn dedup_json_field(
    v: &mut serde_json::Value,
    root: &Path,
    session: &str,
    parent: Option<&str>,
    key: &str,
) -> bool {
    map_json_field(v, parent, key, &mut |ctx| {
        dedup_sections(root, session, ctx)
    })
}

/// O corpo comum dos dois estágios: aplica `reduce` ao campo-string de contexto e grava o
/// resultado — vazio REMOVE o campo. `true` quando o campo mudou.
fn map_json_field(
    v: &mut serde_json::Value,
    parent: Option<&str>,
    key: &str,
    reduce: &mut dyn FnMut(&str) -> String,
) -> bool {
    let field = match parent {
        Some(p) => v.get(p).and_then(|h| h.get(key)),
        None => v.get(key),
    };
    let Some(ctx) = field.and_then(serde_json::Value::as_str) else {
        return false;
    };
    if ctx.is_empty() {
        return false;
    }
    let deduped = reduce(ctx);
    if deduped == ctx {
        return false;
    }
    let target = match parent {
        Some(p) => v.get_mut(p),
        None => Some(&mut *v),
    };
    let Some(t) = target else { return false };
    if deduped.is_empty() {
        t.as_object_mut().map(|m| m.remove(key));
    } else {
        t[key] = serde_json::Value::String(deduped);
    }
    true
}

// ── Janela por (emissor, alvo) — retro 11/11/2026, item (b) ─────────────────────────────
//
// POR QUE EXISTE. A dedupe acima é por ÉPOCA e por linha: uma linha só é omitida se o seu
// molde (ou o texto, para diretivas) já foi entregue. Medido em 5 transcripts reais do
// `analise` (hook_success com additionalContext, agrupados por (emissor, arquivo)): de 17 % a
// 51 % dos bytes injetados eram a 2ª+ emissão do MESMO emissor para o MESMO arquivo, e quase
// nenhuma byte-idêntica (2 a 9 em 100 a 234). O bloco do pre-edit/pre-read é UMA linha de
// segmentos ` | ` — fatos do arquivo (`HOTSPOT`, `callers:`) ao lado de telemetria que deriva a
// cada chamada (a ORDEM de `related docs`, `ANN related`, `Next: Bash (51%)`) — e basta um
// segmento derivar para a dedupe por linha tratar a linha INTEIRA como nova e reenviar os ~2 KB.
//
// A REGRA. Por (emissor, alvo, ator) e dentro de uma janela (`TARGET_WINDOW_SECS`, 10 min por
// padrão; `TOURING_DEDUP_WINDOW_SECS` a muda, 0 a desliga), um SEGMENTO já entregue não volta —
// salvo se mudou. "Mudou" é o hash do segmento NORMALIZADO (o molde de `line_key` e a lista
// entre colchetes em ordem canônica): reordenar `related docs` ou variar um contador não é
// conteúdo novo, um item novo na lista ou uma função nova no `note:` é. O que mudou passa
// sozinho; o que já foi dito fica na janela do modelo (a 1ª cópia vale até a compactação, e a
// compactação reabre o estágio — o estado carrega a época).
//
// O QUE NUNCA É TOCADO. Linhas estruturais (tabela, fence, heading, < 24 B) e o payload de
// GATE/decisão (`WINDOW_EXEMPT_FAMILIES`: deny, contrato, contexto do loop…): um aviso de gate
// repetido é um fato novo para quem o lê, e `permissionDecisionReason` nem passa por aqui.
// O estágio roda ANTES da dedupe por época, então o que ele omitiu nunca entra no estado dela.
//
// ESCOPO. O ator é (sessão, subagente): um subagente tem contexto PRÓPRIO, e o que o pai
// recebeu não está na janela dele. Só a rota do daemon conhece o alvo (`tool_input`); o
// standalone segue só com a dedupe por época — declarado, não um buraco silencioso.

/// Quem chamou, para a dedupe por (emissor, alvo).
pub struct TargetCall<'a> {
    /// A raiz do projeto (onde vive o arquivo de estado).
    pub root: &'a Path,
    /// A sessão do Claude Code.
    pub session: &'a str,
    /// O subagente que fez a chamada, quando foi um.
    pub agent: Option<&'a str>,
    /// O emissor (o hook da dispatch table) que produziu a saída.
    pub emitter: &'a str,
    /// O arquivo que a chamada de ferramenta mira.
    pub target: Option<&'a str>,
}

/// Janela padrão do estágio por (emissor, alvo), em segundos.
pub const TARGET_WINDOW_SECS: u64 = 600;

/// A env var HUMANA que muda a janela (segundos; `0` desliga o estágio).
const TARGET_WINDOW_ENV: &str = "TOURING_DEDUP_WINDOW_SECS";

/// Teto de entradas do arquivo de estado — o estado de uma sessão longa não cresce sem fim.
const TARGET_MAX_ENTRIES: usize = 4096;

/// O separador dos segmentos de um bloco de contexto de uma linha só.
const SEGMENT_SEPARATOR: &str = " | ";

/// Famílias que são payload de GATE ou de decisão: nunca entram na janela. Um aviso de gate
/// repetido não é enriquecimento redundante — é o gate falando de novo.
const WINDOW_EXEMPT_FAMILIES: &[&str] = &[
    "code-mode-deny",
    "code-mode-contract",
    "session-contract",
    "compaction-gaps",
    "context-monitor",
    "pipefail-evidence",
    "evidence-guard",
    "loop-outer",
    "failure-echo",
    "quality-gate",
];

/// A decisão pura da janela: inteiro de segundos; ausente ou lixo cai no padrão.
fn window_from(raw: Option<&str>) -> u64 {
    raw.and_then(|v| v.trim().parse::<u64>().ok())
        .unwrap_or(TARGET_WINDOW_SECS)
}

fn target_window_secs() -> u64 {
    window_from(std::env::var(TARGET_WINDOW_ENV).ok().as_deref())
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Um segmento entregue: a chave (emissor+alvo), o hash do segmento normalizado e quando saiu.
struct Seen {
    key: u64,
    seg: u64,
    at: u64,
}

fn target_state_file(root: &Path, session: &str, agent: Option<&str>) -> PathBuf {
    let actor = match agent {
        Some(a) if !a.is_empty() => format!("{session}-agent-{a}"),
        _ => session.to_string(),
    };
    let safe: String = actor
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    root.join(".claude/touring")
        .join(format!("dedup-target-{safe}.txt"))
}

/// Lê o estado da época corrente; arquivo ausente, corrompido ou de OUTRA época = vazio (a
/// compactação podou o contexto, então nada do que foi dito antes está mais na janela).
fn read_target_state(path: &Path, epoch: u64) -> Vec<Seen> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let mut lines = text.lines();
    let file_epoch = lines
        .next()
        .and_then(|l| l.strip_prefix("epoch="))
        .and_then(|v| v.trim().parse::<u64>().ok());
    if file_epoch != Some(epoch) {
        return Vec::new();
    }
    lines
        .filter_map(|l| {
            let mut p = l.split(' ');
            Some(Seen {
                key: u64::from_str_radix(p.next()?, 16).ok()?,
                seg: u64::from_str_radix(p.next()?, 16).ok()?,
                at: p.next()?.parse().ok()?,
            })
        })
        .collect()
}

fn write_target_state(path: &Path, epoch: u64, seen: &[Seen]) {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let keep = seen.len().saturating_sub(TARGET_MAX_ENTRIES);
    let mut text = format!("epoch={epoch}\n");
    for e in &seen[keep..] {
        text.push_str(&format!("{:016x} {:016x} {}\n", e.key, e.seg, e.at));
    }
    let _ = std::fs::write(path, text);
}

/// Cada lista `[a, b, c]` do molde em ordem canônica: a ORDEM de uma lista (os `related docs`
/// embaralham a cada chamada) não é conteúdo; um item novo é.
fn sort_bracket_lists(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(open) = rest.find('[') {
        let after = &rest[open + 1..];
        let Some(close) = after.find(']') else { break };
        let inner = &after[..close];
        out.push_str(&rest[..=open]);
        if inner.contains('[') {
            rest = after; // aninhado: deixa como está e segue pelo `[` interno
            continue;
        }
        let mut items: Vec<&str> = inner.split(',').map(str::trim).collect();
        items.sort_unstable();
        out.push_str(&items.join(","));
        out.push(']');
        rest = &after[close + 1..];
    }
    out.push_str(rest);
    out
}

/// A chave de um segmento: o molde de [`line_key`] (diretiva exata; prosa sem dígitos e paths)
/// sobre o texto com as listas já em ordem canônica. A ordem é fixada ANTES do molde: o molde
/// troca um token com `/` por `P` INTEIRO, e leva junto o `]` do último item da lista — com a
/// lista ainda embaralhada, o `]` apareceria ou sumiria conforme a ordem e a chave variaria.
fn segment_key(seg: &str) -> String {
    line_key(&sort_bracket_lists(seg.trim()))
}

fn window_exempt(seg: &str) -> bool {
    touring_foundation::injection_family::families(seg)
        .iter()
        .any(|f| WINDOW_EXEMPT_FAMILIES.contains(f))
}

/// Um segmento entra na janela se tem conteúdo de sobra (o piso das linhas), não é cabeçalho
/// e não é payload de gate.
fn window_candidate(seg: &str) -> bool {
    let t = seg.trim();
    t.len() >= MIN_LINE_BYTES && !t.starts_with('#') && !t.starts_with("```") && !window_exempt(t)
}

/// O estágio sobre UM texto de contexto: omite os segmentos já entregues para `key` dentro de
/// `window` segundos e registra os novos. Sem nada omitido, o texto volta byte a byte.
fn window_filter(seen: &mut Vec<Seen>, key: u64, text: &str, now: u64, window: u64) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut in_fence = false;
    let mut dropped = false;
    for line in text.lines() {
        let t = line.trim_start();
        if t.starts_with("```") {
            in_fence = !in_fence;
            out.push(line.to_string());
            continue;
        }
        if in_fence || t.starts_with('|') || t.starts_with('#') {
            out.push(line.to_string());
            continue;
        }
        let mut kept: Vec<&str> = Vec::new();
        for seg in line.split(SEGMENT_SEPARATOR) {
            if !window_candidate(seg) {
                kept.push(seg);
                continue;
            }
            let h = hash_line(&segment_key(seg));
            match seen.iter_mut().find(|e| e.key == key && e.seg == h) {
                // entregue dentro da janela: omitido (a omissão NÃO renova o relógio — passada a
                // janela desde a ÚLTIMA entrega, o conselho volta uma vez)
                Some(e) if now.saturating_sub(e.at) <= window => dropped = true,
                Some(e) => {
                    e.at = now;
                    kept.push(seg);
                }
                None => {
                    seen.push(Seen {
                        key,
                        seg: h,
                        at: now,
                    });
                    kept.push(seg);
                }
            }
        }
        if !kept.is_empty() {
            out.push(kept.join(SEGMENT_SEPARATOR));
        }
    }
    if dropped {
        out.join("\n")
    } else {
        text.to_string()
    }
}

/// Aplica o estágio por (emissor, alvo) e, por cima do que sobrou, a dedupe por época
/// ([`dedup_hook_output`]). Sem alvo (Bash, prompt), sem emissor, com o kill switch
/// `TOURING_DEDUP_DISABLED` ou com a janela em 0, é só a dedupe por época — o comportamento
/// de antes, byte a byte.
#[must_use]
pub fn dedup_hook_output_for_target(call: &TargetCall<'_>, output: &str) -> String {
    dedup_hook_output_for_target_with(
        call,
        output,
        now_secs(),
        target_window_secs(),
        dedup_disabled(),
    )
}

/// O corpo, com o relógio, a janela e o kill switch como PARÂMETROS (testes não mutam o env
/// global nem esperam o tempo passar — ver `disabled_from`).
fn dedup_hook_output_for_target_with(
    call: &TargetCall<'_>,
    output: &str,
    now: u64,
    window: u64,
    disabled: bool,
) -> String {
    let (root, session) = (call.root, call.session);
    let target = call.target.filter(|t| !t.is_empty());
    let (Some(target), false, false) = (target, call.emitter.is_empty(), window == 0) else {
        return dedup_hook_output_with(root, session, output, disabled);
    };
    if disabled {
        return output.to_string();
    }
    let Ok(mut v) = serde_json::from_str::<serde_json::Value>(output) else {
        return output.to_string();
    };
    let epoch = current_epoch(root, session);
    let state_path = target_state_file(root, session, call.agent);
    let mut seen = read_target_state(&state_path, epoch);
    // uma entrada passada da janela é igual a nenhuma: na próxima vista ela reemite e se renova
    seen.retain(|e| now.saturating_sub(e.at) <= window);
    let key = hash_line(&format!("{}\u{2}{target}", call.emitter));
    let mut changed = false;
    for (parent, field) in [
        (Some("hookSpecificOutput"), "additionalContext"),
        (Some("hookSpecificOutput"), "systemMessage"),
        (None, "systemMessage"),
    ] {
        changed |= map_json_field(&mut v, parent, field, &mut |ctx| {
            window_filter(&mut seen, key, ctx, now, window)
        });
    }
    write_target_state(&state_path, epoch, &seen);
    let staged = if changed {
        finish_envelope(&mut v);
        serde_json::to_string(&v).unwrap_or_else(|_| output.to_string())
    } else {
        output.to_string()
    };
    dedup_hook_output_with(root, session, &staged, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (tempfile::TempDir, String) {
        let d = tempfile::tempdir().expect("tempdir");
        (d, "sess-teste".to_string())
    }

    #[test]
    fn dedupe_primeira_emissao_passa_intacta() {
        let (d, s) = setup();
        let block = "[TOURING SUGGEST · x]\n  MUST touring doctor -j\n  Reason: uma razão suficientemente longa para contar";
        assert_eq!(dedup_sections(d.path(), &s, block), block);
    }

    #[test]
    fn dedupe_bloco_repetido_na_mesma_epoca_e_omitido() {
        let (d, s) = setup();
        let block = "[TOURING SUGGEST · system-health-precheck]\n  MUST touring doctor -j // daemon + index health gate — um bloco grande o bastante para a referência valer a pena em bytes medidos agora";
        let first = dedup_sections(d.path(), &s, block);
        assert_eq!(first, block, "primeira emissão é intacta");
        let second = dedup_sections(d.path(), &s, block);
        assert!(
            second.is_empty(),
            "repetição é omitida por inteiro: {second:?}"
        );
    }

    #[test]
    fn dedupe_linha_estatica_some_na_2a_chamada_mas_o_que_mudou_fica() {
        let (d, s) = setup();
        let estatica = "  (cached 300s — set TOURING_SUGGESTER_DISABLED=1 to silence)";
        let b1 = format!("[TOURING SUGGEST · a]\n  MUST touring doctor -j\n{estatica}");
        let b2 = format!("[TOURING SUGGEST · b]\n  MUST touring status -j\n{estatica}");
        let _ = dedup_sections(d.path(), &s, &b1);
        let segundo = dedup_sections(d.path(), &s, &b2);
        assert!(
            segundo.contains("touring status -j"),
            "o que é novo passa: {segundo}"
        );
        assert!(
            !segundo.contains("cached 300s"),
            "a linha estática repetida é omitida: {segundo}"
        );
    }

    #[test]
    fn dedupe_nunca_suprime_linhas_de_tabela_ou_fence() {
        let (d, s) = setup();
        let tabela = "| Gate | Nega em |\n|---|---|\n| rajada | 2ª |";
        let b1 = format!("contexto inicial longo o bastante para contar como prosa\n{tabela}");
        let b2 = format!("outro bloco distinto, também longo o bastante\n{tabela}");
        let _ = dedup_sections(d.path(), &s, &b1);
        let segundo = dedup_sections(d.path(), &s, &b2);
        assert!(
            segundo.contains("|---|---|"),
            "a 2ª tabela fica intacta: {segundo}"
        );
        let fence = "```bash\ntouring doctor -j\n```";
        let b3 = format!("prefixo com bastante texto para não ser linha curta\n{fence}");
        let b4 = format!("outro prefixo longo e distinto para o segundo bloco\n{fence}");
        let _ = dedup_sections(d.path(), &s, &b3);
        let quarto = dedup_sections(d.path(), &s, &b4);
        assert!(
            quarto.contains("touring doctor -j"),
            "o conteúdo do fence fica: {quarto}"
        );
    }

    #[test]
    fn epoch_avanco_reemite_o_que_ja_foi_dito() {
        let (d, s) = setup();
        let estatica = "  (cached 300s — set TOURING_SUGGESTER_DISABLED=1 to silence)";
        let b1 = format!("[TOURING SUGGEST · a]\n{estatica}");
        let _ = dedup_sections(d.path(), &s, &b1);
        // bloco distinto com a MESMA linha estática: ela é omitida na mesma época
        let b2 = format!("[TOURING SUGGEST · b]\n{estatica}");
        let suprimido = dedup_sections(d.path(), &s, &b2);
        assert!(
            !suprimido.contains("cached 300s"),
            "antes do post-compact: omitida"
        );
        assert!(suprimido.contains("SUGGEST · b"), "o que mudou passa");
        advance_epoch(d.path(), &s);
        let b3 = format!("[TOURING SUGGEST · c]\n{estatica}");
        let reemitido = dedup_sections(d.path(), &s, &b3);
        assert!(
            reemitido.contains("cached 300s"),
            "depois do post-compact: reemitida"
        );
    }

    #[test]
    fn epoch_nao_avanca_sem_chamada_e_sobrevive_a_processo_novo() {
        let (d, s) = setup();
        assert_eq!(current_epoch(d.path(), &s), 0);
        let e1 = advance_epoch(d.path(), &s);
        assert_eq!(e1, 1);
        // "processo novo": o estado vem do disco, não de memória
        assert_eq!(current_epoch(d.path(), &s), 1, "a época vive no arquivo");
    }

    #[test]
    fn dedupe_uma_sessao_nunca_suprime_na_outra() {
        let (d, _s) = setup();
        let block = "  (cached 300s — set TOURING_SUGGESTER_DISABLED=1 to silence) — prosa longa";
        let _ = dedup_sections(d.path(), "sessao-a", block);
        let outra = dedup_sections(d.path(), "sessao-b", block);
        assert_eq!(outra, block, "outra sessão nunca viu a 1ª cópia");
    }

    #[test]
    fn dedupe_linhas_curtas_nunca_sao_suprimidas() {
        let (d, s) = setup();
        let b1 = "linha de prosa inicial suficientemente longa para registrar\ncurta";
        let b2 = "outra prosa inicial, também suficientemente longa\ncurta";
        let _ = dedup_sections(d.path(), &s, b1);
        let segundo = dedup_sections(d.path(), &s, b2);
        assert!(
            segundo.contains("curta"),
            "linha curta estrutural fica: {segundo}"
        );
    }

    /// O kill switch como PARÂMETRO (a interna), sem `set_var` — a race medida
    /// em 08/10 (41/150 rodadas paralelas) nasceu do env global do processo.
    #[test]
    fn dedupe_kill_switch_devolve_intacto() {
        let (d, s) = setup();
        let block = "  (cached 300s — set TOURING_SUGGESTER_DISABLED=1 to silence) prosa";
        let _ = dedup_sections_with(d.path(), &s, block, true);
        let de_novo = dedup_sections_with(d.path(), &s, block, true);
        assert!(de_novo.len() > 0, "kill switch: texto sai mesmo repetido");
        assert!(de_novo.contains("cached 300s"));
        // e a dedupe ligada na MESMA sessão segue funcionando: as chamadas com
        // o switch ligado NÃO registram estado (retornam antes de gravar), a
        // 1ª ligada registra, a 2ª omite.
        let _ = dedup_sections_with(d.path(), &s, block, false);
        let ligada = dedup_sections_with(d.path(), &s, block, false);
        assert!(
            ligada.is_empty(),
            "com o switch desligado, a repetição é omitida"
        );
    }

    #[test]
    fn dedupe_kill_switch_decisao_pura() {
        assert!(disabled_from(Some("1")), "1 desliga");
        assert!(!disabled_from(Some("0")), "0 mantém ligado");
        assert!(!disabled_from(None), "ausente mantém ligado");
    }

    #[test]
    fn dedupe_erro_de_io_e_fail_open() {
        // root impossível de escrever: leitura falha → estado vazio → passa intacto
        let block = "qualquer prosa suficientemente longa para passar no filtro de tamanho";
        let out = dedup_sections(Path::new("/proc/nao-existe/impossivel"), "s", block);
        assert_eq!(out, block);
    }

    #[test]
    fn dedupe_json_output_dedupa_o_additional_context_e_preserva_o_resto() {
        let (d, s) = setup();
        let ctx =
            "  (cached 300s — set TOURING_SUGGESTER_DISABLED=1 to silence) prosa longa de verdade";
        let mk = |c: &str| {
            serde_json::json!({
                "hookSpecificOutput": {"hookEventName": "PreToolUse", "additionalContext": c},
                "outroCampo": {"ordem": ["b", "a"]}
            })
            .to_string()
        };
        let o1 = dedup_hook_output(d.path(), &s, &mk(ctx));
        assert!(o1.contains("cached 300s"), "primeira passa: {o1}");
        // segunda chamada com o MESMO contexto: omitida — a chave sai do JSON
        let o2 = dedup_hook_output(d.path(), &s, &mk(ctx));
        assert!(
            !o2.contains("additionalContext"),
            "repetição sai do JSON: {o2}"
        );
        assert!(!o2.contains("cached 300s"));
        // e o resto do JSON sobrevive
        let v: serde_json::Value = serde_json::from_str(&o2).expect("JSON válido");
        assert_eq!(v["outroCampo"]["ordem"][0], "b");
        assert_eq!(v["hookSpecificOutput"]["hookEventName"], "PreToolUse");
    }

    /// S-4p (26/10/2026): `systemMessage` (raiz e dentro de hookSpecificOutput)
    /// é o MESMO canal de contexto — o corpus mediu o Touring injetando por ele
    /// — então passa pela mesma dedupe; a repetição na época sai do JSON.
    #[test]
    fn dedupe_json_system_message_tambem_e_dedupado() {
        let (d, s) = setup();
        let msg = "prosa de sistema suficientemente longa para ser suprimível na repetição";
        let mk = |m: &str| {
            serde_json::json!({
                "systemMessage": m,
                "hookSpecificOutput": {"hookEventName": "PostToolUse", "systemMessage": m}
            })
            .to_string()
        };
        let o1 = dedup_hook_output(d.path(), &s, &mk(msg));
        assert!(o1.contains(msg), "primeira emissão passa intacta: {o1}");
        let o2 = dedup_hook_output(d.path(), &s, &mk(msg));
        assert!(
            !o2.contains(msg),
            "a repetição dos DOIS systemMessage sai do JSON: {o2}"
        );
        let v: serde_json::Value = serde_json::from_str(&o2).expect("JSON válido");
        assert!(v.get("systemMessage").is_none(), "raiz omitida: {v}");
        assert!(
            v.pointer("/hookSpecificOutput/systemMessage").is_none(),
            "aninhado omitido: {v}"
        );
        // e o que não é contexto sobrevive
        assert_eq!(
            v.pointer("/hookSpecificOutput/hookEventName")
                .and_then(|h| h.as_str()),
            Some("PostToolUse")
        );
    }

    /// S-4p: `permissionDecisionReason` é o canal da DECISÃO — a separação está
    /// declarada em `client/hooks/injection_policy.py`. Suprimir a razão
    /// repetida de um deny esconderia do operador por que a 2ª negação
    /// aconteceu: o campo passa INTACTO, byte a byte, mesmo repetido.
    #[test]
    fn dedupe_json_permission_decision_reason_nunca_e_tocado() {
        let (d, s) = setup();
        let mk = || {
            serde_json::json!({
                "hookSpecificOutput": {
                    "hookEventName": "PreToolUse",
                    "permissionDecision": "deny",
                    "permissionDecisionReason": "a razão da negação, repetida a cada deny idêntico"
                }
            })
            .to_string()
        };
        let o1 = dedup_hook_output(d.path(), &s, &mk());
        let o2 = dedup_hook_output(d.path(), &s, &mk());
        assert_eq!(o1, mk(), "primeira passa intacta");
        assert_eq!(o2, mk(), "a razão da decisão NUNCA é suprimida");
    }

    /// S-4p: controle positivo por campo — a 1ª emissão de um systemMessage com
    /// conteúdo NOVO passa mesmo quando o additionalContext do mesmo payload já
    /// foi visto na época (a dedupe é por conteúdo, não por payload).
    #[test]
    fn dedupe_json_primeira_emissao_de_cada_campo_passa() {
        let (d, s) = setup();
        let ctx = "  (cached 300s — set TOURING_SUGGESTER_DISABLED=1 to silence) prosa longa";
        let msg = "aviso de sistema novo, suficientemente longo para ser suprimível";
        let o1 = dedup_hook_output(
            d.path(),
            &s,
            &serde_json::json!({"hookSpecificOutput": {"hookEventName": "PreToolUse", "additionalContext": ctx}}).to_string(),
        );
        assert!(o1.contains("cached 300s"));
        // 2º payload: additionalContext repetido (omitido), systemMessage NOVO (fica)
        let o2 = dedup_hook_output(
            d.path(),
            &s,
            &serde_json::json!({"hookSpecificOutput": {"hookEventName": "PreToolUse", "additionalContext": ctx, "systemMessage": msg}}).to_string(),
        );
        assert!(!o2.contains("cached 300s"), "ctx repetido omitido: {o2}");
        assert!(o2.contains(msg), "systemMessage novo passa: {o2}");
    }

    /// S-4p: uma linha partilhada entre campos fica no PRIMEIRO processado
    /// (additionalContext) e é omitida no segundo — a ordem é declarada.
    #[test]
    fn dedupe_json_linha_partilhada_fica_no_primeiro_campo() {
        let (d, s) = setup();
        let linha = "a mesma lição suficientemente longa aparece nos dois campos do payload";
        let mk = || {
            serde_json::json!({
                "hookSpecificOutput": {"hookEventName": "PostToolUse", "additionalContext": linha, "systemMessage": linha}
            })
            .to_string()
        };
        let o1 = dedup_hook_output(d.path(), &s, &mk());
        let v: serde_json::Value = serde_json::from_str(&o1).expect("JSON válido");
        assert_eq!(
            v.pointer("/hookSpecificOutput/additionalContext")
                .and_then(|c| c.as_str()),
            Some(linha),
            "o canal principal retém a linha: {o1}"
        );
        assert!(
            v.pointer("/hookSpecificOutput/systemMessage").is_none(),
            "a cópia no segundo campo é omitida na MESMA passada: {o1}"
        );
    }

    #[test]
    fn dedupe_json_output_sem_additional_context_passa_intacto() {
        let (d, s) = setup();
        let out =
            serde_json::json!({"hookSpecificOutput": {"permissionDecision": "deny"}}).to_string();
        assert_eq!(
            dedup_hook_output(d.path(), &s, &out),
            out,
            "sem o campo, intacto"
        );
        let prosa = "isto nem é JSON".to_string();
        assert_eq!(
            dedup_hook_output(d.path(), &s, &prosa),
            prosa,
            "não-JSON intacto"
        );
    }

    /// S8 (08/10): a chave da linha é NORMALIZADA (o "hash da seção normalizada"
    /// do brief). Telemetria com números novos é a MESMA seção; diretiva com
    /// path novo é diretiva NOVA e nunca é suprimida (controle do aceite A2).
    #[test]
    fn dedupe_molde_suprime_telemetria_mas_nunca_diretiva_com_path_novo() {
        let (d, s) = setup();
        let b1 = "⚙ rust-workflow: pub_surface=10 complexity=0.07 health=0.98 [wiring:0.88]\n  \
                  MUST    touring ast meta /home/user/projeto/foo.rs --depth summary -j\n  \
                  Reason: prosa suficientemente longa para contar como linha suprimível";
        let b2 = "⚙ rust-workflow: pub_surface=3 complexity=0.41 health=0.66 [wiring:0.71]\n  \
                  MUST    touring ast meta /home/user/projeto/bar.rs --depth summary -j\n  \
                  Reason: prosa suficientemente longa para contar como linha suprimível";
        let _ = dedup_sections(d.path(), &s, b1);
        let segundo = dedup_sections(d.path(), &s, b2);
        assert!(
            !segundo.contains("rust-workflow"),
            "a telemetria com números novos é o mesmo molde — suprimida: {segundo}"
        );
        assert!(
            segundo.contains("bar.rs"),
            "a diretiva com path NOVO é diretiva nova — emitida: {segundo}"
        );
        assert!(
            !segundo.contains("Reason: prosa"),
            "a prosa idêntica segue suprimida: {segundo}"
        );
    }

    /// Camada 1 por molde: mesmo bloco com só a telemetria variável (diretivas
    /// idênticas) é omitido — é a fatia dominante do corpus real (os blocos de
    /// pre-edit variam números a cada arquivo).
    #[test]
    fn dedupe_bloco_molde_repetido_e_omitido() {
        let (d, s) = setup();
        let mk = |health: &str| {
            format!(
                "⚙ rust-workflow: health={health} [wiring:0.88] 1ms — 8931 orphan pub symbol(s)\n  \
                 MUST    touring ast meta foo.rs --depth summary -j\n  \
                 Reason: uma razão suficientemente longa para o bloco inteiro valer a referência"
            )
        };
        let b1 = mk("0.98");
        let first = dedup_sections(d.path(), &s, &b1);
        assert_eq!(first, b1, "primeira emissão intacta");
        let second = dedup_sections(d.path(), &s, &mk("0.66"));
        assert!(
            second.is_empty(),
            "só a telemetria mudou → omissão total: {second:?}"
        );
        // e a diretiva nova (path diferente) NÃO casa o molde → bloco emitido
        let b3 = mk("0.66").replace("foo.rs", "bar.rs");
        let third = dedup_sections(d.path(), &s, &b3);
        assert!(
            third.contains("bar.rs"),
            "diretiva nova emite o bloco: {third}"
        );
    }

    // ── Retro 11/11/2026, item (b): a janela por (emissor, alvo) ─────────────────────────
    //
    // Medido em 5 transcripts reais do `analise` (hook_success com additionalContext,
    // agrupados por (emissor, arquivo)): 17–51 % dos bytes injetados eram a 2ª+ emissão do
    // MESMO emissor para o MESMO arquivo, e quase nenhuma byte-idêntica (2–9 de 100–234) — o
    // bloco é UMA linha de segmentos ` | `, e basta a telemetria ao redor derivar (a ordem de
    // `related docs`, `knowledge: 21501/…`, `Next: Bash (51%)`) para a dedupe por linha tratar
    // a linha inteira como nova. O estágio por segmento fecha esse buraco.

    const T0: u64 = 1_000_000;
    const JANELA: u64 = 600;

    fn out_json(ctx: &str) -> String {
        serde_json::json!({"hookSpecificOutput": {"hookEventName": "PreToolUse", "additionalContext": ctx}})
            .to_string()
    }

    /// Um bloco de pre-edit com a forma dos medidos: fatos do arquivo estáveis, telemetria ao
    /// redor a derivar a cada chamada (ordem de lista, contadores, percentuais).
    fn pre_edit_block(i: usize) -> String {
        let docs = [
            "Touring CLI — Task Lifecycle@touring-cli-tasks.md",
            "Touring CLI — Code Intelligence@touring-cli-intelligence.md",
            "cli_anything/touring/touring_cli.py@pgcc_warm.json",
        ];
        // quatro ordens DISTINTAS — a ordem real embaralha e não cicla (um ciclo repetiria o molde)
        const ORDENS: [[usize; 3]; 4] = [[0, 1, 2], [1, 0, 2], [2, 1, 0], [0, 2, 1]];
        let rotated: Vec<&str> = ORDENS[i % 4].iter().map(|&k| docs[k]).collect();
        format!(
            "HOTSPOT: `len` has 12 callers | callers: [chave_norma, chave_instrumento, \
             _e_codigo_planilha, _flush_fonte, flush_grafos (+7 more)] | note: High complexity: \
             indexar_inventario_familia(CC=15), _normalizar_artefato(CC=12) | 📚 related docs: [{}] \
             | knowledge: {}/21569 files indexed (score 0.99) | health: healthy 0.84 [wiring:0.84] \
             | 🔮 Next: Bash ({}%) | ⚠️ distribution drift detected: KS=0.533 JS=0.000",
            rotated.join(", "),
            21500 + i,
            50 + i
        )
    }

    fn call<'a>(root: &'a Path, target: Option<&'a str>) -> TargetCall<'a> {
        TargetCall {
            root,
            session: "sess-janela",
            agent: None,
            emitter: "pre_edit",
            target,
        }
    }

    #[test]
    fn janela_o_mesmo_arquivo_repetido_corta_os_bytes_antes_e_depois() {
        let blocks: Vec<String> = (0..4).map(|i| out_json(&pre_edit_block(i))).collect();
        let raw: usize = blocks.iter().map(String::len).sum();

        // ANTES: a dedupe de hoje (bloco/linha por molde) — a ordem de `related docs` e os
        // contadores fazem a linha única parecer nova a cada chamada.
        let (a, s) = setup();
        let before: usize = blocks
            .iter()
            .map(|b| dedup_hook_output_with(a.path(), &s, b, false).len())
            .sum();

        // DEPOIS: o estágio por (emissor, alvo) + a dedupe de hoje por cima.
        let b = tempfile::tempdir().expect("tempdir");
        let outs: Vec<String> = blocks
            .iter()
            .enumerate()
            .map(|(i, blk)| {
                dedup_hook_output_for_target_with(
                    &call(b.path(), Some("/p/a.rs")),
                    blk,
                    T0 + 60 * i as u64,
                    JANELA,
                    false,
                )
            })
            .collect();
        let after: usize = outs.iter().map(String::len).sum();

        eprintln!(
            "retro-b bytes (4 emissões do mesmo emissor para o mesmo arquivo): \
             bruto={raw} antes(dedupe de hoje)={before} depois(janela)={after} \
             → corte {:.1}% sobre o que passava",
            100.0 * (before - after.min(before)) as f64 / before as f64
        );
        assert_eq!(
            before, raw,
            "a dedupe de hoje não tira NADA deste padrão — é o defeito"
        );
        assert_eq!(outs[0], blocks[0], "a 1ª emissão passa intacta");
        assert!(
            outs[1..]
                .iter()
                .all(|o| !o.contains("HOTSPOT") && !o.contains("callers:")),
            "as repetições não carregam de novo os fatos do arquivo: {outs:?}"
        );
        assert!(
            after * 2 < before,
            "o corte é maior que a metade: {after} vs {before}"
        );
    }

    #[test]
    fn janela_conteudo_novo_passa_e_so_ele() {
        let d = tempfile::tempdir().expect("tempdir");
        let c = call(d.path(), Some("/p/a.rs"));
        let _ =
            dedup_hook_output_for_target_with(&c, &out_json(&pre_edit_block(0)), T0, JANELA, false);
        let novo = format!(
            "{} | note: nova_funcao_quebrada(CC=21) passou do limite",
            pre_edit_block(1)
        );
        let out = dedup_hook_output_for_target_with(&c, &out_json(&novo), T0 + 30, JANELA, false);
        assert!(
            out.contains("nova_funcao_quebrada"),
            "o que mudou passa: {out}"
        );
        assert!(
            !out.contains("HOTSPOT"),
            "o que já foi dito não volta: {out}"
        );
    }

    #[test]
    fn janela_expirada_reemite_no_estagio_e_a_garantia_da_epoca_nao_afrouxa() {
        // o estágio puro: dentro da janela omite, passada a janela desde a ÚLTIMA entrega volta.
        let mut seen = Vec::new();
        let blk = pre_edit_block(0);
        assert_eq!(window_filter(&mut seen, 1, &blk, T0, JANELA), blk);
        // (só sobra o segmento curto — < 24 B, o piso que a dedupe nunca toca: `🔮 Next: Bash (N%)`)
        let dentro = window_filter(&mut seen, 1, &blk, T0 + JANELA, JANELA);
        assert!(
            dentro.len() < MIN_LINE_BYTES,
            "dentro da janela (<=): só o resíduo curto fica — {dentro:?}"
        );
        assert_eq!(
            window_filter(&mut seen, 1, &blk, T0 + 2 * JANELA + 1, JANELA),
            blk,
            "passada a janela o conselho volta"
        );
        // o pipeline inteiro: um bloco IDÊNTICO segue retido pela dedupe por época — o estágio só
        // soma omissão, nunca devolve o que a época já segura (a 1ª cópia vale até a compactação).
        let d = tempfile::tempdir().expect("tempdir");
        let c = call(d.path(), Some("/p/a.rs"));
        let out = out_json(&pre_edit_block(0));
        let _ = dedup_hook_output_for_target_with(&c, &out, T0, JANELA, false);
        let tarde = dedup_hook_output_for_target_with(&c, &out, T0 + 2 * JANELA + 1, JANELA, false);
        assert!(
            !tarde.contains("HOTSPOT"),
            "a época ainda retém o idêntico: {tarde}"
        );
    }

    #[test]
    fn janela_alvos_e_agentes_distintos_nao_se_suprimem() {
        // dois ARQUIVOS: a chave (emissor, alvo) é outra, e o que o estágio já entregou para
        // `a.rs` não protege `b.rs` — o mesmo segmento volta para o outro alvo.
        let mut seen = Vec::new();
        let blk = pre_edit_block(0);
        let key = |t: &str| hash_line(&format!("pre_edit\u{2}{t}"));
        assert_eq!(
            window_filter(&mut seen, key("/p/a.rs"), &blk, T0, JANELA),
            blk
        );
        assert_eq!(
            window_filter(&mut seen, key("/p/b.rs"), &blk, T0 + 1, JANELA),
            blk
        );
        let de_novo = window_filter(&mut seen, key("/p/a.rs"), &blk, T0 + 2, JANELA);
        assert!(
            de_novo.len() < MIN_LINE_BYTES,
            "a.rs já recebeu: {de_novo:?}"
        );

        // um subagente tem contexto PRÓPRIO: o estado dele é outro arquivo, e o 1º conselho
        // dele sai inteiro mesmo depois de o pai ter recebido o dele.
        let d = tempfile::tempdir().expect("tempdir");
        assert_ne!(
            target_state_file(d.path(), "sess-janela", None),
            target_state_file(d.path(), "sess-janela", Some("agent-7"))
        );
        let pai = call(d.path(), Some("/p/a.rs"));
        let _ = dedup_hook_output_for_target_with(&pai, &out_json(&blk), T0, JANELA, false);
        let sub = TargetCall {
            agent: Some("agent-7"),
            ..call(d.path(), Some("/p/a.rs"))
        };
        // (texto distinto do do pai para não cair na dedupe por época, que é por sessão)
        let do_sub = blk.replace("_normalizar_artefato", "_normalizar_do_subagente");
        let out =
            dedup_hook_output_for_target_with(&sub, &out_json(&do_sub), T0 + 2, JANELA, false);
        assert!(out.contains("HOTSPOT") && out.contains("callers:"), "{out}");
    }

    #[test]
    fn janela_sem_alvo_ou_desligada_e_como_antes() {
        let blk = out_json(&pre_edit_block(0));
        let (a, s) = setup();
        let d = tempfile::tempdir().expect("tempdir");
        for i in 0..3u64 {
            let b = out_json(&pre_edit_block(i as usize));
            let want = dedup_hook_output_with(a.path(), &s, &b, false);
            let got =
                dedup_hook_output_for_target_with(&call(d.path(), None), &b, T0 + i, JANELA, false);
            assert_eq!(got, want, "sem alvo: só a dedupe de sempre (emissão {i})");
        }
        // kill switch humano: nada é tocado, nem pela época.
        let e = tempfile::tempdir().expect("tempdir");
        let c = call(e.path(), Some("/p/a.rs"));
        let x = dedup_hook_output_for_target_with(&c, &blk, T0, JANELA, true);
        let y = dedup_hook_output_for_target_with(&c, &blk, T0 + 1, JANELA, true);
        assert!(x == blk && y == blk, "disabled: intacto");
        // janela 0 desliga SÓ o estágio: o que sai é exatamente o da dedupe por época.
        let (f, g) = (
            tempfile::tempdir().expect("tempdir"),
            tempfile::tempdir().expect("tempdir"),
        );
        for i in 0..3u64 {
            let b = out_json(&pre_edit_block(i as usize));
            let got = dedup_hook_output_for_target_with(
                &call(f.path(), Some("/p/a.rs")),
                &b,
                T0 + i,
                0,
                false,
            );
            assert_eq!(
                got,
                dedup_hook_output_with(g.path(), &s, &b, false),
                "window=0, emissão {i}"
            );
        }
    }

    #[test]
    fn janela_epoca_nova_reabre() {
        let d = tempfile::tempdir().expect("tempdir");
        let c = call(d.path(), Some("/p/a.rs"));
        let blk = out_json(&pre_edit_block(0));
        let _ = dedup_hook_output_for_target_with(&c, &blk, T0, JANELA, false);
        assert!(
            !dedup_hook_output_for_target_with(&c, &blk, T0 + 5, JANELA, false).contains("HOTSPOT")
        );
        advance_epoch(d.path(), c.session);
        let depois = dedup_hook_output_for_target_with(&c, &blk, T0 + 10, JANELA, false);
        assert!(
            depois.contains("HOTSPOT"),
            "a compactação podou o contexto: reemite — {depois}"
        );
    }

    #[test]
    fn janela_payload_de_gate_nunca_e_suprimido() {
        let mut seen = Vec::new();
        let g3 = "[G3 edit-sem-read] Edit de /p/a.rs sem Read recente (advisory 1/1 — o 2º seguido nega).";
        let a = window_filter(&mut seen, 7, g3, T0, JANELA);
        let b = window_filter(&mut seen, 7, g3, T0 + 1, JANELA);
        assert_eq!(a, g3);
        assert_eq!(
            b, g3,
            "o aviso de um gate é decisão, não enriquecimento: nunca vira omissão"
        );
        // e uma linha de tabela/curta passa sempre.
        let t = "| a | b |\n|---|---|";
        assert_eq!(window_filter(&mut seen, 7, t, T0, JANELA), t);
        assert_eq!(window_filter(&mut seen, 7, t, T0 + 1, JANELA), t);
    }

    #[test]
    fn janela_a_ordem_de_uma_lista_nao_e_conteudo() {
        assert_eq!(
            segment_key("📚 related docs: [b doc one, a doc two, c doc three]"),
            segment_key("📚 related docs: [c doc three, b doc one, a doc two]")
        );
        assert_ne!(
            segment_key("📚 related docs: [b doc one, a doc two]"),
            segment_key("📚 related docs: [b doc one, z doc nova]"),
            "um item novo é conteúdo novo"
        );
    }

    #[test]
    fn janela_from_env_so_aceita_inteiro() {
        assert_eq!(window_from(None), TARGET_WINDOW_SECS);
        assert_eq!(window_from(Some("30")), 30);
        assert_eq!(window_from(Some(" 90 ")), 90);
        assert_eq!(window_from(Some("0")), 0, "0 desliga o estágio");
        assert_eq!(
            window_from(Some("dez")),
            TARGET_WINDOW_SECS,
            "lixo cai no padrão"
        );
    }
}
