# C5 — Espelho de sinais com uma interface (nota de desenho, 12/11/2026)

DAG `task_1794458342745990025`, subtask C5. Base `a3bfb950`.

## Problema (medido por grep em `crates/`)
O espelho (`~/.claude/touring/sdk_signal_mirror.jsonl`) tem escrita e leitura espalhadas:
- escrita: `sdk::record_hook_call` (sdk.rs:260) é repasse 1:1 para `sdk_signal_mirror::record`;
- caminho padrão: `default_mirror_path` chamado em 5 sítios (post_bash.rs:103, run.rs:878, code_mode.rs:562 e :633, exemplo);
- leitura: o CLI relê o arquivo 2× em `code_mode_signal_use` (:563 `read_to_string` + :571 `mirror::read`) e 1× em `hooks_complement` (:634); reparseia cada linha como `Value` pela chave `"hook_name"` (:597-600, :668-675) com `TOTAL_HOOKS = 8` fixo (:588);
- sem consumidor de produção: `augment_with_mirror`, `signal_report_from_journal_and_mirror` (só o exemplo `sdk_posttool_end_to_end`), `HookCallEntry`, `HookCallCounts`, `by_hook`, `total_failures`, `MirrorError::{Malformed,UnknownHook}`.

## Interface nova (`touring_code::sdk_signal_mirror`)
```rust
pub struct Mirror { /* path */ }
impl Mirror {
    pub fn at(path: impl Into<PathBuf>) -> Self;      // testes / cópia
    pub fn from_env() -> Option<Self>;                // $HOME/.claude/touring/sdk_signal_mirror.jsonl
    pub fn path(&self) -> &Path;                      // o sandbox recebe o caminho (run.rs)
    pub fn record(&self, hook: HookName, duration_ms: u32, success: bool, origin: MirrorOrigin) -> io::Result<()>;
    pub fn read(&self, since_epoch: u64) -> io::Result<Option<MirrorAggregate>>; // None = sem arquivo; 0 = tudo
}
pub struct MirrorAggregate { total_calls, canonical_hooks_used, non_canonical_calls,
    post_bash_calls, sdk_calls, unknown_origin_calls, duration_ms_p50, duration_ms_p99 }  // dado puro, Default
pub enum MirrorOrigin { PostBash, Sdk }  // + as_str — inalterado (run.rs:3026 o usa)
```
Atrás do seam: o formato da linha (`ts`, `hook_name`, `duration_ms`, `success`, `origin`), o caminho
padrão, o conjunto canônico (`sdk::parse_hook`, que ganha consumidor), os percentis, a janela por `ts`.
Padrão do seam irmão `touring_foundation::run_outcome::Journal` (`at`/`from_env`/`path`).

## Invariantes (provados por teste de caracterização escrito ANTES)
1. `code_mode_signal_use`: `{available, used, total, ratio, total_calls, non_canonical_calls, duration_ms_p50, duration_ms_p99}` e a forma
   `{"available": false, "reason": "no mirror yet"}` idênticas byte a byte para o mesmo arquivo (golden das duas implementações).
2. `hooks_complement_from`: mesmo JSON (janela `ts < since` fora, origem `post_bash|sdk|unknown`, não-canônicos à parte).
3. Formato gravado inalterado (campos e ordem); espelho real lido sem mudança de saída (cópia somente leitura).
4. `SignalReport`, `classify_bash_command`, os 8 nomes: intactos (`sdk_smoke` antes/depois).

## Único desvio de comportamento (defeito que o aprofundamento conserta)
Hoje `mirror::read` ABORTA na 1ª linha malformada (CLAUDE.md do crate promete "skipped, not abort"; o módulo
promete "one torn line is recoverable"), então `code_mode_signal_use` perde `duration_ms_p50/p99` quando o
arquivo tem UMA linha torta, enquanto a contagem (leitor tolerante) segue. Um leitor único precisa de uma
semântica: tolerante. Arquivos íntegros (todos os do escritor, 0 linhas ruins nas 2109 do espelho real) não mudam.
Teste RED→GREEN: `a_torn_line_does_not_hide_the_durations`.

## Chamadores que mudam (arquivo:linha, antes)
- post_bash.rs:103-114 → `Mirror::from_env()` + `mirror.record(..)` (só essas linhas);
- run.rs:876-880 → `Mirror::from_env().map(|m| m.path().to_path_buf())` (só essas linhas);
- code_mode.rs:556-579, 587-617, 629-637, 644-688 → leem `Mirror`/`MirrorAggregate`; sem `"hook_name"`, `TOTAL_HOOKS`, 2ª/3ª leitura;
- kpi.rs testes :1273/:1306/:1324/:1333/:1351/:2457-2480 → mesma asserção, via o seam (`Mirror::at(tmp)`).

## O que sai
`sdk::record_hook_call`; `default_mirror_path`; `record`/`read` livres; `augment_with_mirror`;
`signal_report_from_journal_and_mirror`; `HookCallEntry`, `HookCallCounts`, `MirrorAggregate.by_hook/total_failures`,
`MirrorError` (vira `io::Error`); o exemplo `sdk_posttool_end_to_end` (só existia para essas funções e, por
padrão, GRAVAVA 9 chamadas sintéticas no espelho real). Testes removidos: `augment_zeroed_hooks_get_mirror_counts` e
`augment_does_not_clobber_existing` — testavam só `augment_with_mirror`; a leitura/gravação segue coberta no seam.
Fora do escopo (cópias independentes, relatadas ao lead): `kpi_f6_smoke.rs`, `touring-quality/.../best_practices.rs::signal_use_counts`,
o escritor Python do sandbox em run.rs:309-370 (mesmo formato, outro idioma; guardado por run.rs:3026).
