# TRANSFERÊNCIA VGP — RELATÓRIO COMPLETO da sessão analise-0e para a coordenadora touring-09

> Pedido do Gabriel via coordenadora analise-89 (09/10/2026, noite). Mesmo texto enviado por
> mensagem a touring-09. Regra da casa: todo veredito é exit code; toda fração carrega o
> denominador e o método.

## 1. Identidade e mandato

- **Sessão**: `analise-0e` (uuid `647e47b7-8a50-4883-8b7e-7aa67c212a06`), backend **k3[1m]**
  (sessão irmã Kimi K3 1M), cwd `~/projects/analise`, trabalho executado em `~/projects/touring`
  (branch `feat/2026-09-16-shell-quality-wiring-python`).
- **Tarefas recebidas** (todas da coordenadora **analise-89**, por SendMessage cross-session):
  1. **V0d** (09/10, madrugada): «consertar o CONSTRUTOR do corpus de replay até o juiz passar,
     SEM tocar o juiz» — proibido editar `juiz_v0d.py`/limiares, crates/, settings, commit/push,
     deploy, `rm -rf`. **Aceita por ela no mesmo dia.**
  2. **V1+V2+V5** (09/10): motor VGP camada 1 puro/daemonless que nega API Python inventada ANTES
     da edição; primeiro em SOMBRA nos hooks (deny só na V4); aceite = juiz v2 rc 0. Adendos em
     voo: juiz v2 final (sha `91d8a2a608c8bd05`, recall conta deny OU warn; falso deny ≤1% por
     balde; forma fora do recall; política de severidade **P4** final medida); contrato da sombra
     (adendo com campos exatos); negativos inline reatribuídos à analise-25; drift A1 no aceite
     (REGRA #21); cli_ast_detail (bug de cwd relativo + zero mudo); **bug do from-import sem
     alias** (achado dela, com aceite RED→GREEN por 4 formas + M-I matando o (a)).
  3. **Wiring da camada 2** (09/10, aceitei fazer eu mesma): `--camadas 1|1,2` na CLI + hooks via
     `pool::checar`. Executado depois que o ciclo de pacote foi medido e **ela** extraiu a crate
     `touring-pyright` (opção E, escolha dela). Regra do pool: desligado por padrão no processo;
     `habilitar_neste_processo()` no daemon e na CLI `--camadas 1,2`; hooks NÃO chamam.
  4. **R1/R2/R3** (09/10, análise de causa-raiz medida por ela nos 85 falsos): consertar por
     construto, um RED→GREEN + um mutante morto por causa.
  5. **Encerramento** (09/10, noite): entrega aceita; fechamento formal depende do **held-out
     disjunto** (decisão da touring-09); VGP passa a touring-09/35/4f; eu, liberada para o analise.

## 2. Feito, com evidência

### 2.1 Arquivos criados (?? no git)

| arquivo (absoluto) | o que é |
|---|---|
| `~/projects/touring/crates/touring-code/src/vgp/mod.rs` | API pública do motor: `Finding/CheckInput/CheckOutput{camada2}` + `check()` + `is_inline()` |
| `~/projects/touring/crates/touring-code/src/vgp/scope.rs` | árvore de escopos pyflakes-grade: classe invisível em método · comprehension isolada · global/nonlocal · forward-ref · aug-assign · **except/with `as_pattern_target`** · **chained `T=E=R=A=0`** · **walrus `named_expression`** · **match `case_clause` posicional + `walk_case_pattern`** · **`future_import_statement`** · **`walk_interpolacoes` (f-strings)** · **splats tipados por campo** · dunders de módulo |
| `~/projects/touring/crates/touring-code/src/vgp/modules.rs` | resolver projeto→irmão(/tmp)→venv, overlay primeiro, cache `RefCell` (path, sha256) · `ModuleInfo{imports: alias↦(módulo, original), plain_imports, …}` · `func_sig` com **splats tipados** (`**kw: Unpack[X]`) · `Tri` |
| `~/projects/touring/crates/touring-code/src/vgp/stdlib_model.rs` | builtins 3.12 + tabelas fechadas (unicodedata, math, tipos builtin, stat_result, Path) |
| `~/projects/touring/crates/touring-code/src/vgp/checks.rs` | as 5 classes + inferência (construtor/anotação/retorno/**re-export `resolve_funcao`**/**construtor pontuado `pathlib.Path`**/builtin provado/`Path / str`) + **guarda de ciclo `em_curso`** + `verdict_dos` + política **P4** por balde×alvo |
| `~/projects/touring/crates/touring-code/src/vgp/checks/tests.rs` | 67 testes (era 38 no V1 baseline) |
| `~/projects/touring/crates/touring-code/src/vgp/sombra.rs` | `Mode{Off,Sombra,Ativo}` · `vgp_mode()` (env>toml>default sombra) · `sombra_check()` com **`SombraExtra`** (ponto de extensão da camada 2, fail-open) · `fundir_findings` pub · linha com `camadas:[1,2]` e `camada2` |
| `~/projects/touring/crates/touring-server/src/cli/vgp.rs` | handler `touring vgp check` (daemonless): parse por `position("check")` (o dispatch passa o argv inteiro) · **`--camadas 1|1,2`** + `habilitar_neste_processo()` + `fundir_camada2` (merge por (nome,linha), mais forte vence, layer=2, elapsed somado, unknown→`camada2`) · 7 testes |
| `~/projects/touring/crates/touring-hook-handlers/src/hooks/vgp_l2.rs` | glue L2 dos hooks: `extra(&CheckInput)->Option<SombraExtra>` via `touring_pyright::pool::checar` (sem habilitar no processo efêmero — unknown com motivo, sem spawn) |
| `~/projects/touring/docs/plans/2026-10-08-vgp-motor-base/RELATORIO-V1V2V5-2026-10-09.md` | o relatório vivo da entrega (sha256 `3e477dfec596b7b2`…, §0 ENTREGUE) |

### 2.2 Arquivos alterados (M no git)

| arquivo | o que mudou |
|---|---|
| `crates/touring-cli/src/cli/handlers/index.rs` | `cli_ast_detail`: caminho relativo resolve contra `payload["cwd"]`; ilegível → erro explícito (era zero mudo). +2 testes |
| `crates/touring-hook-handlers/Cargo.toml` | +`touring-pyright` (path) |
| `crates/touring-hook-handlers/src/lib.rs` | declara `vgp_l2` (pre-hooks) |
| `crates/touring-hook-handlers/src/hooks/pre_edit_prevention.rs` | VGP sombra pós-replay do AppliedEdit: Edit **e MultiEdit** (dobra sequencial) · `sombra_check(…, crate::vgp_l2::extra(&vgp_in))` · etiquetas `[VGP · deny|aviso]` só em Ativo |
| `crates/touring-hook-handlers/src/hooks/pre_write.rs` | idem para Write (disk = arquivo atual, content = proposto) |
| `crates/touring-hook-handlers/src/hooks/pre_bash.rs` | V5 inline: `python3 -c` · heredoc · `touring run --code|--file` (raiz por `-projects-<nome>` → CLAUDE_PROJECT_DIR → hook root) |
| `crates/touring-hooks/Cargo.toml` | +`touring-pyright` (path) |
| `crates/touring-hooks/src/daemon_main.rs` | `touring_pyright::pool::habilitar_neste_processo()` após `install_hook()` (linha ~62) |
| `crates/touring-server/Cargo.toml` | +`touring-pyright` (path) — a aresta `touring-lsp` saiu |
| `crates/touring-server/src/cli/mod.rs` | `pub mod vgp;` |
| `crates/touring-server/src/cli/command_table.rs` | `CommandDescriptor{ name: "vgp", … ExitOnError }` em `standalone_commands()` (o 107º comando — drift C4) |
| `scripts/test_drift_coverage.py` | 7ª reconciliação: `EXPECTED_TOTAL 393→395`, C1 84→85, C4 106→107 + docstring |
| `docs/plans/2026-10-08-drift-reconciliacao/ledger.jsonl` | +2 entradas (lote `2026-10-09-vgp-motor`): C4 `vgp` (GT é o binário INSTALADO — destrava no deploy 30.4.96) · C1 `30.4.96` (release futura no RETOMAR-AQUI) |

### 2.3 Testes, juízes e gates rodados (comando → exit code → resultado com denominador)

| comando | rc | resultado |
|---|---|---|
| `python3 docs/plans/2026-10-08-vgp-motor-base/scripts/juiz_motor_vgp.py --touring /tmp/v0d/touring-congelado --json …/diagnostics/juiz_v2.json --v5-estrito` (cwd: analise) | **0** | **PRONTO** — 762 chamadas: arquivo 11/13 (0,846) · inline 20/27 (0,741) · falsos 0/400 + 0/300 · p95 108,1 ms · 0 falhas fechadas · 0 não classificados · 3 defeitos de corpus (excluídos pelo próprio juiz) |
| `python3 …/juiz_motor_vgp.py --touring /tmp/v0d/touring-congelado --arg-motor=--camadas --arg-motor=1,2 --p95-ms 1000 --base …/juiz_v2.json --json …/juiz_v3.json` | **0** | **PRONTO** — arquivo 12/13 · inline 22/27 · falsos 0 (L2 não introduziu nenhum) · p95 939,9 ms ≤ 1000 · L2 recuperou +3 sem perder nenhum |
| `python3 …/scripts/prova_vgp_motor.py --so 1 --touring /tmp/v0d/touring-congelado` (cwd: analise) | **0** | **8/8 TUDO AFIRMADO** (células P4: externo deny em inline+arquivo · import projeto deny · projeto warn · forma warn · dinamico warn · limpo allow · delta allow) |
| `cargo test -p touring-code -p touring-server -p touring-hook-handlers -p touring-hooks --lib` | 0 | 857+749+2+1652 = **3.260 testes, 0 falhas** (touring-code vgp: 67/67; cli::vgp: 7/7) |
| `cargo clippy -p touring-code -p touring-server -p touring-hook-handlers -p touring-hooks --tests` | 0 | 0 erros (`-D warnings`) |
| `cargo fmt --all -- --check` | 0 | 0 diffs |
| `python3 scripts/test_drift_coverage.py` | 0 | 8 testes ALL PASS (7ª reconciliação, 395 sinais) |
| `python3 docs/plans/2026-10-08-drift-reconciliacao/reconciliar.py verify` | 0 | VERIFY OK (561 reproduzido; cada sinal do delta com 1 disposição) |
| `cargo check -p touring-hook-handlers -p touring-hooks -p touring-server --tests` | 0 | sem ciclo de pacote (a extração `touring-pyright` resolveu) |
| replay do crash (`vgp check` no snap jev_client `923da8f9c557`) | 0 | JSON com verdict (era SIGABRT/stack overflow no baseline) |
| aceite pyright: `pgrep -fc pyright-langserver` antes/depois de `--camadas 1,2` na CLI | — | 3 → 3 (sem órfão novo; os 3-4 pré-existentes são de outras sessões) |
| selftest do juiz v2 (`--selftest`) | 0 | 9/9 (medido quando recebi o juiz; juiz intocado por mim) |

**Mutações mortas (23 no total da entrega)** — cada uma aplicada, suite rodada, morte confirmada,
árvore restaurada a verde: M-CLI · M-N · M-I · M-M · M-B · M-K · M-D · M-P4 · M-CICLO (mata por
SIGABRT) · M-R1 · M-F3 · M-ALIAS · M-FUNC · M-F1 · M-CHAIN · M-WALRUS · M-MATCH · M-FUTURE ·
M-FSTRING · M-SPLAT-TIPADO (func_sig) · M-SPLAT-TIPADO-SCOPE (bind_parameters) · M-PATHLIB ·
M-REEXPORT.

## 3. Estado por item

| item | estado | o que falta / próximo comando |
|---|---|---|
| V0d (corpus de replay) | **feito** (aceito por analise-89) | nada |
| V1 motor camada 1 | **feito** (juiz v2 rc 0) | held-out disjunto — **touring-09 decide** |
| V2 hooks sombra (Edit/MultiEdit/Write) | **feito** | validação em daemon vivo pós-deploy 30.4.96 (deploy é da coordenação) |
| V5 inline (pre-bash) | **feito** (recall inline 0,741 ≥ 0,70 com `--v5-estrito`) | idem |
| cli_ast_detail | **feito** | nada |
| Wiring camada 2 (CLI + hooks + daemon) | **feito** (V3 rc 0; sem ciclo) | re-medir V3 pós-deploy |
| Drift A1 | **feito** (rc 0, 7ª reconciliação) | próximo release reconcilia de novo (C1 por construção) |
| **Held-out disjunto** | **não iniciado (é da touring-09)** | os consertos foram medidos no mesmo corpus em que foram feitos — o fechamento formal é vosso |
| As 3 correções do RELATORIO (§8 da analise-89) | **não iniciado (registradas para a touring-35)** | ver §6 abaixo |
| Perdidos remanescentes (12) | **classificados com evidência** (não são pendentes meus) | irredutíveis por construção: `defaultdict` (nome ausente do programa) · `DriftDetector`/`simd` (extensão .so sem stub) · `last` (laço não executado — dependência de dado). Lacunas registradas: `parse` (inferência de tupla) · `slides_fisica_eleitoral` · `JaccardComputer` · `kwarg tipo` · `__version__` (ver correção 3 da analise-89: é defeito de corpus) |

## 4. Processos e builds

- **Rodando agora**: nada (todas as tasks background terminaram: builds, juízes, suites — verifiquei `pgrep -fa "juiz_motor_vgp|cargo build --release"` = vazio).
- **`/tmp/v0d/`** (meu scratch, reapaga a qualquer momento): `touring-congelado` (**o binário de
  aceite**, sha256 `e935aaf17d9a09d0cdb24010df500fb4e4dab20f4200fa30362258ef8731fd1d` — rebuildável
  com o comando da linha abaixo) · `touring-congelado.sha` · logs dos juízes (`juiz-v2-r2.log`
  final, `juiz-v3.log`) · `crash1.py` (snap do crash bisseccionado, 66 linhas) · backups das
  mutações (`checks.rs.bak*`/`scope.rs.bak*`/`modules.rs.bak*`) · `vereditos.sh` (a cadeia de
  vereditos) · `scan-full.json` (drift) · logs de build (`vgp-build*.log`).
- **`~/projects/touring/target/vgp-judge/`**: meu target de builds de juiz (release,
  `CARGO_TARGET_DIR=… cargo build --release -p touring-server --bin touring` — nunca em `target/`,
  que é o executável do daemon global). O binário atual contém TODO o fonte final (build `build6`,
  rc 0).
- **`~/projects/touring/target/vgp-test/`**: meu target de checks/clippy/testes (debug, quente).

## 5. Dados gerados

| dado | n | sha256 (curto) |
|---|---|---|
| `…/diagnostics/juiz_v2.json` (veredito final camada 1) | 762 chamadas | `d2a21c546799e007` |
| `…/diagnostics/juiz_v3.json` (comparativo camadas 1,2) | 762 chamadas | `7d882652d4b7e392` |
| `/tmp/v0d/touring-congelado` (binário de aceite) | 1 | `e935aaf17d9a09d0` |
| `RELATORIO-V1V2V5-2026-10-09.md` | — | `3e477dfec596b7b2` |
| corpus V0d (`~/.claude/touring/vgp/corpus_v0d/` — construído na V0d, **congelado pela coordenação**; não regenerei desde então) | 391 entradas (91 pos · 300 neg) + `entradas_inline_neg.jsonl` (analise-25) | — |
| `~/.claude/touring/vgp/sombra.jsonl` (contrato da sombra, só escreve em runtime de hook/daemon) | 0 linhas nesta máquina de dev (nunca rodei um hook real em daemon vivo) | — |

## 6. Decisões, premissas e contratos recebidos em voo; achados, lacunas e riscos

**Contratos/decisões (ordem cronológica)**: política de severidade **P4** (forma+dinamico → warn
sempre; externo → deny em todo balde; proprio/projeto → deny só em /tmp+inline) · juiz v2 sha
`91d8a2a608c8bd05` revogando `3952defd…` e `738b621c…` (recall conta deny OU warn; falso deny por
balde; forma fora do recall) · `__file__` definido em arquivo e AUSENTE em inline (regra
confirmada por ela) · contrato da sombra (`~/.claude/touring/vgp/sombra.jsonl`, campos exatos,
`TOURING_VGP_DISABLED=1`, modo default sombra, `[VGP · deny|aviso]` só em ativo) · camada 2 API
`pool::checar(...) -> L2Resultado` + pool process-gated (`habilitar_neste_processo()` no daemon e
na CLI; hooks NÃO chamam — standalone seguro por construção) · corpus V0d e `corpus_*.py`/
`shell_inline.py` exclusivos da coordenação · **proibições honradas**: sem update-touring/
propagate/deploy · sem git que não leitura · sem kill de processos touring · juiz/corpus/juízes
V0c/V0d intactos · `cargo build --release` nunca em `target/` (sempre em `target/vgp-judge`).

**Achados próprios que viraram conserto**: o handler recebe o argv INTEIRO (parse por
`position("check")`) · stack overflow por inferência sem visitados (`line = line.strip()` —
58 chamadas do baseline) · p95 532 ms = re-parse por nome + `Resolver::new` de cache zerado por
inferência · `case_pattern` é filho POSICIONAL do `case_clause` (sem field na gramática) ·
`from __future__ import` é kind próprio · interpolação de f-string é nó `interpolation` · splats
tipados embrulham em `typed_parameter` (mesma raiz em `func_sig` e `bind_parameters`) ·
`resolve_class_module` fazia lookup pelo alias, não pelo nome original.

**Lacunas/riscos abertos (para a touring-09/35)**: inferência de desempacotamento de tupla
(`parser, … = cc._motor()`) · extensões .so sem tabela de stub · a contagem «17 mutações» no
texto da §6 do RELATORIO contra as **23 listadas** (correção 1 da analise-89) · a §0 do
RELATORIO foi atualizada para ENTREGUE **depois** da leitura dela — a correção 2 («§0 descreve a
camada 2 como não ligada») refere-se à versão anterior; a §0 atual já descreve o wiring
(`camadas [1,2]` no daemon, `[1]`+motivo no standalone) — fica o registro para conferência ·
correção 3: `__version__` (e provavelmente `defaultdict`) são **defeito de corpus** (o snap vivo
guarda o `-c` errado; o corpus com evidência os resolve) — a minha §6 os classificava como
lacuna/irredutível; a classificação dela prevalece.

## 7. Git (`git -C ~/projects/touring status --short` só dos meus arquivos)

```
 M crates/touring-cli/src/cli/handlers/index.rs
 M crates/touring-hook-handlers/Cargo.toml
 M crates/touring-hook-handlers/src/hooks/pre_bash.rs
 M crates/touring-hook-handlers/src/hooks/pre_edit_prevention.rs
 M crates/touring-hook-handlers/src/hooks/pre_write.rs
 M crates/touring-hook-handlers/src/lib.rs
 M crates/touring-hooks/Cargo.toml
 M crates/touring-hooks/src/daemon_main.rs
 M crates/touring-server/Cargo.toml
 M crates/touring-server/src/cli/command_table.rs
 M crates/touring-server/src/cli/mod.rs
 M docs/plans/2026-10-08-drift-reconciliacao/ledger.jsonl
 M scripts/test_drift_coverage.py
?? crates/touring-code/src/vgp/
?? crates/touring-hook-handlers/src/hooks/vgp_l2.rs
?? crates/touring-server/src/cli/vgp.rs
?? docs/plans/2026-10-08-vgp-motor-base/RELATORIO-V1V2V5-2026-10-09.md
```

**Nada commitado em lugar nenhum** — commit só por ordem do Gabriel.

## 8. O que eu NÃO verifiquei e o que ficou na memória sem ir para disco

- **Held-out disjunto**: não verificado por desenho (é o vosso aceite; medir no mesmo corpus em
  que se consertou prova instrumentação, não generalização).
- **Hook em daemon vivo**: nunca reiniciei/deployei daemon (proibido) — o caminho
  `camadas [1,2]` nos hooks foi verificado por compilação/testes/contrato, NÃO por um hook real
  servido por daemon vivo. O modo Ativo (etiquetas `[VGP · deny|aviso]` no contexto) também só
  foi exercido em teste, nunca numa sessão real.
- **Aferição da touring-09**: `defaultdict`/`__version__` como defeito de corpus é a medição
  dela — não re-verifiquei os snaps.
- **Ficou na memória local (disco, mas fora do bundle)**: entrada da sessão em
  `~/.claude/projects/-home-gabrielgadea-projects-analise/memory/MEMORY.md` (ponto de retomada +
  proibições); os backups de mutação e `vereditos.sh` em `/tmp/v0d/` (reapagáveis).
- **Não verificado fora do escopo**: a prova com `--so 1` cobre as 8 células P4; cenários com
  overlays do manifesto em volume (os 182 módulos sobrepostos) foram exercidos PELO JUIZ nas 762
  chamadas, não por mim em sondas separadas uma a uma.
