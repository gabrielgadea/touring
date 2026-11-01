---
type: Review
title: Revisão adversarial C2 (context_budget.rs) e D-5 (daemon_main.rs)
scope: crates/touring-cli/src/cli/context_budget.rs, crates/touring-hooks/src/daemon_main.rs
date: 2026-10-08
mode: somente-leitura (sem git, sem cargo build/test, sem update-touring, sem pkill)
---

# Revisão adversarial — C2 e D-5 (diff da touring-f9, sem commit)

Toda linha de evidência abaixo é saída de comando executado nesta revisão. Onde não
verifiquei, digo por quê.

---

## Respostas explícitas

### (a) Existem DOIS acumuladores, `seen` global e `seen` próprio da sessão?

**SIM.** `context_budget.rs:571-609`, função `record_injection`:

- `seen` (sessão): `:589` `let count = seen.entry(hash).or_insert(0);` → `:591`
  `repeat_here = *count > 1` → `:592-597` soma em `agg.duplicate_bytes/blocks`. Este mapa
  é **reiniciado por sessão**.
- `project_seen` (projeto): `:575` é `Option<&mut BTreeMap<..>>`; `:598-600` retorna cedo
  quando é `None`; `:601-608` só conta quando `*pc > 1 && !repeat_here`, indo para
  `cross_session_duplicate_*`.

Comportamento de uma linha vista na sessão X e de novo na Y: conta **1 vez no projeto**
e **1 vez em cada sessão**, e a sessão **não** a marca como duplicata própria. A prova é
o teste em `:1508`, que exige `row["duplicate_injection_ratio"] == 0.0` para a sessão que
só viu o bloco em outra sessão. O agregador do projeto recolhe as duas unidades em
`absorb_session` (`:150-159`), onde `:156` faz
`self.duplicate_bytes += s.duplicate_bytes + s.cross_session_duplicate_bytes`.

**Não verifiquei por execução** (o peer proibiu `cargo test`, e a Kimi está compilando o
mesmo workspace). A resposta vem da leitura do código e é corroborada por dois testes que
exercitam exatamente essa propriedade.

### (b) `aggregate_lines` virou wrapper: a saída antiga continua idêntica? Quem quebra?

**A saída antiga continua idêntica — e há três razões independentes, todas verificadas.**

1. `aggregate_lines` (`:561-567`) chama `aggregate_session_lines(..., None)`, e o `None`
   faz `record_injection` retornar em `:598-600` **antes** de tocar qualquer campo novo.
   Quem chamava `aggregate_lines` vê os mesmos números.
2. **O teto de transcripts é o mesmo**: `MAX_TRANSCRIPTS = 3` (`:40`) é aplicado em
   `:909` (`.take(MAX_TRANSCRIPTS)`), ou seja, o agregado do projeto caminha os mesmos
   até 3 arquivos que a versão anterior. A entrada não mudou, então o número não muda.
3. **Não há consumidor de schema estrito.** Único consumidor do JSON:
   `crates/touring-cli/src/cli/kpi.rs:238` faz
   `out["context_budget"] = super::context_budget::context_budget(...)` — embute o objeto
   inteiro, sem deserialização estrita. `deny_unknown_fields` existe no workspace em um
   lugar só (`crates/touring-generator/src/plan/schema.rs:21`), em outro subsistema.

**Os campos novos** (`:929-932`): `unit` = `"project"`, `cross_session_duplicate_bytes`,
`cross_session_duplicate_blocks`, e `by_session` (array de `session_summary`, cada linha
com `unit` = `"session"`, `:940-948`).

**Não quebra consumidor — mas QUEBRA uma verdade.** Ver P1 abaixo.

### (c) `the_project_number_through_sessions_is_the_single_seen_scan` prova a propriedade? Falharia com um `seen` só?

**SIM, e é um discriminador de verdade.** `context_budget.rs:1481-1500`.

O teste monta `old` com **um** `seen` compartilhado por duas chamadas de
`aggregate_lines` (`:1487-1489`), monta `project` pelo caminho novo, e exige que
zerando os campos cross-session os dois agregados sejam iguais (`:1493-1499`).

Se os dois `seen` fossem **um só**, o mesmo hash seria contado duas vezes no mesmo mapa:
na segunda injeção `repeat_here` viraria `true` (`:591`), e a condição
`*pc > 1 && !repeat_here` (`:603`) seria **falsa** — o campo cross-session ficaria `0`.
A asserção `:1492` (`assert_eq!(project.cross_session_duplicate_blocks, 1)`) falharia.
A mutação está declarada na própria docstring do teste (`:1483-1484`).

Ressalva honesta (P2): o teste **não** distingue `absorb_session` somando
`duplicate + cross_session` de outra combinação que dê o mesmo total. Ele prova a
igualdade do número publicado, não a forma da soma.

### (d) RISCO PRINCIPAL — algum lançador legítimo passa argumento? (P0 se sim)

**NÃO. Não há P0.** Verifiquei toda a superfície de lançamento:

| lançador | onde | argumento? | evidência |
|---|---|---|---|
| systemd-run (scope) | `crates/touring-foundation/src/daemon_spawn.rs:166-181` | **nenhum** | `daemon_bin.as_os_str().to_owned()` é o **último** elemento do argv (`:179`), depois do `--` (`:178`). Os flags antes dele (`--user --scope --quiet --collect --expand-environment=no --description=`) são **do systemd-run**, não do daemon. |
| spawn direto (fallback) | `daemon_spawn.rs:207-213` | **nenhum** | `Command::new(daemon_bin)` sem nenhum `.arg()`. |
| autostart pelo hook | `crates/touring-hooks/src/main.rs:1047-1072` | **nenhum** | O closure `configure` só põe `env()`, `stdin/stdout/stderr` e `current_dir`. Nenhum `.arg()`. |
| `daemon-ctl` | `crates/touring-server/src/cli/daemon_ctl.rs:748-766` | **nenhum** | Resolve o binário (`:764`) e delega ao mesmo `spawn_daemon_detached_within`. |
| shim `~/.claude/hooks/touring-hook` | shim:43,69,82,102 | **n/a** | `exec "$project_bin" "$@"` — exec do **touring-hook**, não do daemon. O shim nunca lança o daemon. |
| `scripts/debug-touring-daemon.sh` | linha 124 | **n/a** | `exec bs "${mode_flags[@]}" -p "$target_pid"` — executa o BugStalker, anexando a um daemon já rodando. As flags (`--tui --oracle --dap --pid`) são do script de debug. |
| unidades systemd | — | **nenhuma** | Nenhum `.service` referencia `touring-daemon` com argumento. |
| docs | 4 ocorrências | **falso positivo** | `client/skills/Touring/references/touring-rebuild-rule.md:22,29,37` e `crates/touring-hooks/TACO_ANALYSIS.md:80` — são prosa/tabela markdown (`(~60 MB)`, `→ release/touring-daemon`), não execuções. |

Corrobora que o caminho legado não conta: `daemon_ctl.rs:751-752` diz que
`touring-hook --start-daemon` está **deprecado por S-9** — o `--start-daemon` pertence
ao `touring-hook` (`main.rs:44-46`), que roda o daemon **in-process**, não por `exec`.
São binários diferentes e `early_exit` só existe no `touring-daemon`.

**Risco residual verificado e descartado**: algum código usa `touring-daemon --version`
como sonda de vivacidade? `grep -rn --include=*.rs --include=*.py --include=*.sh -E
"daemon.*--version|--version.*daemon"` em `crates/ scripts/ client/` → **0 resultados**.
Sem isso, não haveria o falso "daemon está de pé" que a mudança poderia introduzir.

### (e) Alguma coisa acontece ANTES do `early_exit`?

**NÃO.** `crates/touring-hooks/src/daemon_main.rs:39-48`: `early_exit` é avaliado na
linha 41 e o `std::process::exit(code)` na 47. Tudo que tem efeito está **depois**:

- `:58` `panic_log::install_hook()`
- `:64` `set_process_name("touring-daemon")`
- `:72` `token_meter::install()`
- `:75-78` `tracing_subscriber::...init()`

Não há criação de arquivo, socket nem leitura de config antes da linha 41. A docstring
de `:18-23` diz exatamente isso ("decided BEFORE any side effect"), e o código confirma.

---

## Achados

**P1 · `scripts/r3_a1_kpi.py:152,192-197` · o "porte fiel" do agregador ficou desatualizado e nenhum teste o denuncia.**
O script se declara "porte FIEL do agregador do KPI `context_budget`" (linhas 2-4) e
mantém a semântica de **um** `seen`: `:152 seen: dict[str, int] = OrderedDict()`, com
`:192-197` marcando `duplicate_bytes/blocks` no primeiro acerto de contagem maior que 1.
Não há `cross_session_duplicate_*`, `absorb_session`, `unit` nem `by_session` — a
semântica que o Rust acabou de abandonar. O teste de paridade
`scripts/test_r3_a1_kpi.py` exercita **só funções Python** (`kpi.injection_family`,
`kpi.count_propositions`, `kpi.hook_contexts`, `transcripts_do_kpi`, `is_exempt`,
`is_duplicate`) e **nunca invoca o binário Rust** — `grep -c "cargo\|subprocess\|Command"`
retorna 1, e é import ou comentário. Consequência: o CI fica **verde**, o deploy não
trava, e qualquer leitura feita pelo porte entrega a semântica antiga com aparência de
atual. É o modo de falha mais caro dos que a REGRA #21 nomeia: verde com número errado.
Evidência: `grep -n "duplicate|seen|absorb|cross_session|def |by_family" scripts/r3_a1_kpi.py`
→ linhas 152, 185, 192-197, 204-206; nenhuma menção a `cross_session`.
Remediação: atualizar o porte para os dois acumuladores e adicionar ao
`test_r3_a1_kpi.py` uma comparação real Rust contra Python sobre um fixture (o teste de
`daemon_main.rs:138-143` mostra que o padrão de testar a função pura é aceito no repo).

**P2 · `crates/touring-cli/src/cli/context_budget.rs:1493-1499` · o teste que valida a igualdade do número publicado não valida a forma da soma.**
`:1493-1498` zeram os campos cross-session **à mão** antes do `assert_eq!(project, old)`.
Isso prova que o total publicado não se move ao dividir em sessões — o que o nome do teste
promete — mas não distingue `absorb_session` somando `duplicate + cross_session` (`:156`)
de qualquer outra combinação que produza o mesmo total. Uma mutação que redistribua os
mesmos bytes entre `duplicate_*` e `cross_session_duplicate_*` de forma compensada
passaria. Não é defeito do código de produção, é limite do guarda.
Evidência: leitura de `:1481-1500`.

**P2 · `crates/touring-cli/src/cli/context_budget.rs:561` · `aggregate_lines` reteve o lifetime `<'a>` que o wrapper deixou de precisar.**
A assinatura ainda declara `<'a>` sobre `lines: impl Iterator<Item = &'a str>`, herdado da
versão anterior, quando a função tinha corpo próprio. Não há efeito funcional; fica como
fricção para quem ler. Sem consumidor quebrado — registrado por ser dívida de leitura, não
por risco.
Evidência: `:561-567` (wrapper de uma linha) contra `:618` (a função real, que usa `<'a`).

**NÃO é achado — registrado porque quase foi.**
`daemon_main.rs:140` usa `["--socket", "/tmp/x.sock"]` como exemplo de argumento
desconhecido, e `--socket` é precisamente o que um daemon per-project poderia querer.
Verifiquei: o daemon resolve o socket por `TOURING_DAEMON_SOCKET` no ambiente
(`main.rs:1055` `cmd.env("TOURING_DAEMON_SOCKET", &socket_path)`), nunca por argv, e
`daemon_ctl.rs:776` deriva a raiz do próprio socket. O exemplo do teste é representativo,
não um caminho de regressão.

---

## Limites desta revisão

- **Sem `cargo test`/`cargo build`** (instrução do peer; Kimi compilando o mesmo workspace).
  As respostas (a) e (c) vêm de leitura de código corroborada por testes existentes, **não**
  de execução. Marquei isso explicitamente onde acontece.
- **Sem `git`**: não comparei com o HEAD. Todas as afirmações "é novo" contra "já existia"
  foram evitadas — o relatório descreve o **estado atual** do código.
- **`scripts/test_human_turn_predicate_parity.py`** ancora em
  `NON_HUMAN_TURN_MARKERS`, que **sobrevive** em `context_budget.rs:361` — esse teste de
  paridade textual não quebra com este diff.

VEREDITO: APROVADO COM RESSALVAS