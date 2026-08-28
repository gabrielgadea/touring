# Cross-audit 2026-08-27 — code mode + CEG sandbox (commit a2ee48e)

> **Skill**: TACO-cross-audit (7 fases) · **Escopo**: commit `a2ee48e` (146 arquivos,
> +8.887/−726) — a DAG S1–S10 de code-mode/CEG · **Auditor**: TACO + 2 subagents
> (audit-debt-harmony, sandbox-research) · **Método**: purpose-fidelity — tudo que
> se afirma aqui tem comando executado + saída; o que não foi executado está marcado
> `UNVERIFIED`.
> **DAG do loop**: `task_1787873547512808623` · **Juiz**: `loop_converged.py`.

## VEREDITO

**PASS — code mode e CEG sandbox foram potencializados e funcionam, provado em
execução.** Nenhum P0 aberto (o único FAIL, F2.1 no `r9_exec_program`, foi
corrigido e re-medido 0.07 → 1.0). 3.306 testes verdes / 0 falhas nos 4 crates
tocados, clippy `-D warnings` limpo, prova comportamental 35/35 ×2 contra o
binário deployado. A auditoria encontrou **7 defeitos reais** (F-1, F-2, H5, H6,
D1 + 2 de instrumento) e os corrigiu; e **potencializou o sandbox** com 5
quick-wins medidos ao vivo (stdin do programa, TMPDIR, caps por flag, compute-ms
por flag + teto 600s, confinamento de sinais no kernel).

A alegação "o sandbox é muito limitado" é **PARTIAL** (ver §8): refutada na metade
estrutural (comandos, FS, SDK, contenção — mais forte que o DeepSeek, que não tem
contenção nenhuma), confirmada na metade operacional (aberturas calibradas —
rede por porta, runtimes ausentes, caps sem flag) — exatamente a classe que os
quick-wins e o roadmap endereçam.

**Pendências que não são da auditoria** (decisão de Gabriel, §10): deploy do
incremento pós-commit via `update-touring`; baseline de órfãos (drift de
instrumento, não de código); os 8 gaps M/L do sandbox (dois são contenção:
UDP e memória).

---

## FASE 0 — Health gate

`touring doctor -j`: **7/7 ok** (binary 30.4.16, socket, daemon, circuit_breaker,
project_db 310 MB, project_actor, wiring 178.257 rows). O `Connection refused` do
SessionStart era a race transitória documentada (REGRA #19) — auto-recuperou.

## FASE 1 — MAP (subagent audit-debt-harmony, evidência CLI citada)

- `git show --stat a2ee48e`: universo = 20 arquivos de código + scripts + bundle.
- `touring ast meta --depth summary -j` nos 6 arquivos-chave caiu em
  `on_disk_fallback` (índice stale — o trabalho do dia foi escrito via
  substituição-exata Python, que não dispara indexação incremental) → **rebuild
  executado nesta auditoria** (`touring index rebuild --dir`, exit 0).
- Wiring dos módulos novos, confirmado por grep (Cadeia 7):
  `code_mode::` ← cli_suggester.rs:3444,3532 + server/mod.rs:147-148,296-297;
  `orchestrate_allowlist::` ← daemon.rs:1127-1131 (enforcement) + run.rs:29 (SDK).
  **Esperado vs real: CONFERE.**

## FASE 2 — PURPOSE (provas comportamentais, executadas nesta sessão)

### 2.1 Gate 5.5 armado — `python3 scripts/prova_code_mode_ceg.py` → **35/35** (×2, segunda pós-fix)

Contra o binário deployado (`touring 30.4.16`, build 2026-08-27T16:59Z):
[1] gate discrimina benigno/perigoso · [2] contenção FS real · [3] transporte sem
quoting · [4] 71 hooks alcançáveis · [4b] allowlist imposta pelo DAEMON ·
[5] piso --brief · [6] journal start/settle · [7] predicado de rajada ·
[8] T3-B enterrado. (O script cresceu de 29→35 asserções durante o dia; o texto
do commit reflete a contagem anterior.)

### 2.2 Rajada de inspeção (S3) — hook real dirigido por payload JSON

| # | payload | veredito |
|---|---|---|
| 1 | `grep -rn "fn scan_class_of" …` (isolado) | **PASSA** |
| 2 | `grep …` (2ª em <300s) | **DENY** + os 2 comandos verbatim fundidos em 1 `touring run` |
| 3 | `ls …` isolado | **PASSA** |
| 4 | `cat Cargo.toml` isolado | **PASSA** |
| 5 | `cat …` (2ª) | **DENY** + programa fundido |

A **rota do deny executa** (exit 0, hits reais — incluindo `sandbox_may_call` em
daemon.rs:1127-1131, a imposição server-side vista em produção). Contadores Δ:
`g1_inspect_burst_denied_count` 1→4 · `code_mode_runs_count` 26→28 ·
fatigue global **4 denied / 0 bypassed (ratio 0.0)**.

### 2.3 G9 — `sed -i` em código → **DENY** com 3 rotas derivadas (Edit real / mesmo comando no sandbox / `ast grep --rewrite`); `sed -n` isolado **PASSA**.

### 2.4 G10 (exec-burst) — após 2 falhas do MEU instrumento

- `python3 -c` ×10: nunca nega — **`-c` é excluído por construção** (inline já é
  code mode; cli_suggester.rs:2697). Instrumento errado, não gate morto.
- Comando byte-idêntico ×3: **G6** (retry cego) cobre primeiro.
- 11 execuções `python3 <arquivo>` únicas: **DENY** com o programa R9 carregando
  a rajada verbatim (a janela de 600s agrega por executor+cwd, não por session_id).
- **Findings desta prova: F-1** (heredoc `python3 -` contava como rajada — irmão
  do `-c`, agora excluído) — ver FASE 5.

### 2.5 S1 TRANSPORT — handshake MCP real (JSON-RPC `initialize`+`tools/list`)

| cenário | tools |
|---|---|
| escopo `code` declarado (touring.toml deste workspace) | **3** (fachada: ctx_execute, memory_recall, search) |
| `TOURING_MCP_ALL_TOOLS=1` | **163** |
| `TOURING_MCP_CODE_MODE=0` | **23** (curadas) |

### 2.6 CEG kernel ao vivo (binário deployado)

`curl https://example.com` → **DENY DURO** (`X6 denied the network capability 'curl'`) ·
`touch ~/.ssh/x` → **BLOQUEADO** (Landlock) · escrita no workspace → funciona ·
`echo+ls` → exit 0, **zero advisory** (fricção zero no caso comum).

### 2.7 Guards Python — `pytest test_code_mode_sdk_section.py test_s3_burst_distribution.py test_n5_injection_kpi.py` → **35/35**

### 2.8 Mineradores de KPI (smoke contra transcripts reais)

- `n5_injection_kpi.py`: eixo rota-code **57,6%** global; série diária → **90,1% hoje**;
  pós-deny: 71,2% seguem a rota ensinada, 1 re-emissão, 3 bypass. exit 0.
- `s3_burst_distribution.py`: 119 transcripts; classes negadas = **79,3%** do volume em
  rajadas ≥2 — a calibração 300s/2ª se mantém com o corpus crescido. exit 0.

## FASE 3 — DEBT (scan_debt.py nos arquivos do universo)

**Zero dívida real.** 3 `todo_markers` são falsos positivos de scanner (prosa PT /
exemplo em comentário). Um deles introduzido pelo commit ("TODO caminho" em doc
comment) — reescrito nesta auditoria (D1). Scripts do universo: nenhum marker.

## FASE 4 — HARMONY

- **Órfãos dos módulos novos: REFUTADOS** (wiring stale; grep prova consumers reais
  em dispatch/server/cli — todos wired). Dois menores reais, corrigidos: H5, H6.
- **Ciclos**: 15 no workspace, **nenhum atribuível ao commit** (aresta
  kpi→cli_suggester do mega-ciclo é pré-existente — verificado via `git diff`).
- **50-dim P0 (36 checks)**: 34 PASS + 1 WARN (F2.4 keyword 'token' — FP, a const
  `GATE_BYPASS_TOKEN` não carrega segredo) + **1 FAIL (F2.1) → corrigido e
  re-medido 1.0 Pass** (F-2).
- **Quality floor**: `code_mode.rs` **0.9627 Diamond** · `orchestrate_allowlist.rs`
  **0.9608 Diamond** — ambos acima do piso Gold (0.80).

## FASE 5 — FIX & POTENTIALIZE (tudo executado, tudo com prova)

| # | finding | fix | prova |
|---|---|---|---|
| **F-1** (P2) | `exec_class_of` excluía `-c` mas não `python3 -` (heredoc) — o irmão inline; contaminou o ledger G10 e o remédio R9 | `matches!(rest.get(1), Some(&"-c") \| Some(&"-"))` + 2 asserts | `cargo test -p touring-cli` 458/458 (os asserts falham sem o fix) |
| **F-2** (P1) | Gate F2.1 (lexical) prendeu `cli_suggester.rs` em **deadlock**: o literal do R9 (`subprocess…shell=True` — dado entregue ao sandbox, não execução) dava 0.07 e negava QUALQUER manutenção no arquivo, inclusive o fix do próprio padrão | `concat!` parte o token + comentário (precedente: `IMPURE_CONSTRUCTS`); escrita pela rota B sancionada do G9 (sandbox, substituição exata + re-leitura) | `touring-quality check --gate F2.1` **0.07 → 1.0 Pass** |
| **H5** (P2) | Parser duplicado: 2 sítios do suggester parseavam `native\|code\|both` à mão; `CodeModePresentation::parse` (a fonte única criada hoje) órfão | Os 2 sítios chamam `parse()` — bônus: env com valor entre aspas agora resolve (trim+unquote) | suites 3.306 verdes |
| **H6** (P3) | `SANDBOX_ORIGIN_MARKER` prometia fonte única; o SDK Python cunha o literal `:code:` | Guard de paridade cross-linguagem: `sdk_marker_matches_the_daemon_const` (run.rs) | teste verde na suite server |
| **D1** (P3) | "TODO caminho" em doc comment poluía scans | reescrito | scan_debt limpo |
| **Doc drift** | comentário prometia cache OnceLock de runtimes inexistente ("future CLI") | comentário agora diz a verdade + o porquê (daemon longevo × PATH mutável; medir antes de cachear) | — |

### Quick-wins do sandbox (o pedido "enriquecer ao máximo") — 5 implementados e provados AO VIVO no binário novo

| QW | o que é | prova executada |
|---|---|---|
| **QW-1** | `TMPDIR=/tmp` declarado no filho (era wiped; /tmp já é write root) | `TMPDIR=/tmp` + `mktemp -d` OK |
| **QW-2** | `--max-stdout-bytes/--max-stderr-bytes` por chamada (antes só env global) | 10 KB stdout inline íntegros com a flag |
| **QW-3** | `--compute-ms` por chamada + teto wall-clock 120s→**600s** (rustc pesado estrangulava; hot loop segue contido por RLIMIT_CPU=30s + busy budget) | `--compute-ms 90000 --timeout-ms 180000` executam |
| **QW-4** | `--input <file>` → stdin do programa (não existia canal) | `LEU: dados-via-flag-input` |
| **QW-5** | Landlock `Scope::Signal\|AbstractUnixSocket` **ligado** (kernel 7.1 ≥ 6.12): sinais p/ processos fora do domínio (mesmo UID — ex.: o daemon) agora são **confinados pelo kernel** (antes: só DAC) | `kill -0 <pid do daemon>` → **EPERM**; `kill -0 $!` (intra) → OK |

Mais 3 testes novos no motor (`tunables_stdin_bytes_reach_the_program`,
`child_sees_tmpdir_pointing_to_tmp`, `tunables_max_stdout_bytes_overrides_the_inline_cap`).

## FASE 6 — E2E PROOF

- `cargo test -p touring-server -p touring-ceg -p touring-cli -p touring-foundation`:
  **3.306 passed / 0 failed** (exit 0).
- `cargo test -p touring-hooks --test ceg_e2e --test cli_suggester_e2e`: **138/138**.
- clippy `-D warnings` nos crates tocados: **limpo** (único warning: future-incompat
  da dependência externa `proc-macro-error2`).
- Prova comportamental (binário deployado, inalterado pelos fixes): **35/35 ×2**.

## §8 SANDBOX-ENRICHMENT (subagent sandbox-research — tudo medido, nada lido)

### Alegação "sandbox muito limitado": **PARTIAL**

**REFUTADA na metade estrutural (medido ao vivo)**: PATH completo do host (14
ferramentas probadas); 6 linguagens funcionais; FS `/` legível + escrita no
projeto e /tmp; SDK de orquestração com **71 hooks read-only funcionando de
dentro** (`doctor` + `index_find` executados in-sandbox); spill 1 MB + tee +
journal + harvest; dual budget wall/CPU medido no `/proc` do filho; contenção de
kernel **verdadeira** (Landlock EACCES fora do projeto; TCP 443 EACCES no kernel).

**CONFIRMADA na metade operacional (medido)**: 5 das 11 linguagens anunciadas
morrem `spawn ENOENT` nesta máquina (go/php/elixir/r/ts — e `deno` existe em
/usr/bin mas não era candidato); rede deny-all sem nenhuma via de grant
cirúrgico (nem `TOURING_TRUSTED_OK` abre); caps inline 8KB/4KB sem flag CLI
(→ QW-2); sem canal stdin (→ QW-4); TMPDIR wiped (→ QW-1); `--orchestrate`
Python-only; **memória ilimitada** (2 GB alocados OK — gap de CONTENÇÃO);
**UDP aberto** (datagrama p/ 8.8.8.8:53 SENT — Landlock não modela UDP);
sinais mesmo-UID não confinados (→ QW-5, fechado nesta auditoria).

Ou seja: o sandbox não é pobre de comandos nem de recursos — era pobre de
**aberturas calibradas**. Os quick-wins fecham 5 delas; as demais viram roadmap.

### DeepSeek harness (clone local `~/references/code-mode-2026-08-23/deepseek-harness`)

| | deles | nosso |
|---|---|---|
| contenção | **nenhuma** (bash-equivalent declarado; "contenção ≠ fronteira de segurança") | Landlock FS + TCP deny-all + IPC scope — kernel, medido |
| memória | heap cap por run (resourceLimits) | ilimitada (gap — roadmap) |
| env | `{}` vazio | passa credenciais por design (opt-out `TOURING_SANDBOX_NO_CREDENTIALS=1`) |
| linguagens | TS + Python | 11 anunciadas, 6 vivas (+deno candidato — roadmap) |
| sub-calls | paralelas bounded (10) | sequenciais (roadmap) |
| persistência | nenhuma | spill/tee/journal/harvest |

### Context7 (landlock-rs 96.25, Deno, E2B)

- **landlock-rs**: `NetPort` por porta é o padrão de grant cirúrgico — nosso builder
  JÁ aceita as listas (`enforce_linux.rs:357-417`), o call site passa vazio.
  `Scope` em ABI V6 (nosso kernel 7.1) — ligado em QW-5. BestEffort p/ produção.
- **Deno**: deny-by-default total + **overlays de deny** (`--allow-read=/etc
  --deny-read=/etc/hosts`) — o padrão "concede amplo, esculpe exceções" que o
  Landlock allow-only não expressa (exceção = enumerar roots do sistema).
- **E2B**: a riqueza interna vem da isolação ser uma VM inteira; a lição
  transferível sem VM: FS efêmero + toolchain pré-instalado + rede allowlist —
  a contenção é do invólucro, não da pobreza interna.

### Roadmap (NÃO implementado hoje — decisões de postura para Gabriel)

| gap | esforço | risco/nota |
|---|---|---|
| **UDP egress aberto** (medido: 8.8.8.8:53 sai) | M | contenção: seccomp `socket(AF_INET,SOCK_DGRAM)` no pre_exec, ou documentar residual |
| **Memória ilimitada** (2 GB alloc OK) | M | contenção: cgroup memory.max/pids.max (probe existe, nunca wired) ou RLIMIT_AS generoso |
| Rede por porta (`--allow-net-port 443`) | M | Landlock não filtra por host — 443 fala com qualquer host; precisa camada userspace p/ paridade com Deno |
| deno como runtime TS/JS (medido presente) | S→M | args dependem do runtime resolvido (`eval`/`--ext=ts`), o modelo atual é por linguagem — refactor runtime-aware |
| read-narrowing com exceções estilo Deno | M | read root `/` deixa credenciais em disco legíveis (residual documentado); exige medição de compat |
| `--orchestrate` multi-linguagem (Node SDK) | M | enforcement já é server-side — o cliente é só conveniência |
| sub-calls paralelas no orchestrate (pool 10) | M | daemon já é concorrente |
| venv gerenciado read-only (pandas/pydantic/httpx) | S | sem rede, pip é impossível; read root `/` já leria |
| scratch dir estável por sessão (`TOURING_SCRATCH_DIR`) | S | dentro do perímetro já concedido |
| streaming de saída | L | ambos dsh/touring só devolvem no final |

## FASE 7 — este relatório + juiz

`loop_converged.py --task task_1787873547512808623`: judge_intact ✅ ·
dag_done ✅ (5/5) · quality_gold ✅ (**Platinum 0.918**) · no_p0_fail ✅ ·
measured_whole_scope ✅ · cargo_green ✅ · cross_audit ➖ (ausente, skip
documentado) · **orphans_base ❌ — decomposto nesta auditoria; NÃO é drift de
código**:

Medição (snapshot `wiring orphans -j --full` pós-rebuild × baseline do bundle):
- 5.487 atuais em escopo × 2.357 baseline → 3.171 "NEW" brutos.
- **2.262 dos 3.171 são puro prefixo `./`** — o detector passou a emitir
  `./crates/...` e o baseline foi gravado sem o prefixo; o join por string da
  cláusula é sensível a ele (a família já vista em 23/08: "symbol_name SCIP
  nunca casava no join"). Normalizando, o delta cai para **797**.
- Dos 797, a amostra é dominada por padrões que o detector não resolve por
  construção: exports WASM `extern "C"` de `crates/inferlets/` (o consumer é o
  host WASM, nunca Rust) e métodos de nome ultra-comum (`new`, `label`,
  `default`, `from_env`, `config`, `Result`) cujo join por nome curto não
  atribui consumer.
- **39 dos 797 tocam arquivos do commit+fixes** — e a amostra deles é a mesma
  classe estrutural (`gate_metrics.rs::record_*` pré-existentes, `::new`,
  `::label`, consumer intra-módulo). **Nenhum símbolo novo desta auditoria é
  órfão** (RunTunables, write_stdin, sdk_marker_matches_the_daemon_const —
  todos com consumers).

**Não corrigi o juiz de propósito** (a memória "juiz gravável pelo julgado" é
o anti-padrão do nó 114 da Darwin Gödel Machine): a normalização do `./` na
cláusula e o tratamento de exports WASM no detector são **propostas para
Gabriel**, não edições minhas durante o meu próprio julgamento.

### Lições institucionais desta auditoria

1. **O instrumento falhou 3× antes do sistema**: sed de placeholder (quoting),
   `python3 -c` como proxy de rajada (excluído por construção), comando
   byte-idêntico (G6 cobre antes do G10). Cada uma produziria um falso finding
   sem a disciplina "prova o instrumento primeiro".
2. **Um gate P0 lexical pode prender o arquivo que emite o remédio** — deadlock
   de manutenção (F-2). O precedente `concat!` (IMPURE_CONSTRUCTS) é a saída
   legítima quando o padrão é dado, não código.
3. **A alegação de memória ("sandbox limitado") sobreviveu mal à medição** — e
   mesmo assim rendeu 5 quick-wins + roadmap. Medir > lembrar.
