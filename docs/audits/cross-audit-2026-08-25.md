---
type: AuditReport
title: "Cross-audit — code mode opera de forma automática (sem prompt deliberado)"
description: "Auditoria cruzada de tudo que foi implementado no plano afordância code mode, com prova executada de que os gates disparam sobre trabalho normal"
tags: [cross-audit, code-mode, afordancia, e2e-proof]
timestamp: 2026-08-25T18:30:00-03:00
scope: /home/gabrielgadea/projects/touring
auditor: TACO-cross-audit (7 fases)
---

# Cross-audit — o code mode funciona de forma automática, provado na prática

**Pergunta da auditoria** (Gabriel): *prove, na prática, que o code mode funciona de
forma perfeita e automática, sem precisar que um prompt exija isso de forma deliberada.*

**Veredito: PROVADO — 11/11 provas ao vivo + 3.275 testes cargo verdes + 15 testes de
guards.** Todo gate disparou sobre comandos de trabalho normal, nenhum sobre incantação.

## FASE 1 — MAP

Superfície mapeada; **zero orphans nos símbolos novos** (todos têm consumers reais):

| símbolo novo | refs | consumers |
|---|---|---|
| `turn_gate_pre_bash` | 14 | cli_suggester + tests |
| `turn_gate_close_for_payload` | 2 | cli_suggester + hook_registry (dispatch daemon) |
| `record_t3_turn_fused`/`_first_passed` | 2+2 | cli_suggester + foundation |
| `READ_ONLY_BINARIES` | 6 | builtins + exec_tests |
| `code_mode_presentation` | 10 | cli_suggester + tests |

## FASE 2 — PURPOSE AUDIT (propósito vs comportamento executado)

| componente | propósito | prova executada | ✓ |
|---|---|---|---|
| presentation | resolve prefixo→env→alias→projeto→default | prefixo `native` relaxa projeto `code` ao vivo; env `both`/`native` declarados certos na seção | ✓ |
| scan_class_of | cat-redirect NÃO é inspeção | `cat > heredoc` passa; `cat arquivo` nega em code | ✓ |
| deny do modo code | rota derivada RODA | rota capturada do deny real e executada: **exit 0** | ✓ |
| G8 | laço de inspeção vira varredura | `for c in …; do grep` → `updatedInput` com `touring run` | ✓ |
| T3-B | 1ª intacta, K−1 fundidas | 2ª `sed -n` no mesmo turno → deny com programa derivado | ✓ |
| counters t3 | telemetria viva | `first_passed` 7→8, `fused` 3→4 (Δ+1/+1) ao vivo | ✓ |
| session hook | declara apresentação+origem 1×/sessão | 3 escopos (projeto/env both/env native) corretos | ✓ |

### Finding F-1 (D8) — CORRIGIDO nesta auditoria

O texto do deny do modo code e a seção de sessão diziam *"`ls`/`wc`/`sed-n` passam:
chamada única domina"* — mas o **T3-B os nega na 2ª do turno** (provado ao vivo:
`wc -c` fundida). O prompt prometia o que o executor não aplica — a lição D8
(`rules/touring-4-pillars.md`). **Fix**: os 3 textos agora dizem *"ISOLADAS passam;
em RAJADA DE TURNO, o T3-B funde qualquer classe de inspeção, estas incluídas"*.
Validado: 12/12 testes D8 + 406/406 cli + deploy.

## FASE 3 — DEBT SCAN

`scan_debt.py` em todos os arquivos tocados (cli_suggester, gate_metrics ×2,
hook_registry, builtins): **0 findings** — zero dead code, zero `allow(unused)`,
zero TODO/FIXME novo.

## FASE 4 — HARMONY CHECK

| verificação | resultado |
|---|---|
| wiring orphans | 2.358 = baseline (zero novos) |
| ciclos de dependência | 0 envolvendo arquivos de produção tocados |
| F2.1 OWASP | **Pass 1.00** |
| F2.4 Secrets | Warn 0.5 — FP: keyword `TOKEN` na constante pública `GATE_BYPASS_TOKEN` (não-Fail) |
| F2.6 Config | **Pass 1.00** |
| F4.3 Deprecated | **Pass 1.00** |
| F2.5/F4.5 | N/A (não-Cargo target) |
| Gold floor | cli_suggester **0.862** (≥0.80) · builtins **0.962** Diamond |

## FASE 5 — FIX & POTENTIALIZE

Aplicado o fix F-1 (acima). REGRA #0 respeitada: o fix **expandiu** a verdade dos
textos (não suprimiu o comportamento do T3-B — a fusão de classes não-colapsadas em
rajada de turno é o desenho correto; o que mentia era o texto).

## FASE 6 — E2E PROOF (executada, não escrita)

### Bateria ao vivo — o AUTOMÁTICO (11/11)

Todos os comandos são trabalho normal; **nenhum pede code mode**:

1. grep isolado de trabalho → **deny automático** (política do projeto) com rota derivada ✓
2. a rota do deny **executa exit 0** com resultado real ✓
3. laço `for` de greps → **G8 reescreve** automaticamente ✓
4. duas inspeções no mesmo turno → 1ª intacta, **2ª fundida pelo T3-B** ✓
5. `ls` isolado → **passa** (a calibração protege o caso comum) ✓
6. `cat >` heredoc → **passa** (o fix da calibração) ✓
7. prefixo `TOURING_CODE_MODE=native` → **relaxa** sem config nem pedido ✓

### Regressão cargo — 3.275 testes verdes

| suíte | resultado |
|---|---|
| touring-cli | **406/406** |
| touring-foundation | **484/484** |
| touring-dispatch | **1.321/1.321** |
| touring-ceg | **546/546** |
| touring-integration-tests | **23 suites ok, 0 failed** |
| guards pytest (serial + D8) | **15/15** |

## A nuance operacional (respondida ao Gabriel nesta sessão)

Uma sessão do projeto `analise` recebeu o block `[G-turno rajada-sem-programa]`
(24 Bash, 0 `touring run`, 7 colapsáveis reais: find×4 em 4s + grep×3). **Não é
evidência contra o code mode**: a sessão analise roda **binários de 24/08 15:00**
(toolchain pinada 30.4.14 + daemon per-project) — um dia antes de T3-B/escopo/fix
existirem. O G-turno (Stop hook) funcionou como rede de segurança e acertou o
diagnóstico. Remédio proposto (decisão do Gabriel): propagar via
`touring toolchain install --from-source` + `touring update --project analise`.

## Conclusão

O code mode opera **automaticamente**: a política `.touring/touring.toml` é lida sem
ação do usuário; a seção de sessão injeta o contrato 1× sem pedido; os gates (modo
code, G8, T3-B, G6, G1) disparam sobre trabalho normal com rotas que **executam de
verdade**; a calibração protege o caso comum (ls isolado passa); a escrita não é
confundida com inspeção (cat-redirect passa); e o relaxamento por-comando existe sem
config (prefixo native). Nada disso exigiu um prompt deliberado — a afordância mora
no executor, como o plano exigia.

Evidência bruta: `/tmp/e2e_final.txt` (11 provas), suites cargo acima,
`docs/plans/2026-08-25-code-mode-afordancia/phases/P2.3.md` + `P2.4.md` (calibração),
`docs/plans/2026-08-25-code-mode-afordancia/phases/P3.md` (T3-B).
