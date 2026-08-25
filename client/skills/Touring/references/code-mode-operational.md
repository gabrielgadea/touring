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
| **Projeto em adoção inicial** | `both` (default) | G1 nega só a 4ª+ da mesma classe/180s com a rajada como programa; T3-B funde rajada de turno — o colapso chega sem quebrar o fluxo |
| **Projeto/equipe que não quer gates** | `native` | silêncio total: sem deny, sem nudge, sem fusão |
| **Uma chamada fora da regra** | prefixo `TOURING_CODE_MODE=<v>` | por-comando, sem config nem export (o shell e o hook são processos irmãos — export não atravessa) |
| **Inspeção de 1 arquivo conhecido** | nativa (`Read`) | chamada única domina — a recusa que o DeepSeek documentou (*"forcing every edit through a program taxes the common case"*) |
| **Inspeção ≥3 arquivos/fatos ou rajada** | `touring run` (Reflexo #8) | 1 varredura no sandbox, agregado ≤~200 tok no contexto (prática M5) |
| **Escrita/mutação (T4)** | nunca colapsa | 31% das chamadas medidas — taxar escrita taxa o caso comum |
| **Build/teste/veredito (T0)** | nunca colapsa | cargo/pytest produzem o veredito que o programa consumiria |
| **`ls`/`wc`/`sed-n` isoladas** | passam em qualquer modo | calibração: chamada única domina nessas classes |
| **`ls`/`wc`/`sed-n` em rajada de turno** | T3-B funde | qualquer classe de inspeção, estas incluídas |

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

### T3-B — fusão automática da rajada do turno (first-wins, fold-the-rest)

K≥2 Bash de classe fan-out **no mesmo turno** (sem PostToolUse intercalado — a assinatura
do batch paralelo do CC): a 1ª executa intacta; as K−1 são negadas com **um programa que
o modelo não escreveu** (comandos verbatim fundidos, orçamento 3000 chars, omissão
declarada). Janela 10s, ledger por (projeto, sessão), TTL 60s. **Qualquer** PostToolUse
fecha o turno (`post-tool-rl`, matcher `*`); um deny de G1/G6 também fecha (a resposta do
modelo a um deny é decisão nova, não batch). Ordem dos gates: **G6 → T3-B → G1**
(especificidade decrescente: byte-idêntico > turno > seriada).

Telemetria: `t3_turn_first_passed_count` / `t3_turn_fused_count` em
`touring gate-metrics -j` — é ela que decide se T3-A (hold-and-fuse, 25ms) se justifica.
Achado medido: o CC dispara hooks **serial-com-sobreposição**, não all-pres-first — a
captura do T3-B depende do timing de dispatch (a 1ª de cada mini-janela passa intacta).

### Kill switches (humano-only, REGRA #19)

| var | efeito |
|---|---|
| `TOURING_CODE_GATES_DISABLED=1` (env do daemon + restart) | desliga TODOS os gates code mode |
| `TOURING_T3_FUSE_DISABLED=1` | desliga só a fusão de turno |
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

## 5. Os guards (o que mantém a estratégia corrigida)

| guard | o que impede | prova |
|---|---|---|
| `scripts/test_ceg_serial_gate_metrics.py` (CI) | teste que toca contadores globais medidos por delta fora do grupo serial (2 regras: ceg-gateway, health-delta) | mutação 0→1→0 |
| `scripts/test_code_mode_sdk_section.py` | **guard D8 cruzado**: a declaração da seção (classes que colapsam) deve casar com o predicado do executor (`CODE_MODE_COLLAPSED_CLASSES` no Rust) — texto e executor reconciliados por teste | 12 testes |

## 6. Prova da afordância (o padrão de auditoria)

Cross-audit 2026-08-25 (`docs/audits/cross-audit-2026-08-25.md`): 11/11 provas ao vivo
**sem prompt deliberado** — grep de trabalho negado pela política do projeto, a rota do
deny **executada** (exit 0), laço reescrito pelo G8, 2ª do turno fundida pelo T3-B,
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
