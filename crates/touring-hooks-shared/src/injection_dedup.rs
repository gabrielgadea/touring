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
/// (`run_hook_command`), por onde TODOS os handlers da dispatch table saem.
///
/// Extrai `hookSpecificOutput.additionalContext`, passa por [`dedup_sections`] e
/// re-serializa. Qualquer outra forma (não-JSON, sem o campo, contexto vazio,
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
    let Some(ctx) = v
        .pointer("/hookSpecificOutput/additionalContext")
        .and_then(|c| c.as_str())
    else {
        return output.to_string();
    };
    if ctx.is_empty() {
        return output.to_string();
    }
    let deduped = dedup_sections(root, session, ctx);
    if deduped == ctx {
        return output.to_string();
    }
    if deduped.is_empty() {
        // Omissão total: o additionalContext repetido sai do JSON — o envelope
        // vazio (~2 B) é o menor rastro possível, e o CC não injeta nada.
        if let Some(hso) = v.get_mut("hookSpecificOutput") {
            hso.as_object_mut().map(|m| m.remove("additionalContext"));
            if hso.as_object().is_some_and(|m| m.is_empty()) {
                v.as_object_mut().map(|m| m.remove("hookSpecificOutput"));
            }
        }
        return serde_json::to_string(&v).unwrap_or_else(|_| output.to_string());
    }
    let Some(hso) = v.get_mut("hookSpecificOutput") else {
        return output.to_string();
    };
    hso["additionalContext"] = serde_json::Value::String(deduped);
    serde_json::to_string(&v).unwrap_or_else(|_| output.to_string())
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
}
