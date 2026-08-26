---
type: AuditReport
title: Cross-audit — bundle autoresearch RL/inteligência + code mode (sessão noite 25/08)
description: Auditoria de fidelidade de propósito sobre tudo que a sessão implementou, com evidência executada; 10 achados, 9 corrigidos, 1 hipótese refutada.
plan_id: 2026-08-25-autoresearch-rl-intelligence
tags: [cross-audit, purpose-fidelity, code-mode, regra-11, flaky, ci-guards]
timestamp: 2026-08-25T23:20:00-03:00
okf_version: "0.1"
---

# Cross-audit — sessão autoresearch RL/inteligência (25/08/2026, noite)

> Auditoria da segunda sessão do dia. A das 16:06
> ([cross-audit-2026-08-25.md](/docs/audits/cross-audit-2026-08-25.md)) cobriu a
> afordância do code mode e **não foi sobrescrita** — é evidência de outro trabalho.

## 1. VERDICT

**APROVADO COM CORREÇÕES** — 10 achados, **9 corrigidos e provados**, 1 registrado sem
correção (dívida de complexidade pré-existente), 1 hipótese minha **testada e refutada**.

O escopo entregue funciona: os mecanismos da sessão (política de braço, evidência
durável, fusão de turno, calibração por similaridade, laço de crédito) foram exercitados
**ao vivo contra o hook real**, não lidos. Mas a auditoria encontrou defeitos reais em
todas as camadas — inclusive **em correções feitas durante a própria auditoria** — e o
padrão que mais se repetiu é o mesmo de sempre: *a garantia estava declarada e não
executada.*

### Limitação estrutural desta auditoria (declarada, não escondida)

Quem auditou é quem produziu o artefato. A skill prescreve `critic-panel` com sessões
frescas justamente para isso, e a sessão está sob instrução de não invocar subagentes
sem pedido explícito. **Nenhum veredito aqui vem de leitor independente.** A mitigação
usada foi mecânica, não social: todo guard novo foi **provado por mutação** (quebrar o
código deve reprovar o guard), e todo número veio de comando executado. Onde não houve
execução, está marcado `UNVERIFIED`.

## 2. SCORECARD

| Eixo | Medida | Evidência |
|---|---|---|
| Testes (4 crates) | **2.541 passed, 0 failed** | 436 cli · 484 foundation · 103 hook-handlers · 1518 intelligence |
| Clippy (5 crates, `-D warnings`) | **exit 0** | após corrigir 4 erros meus (dead_code, 2× collapsible_if, lifetime) |
| Flakiness da suíte | **1/6 → 0/15** | 15 execuções paralelas pós-correção |
| Guards no CI | **4/7 → 7/7** | 3 guards existiam e nunca rodavam |
| Guards que afirmam algo sob pytest | **5/7 → 7/7** | 2 davam "no tests ran" (falso verde) |
| Débito novo introduzido | **0** | `scan_debt.py` nos 4 crates |
| Órfãos entre 27 símbolos novos | **1 achado → 0** | integrado, não removido (REGRA #0) |
| Deploy | **exit 0** | `update-touring`, daemon PID 666975 |
| Espelho `client/` | **CLEAN, 310 arquivos** | drift detectado e sincronizado |

## 3. FINDINGS (todos com evidência executada)

### F1 — O nudge do produto afirmava uma regra revogada · **CORRIGIDO**

`cli_suggester` respondia a qualquer `git` com o cluster `regra-11-git-prohibited` e o
texto *"REGRA #11 — git is prohibited in TACO. Touring is the source of truth; the
block_git.sh hook will reject this command."*

Para `git status` isso é **falso duas vezes**: a proibição foi revogada por Gabriel em
23/08 (REGRA #11 v2) e o executor `block_git.sh` **permite** leitura/aditivas — só gata a
classe DESTRUTIVA, e por ritual, não por banimento. Era o anti-padrão D8 dentro do
próprio produto: *o texto declarado prometia o que o executor não aplica*.

**5 sítios**, não 1 (a família `definer-module-cinco-sitios`): `cli_suggester.rs` (o arm),
`workflow_templates.rs:373` (nota do W10), e **3 testes que encodavam a afirmação falsa** —
`classify_bash_git_routes_to_regra11` assertava `cluster == "regra-11-git-prohibited"`,
isto é, o teste *defendia o defeito*.

**Correção (potencializadora)**: o arm passou a discriminar a classe.
`regra-11-git-destructive` carrega o ritual completo (MEDIR → SNAPSHOT em branch
`safety/` → token `GIT_DESTRUCTIVE_OK=1`) com confiança 0.99 — que é onde a consequência
mora; `regra-11-git-safe` é MAY informativo (confiança 0.55) só para `log|status|diff|
blame|show`, e o resto do git seguro não emite nada (invariante de densidade). Carve-outs
espelhados do executor: `stash list`, `restore --staged`, `reset --soft`, `clean -n`.

**Prova ao vivo** (o único teste que vale — o fonte corrigido não é o binário em uso):

```
git reset --hard HEAD~1  →  regra-11-git-destructive · conf=0.99
                            MUST git status --porcelain && git stash list   (MEDIR)
                            MUST git checkout -b safety/$(date +%F)-<slug>  (SNAPSHOT)
                            MUST GIT_DESTRUCTIVE_OK=1 <o mesmo comando>     (TOKEN)
```

E no binário que de fato classifica (`touring-daemon`, reiniciado pelo deploy):

| string no binário | antes | depois |
|---|---|---|
| `git is prohibited in TACO` | 1 | **0** |
| `regra-11-git-destructive` | 0 | **1** |
| `regra-11-git-safe` | 0 | **1** |
| `ritual anti-perda` | 0 | **1** |

**Guard novo**: `scripts/test_git_nudge_matches_executor.py` lê **os dois lados** — o arm
no Rust e o `DESTRUCTIVE_RE` do shell — e reprova se divergirem. Provado por 3 mutações
(reintroduzir "prohibited" → exit 1; remover o token do ritual → exit 1; declarar um verbo
que o executor não conhece → exit 1).

### F2 — Instrução do harness mandando desarmar as defesas do projeto · **CORRIGIDO NA ORIGEM**

Todo PreToolUse trazia anexado: *"Do your work through the Bash tool… read files with
cat, head, or sed -n, search with grep and find, and make file changes with sed…"* — o
oposto do code mode deste workspace e da edição com gate.

**Eu classifiquei errado de início** (chamei de injeção sob REGRA #20) e corrigi ao
verificar: a frase tem **0 ocorrências** no repo e em `~/.claude/` fora do changelog do
produto. A origem real é o **binário do Claude Code v2.1.245, offset 201747702**, montada
por 6 fragmentos e disparada por `permission mode ∈ {auto, bypassPermissions}` — e a
config de Gabriel usa `defaultMode: "auto"`, então acompanhava as duas.

Corrigir `cache/changelog.md` (a hipótese inicial) não teria efeito: é cache regenerável
que apenas *menciona* bypass mode.

**Correção escolhida por Gabriel**: patch do binário.
`~/.claude/tools/patch-claude-bash-nudge.py` substitui os fragmentos por espaços **do
mesmo comprimento em bytes** (o formato embute `<len:u32>` antes da string, então
preservar o tamanho preserva o layout). Validado **numa cópia antes do original**:
12 ocorrências neutralizadas · tamanho idêntico (391.948.592 B) · `--version`, `--help`
(242 linhas) e `mcp list` todos exit 0 · idempotente · backup em `claude.orig`.

Um update do mise desfaz o patch — por isso `--ensure` (compara um stamp: **423 ms**
varrendo vs **32 ms** com stamp) registrado no `SessionStart`. Cenário de update
simulado: binário restaurado → `--ensure` detectou e reaplicou.

**A precedência já era resolvida por afordância**, e isso ficou provado: com a instrução
do harness ativa mandando usar `grep`, o hook respondeu `permissionDecision: "deny"` e
devolveu a rota. O executor vence o texto — a tese D8.

### F3 — Suíte flaky por env global, com 22 testes expostos e vítima rotativa · **CORRIGIDO**

`turn_gate_kill_switch_desliga_a_fusao` faz `set_var("TOURING_T3_FUSE_DISABLED","1")`.
Env var é **global ao processo**, e `cargo test` roda threads paralelas no mesmo processo.

Diagnóstico por medição, não por leitura:

| condição | falhas |
|---|---|
| suíte completa, paralela | **1/6** |
| suíte completa, `--test-threads=1` | **0/6** |
| só os `turn_gate_*`, paralelos | **4/10** |
| o alvo sozinho | **0/10** |

O comentário do teste dizia *"a var só é lida por este gate"* — verdade e irrelevante: os
outros testes **chamam esse gate**. **22 testes** atravessavam o gate; só o autor sabia
disso, e a vítima que aparecia era acidental.

**Correção**: `#[serial(t3_env)]` nos 22 (o `serial_test` já era dev-dependency; a
primeira tentativa com `Mutex` manual foi substituída pela forma idiomática, que reusa o
guard existente). **0/15 execuções paralelas** após a correção.

**Uma hipótese minha foi refutada aqui, e vale registrar**: atribuí a falha à consistência
eventual do `moka` e adicionei `run_pending_tasks()` em 3 pontos. A medição deu **2/12** —
idêntico ao baseline de 1/6. **Revertido**, porque o código carregava um comentário
afirmando uma causalidade que a medição negava. Eu quase declarei vitória com um número
que não tinha melhorado.

### F4 — Órfão + duas escritas para a mesma contagem · **CORRIGIDO**

`code_mode_arm_counts()` (leitor dos 6 átomos) ficou sem chamador quando o P2b decidiu que
a política lê o **arquivo durável**, não a memória. Sua docstring seguia afirmando o uso
que deixou de existir.

Investigando o consumidor natural, apareceu o defeito maior: `bump_arm` gravava a vista
durável e `record_code_mode_arm_*` a volátil, em **call sites adjacentes**. Funcionava por
vizinhança, não por construção — um terceiro ponto de oferta que chamasse só um faria a
política aprender de um número que o operador não vê no `gate-metrics`. E a docstring de
`record_route_offer` **já proibia exatamente isso** para o campo `route`: *"escritor
ÚNICO… dois sítios gravando a mesma decisão"*. O princípio estava escrito e não aplicado
ao vizinho.

**Correção**: `bump_arm` virou o escritor único (grava as duas vistas), e o órfão foi
**integrado** como oráculo da invariante em
`bump_arm_mantem_duravel_e_volatil_em_sincronia` — que compara o delta volátil com o
durável e verifica que a economia (sem par volátil) não mexe no `gate-metrics`.

**A correção criou uma classe nova, e o guard a pegou**: com `bump_arm` tocando
contadores globais, seus testes entraram na família "medido por delta" — o teste de
sincronia mediu 4 contra 2 porque um irmão paralelo também bumpou. Regra `arm-counters`
adicionada; o guard **nomeou os dois testes** que faltavam marcar.

### F5 — Três guards existiam e nunca rodavam no CI · **CORRIGIDO**

O CI lista os testes um a um, então um guard novo não roda até ser registrado. Faltavam:

- **`test_code_mode_sdk_section.py`** — o guard D8 que `rules/touring-4-pillars.md`
  celebra como *"o remédio institucional… texto e executor reconciliados por teste, não
  por boa vontade"*. Existia, era citado como a garantia, **nunca executava**. É o
  anti-padrão D8 aplicado ao guard do D8.
- `test_cli_tests_isolate_project_root.py`
- `test_git_nudge_matches_executor.py` (criado nesta auditoria)

Cobertura: **4/7 → 7/7**.

### F6 — Dois guards não afirmavam nada sob pytest · **CORRIGIDO**

`test_git_nudge_matches_executor.py` e `test_cli_tests_isolate_project_root.py` só tinham
`main()`; sob pytest reportavam **"no tests ran"** — verde que não verificou nada.
Registrá-los no CI sem isso teria criado cobertura aparente. Entradas `test_*`
adicionadas, uma delas com prova por mutação embutida.

### F7 — O guard reportava a regra errada · **CORRIGIDO**

A mensagem de violação era fixa no texto da primeira regra: uma violação de
`health-delta` ou `t3-env` era anunciada como *"sem serial(gate_metrics)"*, mandando quem
lê o CI investigar o subsistema errado. Agora deriva da regra violada.

### F8 — O guard que escrevi tinha um buraco · **CORRIGIDO** (achado por mutação)

Minha primeira versão verificava `GIT_DESTRUCTIVE_OK=1` em **qualquer ponto** do arm.
Removi o token das `must` numa mutação e o guard **passou** — porque a mesma string
aparece no `reason`. Classe "verificador usa menos que o extrator". Corrigido para ler
dentro do bloco `must:`; as 3 mutações passaram a ser detectadas.

Sem a rodada de mutação eu teria entregue um guard decorativo.

### F9 — Memória minha desatualizada · **A CORRIGIR**

`block-git-guard-desativado` afirma *"kill switch em 0 desde 02/08"*. O executor real tem
`GIT_GUARD_ENABLED=1` com a v2 implementada. A memória foi escrita antes da v2 e induz
erro sobre o estado do guard.

### F10 — Complexidade em `code_mode_gates` · **REGISTRADO, NÃO CORRIGIDO**

`touring ast meta` reporta **CC=55** em `code_mode_gates` e **quality 0.36** no
`cli_suggester.rs` (4.718 linhas, 20 funções acima do limiar). É dívida real e
pré-existente, mas refatorar o orquestrador dos gates **durante** uma auditoria que usa
esses gates como instrumento trocaria um risco medido por um não medido. Fica nomeado
com número, para decisão de Gabriel.

## 4. FUSED RISK

| Risco | Estado |
|---|---|
| Nudge do produto mentindo sobre o executor | **fechado** por guard cruzado provado por mutação |
| Suíte não-determinística | **fechado**; 0/15, com guard estrutural em 4 famílias |
| Guard existente porém inerte | **fechado**; 7/7 no CI e 7/7 afirmando algo |
| Divergência entre vista durável e volátil | **fechado** por escritor único + oráculo |
| Instrução do harness contra a política do projeto | **fechado na origem**, com reaplicação automática |
| Complexidade do orquestrador de gates (CC=55) | **aberto**, nomeado, aguardando decisão |

## 5. ROOT-CAUSE

Um padrão explica F1, F5, F6 e F8, e é o mesmo do D8:

> **A garantia estava declarada e não executada.** Um nudge que afirma o que o executor
> não faz; um guard citado como remédio institucional que o CI nunca chama; um teste que
> é coletado e não afirma nada; um verificador que lê um escopo mais largo que o contrato
> que verifica.

Em todos, existia texto convincente e nenhum predicado rodando. O antídoto que funcionou
não foi mais texto: foi **executar** — mutação para provar que o guard reprova, payload
real contra o hook para provar que o gate age, contagem de string no binário deployado
para provar que o fonte virou comportamento.

Um segundo padrão, em F3 e F4: **o princípio certo aplicado a um vizinho e não ao
outro**. A docstring que proíbe dois escritores para o campo `route` convivia com dois
escritores para a contagem ao lado; a extração de `resolve_with_policy` para conter o
vazamento de `TOURING_CODE_MODE_ARM_ARMED` conviveu com o vazamento idêntico de
`TOURING_T3_FUSE_DISABLED`. Corrigir a instância não corrige a classe.

## 6. PROVENANCE

Comandos cuja saída sustenta este relatório:

```bash
touring daemon-ctl status                       # daemon vivo, PID 481672 → 666975 pós-deploy
python3 ~/.claude/skills/TACO-cross-audit/scripts/scan_debt.py crates/<c> --json
cargo clippy -p touring-cli -p touring-foundation -p touring-hook-handlers \
             -p touring-intelligence -p touring-server --all-targets -- -D warnings   # exit 0
cargo test -p touring-cli -p touring-foundation -p touring-hook-handlers \
           -p touring-intelligence --lib                                              # 2541 ok
cargo test -p touring-cli --lib          # ×15 paralelas → 0 falhas (baseline 1/6)
cargo test -p touring-cli --lib -- --test-threads=1                                   # 435 ok
python3 scripts/test_ceg_serial_gate_metrics.py                                       # 4 regras
python3 scripts/test_git_nudge_matches_executor.py                                    # exit 0
python3 scripts/sync-client-skills.py --check                                         # CLEAN 310
update-touring                                                                        # exit 0
echo '<payload>' | ~/.claude/hooks/touring-hook cli-suggest                           # 6 baterias
touring gate-metrics -j    # t3 0,0 → 1,1 após uma rajada induzida
touring kpi -j             # 5 checks code_mode; STUB honesto abaixo do piso
```

Estado vivo da evidência do braço ao fim da auditoria — coletada pelas próprias provas:

```json
{"code":{"offered":7,"followed":2,"economical":1},"both":{...0},"native":{...0}}
```

`STUB` nos KPIs de braço **não é defeito**: é a convenção documentada do próprio KPI
(*"return actual: null, status: STUB to keep the dashboard honest"*) para amostra abaixo
do piso `ARM_MIN_SAMPLE = 20`. Verifiquei antes de acusar.

### UNVERIFIED (declarado, nunca contado como aprovado)

- **Emissão de nudge em projeto temporário**: nenhum cluster é emitido para `cwd` em
  tmpdir, mesmo em casos que funcionam no projeto real. Não determinei se é gate de
  projeto conhecido ou falha-open do daemon. **Não afeta o caminho real** (provado no
  projeto), mas não foi explicado.
- **`git push --force` ao vivo**: o predicado é provado por teste unitário e o executor
  nega, mas a emissão do cluster ao vivo caiu em dedupe/G6 durante as tentativas.
- **`touring e2e -j`** não foi executado nesta rodada.
- O guard `block_git.sh` **negou meu próprio comando de teste** porque a string
  `git reset --hard` aparecia dentro de um payload JSON. Fail-safe conservador e correto
  na direção, mas é um falso positivo de superfície — não investigado a fundo.

## 7. ACTIONS

**Feito nesta auditoria** — 9 correções, todas com prova executada; 2 guards novos
(`test_git_nudge_matches_executor.py`, regra `arm-counters`); 2 regras adicionadas ao
guard serial; 3 guards registrados no CI; 2 guards que passaram a afirmar algo;
1 símbolo órfão integrado; 1 hipótese refutada e revertida.

**Aberto para decisão de Gabriel**:

1. **F10** — refatorar `code_mode_gates` (CC=55) e o `cli_suggester.rs` (quality 0.36).
   Precisa de janela própria: é o orquestrador que todos os gates atravessam.
2. **F9** — corrigir a memória `block-git-guard-desativado` (feito no fecho da sessão).
3. Investigar o silêncio do suggester em projeto temporário (UNVERIFIED acima).
4. Nada foi commitado. A árvore segue em `safety/2026-08-24-c2-w0-subcall-identity`.

---

_Cross-audit 2026-08-25 (noite) · 7 fases · evidência executada, sem veredito por
narrativa · auditor não-independente (declarado em §1)._
