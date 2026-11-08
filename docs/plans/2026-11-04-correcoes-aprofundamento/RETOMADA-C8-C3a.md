# RETOMADA — C8 (guarda `caminho`) + C3a (denominador do `code_mode_adherence`)

> Worktree: `~/projects/touring-wt-c8` · branch `feat/2026-11-04-c8-c3`
> base `4cc2d806` ·|Coordenadora: touring-02 · Aprovação do Gabriel: 04/11/2026 ("opção A, P0 a P3")
> Bundle do brief: `docs/plans/2026-11-04-correcoes-aprofundamento/BRIEF-touring-0c-minimax-C8-C3a.md`

## Onde o estado está (tudo verificado por execução, nesta sessão)

### C8 — leitura do `.touring/touring.toml` por `caminho`

| fato | evidência |
|---|---|
| `sombra.rs` lê o toml em 2 lugares com `std::fs::read_to_string` | `crates/touring-code/src/vgp/sombra.rs:69` e `:129` |
| `caminho::ler_contido(raizes, alvo)` exige alvo ABSOLUTO e dentro das raízes | `crates/touring-code/src/vgp/lang/caminho.rs:327` (→ `contido` em `:169`, recusa relativa em `:181-185`) |
| `sombra.rs` fica em `src/vgp/` (FORA de `src/vgp/lang/`) | `crates/touring-code/src/vgp/mod.rs`: `pub mod lang; pub mod sombra;` |
| ⇒ a guarda estrutural de hoje (`vgp_guarda_disco.rs`) varre SÓ `src/vgp/lang` | `vgp_guarda_disco.rs`: `raiz_de_lang()` = `CARGO_MANIFEST_DIR/src/vgp/lang` |
| ⇒ `sombra.rs` NÃO está na lista de exceções: está FORA do alcance da guarda | idem (`PERMITIDOS` só cita `cdn/cache.rs`) |
| o log de sombra é escrita em `~/.claude/touring/vgp/sombra.jsonl` (fora do projeto) | `sombra.rs:418-437` (`sombra_path()`) |
| `cdn/cache.rs` é o único arquivo com `std::fs` sob `lang/` | varredura de `src/vgp`: só `sombra.rs`, `lang/caminho.rs`, `lang/cdn/cache.rs` (o `read_dir` de `python/stdlib_model.rs:397` é LITERAL de string — a guarda cega literais) |

**Decisão de desenho (C8)**: a guarda passa a varrer `src/vgp` (raiz da guarda), não só `src/vgp/lang`.
Assim `sombra.rs` entra no alcance e a leitura do disco do projeto fica sob a MESMA regra.
`sombra.rs` ganha uma permissão EXPLÍCITA e nomeada para a ESCRITA do log (`std::fs` + `OpenOptions`),
com o motivo em uma frase; `read_to_string`/`fs::read`/`fs::metadata` continuam PROIBIDOS lá
(o token `read_to_string` continua sem permissão ⇒ reintroduzir a leitura reprova o teste).
`cdn/cache.rs` continua exceção, com o motivo reescrito em uma frase (rede, não disco de projeto).

### C3a — denominador do `code_mode_adherence`

| fato | evidência |
|---|---|
| o KPI lê `~/.claude/touring/run_journal.jsonl` e conta LINHA por linha | `crates/touring-cli/src/cli/kpi/code_mode.rs:272-280` (`code_mode_adherence`) + `:712` (`adherence_from_lines`) |
| `classify_line` (run_outcome) devolve `Record` para QUALQUER JSON válido — inclusive `123` e o evento de recusa | `crates/touring-foundation/src/run_outcome/reader.rs:43-59` |
| hoje o evento de recusa e a linha não-objeto entram como RUN FALHO | `code_mode.rs:723-757` (o `Line::Record` incrementa `total` sem olhar o conteúdo) |
| os predicados nomeados existem e são a fonte da verdade | `run_outcome/outcome.rs:209` (`is_success`), `:222` (`exited_zero`), `:231` (`has_failure`) |
| no journal real: 68 recusas + 3 não-objeto + 413 malformadas + 24 em branco | medição própria sobre 51.137 linhas (snapshot 04/11) |
| **exit≠0 SEM `failure_kind` = 0 linhas** ⇒ "descartar recusa+não-objeto" e "descartar também as falhas sem classe" DÃO O MESMO DENOMINADOR | medição própria: `ok_exit0` 46.993 / `ok_issuccess` 46.992 (difere 1 linha: exit 0 + `output-limit`) |
| razão: hoje 0,926892 → sem recusa/não-objeto 0,928183 (numerador 46.993) | idem (mesma raiz de medição) |

**Decisão de desenho (C3a)**:
1. `recusa` (objeto com `repeat_blocked`) e `linha não-objeto` (JSON que não é objeto) saem do
   DENOMINADOR e do NUMERADOR; os dois contadores ficam expostos no JSON (`refused_events`,
   `non_object_lines`) — nada some em silêncio.
2. Classificação: `classify_line` (run_outcome) continua sendo o dono do que é linha de run;
   as duas famílias novas são reconhecidas por um predicado NOVO no próprio `run_outcome`
   (a classificação da linha é do módulo da taxonomia, não do KPI).
3. Sucesso: o numerador segue `exited_zero` (process-level) — é o que o `run_outcome` documenta
   para ESTE leitor (`outcome.rs:213-220`) e o que o gate M1 (`count_runs`) mede; `is_success`
   fica disponível para o relatório, mas trocar o numerador agora MOVERIA o `success_rate` por
   1 linha (o caso exit 0 + `output-limit`) sem decisão do Gabriel.

## O que falta (ordem de execução)

1. C8: `sombra.rs` (2 leituras → `caminho::ler_contido`), `vgp_guarda_disco.rs` (raiz da guarda +
   permissões nomeadas), `lang/cdn.rs` (doc-comment).
2. C3a: `run_outcome` (predicados de linha) + `code_mode.rs` (régua) + `code_mode/adherence_tests.rs`
   (golden + teste novo do brief).
3. Verificar: `cargo test -p touring-code` (base 1708/0), `cargo test -p touring-cli`,
   `clippy --all-targets --all-features -D warnings` nos dois, replay do #8 (86 casos),
   `cargo run -p touring-cli -- kpi -j` (número antes/depois).
4. Commitar (2 commits, mensagem em inglês) + `RELATORIO-C8-C3a.md` + SendMessage para touring-02.

## Restrições que valem

- Todo cargo via `scripts/hw-slot --class rust -- cargo …` (rc 75 = sem vaga; a máquina está quente).
- AB34 até 08/11 00:10: sem hooks/settings vivos, sem daemon, sem `update-touring`, sem
  `cargo build --release` no `target/` principal. Nada disso foi tocado.
- Sem nome de usuário nem `/home/<user>` em arquivo versionado.
- O `touring run` recusa escrita de arquivo no sandbox (X6) — pararozen o journal, leio em memória.

## Relatórios/medidas já gravados

- `/tmp/c8_warm.log` — build de deps (OK).
- `/tmp/decide_c3a_v3.py` — a medição que decide C3a (executada, saída no transcript).
