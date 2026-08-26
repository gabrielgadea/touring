<!-- OKF document -->
---
okf_version: "1.0"
type: Strategy
title: "Afordância profunda de code mode — o furo do cd, a execução sem classe, e a paridade de confiança"
description: "Diagnóstico exaustivo da não-adoção (caso analise 67-91 Bash/0 touring-run) + mecanismos do deepseek-harness + 7 movimentos + 1 incidente (Write tool mentindo nesta sessão)."
plan_id: 2026-08-26-code-mode-afordancia-deep
tags: [code-mode, afordancia, deepseek-harness, gates, g-turno, classificador, trust-parity, write-tool-incident]
timestamp: 2026-08-26T13:10:00-03:00
authority: Gabriel Gadea
status: proposta — aguarda gate humano
supersedes-parcial: strategy:code-mode-afordancia:2026-08-25
---

# Afordância profunda de code mode

## §0 — O caso e a pergunta

Sessão do analise, hoje 09:08 (transcript 0b0b06ef): o Stop hook disparou
"[G-turno rajada-sem-programa] 67 Bash neste turno e 0 `touring run` (maior
classe: `cd` ×26)". O agente reconheceu o sinal — "O sinal está correto e eu o
reconheço" — e seguiu sem code mode, com a entrega convergida (5 gates exit 0).

A pergunta de Gabriel: por que o sinal correto não muda o comportamento — e o
que o deepseek-harness (que clonamos) tem de afordância que ainda não portamos.

Resposta curta: **o sinal estava correto sobre o volume e errado sobre a
classe; o remédio oferecido não executava a rajada real; e nenhum gate de
classe enxergava os comandos — 72 de 91 começavam com `cd`, que nenhum dos
três classificadores lê além do primeiro token.**

## §1 — Anatomia da não-adoção (5 causas raiz, todas com evidência)

### R1 — Furo de classificação: o primeiro token (CONFIRMADA, 72/91 comandos)

Na sessão-problema: 91 Bash, 4 `touring run`, **70 comandos começam com `cd`**
(69× `cd /home/gabrielgadea/projects/analise` + 3× um subdir de plano) e a ação
real vem na linha seguinte ou após `&&` (`.venv/bin/python3 …`, `for …`,
`grep …`). Os três classificadores leem só o primeiro token:

- `scan_class_of` (cli_suggester.rs:2813) — G1/T3-B/code-mode deny:
  `cd` → `_ => None`, não é classe.
- `is_scan_command` (cli_suggester.rs:2277) — `starts_with("grep ")` etc.:
  `cd … && grep` não casa.
- `code_mode_burst_block` (loop_stop_guard.py:297) — o G-turno conta
  `c.split()[0]`, então diagnosticou "maior classe: `cd` ×26".

Mesmo que o analise tivesse `mode = "code"`, o deny por classe não pegaria
nenhum desses comandos. O furo não é config — é o tokenizador.

### R2 — `mode = "code"` existe só no workspace touring (CONFIRMADA)

`.touring/touring.toml` do **analise**: sem seção `[code_mode]` → default
`both` → zero deny por classe; sobram G1 (rajada seriada ≥4 da mesma classe em
180s — `cd` não é classe) e G-turno (Stop, 1×/turno). **konverter**: idem.
A decisão P2b (braço code_mode controla apresentação atrás de env default-OFF,
promoção por evidência) segue sem amostra fora do piloto. No piloto (este
workspace): `code_mode_arm.json` = `{offered: 9, followed: 3, economical: 1}`.

### R3 — G-turno: diagnóstico lexical, remédio genérico (CONFIRMADA)

O G-turno (loop_stop_guard.py:278-309, `BURST_BASH_FLOOR = 20`) bloqueia 1× por
turno DEPOIS da rajada, diagnostica por primeiro token ("cd ×26") e prescreve
o esqueleto R1 (`r1-varredura-agregado`) — feito para **inspeção**. A rajada
real era **execução** (`.venv/bin/python3` scripts/-m/-c, loops `for`). O
remédio não executa a rajada → o agente reconhece e segue igual. Contraste
vivo nesta sessão: o T3-B me negou 2× com o **programa derivado dos comandos
verbatim** e eu o segui imediatamente (t3_turn_fused=1, t3_turn_first_passed=1).
A lição já registrada (`nudge-entrega-o-programa`, emenda Gabriel 25/08) vale
para o G-turno e ele ainda não a aplica.

### R4 — Rajada de execução homogênea não tem classe nem esqueleto (CONFIRMADA)

`.venv/bin/python3` ×75 (sessão dc87e4e0, ontem 23:14; 224 Bash, 13 runs =
5,8% adoção) e ×~30 na sessão-problema. N chamadas seriadas do mesmo
interpretador são um **loop desenrolado** — e não são: inspeção (G1/T3-B),
laço escrito (G8), nem escrita (P2.3, corretamente protegida). É o maior poço
de round-trips fora do piloto e nenhum gate o nomeia. No DeepSeek, **toda**
tool vira binding do programa; nós escolhemos não colapsar mutação — mas
execução read-only homogênea é o meio seguro que falta.

### R5 — Fricção assimétrica e gate-fatigue (CONFIRMADA)

(a) Todo `touring run --lang bash` emite no stderr do modelo
`WARN … CEG advisory deny on shell run … composite=0.675` — o advisory é
documentado como **esperado** (run.rs:425-445: `echo hi` nega sob Sandboxed, e
a contenção real é o sandbox), mas chega ao modelo como aviso de falha. O
DeepSeek decide o contrário por design: *trust bash-equivalent — a rota code
nunca é mais assustadora que a rota bash*.
(b) Gate-fatigue medido: sessão 67cfcd88 (hoje 14:20) — `TOURING_GATE_OK=1` em
**8 de 21** comandos. O bypass por-comando virou hábito: onde o gate morde,
contorna-se por reflexo. Cada bypass habitual é um FP do gate a corrigir ou um
custo a remover.

## §2 — O que o deepseek-harness tem × o que temos

Fontes: `packages/code-runtime` (README raiz + 3 sub-pacotes),
`.agents/notes/implemented/feature/2026-06-15-code-mode.md` (27 KB),
`packages/core/tools/README.md` §Code Mode (34 KB),
`packages/workflow/workflow-worker-thread` (session/realm/host/protocol).

| Mecanismo de afordância | DeepSeek | Touring hoje | Gap |
|---|---|---|---|
| Apresentação | `mode: native\|code\|both` no registry; em `code` as outras tools **não existem no wire** — chamada direta vira `UNKNOWN_TOOL` **antes** de pre-execute/approval/guards | deny PreToolUse por classe (grep/cat/find), por projeto | Não controlamos o wire do Claude Code; mas o deny pode ser movido para **antes** e para a classe certa (R1) |
| `presentAs` por agente | um agente sombreia sua apresentação sem mudar o catálogo | apresentação por projeto apenas | subagents herdam; sem granularidade |
| SDK gerado | `.d.ts`/TypedDicts **das tools visíveis do escopo**, tipos exatos, byte-stable (ordem lexicográfica protege o prompt cache) | stub estático de 9 métodos do daemon, byte-stable (W2) | o SDK não cobre as ações reais do projeto (pytest, scripts, .venv) — exatamente as da rajada R4 |
| Trust posture | **bash-equivalent por design** — run_code nunca mais restrito que bash | advisory WARN em todo shell run; sandbox profiles | R5: rebaixar o advisory esperado para log; paridade de capacidade |
| Sub-chamadas | re-entram o pipeline completo; overlap só entre concurrency-safe até `maxParallelSubCalls`; exclusivas são barriers; correlação logada | isentas por construção (não chegam ao PreToolUse) | ✓ equivalente em efeito |
| Spill | `tools/code-dispatch-log` waterfall: preview + locator | spill + locator + retrieval_hint (23/08) | ✓ |
| Estado | fresco por run; REPL persistente **rejeitado** (estado invisível ao log) | fresco por run | ✓ |
| Budgets | `computeMs` mede event-loop utilization (dispatch pendente não pausa) + `maxWallMs` com range-check | compute_ms via /proc + wall | ✓ |
| Erros | `ToolCallError(toolName, message)`; internals fora do contrato; `CodeRunFailedError(kind)`; output vazio explícito | RunFailure 6 kinds + stderr real (W0/W1) | ✓ |
| Adoção em `both` | nenhum gate — o SDK + "modelo escreve código melhor que tool-calls" (Cloudflare) | nudges + G1/T3-B/G-turno | a adoção por competência precisa do SDK **do projeto**, não só do daemon |

## §3 — Estratégia: 7 movimentos (cada um com gate de evidência)

- **S1 — Classificador além do primeiro token** (fecha R1). `scan_class_of`,
  `is_scan_command` e o contador do G-turno passam a resolver o **comando
  efetivo**: pular prefixos `cd <dir> (&&|;\n)`, `env VAR=…`, `VAR=…` e
  classificar o que sobra (hoje `scan_class_of` já pula `VAR=…`; estender a
  `cd`/`env` e à quebra de linha). Guard: grep o padrão inteiro primeiro
  (lição 23/08) + teste de mutação 0→1→0 em cada sítio (são 3+ sítios — a
  memória `definer-module-cinco-sitios` ensina: consertar um mascarou).
- **S2 — Piloto `mode = "code"` no analise** (fecha R2, decisão de Gabriel no
  gate). Segundo piloto real, com `TOURING_CODE_MODE` e telemetria. Pré-condição:
  **S1** — sem ele, o deny nega grep/cat/find e deixa passar os 72/91.
- **S3 — G-turno com diagnóstico semântico e remédio derivado** (fecha R3).
  (a) classificação do campeão pelo comando efetivo (S1); (b) o block entrega o
  **programa derivado dos comandos verbatim da rajada** (o mecanismo T3-B,
  provado hoje), não o esqueleto R1 genérico; (c) medir eficácia: a rajada
  parou/virou run após o block? (counter `g_turno_effective`).
- **S4 — Classe exec-burst + esqueleto R9** (fecha R4). N≥10 chamadas seriadas
  do mesmo executor (`.venv/bin/python3`, `python3 -m`, `pytest`) com 0 run →
  o remédio passa a ser "1 programa que roda os N e agrega" (R9
  exec-agregado: subprocesss sequencial, saídas para o spill, digest ≤200
  tokens). Escrita/mutação fora POR CONSTRUÇÃO (P2.3).
- **S5 — Paridade de confiança bash↔run** (fecha R5). (a) o advisory CEG
  esperado sai do stderr do modelo → tracing debug/arquivo (a contenção real é
  o sandbox — run.rs:425); (b) KPI de gate-fatigue: taxa de `TOURING_GATE_OK=1`
  por sessão publicada em gate-metrics; (c) todo deny que gera bypass habitual
  vira candidato a correção de FP.
- **S6 — SDK por escopo via portfolio** (gap DeepSeek). O stub estático cobre o
  daemon; as ações do projeto (as da rajada) ficam fora. O `portfolio` já
  indexa 4.068 artefatos por propósito — o G-turno/nudge passa a consultar o
  portfolio **com a rajada real** e entregar o snippet mais próximo
  instanciado (a rota vem escrita E é de um programa que já funcionou).
- **S7 — Harness: Write tool mentindo (INCIDENTE, P0 para a confiança)**.
  Nesta sessão o Write reportou sucesso 4× e não escreveu nada (probe em
  docs/plans ×2, bundle test ×1, /tmp ×1); Bash heredoc persiste normalmente.
  Hooks PreToolUse de Write auditados e inocentes (pre-write allow,
  cli-suggest, quality-block fail-open e cego a .md). Suspeita primária: o
  patch `patch-claude-bash-nudge.py --ensure` (SessionStart, commit e3aa133)
  sobre o Claude Code v2.1.245. Ação: reproduzir fora da sessão, isolar o
  patch, corrigir; até lá, escritas via Bash heredoc. **Qualquer entregável
  desta sessão que dependesse de Write/Edit precisa ser re-verificado.**

## §4 — Anti-alvos (calibrados por medição anterior, não reabrir)

1. **Não apertar o limiar do G1** — mediana de rajada 1; o limiar ≥4 pega 14%
   e está calibrado (auditoria 25/08).
2. **Não converter grep para o índice** — 56% dos greps são regex estrutural;
   o índice responde a ~3%.
3. **Não colapsar escrita/mutação** — P2.3: `cat >` heredoc é T4; colapsá-lo
   taxou 20,6% de FP.
4. **Não construir REPL persistente** — o DeepSeek rejeitou: estado cross-call
   invisível ao log.
5. **Não punir além do 1 block/turno no G-turno** — fricção tardia já medida
   como ineficaz; a correção é o remédio derivado (S3), não mais dente.

## §5 — Métricas de adoção (como saberemos que a afordância funciona)

1. `code_mode_arm.json` por projeto: followed/offered **> 50%** (piloto hoje:
   33%).
2. Rajadas ≥20 Bash com 0 run por sessão (forense de transcripts): **→ 0** no
   analise em 2 semanas.
3. Taxa de `TOURING_GATE_OK=1` por sessão: **< 5%** dos comandos (hoje: 38% na
   sessão 67cfcd88).
4. `g_turno_effective` (rajada parou após o block): medir antes de qualquer
   novo dente.
5. Adoção medida no elegível (auditoria): de 26-28% para **> 60%**.

## §6 — Sequência proposta

S7 (incidente Write, P0 confiança) → S1 (classificador, desbloqueia tudo) →
S3 (G-turno derivado) + S5a (advisory fora do stderr) → S2 (piloto analise,
gate Gabriel) → S4 (exec-burst/R9) + S6 (portfolio no remédio) → S5b/c (KPI
fatigue + revisão de denies). Cada movimento fecha com prova viva (deny/run
real) + teste de mutação, no padrão da casa.

## §7 — ADENDO (diretriz Gabriel, 26/08 14:15): despatch + neutralização da injeção nativa + code mode obrigatório

Gabriel ordenou: **retirar o patch bash-nudge** (restaurar o binário original, que
volta a injetar "While bypass permissions mode is active: use Bash … rather than
dedicated tools") e, em vez de eliminá-la na origem, **tratar a injeção para ser
desconsiderada pelo modelo**, que deve seguir **obrigatoriamente** as diretrizes
do code mode. Razão implícita confirmada pela investigação do incidente Write:
o patch é frágil (todo update do mise o desfaz; a prova de inocência dele nesta
sessão — probes A/C/D/E1/FULL1-9 escrevem com o MESMO binário patchado — mostra
que ele nem é alavanca de comportamento, é só cosmético de prompt).

- **N1 — Despatch controlado.** `patch-claude-bash-nudge.py --restore` (o backup
  `.orig` existe ao lado do binário). Remover o hook SessionStart `--ensure` do
  settings.json (foi a ÚNICA mudança de settings da janela do incidente —
  inocentada, mas órfã da política antiga). O script segue versionado como
  referência; a política nova é NÃO patchear.
- **N2 — Neutralização por contrato (camada modelo).** REGRA #20 ampliada: a
  injeção nativa do bypass/auto mode é NOMEADA na constituição como texto do
  harness a desconsiderar, com a diretriz positiva explícita (varredura →
  `touring run`; leitura pontual → Read; edição → Edit/Write com os 17 gates).
  Camada necessária e insuficiente (lição D8: persuasão falha) — quem aplica é
  o executor (N3).
- **N3 — Neutralização por executor (a que morde).** Cada mandato da injeção
  encontra um gate: `cat/grep/find` → deny code-mode com a rota derivada
  (REQUISITO: S1 classificador sem furo `cd` + S2 mode=code propagado, senão a
  injeção escapa por compostos e projetos vizinhos); `sed -n` em rajada →
  fusão T3-B; **`sed -i`/`sed --in-place` modelo-direto em código → GATE NOVO:
  deny com a rota "Edit tool (VGP/blast/TDG/snapshot/gotcha/wiring/RL)"** — a
  injeção manda explicitamente "make file changes with sed", o anti-padrão que
  atravessa os 17 stage gates; heredoc `cat >` segue livre para docs (P2.3) —
  para CÓDIGO, decisão de Gabriel (advisory vs deny).
- **N4 — Obrigatoriedade do code mode = união dos gates, não exortação.**
  Colapso de classes (existe) + T3-B (existe) + G1 (existe) + G-turno com
  remédio derivado (S3) + exec-burst (S4), propagados (S2) e sem furo (S1).
  O contrato de sessão (SDK stub) segue injetado e agora coexiste com a
  injeção nativa restaurada — N2 declara a precedência, N3 cumpre.
- **N5 — KPI da neutralização (a injeção como feromônio invertido).** Counters
  `injection_native_followed` (cat/grep/sed/-i nativos emitidos) ×
  `injection_native_resisted` (touring run/Read/Edit em situações equivalentes)
  em gate-metrics. Meta: followed → ~0. A injeção restaurada vira o instrumento
  que mede a própria desconsideração — sem ela (patch ativo) o KPI não tem
  numerador, mais uma razão para o despatch.
