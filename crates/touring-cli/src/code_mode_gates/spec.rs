//! The contract in numbers: every window, threshold and budget of the code-mode gates.
//!
//! Workspace `CLAUDE.md` §10 documents the same table for the humans; the unit test
//! `the_declared_windows_are_the_windows_the_ledgers_enforce` pins this file to the ledgers
//! (`moka` exposes each cache's `time_to_live`), so a window cannot drift from its declaration
//! without a red test.

pub(crate) const SUGGESTION_TTL_SECS: u64 = 300;

pub(crate) const CACHE_MAX_CAPACITY: u64 = 4096;


/// Teto de projetos com τ conformal memoizado simultaneamente. Um daemon atende
/// poucos projetos por janela; o limite existe para que o cache não cresça
/// indefinidamente num processo que vive muito mais que qualquer sessão.
pub(crate) const CONFORMAL_TAU_MAX_PROJECTS: u64 = 64;


/// Window length for the repeated-scan counter. Shorter than the suggestion TTL
/// so the "repeated scanning" signal reflects the *current* burst of activity.
pub(crate) const CODE_MODE_WINDOW_SECS: u64 = 180;


/// S4 — janela da rajada de execução: 600s. O passo de uma suíte de testes
/// (minutos por chamada) é mais lento que o de uma rajada de inspeção (180s
/// do G1) — a janela curta nunca veria o 10º passo.
pub(super) const EXEC_BURST_WINDOW_SECS: u64 = 600;

/// S4 — disparo da rajada de execução homogênea. Nasceu em 10 (R4: sessões
/// reais iam a 30-75); **aperto 29/08 (ordem de Gabriel): 10→5** — 5 já pagou
/// 4 round-trips, e o uso pontual legítimo (2-3 pytest de debug) segue fora.
pub(super) const EXEC_BURST_DENY_AT: u32 = 5;


/// S5 (29/08) — janela do par write→run: a mesma da rajada de execução.
pub(super) const WRITE_RUN_WINDOW_SECS: u64 = 600;

/// S5 — disparo do par write→run: escrever um script e executá-lo em seguida
/// é o loop execute-observe por definição (28 pares no turno de 60 do
/// `analise`, todos invisíveis porque cada metade é individualmente legítima).
/// **Aperto 29/08 (ordem de Gabriel): 3→2** — o 1º par sempre passa (criar e
/// testar UM script é legítimo); o 2º na mesma janela JÁ é o loop. O limiar
/// baixo não taxa o caso comum: pytest/gates/tools nunca casam — o path que
/// eles executam não foi escrito via `cat >`/`tee` na janela.
pub(super) const WRITE_RUN_DENY_AT: u32 = 2;


/// S3 — janela da rajada de INSPEÇÃO, derivada de medição e não de intuição.
///
/// Medido em 115 transcripts desde 01/08/2026 (`scripts/s3_burst_distribution.py`),
/// sobre as 1.118 chamadas das classes que o modo `code` negava por lista fixa:
///
/// | janela | isoladas | volume em rajada ≥2 |
/// |---|---|---|
/// | 60s  | 41,6% | 58,4% |
/// | 120s | 32,1% | 67,9% |
/// | **300s** | **22,5%** | **77,5%** |
/// | 600s | 17,8% | 82,2% |
///
/// 300s é o joelho: de 300 para 600 a janela DOBRA para comprar 4,7 pontos de
/// volume, e cada segundo a mais é uma chance de agrupar duas inspeções que não
/// têm nada a ver uma com a outra — um falso positivo de rajada é exatamente a
/// fricção no caso comum que o S3 existe para remover.
pub(super) const INSPECT_BURST_WINDOW_SECS: u64 = 300;


/// S3 — a 2ª avisa (D, 09/10/2026: antes negava); a 1ª executa intacta.
///
/// A medição é inequívoca no limiar: com janela de 300s, 77,5% do volume das
/// classes de inspeção está em rajadas de tamanho ≥ 2. Deixar a isolada passar
/// renuncia a 22,5% dos denies e preserva os outros 77,5% — o oposto do que a
/// lista fixa fazia, que era cobrar 100% da fricção para capturar o mesmo 77,5%.
/// É a regra que o próprio DeepSeek documentou e que o T3-B já declarava sem
/// nunca conseguir aplicar: *"forcing every edit through a program taxes the
/// common case"*.
pub(super) const INSPECT_BURST_ADVISE_AT: u32 = 2;


/// Janela do orçamento, alinhada à do G10 (o gate de execução em rajada).
pub(super) const G11_WINDOW_SECS: u64 = 600;


/// Bypasses ESTRITAMENTE seguidos tolerados. `1` significa: o primeiro passa, o
/// segundo colado no primeiro não.
pub(super) const G11_MAX_STRICT_RUN: u32 = 1;


/// Bypasses tolerados desde o último uso da rota sancionada, ainda que
/// intercalados por comandos neutros. `2` significa: o terceiro não passa.
pub(super) const G11_MAX_SINCE_ROUTE: u32 = 2;


/// Token de bypass POR-COMANDO: viaja no próprio comando (o padrão
/// `GIT_DESTRUCTIVE_OK` — cada uso é uma decisão, nunca um estado exportado).
pub(super) const GATE_BYPASS_TOKEN: &str = "TOURING_GATE_OK=1";


/// Longest single-line command a reason inlines as a ready-to-paste bypass line; beyond it the
/// reason points at the command it already shows instead of repeating it.
pub(super) const READY_LINE_MAX_CHARS: usize = 600;


/// W2 S-2.1 — a inspeção repetida da MESMA classe na janela nega (o advisory
/// legado fala uma antes). 90% de precisão-proxy medida na 4ª (51/55 sessões);
/// **aperto 29/08 (ordem de Gabriel): 4→3** — na 3ª o padrão já é inequívoco
/// (o mesmo racional do CODE_MODE_SCAN_THRESHOLD) e o custo do FP segue 1 bypass.
pub(super) const G1_DENY_AT: u32 = 3;

/// W2 S-2.3 — piso de precisão viva abaixo do qual o G1 se autodemove a
/// advisory (F7: demote é código, não reunião)…
pub(super) const G1_DEMOTE_FLOOR: f64 = 0.70;

/// …desde que haja volume real: um punhado de eventos iniciais nunca demove.
pub(super) const G1_DEMOTE_MIN_EVENTS: u64 = 100;


/// Files above this are not scanned for the Read-window hint (the deny path stays cheap).
pub(super) const G3_WINDOW_MAX_BYTES: u64 = 2 * 1024 * 1024;

/// Context lines kept on each side of the Edit's `old_string` in the Read-window hint.
pub(super) const G3_WINDOW_CONTEXT_LINES: usize = 5;

/// Files shorter than this are read whole: a window would save nothing.
pub(super) const G3_WINDOW_MIN_FILE_LINES: usize = 80;


/// M3 — quantos arquivos distintos disparam o advisory de delegação.
pub(super) const M3_DELEGATION_AT: u32 = 10;


/// Orçamento total, em chars, do corpo do programa que o remédio entrega.
pub(super) const G1_BODY_BUDGET: usize = 3000;


/// Limiar da calibração por similaridade (Gabriel, 26/08/2026).
pub(crate) const G1_SIMILARITY_AT: f64 = 0.5;


// ── Ledger sizing and the windows that were literals inside the old accessors ────────────

/// Capacity of the small per-session ledgers (pending denies, streaks, M3, G4, G5).
pub(super) const SESSION_LEDGER_CAPACITY: u64 = 1024;
/// A pending deny waits this long for its follow-through (G1/G2/G9/G10 `followed` metric).
pub(super) const PENDING_DENY_TTL_SECS: u64 = 60;
/// G3 streak and reads, G7 counts and stamps, M3 and G5: one hour of session memory.
pub(super) const SESSION_STATE_TTL_SECS: u64 = 3600;
/// G4: a Grep/Glob this recent counts as "the session located something".
pub(super) const G4_LOCATE_TTL_SECS: u64 = 120;
/// Capacity of the G11 bypass budget ledger.
pub(super) const BYPASS_LEDGER_CAPACITY: u64 = 256;
