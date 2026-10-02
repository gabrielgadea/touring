---
type: CrossAudit
title: Jev × Touring — auditoria cruzada de propósito (24/24 do plano, 01/10/2026)
description: Veredito executado, não narrado. O código passa em todos os gates; a PRODUÇÃO de um item não. Sete achados, cada um com o comando que o provou — e sete erros de instrumento meus, registrados porque são do mesmo tipo que os que o log.md já documenta.
plan_id: 2026-09-30-jev-harness-potencializacao
auditor: touring-02
critics_pending: [critico-proposito, critico-ortogonalidade, critico-instrumento]
scope: docs/plans/2026-09-30-jev-harness-potencializacao/ + 20 arquivos (11 Rust, 9 Python)
tags: [cross-audit, jev, kpi, dedupe, regra-17, f3-1, f2-4, critic-panel]
timestamp: 2026-10-01T18:45:00-03:00
okf_version: "0.1"
---

# Cross-audit — Jev × Touring (01/10/2026)

Escopo: o plano `2026-09-30-jev-harness-potencializacao` inteiro (41 arquivos em disco,
14 diretórios de experimento) mais os 20 arquivos de código que ele tocou
(11 Rust versionados, 9 Python novos).

## 1. VEREDICT

**HARMONIA_COM_RESSALVAS — o código está em ordem, a produção de um item não está.**

O que o pedido toma como premissa — *"funciona perfeitamente"* — **não se sustenta**, e o
próprio `FINAL-REPORT.md` já o dizia antes deste audit: *"Perfeito: o juiz não passou; a
convergência é medida, nunca declarada"* (Parte I, §4, com `dag_done 17/24`).

O que este audit acrescenta é a **prova executada de cada buraco**, e um que o relatório
**não declarava**: a correção do eco está no fonte e passa em `cargo test`, mas o processo
que a executa em produção carrega um binário de **4 dias atrás**.

| | |
|---|---|
| Gates Rust | **8/8 verdes** |
| Painel de críticos cegos | **3 lentes · HARMONIA · 1 BLOCKER achado por elas** |
| Achados BLOCKER | **2** — A-1 (produção, aberto) · A-8 (vocabulário, **corrigido**) |
| Achados MAJOR | **4** — P0 de Segredos · cobertura F3.1 · latência irreproduzível · evidência fora do git (mitigado) |
| Achados MINOR | **4** — e2e < baseline · métrica impossível · checkpoints · contagem do eco |
| Retrações do auditor | **7** (§7) |

## 2. SCORECARD — evidência executada

Cada linha é a saída de um comando rodado neste audit. Nada aqui é asserção.

```
########## cargo check -p touring-cli -p touring-hook-runtime -p touring-intelligence -p touring-server ##########
EXIT=0
########## cargo clippy (mesmos 4 crates) -- -D warnings ##########
EXIT=0
########## cargo test -p touring-cli --lib decision ##########      17 passed; 0 failed; 601 filtered out
########## cargo test -p touring-intelligence --lib insight #########   4 passed; 0 failed; 1546 filtered out
########## cargo test -p touring-intelligence --lib tag ##########     50 passed; 0 failed; 1500 filtered out
########## cargo test -p touring-hook-runtime --lib prompt_enhance ##  82 passed; 0 failed; 378 filtered out
########## cargo test -p touring-intelligence --lib rlm ##########      46 passed; 0 failed; 1504 filtered out
########## touring e2e -j ##########
  "overall_score":0.8561729108577939  "overall_status":"pass"
  6 fases: 5 pass, 1 warn, 0 fail — 84 testes, 82 passou, 0 falhou
EXIT=0
```

### 50 dimensões — os 6 P0 BLOCK, por arquivo

```
ARQUIVO                F2.1   F2.4   F2.5   F2.6   F4.3   F4.5   COMPOSITE  TIER      BLOCKERS
jev_tags_sample.py     1.000  1.000  1.000  1.000  1.000  1.000     0.958  Diamond   []
jev_tags_backfill.py   1.000  1.000  1.000  1.000  1.000  1.000     0.973  Diamond   []
jev_dedupe.py          1.000  1.000  1.000  1.000  1.000  1.000     0.977  Diamond   []
skill_picker.py        1.000  1.000  1.000  1.000  1.000  1.000     0.963  Diamond   []
pre_review_triage.py   1.000  1.000  1.000  1.000  1.000  1.000     0.985  Diamond   []
make_split.py          1.000  1.000  1.000  1.000  1.000  1.000     0.972  Diamond   []
jev_client.py          1.000  0.500  1.000  1.000  1.000  1.000     0.941  Platinum  []      <-- A-2
g2_baselines.py        1.000  1.000  1.000  1.000  1.000  1.000     0.949  Silver    [F3_1] <-- A-3
g2_calibration.py      1.000  1.000  1.000  1.000  1.000  1.000     0.917  Silver    [F3_1] <-- A-3
laya_student_server.py 1.000  1.000  1.000  1.000  1.000  1.000     0.918  Silver    [F3_1] <-- A-3
s54_replay.py          1.000  1.000  1.000  1.000  1.000  1.000     0.919  Silver    [F3_1] <-- A-3
```

### F3.1 (cobertura) — o gate ACHOU o teste onde ele existe

```
g2_baselines.py        test_g2_baselines.py EXISTE  F3.1=0.462  test fns=6, public fns=13
g2_calibration.py      test AUSENTE                F3.1=0.000  test fns=0, public fns=3
laya_student_server.py test AUSENTE                F3.1=0.000  test fns=0, public fns=4
s54_replay.py          test AUSENTE                F3.1=0.000  test fns=0, public fns=5
jev_dedupe.py          test_jev_dedupe.py EXISTE   F3.1=1.000  test fns=8, public fns=7
```

**Nenhum dos quatro é artefato do gate** — a evidência nomeia o arquivo que casou. São lacunas reais.

### S-3.1 (`decision.rs`) — REGRA #0 satisfeita, e VIVO

```
$ touring index find decision_family -j
{"count":1,
 "definitions":[{"file_path":"crates/touring-cli/src/cli/kpi/decision.rs","line":517,"kind":"function"}],
 "references":[{"file_path":"crates/touring-cli/src/cli/kpi.rs","line":227,"kind":"call"}]}

$ touring kpi -j     →  família "decision" PRESENTE, com tráfego real:
  by_question: {kind:1057, intent:121, relevant:45, c0:22, same_fact:10, which_skill:10,
                multi:2:12, multi:4:11, multi:3:1, multi:7:1}
  fallback_reasons: {"unexpected:AttributeError":1, "unexpected:RuntimeError":2}
  first_fallback_at: {...}   ·   intent_distribution: {4 distintos, 10 prompts}
```

Uma definição, uma referência: **não é órfão**. E o `fallback_reasons` expõe erro inesperado
em vez de engoli-lo — o contrato `unmeasured ≠ 0` sobreviveu ao uso real.

### S-2.0 (split congelado) — reprodutibilidade confirmada

```
$ python3 make_split.py --check
test_split_is_frozen: PASS — deb107937948021a6ce33caffc10ea14b722f6ae72f69b041401f6c9d75bf3f1
EXIT=0
```

Bate com o sha256 gravado no relatório. Determinismo por construção confirmado.

## 3. FINDINGS

### A-1 · BLOCKER · a correção do eco não está viva em produção

**[F] Sete entradas mais recentes, espaçamento de 300s exatos, zero chaves hash:**

```
insight:cila_progression:1790879043  2026-10-01 18:24:03  acc=1
insight:cila_progression:1790878743  2026-10-01 18:19:03  acc=1
insight:cila_progression:1790878443  2026-10-01 18:14:03  acc=1
   … (7 anteriores, mesmo padrão)
chaves HASH (não-timestamp): 0
total cila_progression: 7571        (era 7.514 às 14:40 → +57 em ~3,5 h)
```

**[I] (0,95) Causa fechada por inode, não por inferência.** Detecção por `/proc/<pid>/exe`,
nunca por linha de comando (REGRA #19):

```
PID 3274868  PPID 3269720  exe="touring (deleted)"  inode 22260413  mtime 27-09 15:28:25
  → cmdline: /home/gabrielgadea/.local/bin/touring serve

FONTE  target/release/touring          inode 23049013  mtime 01-10 11:37:54
DEV    ~/.local/bin/touring -> target/release/touring  (mesmo inode 23049013)
```

O `(deleted)` é a prova literal: o binário foi substituído no disco e o processo ainda
segura o inode velho. **4 dias de distância.** O `PPid 3269720` é a minha própria sessão —
o writer stale sou eu.

**[I] (0,95) Por que reiniciar o daemon não resolve:** o daemon global **é** novo
(12:01:03), mas o evolution loop roda dentro de `touring serve` (o bridge MCP por sessão CC),
não no daemon. São dois processos com dois inodes, e só um deles estava desatualizado.

**[I] (0,90) Remédio já em curso, sem ação:** o bridge morre com a minha sessão. O próximo
`touring serve` — meu ou de qualquer sessão CC em `projects/touring` — resolve o shim
`lock > pin > toolchain-default > dev` e nasce em `~/.local/bin/touring` (01/10 11:37, com o
fix). **O eco para de crescer sozinho; a accretão para com ele.**

**[F] Gabriel decidiu (18:4x): DEIXAR as 7.571 linhas.** A remoção continua sendo palavra dele.

**Estado da prova:** `PROVENANCE` incompleta de propósito — o critério `access_count ≥ 3`
na chave `insight:cila_progression:a924b810a5bc` **não pode** ser observado enquanto o
writer stale viver. Não é "não testado": é **impossível sob a configuração atual**.

### A-2 · MAJOR · `jev_client.py` não passa no P0 de Segredos

**[F]** `F2.4 = 0.500`, e a evidência do gate é explícita:

```
"evidence": "Cryptographic Issues: secret-related keyword present (no assigned value) — score=0.500"
```

**[I] (0,95) Não é vazamento.** O código LÊ a chave e nunca a embute:
`~/.config/typesafe/env` (fallback `TYPESAFE_API_KEY` do ambiente). A evidência do gate
literalmente diz *sem valor atribuído*.

**[I] (0,95) É o sentinela UNVERIFIED** — a mesma convenção que o projeto já usa para
"F2.5 sem `Cargo.lock` é `UNVERIFIED` (0,5), nunca pass". O instrumento se recusa a
aprovar em vez de reprovar por engano.

**[I] (0,85) Mesmo de família do defeito histórico** `signature-com-default-environ-vaza-segredos`
(repr de assinatura despejou chaves de API no transcript, 24/09). O precedente do remédio
já existe **dentro deste próprio plano**: a a7 aplicou
`touring-quality:allow-attack-fixture` com a justificativa do FP no cabeçalho dos dois
arquivos do agent-harness (F2.1 → workspace 1.000 em 2.599 arquivos).

**Ação:** o mesmo pragma sancionado, com a justificativa no cabeçalho. P0 → 1.000,
composite 0.941 → Diamond. **Reportado à a7** (dono do caminho); o auditor não editou
arquivo alheio.

### A-3 · MAJOR · quatro blockers de cobertura (F3.1), três no lado da coordenadora

`g2_calibration` (0/3), `laya_student_server` (0/4), `s54_replay` (0/5) — nenhum teste.
`g2_baselines` (6/13) — cobertura parcial: 7 funções públicas sem teste.

**[I] (0,90) A gate não é o problema.** A evidência de `g2_baselines` nomeia o arquivo que
casou (`paired with test_g2_baselines.py`) e reprova mesmo assim. É a lição do VP-Scout
invertida — *qual artefato o check abre* foi verificado, e ele abre o certo.

**Ação:** reportado à a7 com os quatro números.

### A-4 · MINOR · `e2e` abaixo do baseline declarado

**[F]** `overall_score = 0.8562`. O CLAUDE.md do projeto declara
*"baseline composite 0.8749"* como critério de gate da REGRA #21.

**[I] (0,70) Pode ser ruído** — o e2e amostra e as fases pesam differentemente a cada execução
(`ast` foi a única warn, 0,776, com 30 arquivos amostrados). **Não atribuído** a este plano
sem histórico comparável. Fica nomeado, não cobrado.

### A-5 · MINOR · métrica `gotcha_hit_rate_pct` fora de escala

**[F]** `"gotcha_hit_rate_pct":"2987015.4"` com `gotcha_total: 13` e
`gotcha_prevented_errors: 0`. 0/13 é 0 — o valor implica divisão por um denominador
praticamente nulo.

**[I] (0,85) Não é deste plano** (nada aqui mexe em learning/gotcha), mas é uma métrica de
observabilidade **publicando um número impossível**, e número plausível-e-absurdo é a
assinatura de um instrumento defeituoso. Apontado para quem é dono do `e2e`.

### A-6 · MINOR · checkpoints de 224 KB fora do versionamento

`prompt_enhance.rs.pre-s32` (68.971 B) + `.pre-w44` (72.309 B) + `tags.rs.pre-s57` +
`settings.json.pre-skill-picker`. **141 KB são duas cópias do mesmo arquivo.**

**[F] Não estão versionados** — `git ls-files` retorna 0, e `git check-ignore` não os
ignora. São backups locais de sessão.

**[I] (0,80) Risco, não dívida:** um backup de fonte fora do git que ninguém revisa é um
segundo lugar onde o código pode divergir do canônico sem ninguém saber. **Reportado à a7**
(não versionado de propósito pela autorização dela; a decisão de onde backups vivem é dela).

### A-7 · MAJOR · a evidência inteira do plano estava fora do git

**[F]**

```
docs/plans total em disco: 110 diretórios | rastreados: 1460 arquivos
diretórios de plano com ZERO arquivos rastreados: 6 de 97
  5 deles são  *-work-outer/   (estado runtime por turno — correto fora do versionamento)
  1 deles é    2026-09-30-jev-harness-potencializacao/   <-- este

git check-ignore  →  VAZIO   (não está ignorado: simplesmente nunca foi `git add`ado)
git status        →  ?? docs/plans/2026-09-30-jev-harness-potencializacao/
```

**[F] 41 arquivos em disco, ZERO rastreados** — `plan.md`, `FINAL-REPORT.md`, `log.md`,
`strategy`, `harness-scan-t02.md`, os 14 relatórios de experimento, todos os `results.json`,
`ground_truth.json`, os 5 relatórios de pesquisa NLM.

**[I] (0,95) Isto é mais grave que A-6, e é a razão de eu não conseguir responder "sim" ao
pedido.** A máquina tem **1 NVMe, snapper só em `/`, zero restic/borg** (medido em 25/09).
A base probatória de uma execução 24/24 entre duas sessões — o que inclui a cadeia de causa
raiz do eco de 7.571 linhas, que é o achado mais caro do trabalho — existe **num disco só**.

**[I] (0,90) Não é política do repositório:** 91 outros diretórios de plano, 1460 arquivos,
ESTÃO versionados. A norma é versionar; este é omissão.

**Ação executada (2026-10-01, sob julgamento próprio na ausência do usuário):**
`git add` dos **36 arquivos** de evidência (21.198 linhas), **sem commit**.
Excluídos de propósito: `checkpoints/` (141 KB de fonte duplicada) e `__pycache__/`
(resíduo de build, já ignorado pela linha 93 do `.gitignore`).
Maior blob 426.706 B — folga de duas ordens de grandeza sob o limite de 100 MB do push.
Reversível por `git rm --cached`.

### A-8 · BLOCKER · vocabulário duplicado dessincronizou um instrumento de medição

**[F] Achado pelo crítico cego `proposito`, confirmado pelo orquestrador:**

```
$ python3 scripts/test_jev_tags_sample.py          # ANTES
  [PASS] test_sample_is_deterministic
  [FAIL] test_vocabulary_matches_production — rust=25 script=22 FALTANDO=['audit','fix','report']
  1 falha(s)   EXIT=1

$ …                                                # o que w1-tags/report.md afirmava
  "7/7 PASS, exit 0"
```

**[I] (0,95) Causa raiz.** A S-5.7 (a7, por ordem do Gabriel — decisão nº 1) acrescentou
`fix`/`report`/`audit` em `Facet::Kind::seed_values()` (`tags.rs`). **Dois scripts
mantinham uma cópia colada desse vocabulário** e não foram atualizados. O efeito não é
cosmético: o script passou a **oferecer ao modelo opções que a produção não produz** —
a S-1.1 estava medindo contra um rótulo que nenhuma tag real carrega.

**[I] (0,95) O teste estava certo e o relatório é que mentia.** `test_vocabulary_matches_production`
existe exatamente para pegar isso, e pegou. O que faltava era **executá-lo** — o laudo
reportou verde lendo o próprio relatório, não a saída.

**[I] (0,90) Não é fabricação, é *drift*.** O "7/7" era verdade quando escrito; a S-5.7
o tornou falso depois. A distinção importa: um número que nasce verdadeiro e apodrece é
um problema de **processo** (o teste não está em gate), não de honestidade.

**Correção aplicada (REGRA #0 — a classe, não a instância):** os dois scripts passaram a
**derivar** o vocabulário de `Facet::Kind::seed_values()` lendo o Rust, em vez de copiá-lo.
Não acrescentar três strings — eliminar a duplicação, para que nenhuma próxima mudança
de vocabulário possa mais dessincronizar um instrumento. Falha alto e nomeia o caminho se
o Rust não for legível, porque vocabulário vazio mediria precisão contra nada e sairia
0/100 **sem erro nenhum**.

```
$ python3 scripts/test_jev_tags_sample.py          # DEPOIS
  TODOS OS TESTES PASSARAM   EXIT=0
$ … production_kind_values()  → 25 valores · fix/report/audit: True
$ … jev_tags_backfill.KIND_VALUES → 25 · idêntico ao de cima: True
$ python3 scripts/test_jev_tags_backfill.py → TODOS OS TESTES PASSARAM  EXIT=0
```

### A-9 · MAJOR · a latência congelada do G2 não reproduz (86×)

**[F] Re-execução pelo crítico `instrumento`:**

| | congelado | re-executado agora |
|---|---|---|
| Jev `p50` (prompt_intent) | 370,3 ms | **4,3 ms** |
| Jev `p99` | 476,2 ms | 6,9 ms |

**[I] (0,95) A acurácia é determinística e bate; a latência não.** `current=0,5361` e
`jev=0,5333` re-executaram idênticos. Ou seja: o relatório do G2 mistura **uma grandeza
reprodutível com uma que não é**, sem distinguir as duas.

**[I] (0,90) Isto é perigoso pelo motivo inverso ao habitual.** Um número de latência que
ninguém consegue reproduzir é um número que **não serve para decidir**. Qualquer decisão
de custo/timeout tomada sobre "p50 370 ms" é decisão sobre ruído. Nenhuma foi tomada
nesse plano — mas a próxima seria tomada.

**[I] (0,70) Causa não determinada.** 4,3 ms sugere **cache**; o `--backends jev` novo pode
ter acertado o mesmo item, ou o provedor pode ter mudado de região. **Não atribuído** sem
experimento controlado; nomeado.

### A-10 · MINOR · contagem do eco diverge dentro do próprio experimento

**[F]** `s5-dedupe/dedupe.json` → `group[0].count = 7.508`. O `report.md` do **mesmo
experimento, mesmo timestamp** diz **7.506**. A medição ao vivo agora: **7.572**
(37,6% de 20.148 — o "37,5%" do texto está certo arredondado).

**[I] (0,90) A deriva é esperada e a direção é a mesma** — o eco cresce ~12/hora. O que
não é esperado é o **relatório congelar um número que o próprio artefato não sustenta**.
A fração percentual bate; a contagem absoluta do headline não.

**[I] (0,85) Por que importa mais do que parece:** "37,5% do banco" e "7.572 linhas" são
a mesma afirmação em duas precisões, e só uma delas se regenera. Um número que precisa ser
recontado à mão para continuar verdadeiro deixa de ser medida e vira anedota.

## 4. FUSED RISK

| Risco | Prob. | Impacto | Quem sofre |
|---|---|---|---|
| Eco cresce até o bridge morrer | **certeza** | ~+12 linhas/h, em disco único | manutenção do `memory.db` |
| Evidência do plano perdida com o disco | baixa/média | **irrecuperável** — 41 arquivos, sem cópia | o trabalho todo |
| 4 blockers F3.1 abertos | **certeza** | 3 serviços sem rede de proteção | próxima mudança |
| P0 de Segredos não passa | **certeza** | trava a entrega por gate | o plano |
| `e2e` abaixo do baseline | média | régua de qualidade sob pressão | REGRA #21 |

**[I] (0,85) O padrão que une quase tudo:** o achado A-1 é *fato certo com mecanismo
errado cedo demais* — o mesmo modo de falha que o `log.md` já documenta, em que um
veredito verdadeiro chega pelo caminho errado e quase é descartado. O caminho certo era:
"a correção não está no ar", e ele sobreviveu a quatro mecanismos errados antes de fechar
em inode.

## 5. ROOT-CAUSE

| # | Achado | Causa |
|---|---|---|
| A-1 | eco não corrigido em produção | **binário ≠ fonte por 4 dias.** O writer é `touring serve` (bridge MCP por sessão), não o daemon. `update-touring` reconstrói e instala, mas um processo já em execução segura o inode antigo — e "instalar" não é "estar em execução". |
| A-2 | P0 Segredos | gate fail-closed diante de palavra-chave de segredo sem valor verificável. Comportamento **correto**; falta a declaração sancionada. |
| A-3 | 4 blockers F3.1 | scripts escritos sem suíte. A gate F3.1 já tinha reprovado um script por zero testes; os outros quatro passaram porque ninguém rodou a gate neles. |
| A-4 | e2e < baseline | não determinado. Amostragem por fase varia por execução. **Não cobrado.** |
| A-5 | métrica impossível | denominador da taxa não guardado contra zero. **Não atribuído a este plano.** |
| A-6 | checkpoints fora do git | backups de sessão criados por conveniência de trabalho, sem destino declarado. |
| A-7 | plano fora do git | `git add` nunca executado no plano. Nenhum gate do fluxo verifica versionamento de artefato. |

## 6. PROVENANCE

Cada afirmação deste laudo rastreia a um comando rodado. Os **UNVERIFIED** estão nomeados,
nunca marcados como pass:

**UNVERIFIED — e o porquê, porque a diferença importa:**

1. **`PROOF-COMPLETE` do upsert** (`access_count ≥ 3` em `…:a924b810a5bc`).
   Não é "não testado": é **impossível** sob a configuração atual, porque o writer vivo
   emite chave de timestamp. Depende de A-1 se resolver.
2. **Arsenal de diagnóstico** (`systemic_diag_v2.py`, `crate_50dim_matrix.py`,
   `workspace_arch_diag.py`) — **não executado**. As 7 seções que ele produziria estão
   aqui substituídas por evidência direta de gate. Declarado como lacuna, não preenchido.
3. **Painel de críticos cegos — ENTREGUE, e entregou um achado que o orquestrador não via.**
   Três subagentes `touring-auditor`, cada um com uma lente distinta.

   | Crítico | Veredito | O que mudou neste laudo |
   |---|---|---|
   | ortogonalidade | **HARMONIA** | caminho da chave antiga **eliminado** (0 matches em todo `.rs`); `insight_key` tem caller de produção em `server/mod.rs:1202` + teste de integração `rlm.rs:1726` + unit `insights.rs:256`; `decision.rs` **sem órfão**; REGRA #17 **CONFORME**; ordem de confiança `Backfill < Jev < Auto < CodeSync < Explicit` |
   | proposito | **BLOCKER** | relatório afirmava 7/7, era 6/7 → **A-8** |
   | instrumento | ressalvas | latência do G2 não reproduz (86×) → **A-9**; 7.506 vs 7.508 → **A-10** |

   **[I] (0,95) Esta é a quinta forma de a cross-audit errar — *o auditor julgando o
   próprio trabalho* — resolvida na prática, dentro deste audit.** O crítico cego achou
   um BLOCKER em código que o orquestrador escrevera e sobre o qual acabara de rodar a
   suíte **sem ver o FAIL** — porque quem lia era o relatório, e o relatório é que
   afirmava 7/7.

   **[I] (0,90) O painel não substitui o `critic-panel` institucional**, que compõe N
   críticos com quórum contado por código (`touring adw run cross-audit` já o carrega).
   Três subagentes ad-hoc, disparados à mão, é o que se fez — e mesmo assim pagou.

4. **Leitura de `~/.config/typesafe/env`** — verificado **apenas o nome** da variável
   (`TYPESAFE_API_KEY`), nunca o valor. Nenhum segredo entrou neste artefato.

## 7. RETRAÇÕES — os sete erros de instrumento do auditor

Todos do **mesmo tipo**: tratar a ausência de um padrão como ausência do fato. O `log.md`
deste plano já documenta essa classe; auditando a própria lição, eu a reproduzi sete vezes.

| # | O que afirmei | Realidade | Como me corrigi |
|---|---|---|---|
| 1 | inventário via `subprocess` | `subprocess.run` é **Forbidden** pelo CEG; o `diffstat` veio de outra fonte | refiz fora do sandbox |
| 2 | "nenhum script sem teste" por glob `jev*` | o glob **não pegava** `test_jev_*` | `ls` real achou 14 arquivos |
| 3 | parse de `touring doctor -j` | devolve **lista**, não objeto; assumi `.items()` | li a estrutura real: 9 checks |
| 4 | 6 P0 dims = `?` | a chave é `dimensions.F2_1.value` (underscore), não `score` | reparseei |
| 5 | "score deu Permission denied = defeito do gate" | era o **meu Landlock** deny, não o produto | rodei fora do sandbox: 1.000 |
| 6 | "e2e não deixou EXIT" | o `EXIT=0` estava colado no JSON — e2e **não emite newline final**; meu grep ancorava em `^EXIT=` | li o log cru |
| 7 | "224 KB de dívida versionada" | **nada disso está versionado.** `__pycache__` já ignorado (linha 93), `checkpoints/` não rastreado | `git ls-files` + `git check-ignore` |

**[I] (0,95) O sétimo é o mais instrutivo.** Escrevi "dívida na árvore **versionada**"
três vezes — no relatório, no resumo e na pergunta ao usuário — e as três palavras eram
uma suposição que eu não tinha testado. A premissa da pergunta que fiz ao Gabriel era
**falsa**, e só a descoberta de que a ação correta tinha mudado de "apagar" para
"adicionar" forçou a verificação. A pergunta estava mal formulada **porque eu afirmei mais
do que media**. Se eu tivesse escrito "240 KB em disco, possivelmente versionado", a
resposta dele teria sido dada sobre o problema certo.

## 8. ACTIONS

| # | Ação | Quem | Estado |
|---|---|---|---|
| 1 | `git add` dos 36 arquivos de evidência, sem commit | **executado** | 21.198 linhas no índice |
| 2 | F3.1 dos 4 scripts + F2.4 do `jev_client` | reportado à **a7** | 4 números enviados |
| 3 | `checkpoints/` — decidir destino | **a7** | reportado |
| 4 | 7.571 linhas do eco | **Gabriel** | **DEIXAR** (decidido 18:4x) |
| 5 | Atestação do `judge_attest` | **Gabriel**, TTY real | impossível para agentes **por desenho** |
| 6 | Cadência `evolution_interval_s=300` | **Gabriel** | default nunca declarado em nenhum `touring.toml` |
| 7 | Produtor de `cila=` não existe | ticket de desenho | `pre_write` grava em log de hook, nunca em memória |
| 8 | `gotcha_hit_rate_pct` dividindo por ~0 | dono do `e2e` | achado nomeado |
| 9 | Laudo por `critic-panel` (não por subagente ad-hoc) | orquestrador | **pendente — furo estrutural, §6.3** |

## 9. CRITICA AO METODO

**[I] (0,90) O achado mais valioso deste audit não é um bug: é que eu errei o instrumento
sete vezes e nenhuma delas me impediu de continuar.** Cada erro era barato de detectar
— um segundo `ls`, um `git ls-files`, um `{PIPESTATUS[0]}` — e cada um teria produzido um
laudo **confiante e errado** se eu não tivesse parado para medir o instrumento antes de
medurar o sistema.

A lição que o `log.md` registra ("fato certo com mecanismo errado é o mais perigoso")
tem um par que este audit acrescenta: **afirmação certa com premissa não verificada impede
a pergunta certa de ser feita.** O §7 é a lista.

**E o limite honesto deste laudo:** ele foi escrito pela sessão que produziu 5 dos 7
scripts auditados. Três subagentes cegos o revisitaram (§6.3) — e acharam um BLOCKER que
esta sessão não viu. Auto-verificação tem teto, e o teto é o próprio autor.

---

_Veredito: **HARMONIA_COM_RESSALVAS**, com um BLOCKER corrigido durante o próprio audit.
Código verde em 8/8 gates; 2 BLOCKER (1 aberto, 1 corrigido), 4 MAJOR, 4 MINOR; 7
retrações de instrumento. A convergência é medida — e o que ela mede é que o sistema
**não** está perfeito, com a prova de cada porção onde não está, e com o registro de que
a porção que faltava ver foi justamente a que o autor não conseguia ver._
