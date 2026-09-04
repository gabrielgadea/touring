# Code Mode Operacional — estratégia de utilização por contexto (v1.0, 2026-08-25)

> **Load-on-demand** da skill `Touring` (pointer na SKILL.md — REGRA #13).
> Origem: plano `docs/plans/2026-08-25-code-mode-afordancia/` (CONVERGIDO,
> `loop_converged --rust-full` unmet:[]) + cross-audit `docs/audits/cross-audit-2026-08-25.md`
> (11/11 provas ao vivo do automático). Objetivo (Gabriel): **potencializar o code mode
> adequando a estratégia a cada contexto** — não um modo global, a apresentação certa
> por projeto, por tarefa, por chamada.

## 1. A decisão por contexto (o coração)

| Contexto | Apresentação | Por quê (medido) |
|---|---|---|
| **Projeto maduro / piloto** | `code` | touring: a calibração mediu 71% da inspeção atômica em grep/cat/find **em rajada** — o colapso por classe paga a si mesmo |
| **Projeto em adoção inicial** | `both` (default) | G1 nega só a 4ª+ da mesma classe/180s com a rajada como programa — o colapso chega sem quebrar o fluxo |
| **Projeto/equipe que não quer gates** | `native` | silêncio total: sem deny, sem nudge, sem fusão |
| **Uma chamada fora da regra** | prefixo `TOURING_CODE_MODE=<v>` | por-comando, sem config nem export (o shell e o hook são processos irmãos — export não atravessa) |
| **Inspeção de 1 arquivo conhecido** | nativa (`Read`) | chamada única domina — a recusa que o DeepSeek documentou (*"forcing every edit through a program taxes the common case"*) |
| **Inspeção ≥3 arquivos/fatos ou rajada** | `touring run` (Reflexo #8) | 1 varredura no sandbox, agregado ≤~200 tok no contexto (prática M5) |
| **Escrita/mutação (T4)** | nunca colapsa | 31% das chamadas medidas — taxar escrita taxa o caso comum |
| **Build/teste/veredito (T0)** | nunca colapsa | cargo/pytest produzem o veredito que o programa consumiria |
| **`ls`/`wc`/`sed-n` isoladas** | passam em qualquer modo | calibração: chamada única domina nessas classes |
| **`ls`/`wc`/`sed-n` em rajada (2ª em 300s)** | o predicado do S3 funde | TODA classe de inspeção responde ao mesmo limiar; isolada sempre passa |

## 2. Os mecanismos (o que executa a estratégia)

### Apresentação por escopo — `native | code | both`

Resolução (a mais externa vence): **prefixo do comando** (`TOURING_CODE_MODE=<v> cmd`) →
**env do hook** (`TOURING_CODE_MODE`) → **alias** (`TOURING_CODE_ONLY=1` = code) →
**projeto** (`.touring/touring.toml [code_mode] mode`) → **default `both`**.
A env lida é a do processo do hook (irmão do shell da Bash tool — `export` não chega;
a via que atravessa é a linha de comando, como `TOURING_GATE_OK=1`).

No modo `code`: inspeção `grep`/`cat`/`find` modelo-direta é **negada** com a rota
derivada do comando verbatim (a rota EXECUTA — provado 25/08, exit 0). `cat >`/`cat >>`
(escrita via heredoc) **não é inspeção** — o classificador olha o primeiro operando real
(fix da calibração: 27 escritas, 20,6% de erro antes do fix).

### Rajada de inspeção (S3, 27/08/2026) — a 1ª passa, a 2ª funde

Sob `mode = "code"`, a inspeção ISOLADA de qualquer classe passa intacta. O que
colapsa é a RAJADA: a 2ª chamada da MESMA classe (`grep`/`cat`/`find`/`ls`/`wc`/
`sed-n`) dentro de **300s** volta negada com as duas fundidas em um programa.
Um `touring run` na janela zera a contagem, e o deny zera o lote (um deny por
rajada, nunca fadiga).

Calibrado por medição em 115 transcripts: 77,5% do volume de inspeção está em
rajadas ≥2, e os 22,5% isolados deixaram de pagar pedágio. A lista fixa anterior
(`grep`/`cat`/`find`) errava nos dois sentidos — negava `find` (56% isolada, zero
rajadas ≥3) e isentava `sed-n`/`ls` (746 chamadas, ~76% em rajada).

> **O T3-B foi REMOVIDO no S10 (27/08/2026).** Ele prometia exatamente isto
> ("a 1ª executa intacta, as K−1 voltam fundidas"), mas pendurava a regra no
> fechamento de TURNO — e o PostToolUse fecha o turno entre cada chamada.
> Medido ao vivo com 3 classes distintas no mesmo turno:
> `t3_turn_first_passed = 3`, `t3_turn_fused = 0` — cada uma era "a primeira".
> Um caminho que executava, mantinha estado por sessão e nunca decidia. O
> predicado do S3 entrega a mesma promessa com uma janela de TEMPO.

### Kill switches (humano-only, REGRA #19)

| var | efeito |
|---|---|
| `TOURING_CODE_GATES_DISABLED=1` (env do daemon + restart) | desliga TODOS os gates code mode |

| `TOURING_G8_REWRITE_DISABLED=1` | desliga só o rewrite de laços |
| prefixo `TOURING_GATE_OK=1` | bypass por-comando de todos os gates (contado) |
| prefixo `TOURING_CODE_MODE=native` | bypass por-comando só deste modo |

## 3. A seção de sessão (1× por sessão, não por chamada)

`scripts/hooks/code_mode_sdk_section.py` (SessionStart) injeta: apresentação resolvida
+ origem + efeito + o SDK stub byte-estável (`touring run --sdk-stub`). Medido na sessão
de origem: 1.196 nudges por chamada ≈ 1,12 MB — a persuasão repetida é o modo caro; a
seção única (~2,3 KB, teto 4 KB) é o modo barato. Kill switch: `TOURING_SDK_SECTION_DISABLED=1`.

## 4. O efeito medido (honesto — sem reivindicação incondicional)

Simulação por braços sobre 1.578 chamadas Bash REAIS (`effect_p24.py`, 9 sessões,
payload real dos transcripts): native 2,30 MB · **both 3,68 MB (+60% — o nudge por
chamada do regime ANTIGO dominava)** · code 2,24 MB (−2,9%).

- O valor do modo `code` **não é primariamente bytes**: inspeção colapsável é 12% do
  payload de corpus misto. O valor é **N→1 de round-trips** (40+ chamadas a menos,
  140 programas fundidos) e a disciplina de agregação.
- O +60% do `both` foi medido no regime **pré-P1** (nudge exortativo por chamada,
  940 B × N). No regime pós-P1 (seção 1× + nudges que ENTREGAM o programa), o custo
  cai por construção — a medição dele é o A/B vivo pendente (P4 deixou pré-registrado).
- A reivindicação antiga de 84,8% foi **substituída** (memória
  `prova:code-mode-eficiencia-medida:2026-08-25`, `#status:corrected`) — media o caso
  ideal (inspeção pura N=8) e debitava injeções só de um braço.

## 4.5 A política do braço e a economia (P2/P5b, 2026-08-26)

A apresentação deixou de ser só configuração: existe uma **política** que pode escolher
`native|code|both`, atrás de `TOURING_CODE_MODE_ARM_ARMED` (**default-OFF**, decisão (b) de
Gabriel). Ela só age onde o humano **não** declarou — prefixo, env e `[code_mode] mode` vencem
sempre — e só com **dois** braços acima de 20 amostras e margem ≥ 0,05 entre eles. Sem isso, cala.

A evidência é **durável por projeto** em `<projeto>/.claude/touring/code_mode_arm.json`, não nos
contadores de `gate-metrics`: estes zeram a cada restart de daemon (medido: `t3_turn_first_passed`
2 → 0 em dois minutos), e uma política com piso de amostra nunca alcançaria o piso entre deploys.

Três eixos por braço, e a distinção importa:

| Eixo | Pergunta |
|---|---|
| `offered` | a apresentação entregou uma rota escrita |
| `followed` | o modelo **tomou** a rota — o **canal** |
| `economical` | a rota tomada **fundiu** round-trips (≥2 alvos ou ≥2 operações) — a **economia** |

`adoption_ratio` mede só o canal: `scan_class_of` não reconhece `touring run`, então N programas
diferentes e triviais somam N adoções e **zero avisos** de qualquer gate. Por isso o veredito da
rota vale **0,5** para a casca sobre uma chamada única e **1,0** para o programa que funde — e
0,5, não 0,0, porque sob `code` a chamada atômica é NEGADA: a casca é obrigatória e o custo é da
**apresentação**, não indisciplina do modelo.

Leitura: `touring kpi -j` → `touring.code_mode.{arm_native,arm_both,arm_code,economy_ratio}`,
todos lendo a MESMA fonte que a política lê. STUB abaixo do piso — amostra insuficiente é
desconhecido, nunca adesão nula.

## 5. Os guards (o que mantém a estratégia corrigida)

| guard | o que impede | prova |
|---|---|---|
| `scripts/test_ceg_serial_gate_metrics.py` (CI) | teste que toca contadores globais medidos por delta fora do grupo serial (2 regras: ceg-gateway, health-delta) | mutação 0→1→0 |
| `scripts/test_code_mode_sdk_section.py` | **guard D8 cruzado**: a declaração da seção (classes que colapsam) deve casar com o predicado do executor (`CODE_MODE_COLLAPSED_CLASSES` no Rust) — texto e executor reconciliados por teste | 12 testes |

## 5.1 Precedência sobre a sugestão genérica do harness (2026-08-26)

Em `bypassPermissions`, o **próprio Claude Code** injeta no contexto do PreToolUse uma
instrução genérica: *"Do your work through the Bash tool wherever it can accomplish the
job: read files with cat, head, or sed -n, search with grep and find, and make file
changes with sed, heredocs"*. Ela **contradiz** o code mode deste workspace e a edição
com gate. Três fatos, todos verificados por execução em 26/08:

1. **Não é injeção hostil nem drift nosso.** A frase tem 0 ocorrências no repo e em
   `~/.claude/` exceto o changelog do produto — vem do binário do Claude Code. Não há
   arquivo nosso a corrigir, e `cache/changelog.md` é cache regenerável que apenas
   *menciona* bypass mode (editá-lo não mudaria nada e seria desfeito no próximo update).
2. **A precedência já é a correta**: instruções do usuário (CLAUDE.md/constituição)
   vencem sugestões default do harness — o próprio `using-superpowers` declara isso.
3. **E a precedência é EXECUTADA, não confiada à leitura do modelo** — que é o ponto
   D8. Prova ao vivo (cross-audit 26/08, payload `session_id: "audit-d8-precedencia"`
   contra `~/.claude/hooks/touring-hook cli-suggest`, comando de trabalho
   `grep -rn CODE_MODE_COLLAPSED_CLASSES crates/ | head -5`): com a instrução do
   harness ativa mandando usar `grep`, o hook respondeu `permissionDecision: "deny"`
   e devolveu a rota `touring run --lang bash --code '<o mesmo grep>'`.

Ou seja: o conflito é resolvido por afordância. Nada a "corrigir" a montante — o
executor já ganha do texto, que é exatamente o que o produto existe para garantir.

## 6. Prova da afordância (o padrão de auditoria)

Cross-audit 2026-08-25 (`docs/audits/cross-audit-2026-08-25.md`): 11/11 provas ao vivo
**sem prompt deliberado** — grep de trabalho negado pela política do projeto, a rota do
deny **executada** (exit 0), laço reescrito pelo G8, 2ª da rajada fundida pelo S3,
`ls` isolado passando, `cat >` passando, prefixo native relaxando, counters Δ+1/+1.
A afordância mora no executor, não no anúncio (D8, `rules/touring-4-pillars.md`).

## Cross-references

| Tópico | Local |
|---|---|
| Decision matrix tarefa → comandos (C01-C12) | `~/.claude/rules/touring-decision-matrix.md` |
| 4 pilares + D8 (enforcement no executor) | `~/.claude/rules/touring-4-pillars.md` |
| CEG (sandbox, perfis, T0.1 READ_ONLY_BINARIES) | `references/code-execution-gateway.md` |
| Plano convergido + fases P0-P4 | `~/projects/touring/docs/plans/2026-08-25-code-mode-afordancia/` |
| Calibração das 5 classes (T0-T4) | o bundle: `calibration_p23.py` + `phases/P2.3.md` |

---

_v1.0 — 2026-08-25 | A estratégia por contexto: apresentação certa por projeto,
por tarefa, por chamada — medida, nunca assumida._

---

## Item 10 do CLAUDE.md — calibração e histórico do code mode neste workspace

> Removido do `CLAUDE.md` do projeto em 2026-09-02; preservado sem edição.

10. **Code mode ATIVO neste workspace (v30.4.14+, 25/08)**: `.touring/touring.toml`
    declara `[code_mode] mode = "code"` — este workspace é o **piloto** da apresentação
    por escopo. Efeito (**recalibrado no S3, 27/08**): inspeção **ISOLADA PASSA**, de
    qualquer classe — o que colapsa é a **RAJADA**: a 2ª chamada da mesma classe
    (`grep`/`cat`/`find`/`ls`/`wc`/`sed-n`) dentro de **300s** volta negada com as duas
    fundidas em 1 programa (a 1ª executou intacta). Medido em 115 transcripts: 77,5% do
    volume de inspeção está em rajadas ≥2; a lista fixa anterior cobrava 100% da fricção
    para capturar o mesmo, negando `find` (56% isolada) e isentando `sed-n`/`ls` (746
    chamadas, ~76% em rajada). `cat >` heredoc é escrita e passa. Um `touring run` zera
    a janela.
    Resolução: prefixo `TOURING_CODE_MODE=<v>` no comando → env do hook → alias
    `TOURING_CODE_ONLY=1` → `touring.toml` → default `both`. Relaxar por-comando:
    prefixo `TOURING_CODE_MODE=native` (exportar no shell NÃO chega ao hook — processos
    irmãos). Kill switch humano: `TOURING_CODE_GATES_DISABLED=1` (todos os gates).
    O `TOURING_T3_FUSE_DISABLED=1` saiu com o T3-B no S10 (27/08): o gate de turno
    media `t3_turn_fused = 0` em produção — o PostToolUse fecha o turno entre as
    chamadas, então ele nunca acumulava; o predicado de rajada do S3 o absorveu.
    **Efeito no handshake MCP (S1
    TRANSPORT, 27/08)**: um escopo que declara `code` passa a anunciar só a fachada
    search+execute — as tools MCP (nome de tool verificado no handshake, não é
    crate) `touring_search`/`touring_ctx_execute`/`touring_memory_recall`
    — em vez das ~23 curadas; antes a fachada existia mas exigia
    `TOURING_MCP_CODE_MODE=1` por sessão, então a declaração ficava desligada em
    silêncio. Só o ANÚNCIO estreita: todo tool escondido segue invocável por nome
    via `tools/call`. Precedência: `TOURING_MCP_ALL_TOOLS` > `TOURING_MCP_CODE_MODE`
    `=1` força / `=0` desliga > `touring.toml` > curada. O tipo e o parser da
    declaração são ÚNICOS (`touring_foundation::code_mode`), consumidos pelos dois
    executores — hook e handshake não podem divergir. Estratégia por contexto
    completa: `~/.claude/skills/Touring/references/code-mode-operational.md`. **Propagação
    (regra 2 ampliada)**: o rótulo da toolchain NÃO prova o build — 24/08 o lock dizia
    `30.4.14` rodando binário de 24/08. Após propagar, a prova é SEMPRE comportamental
    (um deny T3/modo-code ao vivo no projeto alvo), nunca por versão.
    **S5 (30.4.19–20, 29/08)**: o G10 também vê o **par write→run** — `cat > script.py`
    + `python3 script.py` do MESMO path (inclusive no MESMO tool_use multi-linha,
    a forma real) na janela de 600s, negado com
    `touring run --file <o próprio script>` (remédio 1:1). E dois furos fechados:
    `2>&1`/`2>/dev/null` e prefixo `VAR=...\n` não cegam mais a classe exec
    (origem: turno de 60 Bash do analise em que o replay negava só 2).
    **Aperto geral 30.4.21 (29/08, ordem de Gabriel)**: G7 re-inspeção advisory
    2ª/deny 3ª (era 3ª/5ª) · G1 rajada deny 3ª (era 4ª), hint na 2ª · G10
    exec-burst deny 5ª (era 10ª) · par write→run deny no 2º (era 3º) · G3
    edit-sem-read deny no 2º seguido (era 3º) · heredoc inline (`python3 -c`/
    `- <<EOF`) entrou na rajada como classe `python-inline` com remédio 1:1 (o
    corpo verbatim). Inalterados por medição/desenho: rajada de inspeção 2ª/300s
    (janela calibrada no joelho), G2/G8/G9 (1ª), G6 (2ª), G5 (telemetria).
    **Extensão 29/08 (ordem de Gabriel)**: python-inline READ-ONLY (corpo sem
    escrita/rede/subprocesso/DB — classificado pelo CORPO, `open(` com modo
    mutante detectado por posição do 2º arg) cai na PRÓPRIA rajada de inspeção
    (deny 2ª/300s, corpos fundidos em `--lang python`); escritor segue só no
    G10 — usado como leitura, o interpretador ganhava 4 passes onde `cat`
    ganha 1.

