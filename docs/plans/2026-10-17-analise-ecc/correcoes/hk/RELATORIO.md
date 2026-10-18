# hk — C4 (guarda falha aberto) e C10 (atribuição de família do `context_budget`)

Base: worktree `/home/gabrielgadea/projects/touring-wt-ecc-hk`, HEAD destacado `98301443`.
Patch: `hk.patch` (13 arquivos, +1312/−87), sha256 `7dc3f40149523493eec050e0530fbdfe0df158e4e04d49d3c11ce3c29d3f8a5a`
(`hk.patch.sha256`). Aplica limpo sobre cópias pristinas do HEAD (`git apply --check`, rc 0).
Nada foi commitado, empurrado, implantado nem stashed. Target: `target/wt-p0bd`, todo cargo sob `hw-slot --class rust`.

## C4 — guarda que não consegue ler a entrada falha FECHADA

### O que estava errado (medido, não suposto)

`read_stdin` devolvia `{}` em `Empty`, `Timeout` e `ReadError`, e `read_all_with_timeout` não tinha teto.
Sonda com o binário instalado, daemon fora (`scripts/probe_hooks_stdin_vazio.sh`), alimentando `{}`:

| hook | resposta com `{}` | leitura |
|---|---|---|
| `pre-bash`, `pre-edit-prevention`, `pre-tool-use` | rc 0, stdout vazio | **LIBERA** (falha aberto) |
| `pre-write`, `pre-edit`, `pre-read` | deny `validation_failed: tool_input missing` | nega por acidente de schema, causa enganosa |
| `pre-grep`, `cli-suggest` (conselheiros) | rc 0, vazio | fail-open correto |

Exposição real, `hook_trace.jsonl` (armado em produção, 839.577 linhas): **0 de 129.250 invocações de guarda** tiveram stdin
degradado; conselheiros: 3 timeouts em 710.327 (`stdin_fraction_antes.json`). O buraco é estrutural e latente, não um vazamento ativo.

### Desenho

- **Estados** (`StdinReadState`): `OkRecovered` (`ok-recovered`) e `TooLarge` (`too-large`) novos; `is_unreadable()` = `Timeout | ReadError | ParseError | TooLarge`.
- **(a)** timeout/erro com bytes que já formam **um objeto JSON completo** → usa o JSON, estado `ok-recovered` contado (o caso F0.3b: payload inteiro, sem EOF). O leitor passou a acumular num buffer compartilhado; antes o timeout descartava os bytes.
- **(b)** timeout/erro sem JSON completo, não-JSON após EOF, ou acima do teto → **guarda nega** com `[entrada-ilegível] <hook>: … nada foi inspecionado … Saída: repita a mesma chamada; se for o teto, divida; se persistir, avise o Gabriel (kill switch humano TOURING_HOOK_STDIN_GUARD_DISABLED=1; diagnóstico: touring doctor -j e stdin_state em TOURING_HOOK_TRACE_FILE)`. Conselheiro segue `{}` como hoje.
- **(c)** `Empty` (0 bytes, EOF limpo) e `Terminal` → libera, contado.
- **(d)** todo estado ≠ `ok` vai ao `hook_trace` (`stdin_state`); o deny grava `exit_reason = stdin-unreadable-deny`. Escolhi o trace, não o `gate-metrics`: o cliente é efêmero e o deny sai antes de falar com o daemon (que é quem guarda os contadores do `gate-metrics`); o trace está armado em produção e já é a fonte da linha de base. Medir antes/depois: `scripts/stdin_fraction.py --since-ms <deploy>`.
- **Teto 16 MiB** (`MAX_STDIN_BYTES`, o leitor para no teto). Justificativa medida: maior payload já visto por QUALQUER hook = 6,1 MB (`post-tool-batch`, conselheiro); por guarda = 126 KB (`pre-write`). 16 MiB dá 2,6× sobre o primeiro (nenhum conselheiro regride) e 130× sobre o segundo. 8 MiB deixaria só 1,3× sobre o máximo real. Consequência declarada: conselheiro com payload > 16 MiB passa a rodar em `{}` (antes processava); nunca ocorreu.
- **Quais são guardas**: `GUARD_HOOK_NAMES = pre-bash, pre-edit, pre-edit-prevention, pre-read, pre-tool-use, pre-write` (+ `pre-exec` quando `CEG_ENFORCE=1`, via `ceg_adapter::enforcement_enabled`, agora `pub` — um só predicado). Os seis são os handlers cujo fonte emite `HookResponse::Deny` ou `validation_deny`. O teste `guard_set_matches_the_handlers_that_can_deny` **re-deriva** o conjunto do fonte dos handlers, então um handler novo que negue não fica conselheiro em silêncio.
- **Onde**: `touring-hook` (`main.rs`) — `deny_if_guard_cannot_read`, antes do daemon e do standalone. Não usei `emit()`: ele carimba `exit_reason=emit-json` e apagaria o rótulo que a fração mede (o 1º run do e2e reprovou exatamente nisso e foi corrigido: imprime `to_json()` e depois fixa a razão).
- `read_stdin()` manteve a assinatura (wrapper de `read_stdin_detailed`); `command_table.rs:25` e `neural.rs:53` não mudam.

### Testes e provas

| teste | o que prova |
|---|---|
| `hook_runtime_ext::stdin_read_tests` (16; 11 novos `c4_*`) | os 4 estados + teto inclusivo + deny que ensina; os 5 testes F0.3b antigos migrados para `read_bounded` e verdes |
| `touring-dispatch` `guard_set_matches_the_handlers_that_can_deny`, `guards_are_registered_hooks_and_advisories_are_not_guards` | classificação derivada do código |
| `touring-hooks/tests/stdin_guard_fail_closed_e2e.rs` (9, binário real) | cada um dos 6 guardas nega JSON inválido e documento truncado; timeout (2 s) e teto nos guardas; payload inteiro sem EOF é USADO (`ok-recovered`); vazio liberado e contado; kill switch (só o valor `1`); **controle**: `pre-grep`, `pre-glob`, `cli-suggest`, `post-bash` com os mesmos bytes seguem fail-open |

**Vermelho no original / mutantes** (`scripts/mutantes/`, cada um aplicado, testado, restaurado; um slot só): **9/9 mortos**.

| mutante | efeito | testes que falharam |
|---|---|---|
| M1 remove o ramo fail-closed (= comportamento original) | guarda libera | 5 e2e (`every_guard_denies_*`, `guard_denies_*`, `the_human_switch_*`) |
| M2 `Timeout` deixa de ser ilegível | | `c4_unreadable_classification_*`, `c4_silent_open_pipe_*`, `c4_timeout_with_a_truncated_json_*` |
| M3 teto ignorado | | `c4_payload_over_the_cap_*`, `c4_the_cap_boundary_is_inclusive` |
| M4 sem recuperação do JSON inteiro | | `c4_timeout_with_a_complete_json_*`, `c4_read_error_*` |
| M5 `pre-edit-prevention` fora do conjunto | | `guard_set_matches_the_handlers_that_can_deny` |
| M6 todo hook é guarda | | `guards_are_registered_hooks_and_advisories_are_not_guards` |

### Não coberto (declarado)

- `touring-cortex/src/runtime.rs:293` tem leitor próprio de stdin; nenhum hook do `settings.json` o chama (só `touring-hook`, scripts Python e shell).
- Guardas em Python/shell (`block_git.sh`, `agent_model_gate.py`, `evidence_guard.py`…) têm o próprio tratamento de stdin; fora do escopo pedido.
- `memory recall "F0.3b socketpair stdin"` não achou nada; o histórico veio dos comentários e testes do próprio código, todos preservados.

## C10 — atribuição de família do KPI `context_budget`

### Causa

`injection_family(text)` classifica por agulhas no TEXTO injetado. Mas cada `hook_success` do transcript já grava o **emissor**
em `command` (o `statusMessage` do hook, ou a linha de comando), e o parser o ignorava. Das 7 famílias, as agulhas só reconhecem
emissores que se anunciam no texto; todo o resto caía em `other`.

Amostra (3 transcripts mais recentes, 13.741 `hook_success`, **23 marcas distintas**): `other` = 82,8 % dos bytes.
Decomposição de `other` por emissor: `Touring: analyzing blast radius...` (pre-edit) 403 KB, `speculative validation` (pre-write) 178 KB,
`cli-suggest` 140 KB, `analyzing file context` (pre-read) 74 KB, `verifying edit quality` (post-edit) 63 KB, `evidence_guard.py` 60 KB,
hooks de plugin (`${CLAUDE_PLUGIN_ROOT}`) 67 KB, `loop_resume.py` 36 KB, o resto < 16 KB cada.

### Desenho (zero byte acrescentado)

Não precisei carimbar emissor Rust algum: a marca já existe e é estável (23 marcas cobrem 100 % dos blocos). Carimbar só somaria bytes.
`attributed_family(text, emitter)` — ordem: **emissor dedicado é dono** (hook de propósito único: pre-edit, pre-write, pre-read, pre-bash,
pre-edit-prevention, post-edit, post-write, evidence-guard, plugin-hook, prose-integrity, context-monitor, delegation-nudge) → agulhas do
texto → **reserva do emissor** (multiuso: `cli-suggest`→touring-suggest, `loop_resume`→loop-outer, `post-bash`→post-tool-echo) → `other`.
Um deny emitido pelo `cli-suggest` continua `code-mode-deny`. `injection_family(text)` ficou intacta (o r3 a usa para o dataset congelado).
Sem marca (Claude Code antigo) o comportamento é o de antes.

### Antes → depois (mesma amostra)

Mesmo instante, réplica Python da lógica antiga vs nova, 1.277.132 B (`scripts/familias_antes_depois.py`; a réplica reproduz o KPI oficial: 82,8 % vs 82,8 %):

| família | antes | depois |
|---|---|---|
| `other` | 1.057.505 B (82,8 %) | **0 B (0,0 %)** |
| `post-tool-echo` | 128.611 (10,1 %) | 25.325 (2,0 %) |
| pre-edit | — | 508.858 (39,8 %) |
| pre-write | — | 178.599 (14,0 %) |
| touring-suggest | 36.203 (2,8 %) | 176.293 (13,8 %) |

Confirmação pelo código Rust novo (a mesma `context_budget()` que o `touring kpi -j` chama, sonda temporária removida) sobre os transcritos vivos, 1.331.346 B:

| família | bytes | % | blocos | sem proposição |
|---|---|---|---|---|
| pre-edit | 524.475 | 39,4 | 323 | 323 |
| touring-suggest | 188.632 | 14,2 | 700 | 67 |
| pre-write | 184.261 | 13,8 | 174 | 174 |
| pre-read | 79.583 | 6,0 | 69 | 69 |
| plugin-hook | 67.917 | 5,1 | 672 | 672 |
| post-edit | 65.898 | 4,9 | 174 | 174 |
| evidence-guard | 62.549 | 4,7 | 390 | 390 |
| loop-outer | 43.711 | 3,3 | 8 | 4 |
| code-mode-deny | 37.763 | 2,8 | 44 | 44 |
| post-tool-echo | 26.457 | 2,0 | 148 | 134 |
| demais (8) | < 20.000 cada | | | |
| **other** | **ausente** | **0** | | |

`injected_bytes_per_turn` = 12.103 (teto 2.000): **FAIL permanece**, como combinado — o defeito era a atribuição, não o volume.
O que a atribuição revela: **pre-edit sozinho é 39 % do volume e 100 % dos seus 323 blocos não têm uma única proposição MUST/SHOULD/MAY**
(idem pre-write 174/174, pre-read 69/69, post-edit 174/174). São enriquecimento puro. É o alvo certo para reduzir volume.
Também corrigiu um viés: 105 KB de blocos do pre-edit estavam em `post-tool-echo` (o texto cita `last fail:`).

### Testes

`context_budget` (25 verdes no lib; novos): `the_real_emitters_leave_other_below_the_ceiling` (fixture real, 45 registros/23 marcas, caminhos anonimizados, em `crates/touring-cli/tests/fixtures/`; sem a atribuição `other` = 54,4 % dos bytes da fixture, com ela 0 %), `every_real_mark_lands_in_its_family`, `a_dedicated_emitter_owns_its_output_…`, `a_fallback_emitter_defers_to_the_text`, `an_unknown_mark_or_a_missing_one_never_invents_a_family`, `the_emitter_table_has_no_stale_needle` (cada agulha existe em `client/claude/hooks.settings.json`, para a tabela não apodrecer quando um `statusMessage` for reescrito).
Mutantes **M7** (emissor `None` = comportamento original), **M8** (dono vira reserva), **M9** (agulha removida): **3/3 mortos**.

### Porte Python (consumidor que o patch quebraria)

`scripts/r3_a1_kpi.py` é o porte "fiel" do KPI e `test_r3_a1_kpi.py` compara família a família (±1 %) com o `touring kpi -j` VIVO. O porte e o KPI Rust passam a ter de mudar JUNTOS: medido hoje, o porte já atualizado contra o `touring kpi -j` instalado (taxonomia antiga) dá `paridade_1pct=False` (`other`: porte 0 B, KPI 1.114.060 B); a leitura só volta a fechar com o binário novo implantado. Atualizei só a função de paridade (`agregar` → `hook_injections` + `attributed_family` + `EMITTERS`); `injection_family` (texto) ficou intacta, então o dataset congelado do r3 não muda. Novos testes: a tabela Python é lida do FONTE Rust e comparada (`test_emitter_table_is_the_rust_one`), e a mesma fixture real roda nas duas linguagens. **Ordem de deploy**: `update-touring` antes de rodar os testes de paridade viva (5 deselecionados aqui; comparam com o binário instalado, que ainda tem a taxonomia antiga).

## Falhas pré-existentes encontradas e corrigidas (REGRA #21)

1. `touring-dispatch` `updated_input_producers_are_pinned`: `verdict.rs` (já no snapshot) re-embrulha um `ContextWithUpdatedInput` recebido sem originá-lo (`a_reescrita_mantem_o_updated_input`); entrou na lista `FRAMEWORK`. 1 linha + comentário.
2. `touring-hooks` `wave24…task_created_event_returns_scaffolded_receipt`: reprovou na suíte cheia sob carga (stdout vazio = fallback silencioso do daemon, a classe de flutuação que o próprio cabeçalho do teste documenta, S216). Isolado: 3/3 verde, e não passa por nenhum caminho que o C4 toque. Mesmo assim endureci o helper `task_receipt`: repete a chamada medida até 3× enquanto for fallback silencioso; a chamada respondida ainda tem de manter o stdout no contrato, e um daemon que nunca responde cai na asserção "produced no receipt" — não passa vazio.

## Gates (árvore final, nada editado depois)

| comando | rc |
|---|---|
| `cargo fmt --all -- --check` | 0 |
| `cargo clippy -p touring-hook-runtime -p touring-dispatch -p touring-hooks -p touring-cli --all-targets -- -D warnings` | 0 |
| `cargo test -p touring-hook-runtime --no-fail-fast` | 0 — 510 passed, 9 ignored |
| `cargo test -p touring-dispatch --features pre-hooks,post-hooks --no-fail-fast` | 0 — 1351 passed, 1 ignored |
| `cargo test -p touring-hooks --features pre-hooks,post-hooks --no-fail-fast` | 0 — 1193 passed |
| `cargo test -p touring-cli --no-fail-fast` | 0 — 701 passed, 4 ignored |
| `pytest scripts/test_r3_a1_kpi.py -k "not paridade and not parity"` (hw-slot) | 0 — 15 passed |

Passadas anteriores, para o registro honesto: o 1º run reprovou `fmt` (14 hunks, só em arquivos meus), 5 e2e (`exit_reason` sobrescrito pelo `emit()`),
`updated_input_producers_are_pinned` e o `wave24`; tudo corrigido e reexecutado na árvore final.

## Efeitos colaterais e avisos ao orquestrador

- **Target compartilhado**: os e2e do `touring-hooks` reconstroem `touring-hook`/deps em `target/wt-p0bd` a partir deste worktree. O cargo invalida por fingerprint, então o `p0bd-eng` só paga rebuild; mas o `debug/touring-hook` uplifted ali é do meu patch até o próximo build dele. Não construí o binário `touring` inteiro lá, de propósito.
- Fila do `hw-slot`: ~1h40 de espera somada (sessões `juizes_p0b`, `ecc-qa`, `ecc-py`), por isso as passadas foram agrupadas em uma aquisição cada.
- `CLAUDE.md` (regra #16) não foi tocado; a lição foi para `touring memory` (`hk-stdin-guard-failclosed-e-atribuicao-por-emissor-2026-10-17`).
- Deploy: `update-touring` (ou `scripts/propagate-release.sh`); depois `scripts/stdin_fraction.py --since-ms <deploy>` e `familias_context_budget.py` rodado da raiz do projeto (a partir de `/tmp` o KPI devolve STUB, 0 sessões — armadilha de cwd).
