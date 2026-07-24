# Auditoria de Efetividade — as infraestruturas de harness do Touring estão convergindo para Premium-Elite?

> **Data**: 2026-06-21 | **Autor**: TACO (Gabriel Gadea, comandante) | **Método**: empírico (CRC EXECUTE→OBSERVE→DIAGNOSE — rodei/li cada harness)
> **Veredito** `[FACT 1.0]`: **NÃO.** As infraestruturas têm a FORMA de elite (50 dims, tiers, BLOCK, CI, runtime-gate, meta-loop) mas as MEDIÇÕES estão divididas entre **teatro** (assume-PASS / proxy) e **bugs** (G10). Os badges "Diamond 0.97" são **parcialmente ilusórios** — assumem-pass ou usam proxy nas dimensões mais difíceis (testing, security, deps).

---

## 1. Universo de harnesses (6 infraestruturas)

| # | Harness | Forma | Papel |
|---|---|---|---|
| 1 | `touring-quality` (50-dim Rust) | 50 verifiers per-file/scope | granular |
| 2 | `touring-harness`/`touring-elite` (17/13-gate Rust) | composite de gates | change-governance |
| 3 | `elite_aggregate.py` (13-gate Python) | composite workspace | **gate de CI (Diamond)** |
| 4 | CEG X7 `harness_extension` (`harness_block_for_tool`) | gate runtime 17-dim | Edit/Write block |
| 5 | `touring-quality-block-all.sh` (hook PreToolUse) | 6 P0 dims do 50-dim | **gate runtime real** |
| 6 | `HarnessQuality` 6-dim (`harness_metric`) | executable·inspectable·stateful·governed·performant·evolving | saúde do meta-loop |

---

## 2. O teste de convergência (a evidência-chave) — mesmo código, 3 vereditos

| Harness | Alvo | Veredito |
|---|---|---|
| 50-dim | crate `touring-harness/src` | **0.660 BRONZE** (18/50 blockers) |
| 50-dim | crate `touring-quality/src` | **0.758 SILVER** (11 blockers) |
| 13-gate `elite_aggregate` | workspace | **0.9703 DIAMOND** |
| 17-gate `touring-elite` | (empty change) | **0.9774 DIAMOND** |

→ **Divergência ~0.3 (Bronze vs Diamond) no MESMO código.** Não medem a mesma coisa. **Não há fonte-da-verdade única. Não convergem.** `[FACT 1.0]`

---

## 3. Scorecard de efetividade (por que divergem)

| # | Harness | Medição real? | Efetividade | Evidência |
|---|---|---|---|---|
| 1 | 50-dim | **50 dims reais per-file** (única que mede tudo) MAS ~8 bugadas (G10) | **~0.55** | f1_8 pune imports, f4_3 invertido, f3_1 test-density, f1_1 keyword-count |
| 2 | 17-gate elite | **9 de 15 gates = "external CI step (assumed PASS)" = 1.00 ZERO medição** + 2 stubs | **~0.25** | output literal: `assumed PASS` em testing/docs/best_practices/scalability/extensibility/naming/navigability/craftsmanship/dependencies |
| 3 | 13-gate aggregate | **~8/13 reais** MAS testing+modularization=`file_size` proxy; security+deps=`None`→1.0 N/A constante | **~0.60** | `# proxy: file size = testability`; 2 gates 1.5-weight sempre 1.0 |
| 4 | CEG X7 17-dim | **NÃO wired** — `harness_block_for_tool` sem caller; `ceg_blocked_count=0` de 125 | **~0.05** | grep vazio nos hook handlers; CEG nunca bloqueou |
| 5 | runtime BLOCK hook | **real + wired + fail-open-loud** MAS f4_3 (1 dos 6 P0) invertido | **~0.65** | `touring-quality-block-all.sh` em settings.json |
| 6 | HarnessQuality 6-dim | **real + exposto** (`touring health`/`harness-metric`) — mede meta-loop, não código | **~0.80** | mede eixo ortogonal (saúde do sistema) |

**Convergência global: ~0.2.**

---

## 4. Os dois modos de falha

### 4.1 TEATRO (badges inflados — **gap novo G11**)
- **17-gate**: composite Diamond 0.9774 numa **mudança vazia** porque **9 gates assumem PASS**. Standalone é sem-sentido — só vale se alimentado via CEG X7 (que não está wired).
- **13-gate**: o eixo **testing é falsificado por file-size** (`05_testing=file_size_gate`); **security+deps são N/A constante 1.0** (peso 1.5 cada). Um auditor de mercado (SonarQube) **rejeita "Diamond" enquanto se falsifica coverage/testing**.

### 4.2 BUGS (medida-core errada — **gap G10**, já no plano)
- 50-dim é a mais honesta (mede 50 dims reais) mas ~8 verifiers medem errado → arrasta o score para Bronze/Silver com **false-blockers**. O Bronze não é "rigor", é parcialmente **bug**.

---

## 5. O que um auditor Premium-Elite de mercado rejeitaria

| Critério de mercado (SonarQube/CodeClimate/OpenSSF) | Estado Touring |
|---|---|
| Toda dimensão **medida**, não assumida | ❌ 17-gate assume-pass 9/15; 13-gate proxy/N-A em testing/security/deps |
| Thresholds ancorados (coverage≥80% real, dup≤3%, ratings worst-of) | ❌ coverage = test-density (sem llvm-cov) |
| Gate em **new code** (Clean-as-You-Code) | ❌ ausente (planejado P8) |
| **Uma** fonte-da-verdade | ❌ 3 composites divergentes |
| Acionável + enforçado | 🟡 runtime BLOCK real mas com dim bugada |

---

## 6. Prescrição

A auditoria dá ao IMPLEMENTATION-plan seu **WHY empírico** — não é polish opcional, é o caminho de teatro+bugs → efetividade genuína convergente:

1. **W0/P-1 (G10)** — consertar a medida-core dos ~8 verifiers (f1_8/f3_1/f4_3 🔴 primeiro). *Já no plano.*
2. **G11 (novo) — DES-TEATRALIZAR**: (a) 13-gate parar de usar file-size como proxy de testing/modularization → ligar ao 50-dim F3.x/F1.7; (b) security+deps deixar de ser N/A-1.0 → ligar ao 50-dim F2.5/F4.5 (que JÁ leem o manifest); (c) 17-gate parar de assume-PASS → projetar do 50-dim. **Até lá, o Diamond 0.9703 NÃO é uma alegação Premium-Elite crível.**
3. **Unificação (P9-P12)** — 50-dim vira a fonte; os outros 3 viram projeções/adapters → convergência por construção.
4. **CEG X7** — ou wirar o gate (alimentado pelo 50-dim) ou removê-lo (REGRA #0: dead code).

---

## 7. Calibração de confiança

| Afirmação | Nível |
|---|---|
| Divergência Bronze-vs-Diamond no mesmo código | **FACT [1.0]** (rodado) |
| 17-gate assume-PASS 9/15; 13-gate proxy testing + N/A security/deps | **FACT [1.0]** (output/código lido) |
| CEG X7 não-wired, ceg_blocked_count=0 | **FACT [1.0]** (grep + gate-metrics) |
| Scores de efetividade 0.05-0.80 por harness | **INFERENCE [0.85]** (julgamento sobre evidência factual) |
| "Auditor de mercado rejeitaria" | **INFERENCE [0.9]** (SonarQube Sonar-way exige medição real) |

---

_Auditoria empírica — os harnesses têm a forma de elite mas não a substância. A infraestrutura Premium-Elite genuína exige: medida-core correta (G10) + des-teatralização (G11) + fonte-da-verdade única (unificação P9-P12). Os badges Diamond atuais são parcialmente ilusórios._
