# ESTADO S8 — dedupe do conselho dos hooks NA ORIGEM (repo touring)

> **MARCO 08/10 ~18:20 — RETOMAR AQUI.** Feito: (1) FASE 0 verde (`cargo check
> --workspace` exit 0 via hw-slot; os 2 testes da f9 verdes: 19/19 context_budget +
> 4/4 daemon); (2) P2-1/P2-2 da revisão adversarial corrigidos em
> `crates/touring-cli/src/cli/context_budget.rs` — teste `the_project_number_...`
> reescrito afirmando a FORMA da soma (prova por mutação real: dropar o cross-term
> em `absorb_session` L157 faz o teste falhar, revertido e verde 19/19); o lifetime
> `<'a>` de `aggregate_lines` NÃO pôde ser removido (E0658 medido: `impl Trait` não
> admite lifetime anônimo em assoc-type binding; ficou `<'a>` com allow pontual
> documentado); (3) módulo NOVO `crates/touring-hooks-shared/src/injection_dedup.rs`
> GREEN **10/10** (`cargo test -p touring-hooks-shared injection_dedup`) — época por
> sessão em arquivo `.claude/touring/dedup-{session}.txt`, `advance_epoch`,
> `dedup_sections` (bloco→referência curta; linha não-estrutural repetida na época→
> omitida; fences/tabelas/headings/<24chars nunca suprimidos; kill switch
> TOURING_DEDUP_DISABLED; fail-open), registrado no `lib.rs`.
>
> **DESIGN DECIDIDO** (recon completo): não existe ponto de fusão nosso — o CC funde
> os hooks. O daemon executa cada handler em `run_hook_command`
> (`crates/touring-dispatch/src/daemon.rs:513`) com `payload` (session_id) e
> `runtime.project_root` disponíveis → a dedupe entra AQUI sobre o output JSON
> (extrai `hookSpecificOutput.additionalContext`, dedupa, re-serializa; fail-open se
> não-JSON). O standalone (`main.rs:758` `forward_daemon_output`, fallback quando o
> daemon cai) fica SEM dedupe — decisão declarada. `repeat_reference` do
> cli_suggester (L6233, moka 300s) será APOSENTADO do fluxo (o central cobre, e por
> ÉPOCA não TTL); tem 0 testes diretos.
>
> **MEDIÇÃO (item 6)**: p50=16.072 B/turno · média=43.591 · p90=129.820 (163 turnos,
> 12 transcripts, `/tmp/s8_medir_turno2.py` — marcador = attachment hookName
> UserPromptSubmit; content dict medido via json.dumps). O teto atual (3000) cobre só
> 27% dos turnos — é ficção. Instrumento da 6b existe e mede por chamada
> (894 chars/call ≈ brief).
>
> **PRÓXIMO PASSO EXATO**: (a) escrever `dedup_hook_output` (pura, em
> injection_dedup.rs, testes `dedupe_json_*`); (b) wire em daemon.rs
> run_hook_command antes do `response.send` + post_compact_handler.rs
> `advance_epoch(runtime.project_root, payload["session_id"])`; (c) aposentar
> repeat_reference do cli_suggester; (d) fixes (a)(b)(c) com testes RED
> (env_prefix_/lessons_verb_/must_echo_); (e) TURN_BUDGET_DEFAULT novo justificado
> (p50/p90 acima — candidato ~10-12k, escrever o raciocínio); (f) corpus A/B (A2);
> (g) A1 completo + clippy; (h) relatório à 6b → ela libera o A3.

## ATUALIZAÇÕES DA 6b DURANTE A EXECUÇÃO (08/10)

## BRIEF VERBATIM DA 6b (08/10/2026)

MEDIDO (12 transcripts do analise, 8.441 chamadas): o conselho que os hooks injetam no
modelo (attachment hook_additional_context) dá 898 chars por chamada. Por hookName:
PreToolUse:Edit 324 por chamada (2.405 por bloco) · PreToolUse:Write 160 (1.664 por
bloco) · PreToolUse:Bash 110 (338) · PreToolUse:Read 93 (1.679) · PostToolBatch 55 ·
PostToolUse:Edit 52 · PostToolUse:Bash 43. Esse texto fica no contexto até a próxima
compactação e perde a utilidade quando a chamada termina. Instrumento pronto:
`python3 /tmp/claude-1000/-home-gabrielgadea-projects-analise/10fb71c6-ef49-4c56-b3c0-493d1f4b1837/scratchpad/medir_conselho.py 12 --no-bundle`
(⚠ caminho de scratchpad da sessão DELA — se não existir mais, reextrair de
`~/.claude/projects/-home-gabrielgadea-projects-analise/*.jsonl`).

O QUE JÁ EXISTE (leia antes de criar):
- `crates/touring-cli/src/cli_suggester.rs:106-164`, repeat_reference: dedupe por
  (sessão, hash do bloco INTEIRO), cache moka em memória, TTL SUGGESTION_TTL_SECS=300s,
  só no cli-suggest. A regra da linha 157 (a referência só substitui o bloco se for
  menor que ele) continua valendo.
- `crates/touring-hooks-shared/src/turn_budget.rs`: teto por turno,
  TURN_BUDGET_DEFAULT = touring_foundation::cila::INJECTION_CEIL_BYTES_PER_TURN, com
  TOURING_TURN_BUDGET e directives_only.
- Kill switch humano TOURING_DEDUP_DISABLED (o modelo nunca o liga).

DEFEITOS VISTOS AO VIVO (casos de teste):
(a) Bash "TOURING_CODE_MODE=native python3 ...": cli-suggest emitiu symbol-lookup
    tratando TOURING_CODE_MODE (atribuição de ambiente) como símbolo, com MUST
    "touring index find TOURING_CODE_MODE -j".
(b) Todo Bash "cd <dir> && ..." traz "⚠ lições de erros passados para esta ação":
    as mesmas 9 falhas genéricas chaveadas em `cd`, repetidas a cada chamada.
(c) code-mode-loop: o MUST ecoa o comando inteiro dentro de touring run --code '…'
    (~700 chars). O conselho custa tanto quanto o comando.
(d) Bloco idêntico volta inteiro depois de 300s, embora a primeira cópia continue
    no contexto até a próxima compactação.
(e) Linhas estáticas repetidas por chamada: "(cached 300s — set
    TOURING_SUGGESTER_DISABLED=1 to silence)", "Reason: …", e no pre-write
    "📚 related docs / ANN related / deep_module / pensieve".

ENTREGAR (política: emitir só quando algo muda):
1. Dedupe por SEÇÃO, não por bloco, em TODOS os emissores do touring-hook que injetam
   additionalContext: pre-edit, pre-edit-prevention, pre-write, pre-read, pre-bash,
   pre-grep, pre-glob, cli-suggest, ceg-observe, post-edit, post-write, post-read,
   post-bash, post-tool-rl, post-tool-batch. Chave: (session_id, época de compactação,
   hash da seção normalizada). Repetição exata sai omitida, ou vira referência curta
   quando menor que a seção. Seção nova ou alterada é emitida.
2. A validade é o CICLO de compactação, não 300s. A época avança no PostCompact
   (AJUSTE: não mais no SessionStart — ver acima). Estado em arquivo por sessão, no
   mesmo idioma do turn_budget.rs, NÃO em cache do daemon (o daemon reinicia).
3. Consertar (a): token seguido de `=` no início do comando, ou depois de `env`, é
   atribuição, não símbolo.
4. Consertar (b): as lições chaveiam no verbo real (o primeiro comando depois de
   "cd X &&" ou ";") e cada lição sai uma vez por ciclo.
5. Consertar (c): acima de ~160 chars, o MUST cita o começo do comando + "…". Valor
   real, nunca placeholder; o guard
   every_derivable_nudge_carries_real_value_not_placeholder deve seguir verde
   (ver ~/.claude/rules/touring-4-pillars.md §Injection-density invariant).
6. Baixar TURN_BUDGET_DEFAULT para número DECIDIDO PELA MEDIÇÃO (p50/p90 de bytes por
   turno nos transcripts), com o raciocínio no relatório.
7. Um teste de regressão por defeito, de (a) a (e), mais o teste de época: a seção
   repetida é suprimida antes do PostCompact e reemitida depois.

ACEITE (cada item por exit code):
A1 cargo test dos crates tocados verde; clippy sem warning novo.
A2 Antes→depois em corpus reproduzível: N≥200 payloads PreToolUse/PostToolUse reais de
   ~/.claude/projects/-home-gabrielgadea-projects-analise/*.jsonl, em sequência, pelo
   binário antigo e pelo novo com o MESMO session_id. Reporte chars/chamada por
   hookName. Meta: queda ≥ 40% no total, SEM perder nenhuma diretiva MUST na primeira
   emissão. Controle positivo: a primeira ocorrência de cada seção continua presente.
A3 Propagação: update-touring (NUNCA cargo build avulso; NUNCA pkill, REGRA #19),
   depois `touring toolchain install --from-source ~/projects/touring <versão> --force`,
   depois `touring update --project ~/projects/analise`. Prova por COMPORTAMENTO: o
   payload do defeito (a), passado ao hook do analise, não traz mais symbol-lookup.
A4 Comentários de turn_budget.rs e cli_suggester.rs e
   ~/projects/touring/docs/economia-de-creditos-e-contexto.md ganham um parágrafo
   datado com o antes→depois.

ARQUIVOS EXCLUSIVOS: crates/** do touring e docs/economia-de-creditos-e-contexto.md.
NÃO tocar: ~/.claude/settings.json, ~/.claude/skills/**, ~/.claude/rules/**,
client/hooks/{check_context.sh,auto_compact.sh,status_monitor.sh}, qualquer CLAUDE.md.
PROIBIDO: git commit/push · pkill/kill em touring · rm -rf dentro de script · imprimir
chave ou token · pedir a outra sessão o que foi negado a você.
ANTES do update-touring: avisar por SendMessage a analise-6b E analise-a9 (o daemon
reinicia).

RELATÓRIO FINAL (uma mensagem à 6b): tabela chars/chamada por hookName antes→depois;
o novo TURN_BUDGET_DEFAULT e o porquê; arquivos editados; saídas A1–A4 com exit code;
o que ficou de fora e por quê.

## PLANO DE ATAQUE (sessão que retomar)

0. FASE 0: `cargo check --workspace` + `touring doctor -j` no repo touring (saúde).
1. Ler `crates/touring-cli/src/cli_suggester.rs` (dedupe atual 106-164) e
   `crates/touring-hooks-shared/src/turn_budget.rs` inteiros; localizar os 14
   emissores (grep `additionalContext` em crates/touring-*/src).
2. Medir p50/p90 de bytes/turno (item 6) com o instrumento da 6b ou reextração.
3. Desenhar o estado de época por sessão (arquivo, formato do turn_budget.rs) +
   avanço no post-compact handler (existe em crates/touring-hooks ou hook-handlers).
4. Implementar dedupe por seção + fixes (a)-(e) com testes (item 7) PRIMEIRO (TDD).
5. Bateria A2 (corpus A/B) → A1 (cargo test/clippy) → relatório → aviso → A3 → A4.

## CONTRATO DE ENTREGA (acréscimo da 6b, 08/10 — parte do aceite)

Ao terminar a S8, gravar
`~/projects/touring/docs/plans/2026-10-08-compactacao-65/diagnostics/item1.json`:

```json
{"cargo_cmd": "<UM comando bash que roda SÓ os testes de regressão nomeados (a)–(e) + época — ex.: cargo test -p touring-cli -- dedupe_ epoch_ env_prefix_ lessons_verb_ must_echo_>",
 "deploy_utc": "<instante ISO UTC em que update-touring + touring update --project ~/projects/analise terminaram>"}
```

O juiz dela (`scripts/verificar_compactacao.py`, cláusula C1) vai:
1. rodar o `cargo_cmd` no repo touring exigindo exit 0;
2. medir nos transcripts do analise, a partir do `deploy_utc`, os caracteres de
   hook_additional_context por chamada de ferramenta — exige **≥100 chamadas** e
   **queda ≥40%** contra a linha de base de 898/chamada. Controle positivo dela já
   medido: hoje, ANTES do deploy, dá 810/chamada em 886 chamadas.

Por que testes Rust + medição ao vivo (racional dela): o cli-suggest é estatístico e
com estado (mesma sessão nova deu exec-gate-advisory uma vez e depois nada), e sem
CLAUDE_PROJECT_DIR o daemon responde com erro que um teste negativo lê como passe —
caixa-preta de fora do CC não é prova. **Testes Rust = prova de mecanismo; medição
ao vivo = prova de efeito.** Os meus nomes de teste devem ser estáveis e cobertos
pelo `cargo_cmd` que eu declarar — escrever o filtro do cargo test com cuidado
(prefixos cobrem todos os 6 grupos).

## ATUALIZAÇÕES DA 6b DURANTE A EXECUÇÃO (08/10)

**Dono dos crates sujos identificado (não refazer a busca)**: touring-f9
(transcript 96191e32, encerrada). `context_budget.rs` = C2 (absorb_session,
by_session, cross_session_duplicate_*, 3 testes) · `daemon_main.rs` = D-5
(early_exit, 4 testes). Intenção: `docs/plans/2026-10-06-retomada/RETOMAR-AQUI.md`
§"Editado e NÃO testado" (L17-18 e L40-46).

**Medido por mim na FASE 0 (08/10 ~17:50)**: os testes dela PASSAM —
`cargo test -p touring-cli --lib context_budget` → 19 passed/0 failed ·
`cargo test -p touring-hooks --bin touring-daemon` → 4 passed/0 failed. E o
`cargo check --workspace` passou com os dois diffs dentro.

**Decisão do Gabriel (via 6b)**: C2+D-5 EMBARCAM no mesmo deploy da S8, desde que
testes + revisão adversarial da MiniMax (analise-a9) aprovem. Os 2 testes dela viram
parte do meu A1; clippy sem warning novo cobre os 2 arquivos; autorizado a consertar
o mínimo sem mudar a intenção (não precisou — estavam verdes). **A3 (deploy) segue
BLOQUEADO até a 6b liberar** (após a9 aprovar + eu mostrar A1+A2 verdes). No A3,
antes de rodar: mandar a ela os comandos exatos + a versão, propondo UM caminho que
concilia o rito da f9 (RETOMAR-AQUI §"Ordem para terminar" passo 4:
`elite_aggregate --check` pré-voo → bump **30.4.93** → clippy `-D warnings` →
`propagate-release.sh`) com o A3 do brief (update-touring → toolchain install
--from-source --force → `touring update --project ~/projects/analise`).

## ESTADO DO GIT DO REPO TOURING AO FECHAR (08/10 ~17:30)

Nada commitado por mim. Modificados e meus: scripts/drift_semantic_scan.py,
scripts/test_drift_coverage.py, docs/plans/2026-10-08-drift-reconciliacao/** (novo),
CLAUDE.md (movimento do changelog — ordem do Gabriel), CHANGELOG.md (idem).
Sujos de OUTROS (não tocar sem coordenar): client/** (6b/MiniMax), crates/.claude/
touring/memory.db, crates/touring-cli/src/cli/context_budget.rs,
crates/touring-code/.claude/CLAUDE.md, crates/touring-hooks/src/daemon_main.rs,
docs/kpi/**, scripts/test_docs_no_phantom_crates.py, docs/economia-de-creditos-e-contexto.md
(untracked — a 6b a editou; na S8 é MINHA exclusiva, verificar drift entre as).

## MARCO 08/10 ~19:15 — itens 3-7 do brief CODADOS, testes de regressão verdes

**Item 7 (testes de regressão, um por defeito) — VERDE, RED→GREEN medido:**
- (a) `env_prefix_atribuicao_nao_e_simbolo` (cli_suggester_tests.rs): RED confirmado
  (exit 101), GREEN após `regex_find_non_assignment`. Reprodução real medida ao vivo:
  `TOURING_CODE_MODE=native grep -rn HookRuntime crates/` — o PascalCase pegava a
  VAR antes do símbolo. O caso python3-puro do relato original cai em
  exec-gate-advisory (legítimo, cita o comando inteiro) e fica como guarda.
- (b) `lessons_verb_real_depois_de_cd_e_assignments` (action_signature.rs): GREEN.
- (c) `must_echo_longo_e_citado_curto_e_verbatim` (cli_suggester_tests.rs): GREEN —
  ≤160 chars verbatim (ordem 25/08), >160 citado com "…" (ordem 08/10).
- (d)+(e) cobertos pelos 12 testes de `injection_dedup` (bloco repetido → referência,
  linha estática some na 2ª, época reemite) — GREEN desde antes.
- Órfão da troca removido (REGRA #0): `regex_first_match` (0 callers, dead code
  denunciado pelo rustc).

**Item 6 (TURN_BUDGET pela medição) — APLICADO:** `INJECTION_CEIL_BYTES_PER_TURN`
3000 → **2000** em `cila.rs` (fonte única; a régua `context_budget.rs:820` e o
executor `TURN_BUDGET_DEFAULT` leem dela — os 23 outros literais "3000" no
workspace são de outros orçamentos, conferidos um a um). Raciocínio: 163 turnos ×
12 transcripts (08/10) → p50 = 16 072 B/turno (média 43 591, p90 129 820); com a
dedupe o gasto legítimo são as seções NOVAS (~2 blocos × ~900 B ≈ 1 800); 2 000 =
12,4 % do p50 — a mesma política de halving de 04/09 (26,6 % → metade) sobre a
régua nova. Errar para baixo cai no fail-soft `directives_only` (MUSTs sobrevivem).
Parágrafo datado em cila.rs + doc comment e teste-pino em turn_budget.rs
(`assert_eq!(TURN_BUDGET_DEFAULT, 2000)`).

**Verificação desta sessão (hw-slot):** touring-hooks-shared **485/485** ·
touring-foundation 8/8 · touring-cli context_budget 19/19 · env_prefix 1/1 ·
must_echo 1/1 · lessons_verb 1/1.

**PRÓXIMO PASSO EXATO**: A2 — corpus A/B N≥200 payloads reais de
`~/.claude/projects/-home-gabrielgadea-projects-analise/*.jsonl` pelo binário antigo
e o novo (mesmo session_id), meta queda ≥40 % sem perder nenhuma diretiva MUST na
1ª emissão (controle positivo). Depois: A1 completo (4 crates + f9 + clippy), A4
(parágrafo datado em cli_suggester.rs — fixes a/c — e docs/economia-de-creditos-e-
contexto.md), relatório final + `diagnostics/item1.json`. A3 segue BLOQUEADO.

## MARCO 08/10 ~20:40 — A1 E A2 VERDES; falta só o relatório + item1.json (A3 bloqueado)

**A2 (corpus A/B) — VERDE: queda 42,2 % no modo COMBINADO** (o deploy real, o que o
juiz da 6b mede ao vivo). Corpus FIXO e reproduzível: 79 253 payloads reais, 12
maiores transcripts encerrados do projeto analise (manifesto em
`diagnostics/corpus_a2_manifest.json`; a 1ª extração por mtime-recente era janela
MÓVEL e não reproduzível — sessões vivas entravam; corrigido e registrado).
Decomposição: A-total (dedupe em tudo) 42,5 % · B-deploy-fiel (só o que o daemon
processa) 37,7 % · C-combinado (B + session-start:compact fora + budget 2000 no
cli-suggest) 42,2 %. Controle positivo dentro do teste (panic se falhar): nenhum
MUST perdido na 1ª emissão + 1º payload de cada sessão intacto.
Evidência: `diagnostics/a2_resultado.json` + `tests/s8_corpus_ab.rs` (teste de
integração permanente; sem `S8_CORPUS_JSONL` roda mini-corpus embutido no CI).

**Evolução da política de dedupe nesta sessão** (cada passo medido no corpus):
linha exata + referência curta 19,4 % → +molde por linha 23,0 % (bug achado: `/`
virava `P` mas o path ficava) → +molde token-based + camada 1 por molde 28,9 % →
+omissão total da repetição de bloco 35,4 % (a referência custava ~0,9 MB de
envelope — o rastro era mais caro que o conselho; o brief permite omitir) → corpus
fixo 42,5 % · combinado 42,2 %. MIN_LINE 24→12 medido: +0,04 p.p. — REVERTIDO
(proteção estrutural vale mais). Simulador Python descartado: divergiu 3,5× do
executor real (anti-padrão D8 — política se decide no executor, nunca na réplica).

**A1 — VERDE**: foundation 8/8 · hooks-shared 487/487 + integração 1/1 · cli
642/643 (2 ignored) — inclui env_prefix/must_echo e os 19 de context_budget (f9) ·
dispatch compila e testes ok · hook-handlers ok · daemon bin (f9 D-5) 4/4 ·
**clippy --all-targets nos 6 crates: EXIT 0, zero warnings** (3 achados meus
corrigidos no caminho: collapsible-if, 2 snake_case, `&runtime`→`runtime`).

**A4 — COMPLETO**: parágrafos datados em cila.rs, turn_budget.rs, cli_suggester.rs
(×3), post_compact_handler.rs (×2), daemon.rs, injection_dedup.rs (topo) e
docs/economia-de-creditos-e-contexto.md (§0, após o fechamento do orçamento 4).

**PRÓXIMO PASSO EXATO**: gravar `diagnostics/item1.json` (cargo_cmd pronto;
deploy_utc=null até o A3) → relatório final à 6b (UMA mensagem) com a tabela
antes→depois, o TURN_BUDGET novo e o porquê, arquivos editados, exit codes, e o
pedido de liberação do A3 com a proposta de comandos.

## FECHAMENTO DA MINHA PARTE (08/10 ~21:10) — S8 ENTREGUE, A3 AGUARDA A 6b

`diagnostics/item1.json` gravado (cargo_cmd pronto, deploy_utc=null) · relatório
único enviado à analise-6b com a tabela por hookName, a decisão do TURN_BUDGET
2000, os exit codes do A1, os parágrafos do A4, o que ficou de fora (standalone
sem dedupe) e a proposta de UM caminho para o deploy (elite_aggregate pré-voo →
clippy -D warnings → bump 30.4.93 → propagate-release.sh → toolchain install
--from-source --force → touring update --project ~/projects/analise), com duas
perguntas (a forma exata do bump; confirmação da ordem). Lição no grafo do
analise: `licao:dedupe-injecao-corpus-fixo-e-omissao:2026-10-08` (com link
relates-to à lição do ground-truth e leitura de volta OK) + reward 0,85.

**SESSÃO SEGUINTE, COMEÇA AQUI**: se a 6b liberou o A3, executar o caminho
proposto (acima) respondendo às 2 perguntas dela; avisar 6b E analise-a9 ANTES do
propagate-release (o daemon reinicia); preencher deploy_utc no item1.json no
instante do deploy. Nada commitado (só Gabriel ordena).

## MARCO 08/10 ~21:45 — RACE DE ENV NOS TESTES CORRIGIDA (juiz da 6b reprovou o cargo_cmd)

**A falha que a 6b mediu no juiz dela (rc 101)**: `dedupe_kill_switch_devolve_intacto`
usava `unsafe { set_var("TOURING_DEDUP_DISABLED","1") }` — o env é GLOBAL ao processo
e os 13 testes `dedupe_` rodam em threads paralelas do mesmo binário: 41/150 rodadas
falhavam (ela mediu; com `--skip dedupe_kill_switch` ou `--test-threads=1`: 0/150).
O Rust 2024 marcou `set_var` como unsafe exatamente por isso.

**O conserto (o padrão dela: decisão pura + interna com parâmetro, produção lê o env
uma vez)**:
- `injection_dedup.rs`: `disabled_from(Option<&str>)` pura + `dedup_sections_with`/
  `dedup_hook_output_with(…, disabled)`; o teste do kill switch exercita a interna
  com `disabled=true`, sem tocar o env (com bug meu achado pelo experimento: chamada
  disabled NÃO grava estado — a 1ª ligada registra, a 2ª omite; assert corrigido).
- `cli_suggester.rs`: `GateOverrides { g2_rewrite_disabled, portfolio_dir }` (objeto
  nomeado, diretriz A2) + `from_env()`; `code_mode_gates` → wrapper de
  `code_mode_gates_with(…, ovr)`; o if do G2 lê `ovr`; `portfolio_remedy_for_burst`
  → wrapper de `portfolio_remedy_for_burst_on(…, &index)`; o callsite do G10 usa
  `store::load_from(dir)` quando o override traz `portfolio_dir`.
- `cli_suggester_tests.rs`: os 6 pontos de env global (G2 ×2, PORTFOLIO_DIR ×4 —
  universo medido, não só os que falharam) viraram overrides; os `#[serial(t3_env)]`
  desses 3 testes saíram junto (não há mais env para serializar).

**Aceites dela, medidos aqui**: ① binário hooks-shared filtro `dedupe_`, **150
rodadas paralelas → 0 falhas** (o `grep -c FALHA` imprimiu 0; o exit 1 do pipe é o
grep com zero matches — leitura fiel, não falha das rodadas) · ② `cargo test -p
touring-cli --lib` 20 rodadas → **REPROVOU na 1ª medição (2/20)**: o teste
`s6_prior_art_instanciado…` panic em `save_to().unwrap()` — `renaming into
/tmp/s6-portfolio-<pid>/index.json: No such file or directory`. CAUSA-RAIZ (com o
output completo da rodada, não hipótese): os 2 testes s6 gravam o MESMO dir
`s6-portfolio-{pid}` (o pid é igual para todos os testes do processo) e o tmp do
`save_to` é `index.json.tmp{pid}` — um rename movia o tmp do outro. O
`#[serial(t3_env)]` que removi com o env mascarava ESTA race de arquivo, não só a
do env (lição: remover um guard sem mapear tudo o que ele cobre troca o defeito
de lugar — `licao:remover-guard-sem-mapear-o-que-ele-cobre:2026-10-08` no grafo).
FIX: dir único por chamada (`SEQ` atômico no `monta_portfolio_scratch`), SEM
reintroduzir o serial. Prova focada: 40/40 rodadas dos 4 testes `s6_`, 0 falhas.
**Re-medição completa: 20 rodadas do cli lib → RESUMO rodadas=20 falhas=0 ✓** ·
③ cargo_cmd do item1.json → **CARGO_CMD_RC=0** no código FINAL (re-rodado após o
fix do dir; 6/6 comandos) · ④ clippy --all-targets nos 6 crates → **EXIT 0** no
código final. **OS 4 ACEITES VERDES — relatório único enviado à 6b (~22:15).**

**SESSÃO SEGUINTE, COMEÇA AQUI (2)**: confirmar o resultado das 20 rodadas (②) →
relatório único à 6b com os 4 números → ela re-roda o juiz → A3 na ordem DELA:
0. elite_aggregate --check + clippy --workspace --all-targets -D warnings ·
1. bump à MÃO: `version = "30.4.93"` em [workspace.package] do Cargo.toml raiz
(hoje 30.4.92; o build regrava o Cargo.lock; o propagate-release confere o rótulo
na linha 249 e morre se o binário declarar outra) ·
2. `scripts/propagate-release.sh 30.4.93 --dry-run` → mandar a saída a ela ·
3. ao sinal dela: avisar 6b E analise-a9 → `propagate-release.sh 30.4.93
--stop-before-propagate` · 4. ela verifica o binário → só então
`--skip-gates --skip-build --skip-freeze` p/ propagar + deploy_utc no item1.json.
NUNCA `touring toolchain install`/`touring update` à mão (pula a checagem de
rótulo e os testes pós-build).

## MARCO 08/10 ~22:50 — H1 EM CURSO (hermeticidade de testes, pedido da 6b pós-juiz)

**O pedido (H1+H2 da 6b)**: 15 testes do touring-cli leem `TOURING_CODE_MODE` do
AMBIENTE do processo (quem roda `TOURING_CODE_MODE=native cargo test` — o que o
CLAUDE.md do analise ensina a prefixar — via 15 vermelhos falsos). A matriz dela
medida: `native` → 15 falhas · `code` → 8 · **`TOURING_CODE_GATES_DISABLED=1` →
29** (o kill switch humano vaza, o pior) · herméticas: GATE_OK,
SUGGESTER_DISABLED, TURN_BUDGET, DEDUP_DISABLED, PILLAR_INDUCTION_ARMED. Aceite:
0 falhas em 4 ambientes (limpo/native/code/GATES_DISABLED), testes de precedência
e do kill switch afirmando com valor INJETADO, sem perder cobertura.

**O conserto (o padrão GateOverrides estendido)**:
- `CodeModeEnv { mode, code_only_alias }` + `from_env()` + `resolve()` pura;
  `code_mode_presentation` → wrapper de `code_mode_presentation_with(root, cmd,
  &CodeModeEnv)`; o caller dentro do `code_mode_gates_with` lê `ovr.code_mode_env`.
- `GateOverrides` ganha `code_mode_env` E `code_gates_disabled` (12 pontos de
  leitura do kill switch: 10 dentro do `code_mode_gates_with` [já têm `ovr`],
  1 em `burst_gate` [:5004] → `burst_gate_with(…, ovr)`, 1 em
  `write_run_pair_gate` [:3185] → `_with(…, ovr)`).
- Testes: helpers `gates_limpo`/`apresentacao_limpa`/`apresentacao_com_env` no
  topo do cli_suggester_tests.rs; shim por ALIAS de import nos mods afetados
  (`use super::gates_limpo as code_mode_gates;` — corpos intactos) +
  `burst_gate_limpo` para actor_isolation_tests/s24_route_tests (chamam
  burst_gate direto); os 4 de precedência ganham asserts com valores injetados;
  1 teste NOVO do kill switch injetado (com controle positivo: desligado, o G6
  dispara).
- Já medido nesta sessão: env limpo 642/0 ✓ · native 642/0 ✓ (os 15 mortos) ·
  code restava 1 (`g6_mutacao_no_meio` — convertido: com code a 2ª sed-n
  colapsava por rajada, outro gate) · GATES_DISABLED: conserto em escrita.

**SESSÃO SEGUINTE, COMEÇA AQUI (3)**: terminar o H1 — (a) `GateOverrides.code_gates_disabled`
+ `code_gates_disabled_from` pura + os 12 pontos lendo `ovr` + `burst_gate_with` +
`write_run_pair_gate_with` (achar o caller de write_run_pair_gate: grep
`write_run_pair_gate(` — provável code_mode_gates_with); (b) shim-alias nos 6 mods
(code_mode_gates_w1, burst_gate_w2, apresentacao_por_escopo, mode_gates_w3,
actor_isolation_tests, s24_route_tests); (c) o teste novo do kill switch;
(d) medir os 4 ambientes (0 falhas cada) + cargo_cmd rc 0 + clippy EXIT 0;
(e) relatório à 6b com os números e a contagem de cobertura (15 → 15 + asserts
de precedência; os 29 cobertos pelo shim). H3 pendente: `cargo report
future-incompatibilities --id 2` → dizer qual dependência é (proc-macro-error2,
confirmar). DEPOIS de H1+H2 verdes ela libera os passos 0-2 do A3.

## MARCO 08/10 ~23:40 — H1 VERDE (fechado; aguarda a re-matriz da 6b)

**Os 4 ambientes, 0 falhas cada, no código final** (cli lib = 645 no universo =
643 rodando + 2 ignored — o "643" das rodadas e o "644 filtered + 1" dos
filtros são o MESMO universo; conferido por `cargo test -p touring-cli --lib`
sem filtro: `643 passed; 0 failed; 2 ignored`):
1. limpo 643/0 · 2. `TOURING_CODE_MODE=native` 643/0 · 3. `=code` 643/0 ·
4. `TOURING_CODE_GATES_DISABLED=1` 643/0.

**Cobertura (nada perdido)**: os 15 testes que liam o env do processo seguem
existindo — agora afirmam com valor INJETADO via `apresentacao_com_env` /
`gates_limpo` (helpers `#[cfg(test)] pub(crate)` no pai cli_suggester.rs;
shim por alias de import nos mods: `use super::super::gates_limpo as
code_mode_gates;`, corpos intactos); os 4 testes de precedência ganharam 6
asserts novos com valores injetados (env>projeto · prefixo>env · prefixo
inválido cede à env · env vale no meio do comando); 1 teste NOVO
`kill_switch_injetado_desliga_os_gates` (3× is_none com o switch + controle
positivo: sem ele, o G6 dispara); os 29 pontos do kill switch passam por
`GateOverrides.code_gates_disabled` (12 pontos de leitura, todos via `ovr`).
642 → 643 rodando (+1 líquido: o kill switch injetado; os 2 ignored já
existiam).

**Causa-raiz estrutural do clippy --all-targets quebrado**: `#[path] mod
tests;` estava SEM `#[cfg(test)]` — o arquivo de testes compilava no alvo lib
puro (sem dev-deps nem helpers cfg(test)). Adicionado o cfg (os mods inline
já tinham). Wrappers mortos removidos (REGRA #0): `code_mode_presentation`,
`burst_gate`, `write_run_pair_gate` — órfãos após a migração para `_with`.

**Aceites re-medidos no código final**: ① 150× dedupe_ 0 falhas (inalterado —
hooks-shared não foi tocado no H1) · ② 20× cli lib 0 falhas (`RESUMO
rodadas=20 falhas=0`) · ③ cargo_cmd do item1.json **6/6 rc=0 medidos SEM
pipe** — a 1ª medição lia o rc do `tail` (modo 4 da REGRA ZERO), refeita
fiel: env_prefix 1p/644f · must_echo 1p/644f · lessons_verb 1p/487f · dedupe_
14p/474f · epoch_ 2p/486f · s8_corpus_ab 1p/0f em 64,26s (= o corpus real de
79.253 payloads) · ④ clippy --all-targets nos 6 crates EXIT 0.

**H3**: `cargo report future-incompatibilities --id 2` →
**proc-macro-error2 v2.0.1** (E0365: `extern crate proc_macro` é privado e
não pode ser re-exportado, src/lib.rs:494:13; repo
github.com/GnomedDev/proc-macro-error-2). Dependência de terceiros, aviso
informativo, não bloqueia o build.

**SESSÃO SEGUINTE, COMEÇA AQUI (4)**: relatório H1 enviado à 6b (~23:40). Ela
re-roda a matriz inteira no binário novo → ao sinal dela, passos 0-2 do A3
(ver marco (2): elite_aggregate --check + clippy --workspace --all-targets
-D warnings · bump à MÃO `version = "30.4.93"` em [workspace.package] do
Cargo.toml raiz · `scripts/propagate-release.sh 30.4.93 --dry-run` → mandar
a saída a ela). Passo 3 (`--stop-before-propagate`) SÓ com sinal explícito
dela + aviso prévio a 6b E analise-a9.

**~23:55 — 6b ACUSOU RECEBIMENTO (cross-session)**: matriz dela em curso no
binário touring_cli de 20:24:42 (20 rodadas limpas + 8 ambientes, ~30 min);
se os 4 ambientes do aceite derem 0 falhas, ela manda o sinal dos passos 0-2.
H3 registrado por ela como dívida de terceiros, não bloqueia. **Esta sessão
fica em ESPERA — nada a fazer até o sinal dela chegar como cross-session
message.**

## MARCO 09/10 ~00:20 — A3 PASSOS 0-2 VERDES (sinal dela ~00:05; passo 3 segue BLOQUEADO)

**Juiz do H1 aprovado pela 6b**: 20 rodadas limpas 0 falhas + matriz de 8
ambientes 643/0 cada (native · code · GATE_OK · SUGGESTER_DISABLED ·
TURN_BUDGET=100 · DEDUP_DISABLED · CODE_GATES_DISABLED ·
PILLAR_INDUCTION_ARMED), controle positivo no binário anterior (native 15 ·
code 8 · GATES_DISABLED 29). Ela avisou de mudanças DELA no repo (client/,
scripts/test_drift_coverage.py 382→391, docs de planos) — não reverter.

**Passo 0a** elite_aggregate --check: 1ª rodada **rc=1** → causa:
`06_documentation score=0.00 FAIL kind=block` = drift do `docs/reference/
modules.md` — o módulo novo `touring-hooks-shared::injection_dedup` (o S8!)
entrou na superfície (Count 379→380, +2/-1). Escopo meu → conserto pela via
prevista do próprio gate (`python3 docs/gen_reference.py`, regenerado) →
re-validado rc=0 → re-rodado **elite_aggregate rc=0** (composite=0.8722 Gold).
Log: diagnostics/a3_passo0a_elite.log.
**Passo 0b** clippy --workspace --all-targets -- -D warnings: **rc=0**
(1m33s; único warning = future-incompat proc-macro-error2, o H3). Log:
diagnostics/a3_passo0b_clippy.log.
**Passo 1** bump à mão: Cargo.toml linha 151 `30.4.92` → `30.4.93`, só essa
linha (o Cargo.lock acompanha no build do passo real).
**Passo 2** dry-run: **rc=0**, 37 linhas, plano de 6 fases; os 2 "FAILED" no
5/6 (analise, konverter: toolchain 30.4.93 não instalada) são ESPERADOS em
dry-run — o próprio script os marca como tal e sai 0. Log:
diagnostics/a3_passo2_dryrun.log.

**PARADO após o passo 2, como ordenado.** SESSÃO SEGUINTE, COMEÇA AQUI (5):
aguardar o sinal explícito da 6b para o passo 3 → então avisar 6b E
analise-a9 → `scripts/propagate-release.sh 30.4.93 --stop-before-propagate`
→ ela verifica o binário → `--skip-gates --skip-build --skip-freeze` para
propagar + preencher deploy_utc no diagnostics/item1.json.

## MARCO 09/10 ~00:45 — PASSO 3 FALHOU com exit 4 (caso previsto; reportado, NADA forçado)

Sinal explícito do passo 3 recebido da 6b (~00:30; ela conferiu 0-2 e avisou
a a9). `scripts/propagate-release.sh 30.4.93 --stop-before-propagate` rodado
em background → **RC_PASSO3=4**. Log completo: diagnostics/a3_passo3.log
(cópia canônica) e /tmp/s8_a3_passo3.log.

**Por fase**: 1/6 GATES **OK** ("gates OK" L147; inclui gen_reference
--validate "OK: reference docs in sync" L94 — o conserto do drift segurou) ·
2/6 BUILD: build release OK (8m14s), symlinks dual-target OK, MAS
verify_or_restart **falhou 2×**: o daemon GLOBAL **PID 3156** (confirmado: é
o conteúdo do pidfile canônico /run/user/1000/touring-daemon.pid) roda
`target/release/touring-daemon` **(deleted)**; o log diz "daemon stopped via
daemon-ctl" (L217) MAS "socket held by live daemon PID 3156 — nothing stale
to clean" (L218) → "daemon already running — skip start" (L219) → doctor 2×
com `degraded checks: daemon_socket,daemon_health,project_actor` (L213/L221)
→ "verify_or_restart failed after rebuild — operator intervention required"
(L223) → exit 4. **3/6 FREEZE e 4/6 DEFAULT NÃO rodaram** (script parou no
2/6). O daemon do PROJETO analise está saudável (PID 2928009, toolchain
30.4.92, socket per-project alive) — o problema é só o daemon global 3156.
**Passo 4 NÃO executado** (o 3 falhou; ordem dela: reportar, não forçar).
Proibido kill/pkill (REGRA #19) — a intervenção no 3156 é decisão DELA.
Hipótese (não verificada): o stop do daemon-ctl mirou outro socket ou o
daemon velho não honrou o shutdown.

## MARCO 09/10 ~01:00 — CAUSA-RAIZ (6b) + TDD + INCIDENTE + RETRY EM CURSO

**Diagnóstico da 6b (confirmado)**: `TOURING_DAEMON_SOCKET` da sessão CC vaza
para dentro do update-touring → o `daemon-ctl stop/restart` ciclou o daemon
PER-PROJECT do analise (PID 2928009 nasceu 21:33:04, no meio do run) enquanto
o `doctor` lia o pidfile GLOBAL (3156, deleted). DAEMON_SOCKET do script é
fixo (L77 `/tmp/touring-daemon-$(id -u).sock`) — o unset é seguro.

**⚠ INCIDENTE (meu, contido)**: o 1º harness do teste rodava o script
END-TO-END; o install_symlinks sobrescreveu o stub do touring → daemon-ctl
"falhou" → fallback legacy matou o daemon global REAL 3156 (kill -TERM,
21:46:49). Global DOWN (pidfile stale); analise intacto. Registro completo:
`diagnostics/a3_incidente_daemon_global.md`. Lição: teste de script-de-deploy
NUNCA roda o dispatch completo — harness = prelude + função.

**TDD (nos meus 2 arquivos exclusivos: scripts/update-touring +
scripts/test_update_touring.py)**:
- RED seguro: 2 failed — (1) stub gravou a isca no env do `daemon-ctl stop`
  (vazamento reproduzido); (2) invariante posicional do unset. Harness:
  `script_prelude()` (até a 1ª função) + `shell_function("kill_daemon")` +
  stub que grava o próprio env; via ARQUIVO (bash -c tem BASH_SOURCE unbound
  sob set -u).
- GREEN: `unset TOURING_DAEMON_SOCKET TOURING_DAEMON_SOCK` no topo (após
  `set -euo pipefail`, comentário datado) → **28 passed** normal E com
  UPDATE_TOURING_REQUIRE_SYMLINK=1.
- **Inocuidade MEDIDA (aceite A dela)**: diff ANTES/DEPOIS da suíte sobre
  {5 readlinks, PIDs touring-daemon, mtime dos 3 ELF} = IDÊNTICO
  (/tmp/s8_snap.sh; snapshots /tmp/s8_inoc_{antes,depois}.txt).
- MUTAÇÃO: sem o unset → 2 failed → restaurado → 28 passed.

**RETRY do passo 3 EM BACKGROUND (task bj37m6q6t, ~01:00)**:
`propagate-release.sh 30.4.93 --stop-before-propagate`, snapshot ANTES em
/tmp/s8_passo3_antes.txt, log /tmp/s8_a3_passo3_v2.log (+RC_PASSO3_V2 no fim).
Aceites do retry: global com PID ≠ 3156 e exe sem "(deleted)" · fases 1/6-4/6
verdes · dizer se o PID 2928009 do analise foi tocado · aceite B: se o script
não lidar sozinho com o pidfile stale "3156", PARE e reporte (não apagar à
mão). Depois: passo 4 (versões em stderr SEM 2>/dev/null + daemon-ctl status)
→ relatório único → PARAR antes do 5/6.

## MARCO 09/10 ~01:15 — PASSO 3 VERDE (rc=0) + PASSO 4 COMPLETO — PARADO antes do 5/6

**Passo 3 refeito: RC_PASSO3_V2=0.** 1/6 GATES OK · 2/6 BUILD OK — com o unset,
o daemon-ctl mirou o GLOBAL: "daemon stopped via daemon-ctl" → "daemon
restarted via daemon-ctl (fresh binary)" → "doctor: 5/5 OK" (zero degraded) ·
3/6 FREEZE OK · 4/6 DEFAULT OK. Aceite por comportamento: global PID
**2971193** (≠3156), exe `target/release/touring-daemon` SEM "(deleted)",
socket /tmp/touring-daemon-1000.sock alive; **analise 2928009 NÃO tocado**;
pidfile stale tratado pelo próprio daemon-ctl (aceite B — zero intervenção
manual). Log: diagnostics/a3_passo3_v2.log.

**Passo 4**: `touring --version` (PATH) = **touring 30.4.93** (git 071d9afb,
mtime 00:32:12Z) · toolchain `~/.touring/toolchains/30.4.93/bin/touring
--version` = **30.4.93** (mesmo git, mtime 01:00:40Z = o freeze) ·
`daemon-ctl status` (cwd neutro, env limpo): global alive PID 2971193.
**Prova DIRETA do S8 no binário instalado EXISTE e passou**: `touring-hook
session-start` do toolchain contra o daemon global — mesma sessão 2×:
443B → 56B (`additionalContext` omitido inteiro, camada 1); sessão nova:
443B completa (controle positivo por sessão). Artefatos: /tmp/s8_prova/.

**SESSÃO SEGUINTE, COMEÇA AQUI (6)**: relatório único enviado à 6b (~01:15).
PARADO antes do 5/6 — ela lê e dá o sinal da propagação (5/6-6/6: `touring
update` nos projetos + prova E2E + verify), que preenche o deploy_utc no
diagnostics/item1.json.

## MARCO 09/10 01:12 UTC — PROPAGAÇÃO COMPLETA (5/6-6/6 rc=0) — S8 ENTREGUE

Sinal dela ~01:05. `env -u TOURING_DAEMON_SOCKET -u TOURING_DAEMON_SOCK
scripts/propagate-release.sh 30.4.93 --skip-gates --skip-build --skip-freeze`
→ **RC_PROPAGACAO=0**: rótulo OK (binário declara 30.4.93) · 5/6: analise
30.4.92→30.4.93 (daemon restarted no socket do analise) + konverter idem (o
daemon dele estava dead → subiu com o 30.4.93) · 5.5/6 PROVA **40/40
asserções** · 6/6 VERIFY lock=touring=30.4.93 nos 2 projetos · "propagação
completa e verificada". Log: diagnostics/a3_propagacao.log.
**DEPOIS**: analise PID 2928009→**2992399** (exe toolchain 30.4.93) ·
konverter dead→**2993130** (30.4.93) · `.touring/bin/touring --version` =
30.4.93 nos 2 projetos (stderr, sem supressão). **deploy_utc =
2026-10-09T01:11:50Z** lido de /proc/2992399/stat (btime+starttime) —
preenchido no diagnostics/item1.json.
**Medição extra pedida por ela**: `touring update --project` NÃO tem o
defeito do env — `update.rs:332` (`let sock = dot.join("daemon.sock")`, dot =
.touring do --project) + `daemon_ctl.rs:374-399` (`restart_socket_with_bin`
usa `target` exclusivamente: `pid_for_socket(target)` L379,
`UnixStream::connect(target)` L386; zero leitura de env).
**S8 ENCERRADA** (dedupe na origem + release 30.4.93 propagado). Pendente só:
a lição do incidente no grafo de memória do analise (G6).
