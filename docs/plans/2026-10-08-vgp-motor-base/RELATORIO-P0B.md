---
type: Report
title: P0B — dunders de módulo, camada 2 só nega inexistência definitiva, MRO de classe comum (P0BMRO)
plan_id: 2026-10-08-vgp-motor-base
timestamp: 2026-10-17T15:40:00Z
autor: sessão efe418c2 (retomada; o agente original `p0-atributo` parou no limite de créditos em 10/10, com o VERDE vermelho)
tags: ["#kind:report", "#status:done", "#domain:vgp", "#lang:python"]
---

# RELATÓRIO P0B

Brief: [briefs/2026-10-10-p0b.md](briefs/2026-10-10-p0b.md). DAG: `P0B`, `P0BMRO`.

## 1. Identidade

- Worktree `~/projects/touring-wt-p0b` (destacado em c118aa52, espelho do int97 no momento do fork), sem commit.
- Código dos itens (a), (b) e (c): agente `p0-atributo`, 10/10. Em 10/10 21:28 o VERDE deu rc 101 com 2 falhas
  (`tests.rs:1857`, `:1882`) e nenhum mutante rodou. Conserto, aceite e este relatório: sessão efe418c2, 17/10.
- Snapshot antes do conserto: `safety/2026-10-17/p0b` (5511d594).

## 2. O que foi feito, com evidência

- **Causa das 2 falhas**: `func_sig` guarda TODOS os parâmetros do `def __init__`, inclusive o 1º (a instância); os
  testes esperam só os kwargs aceitos (`["a","b"]`, não `["self","a","b"]`). Além de sugerir `self` nas `reais`, isso
  deixava `C(self=1)` passar calado (TypeError «multiple values for argument» no CPython).
- **Conserto** (`lang/python.rs`): `sem_instancia(sig)` remove o 1º parâmetro e é aplicada nos DOIS sítios onde a
  assinatura vem de um `def __init__` — `construtor_de` (próprio) e `init_herdado` (MRO). O `__init__` gerado de
  dataclass já nasce sem ele. Teste novo `p0b_c_o_primeiro_parametro_do_init_e_a_instancia_e_nao_um_kwarg`
  (próprio + herdado, uma fonte por caso: o motor guarda um achado por (classe, nome)).
- Mutantes novos no `mut_p0b.py`: `C16-SELF-NO-INIT-PROPRIO`, `C17-SELF-NO-INIT-HERDADO`.

## 3. Estado por item do aceite

| aceite | resultado |
|---|---|
| 1. gates | fmt rc 0 · clippy `--workspace --all-targets -D warnings` rc 0 · touring-code 1.212/0 · pyright, hook-handlers (`pre-hooks,post-hooks`) rc 0 · server 1.810/0 |
| TDD | VERDE dos testes novos ok; VERMELHO (decisões revertidas) = 17 falhas |
| mutantes | **36/36 MORTOS** (A1–A7, C1–C17, B1–B10 + VERMELHO), 0 erro de compilação; cada um derruba os testes certos (C16: o novo + o controle `ComInit(a=1)`; C17: 3, inclusive o de identidade do MRO); fontes restauradas por `cmp` |
| 3. binário | **`4047373f`** (`~/.claude/touring/vgp/binarios/4047373f/`), **reproduzível** (touch nas 5 fontes → build 2 → `cmp` rc 0) |
| 2. juízes por id | ver §3.1 — **0 deny NOVO** nos 2 corpora |
| 4. patch | `diagnostics/p0b/p0b.patch` (sha 1cb86d59bce94fed), contra o int97, 7 arquivos, +767/−30; `git apply --check` limpo no int97 E no `touring-wt-adapt` |

### 3.1 Juízes (referência: INT97 final `8fd0efb0`, saídas em `diagnostics/int97/resultados-2026-10-17/`)

| corpus | juiz | R1 | deny novo / perdido | divergências por id (todas explicadas) |
|---|---|---|---|---|
| vivo | v2 | 1 | 0 / 0 | `2a78ac89aad8` perde o achado `__file__` → **(a)** atributo implícito de módulo |
| vivo | V3 | 3 | 0 / 1 | `2a78ac89aad8` (a); `d94f8090d150` deny→warn em `st_m_size` → **(b)**; `eae0c2b76058` allow→warn → timeout do pyright na rodada do INT97 (o P0B volta ao do 4501e1e5) |
| bak | v2 | 1 | 0 / 0 | `2a78ac89aad8` (a) |
| bak | V3 | 3 | 0 / 1 | (a); `d94f8090d150` (b); `76d4d4a88939` warn→allow → timeout do pyright |

**Recall — a perda declarada do item (b)**: `d94f8090d150` é um POSITIVO real (`'os.stat_result' object has no
attribute 'st_m_size'`, reproduz). O pyright o nega na camada 2; o item (b) rebaixa `reportAttributeAccessIssue` em objeto
que não é módulo a `warn` (tipo estático não é existência) para eliminar os falsos deny do P0M (`no.lineno`,
`n.value.args`). Continua sinalizado (warn), não negado. **Candidato a refinamento** (decisão do Gabriel, depois do R97):
tipo vindo do stdlib tipado sem `__getattr__` (o `os.stat_result`) É prova de inexistência e poderia manter o deny.

**p95 do v2 no bak (175,6 ms > 150)**: medido A/B alternado numa vaga exclusiva — 3 rodadas, 762 chamadas cada:
INT97 164,2 / 163,9 / 70,3 ms × P0B 66,2 / 168,4 / 129,8 ms; mediana por id 5,4 × 5,0 ms; delta mediano por
classe ≈ 0. **Não é regressão do P0B**: o teto de 150 ms está dentro do ruído desta máquina (o mesmo binário varia de
66 a 168 ms entre rodadas; o INT97 estoura em 2 de 3). Dados: `diagnostics/p0b/resultados-2026-10-17/{ab_p95,tempos}/`.

## 4. Processos

Tudo pelo `scripts/hw-slot`; juízes e A/B na classe RUST (exclusiva) para nenhum cargo contaminar o p95. Scripts:
`diagnostics/p0b/scripts/{pipeline_p0b.sh,ab_p95_v2.sh,rodar_juiz_tempos.py}` + os existentes (`gates_p0b.sh`,
`mut_p0b.{sh,py}`, `release_p0b.sh`, `juizes_p0b.sh`, `comparar_porid2.py`).

## 5. Dados com sha

`p0b.patch` 1cb86d59bce94fed · binário 4047373f · referência 8fd0efb0 · corpora vivo 28d93bcb, bak 7b819af2.

## 6. Decisões e riscos

- A integração no int97 (e no adapt) tem 1 conflito conhecido com o FRAG: a linha `use super::{…}` do `sombra.rs`
  (simulação em `diagnostics/int-adapt/simulacao_r97.txt`).
- Risco: o critério p95 ≤ 150 ms do v2 não discrimina nesta máquina — vale um juiz com p95 por mediana de N rodadas
  ou limite por percentil de pares, antes do J13.

## 7. Git

`touring-wt-p0b`: sem commit; difere do `safety/2026-10-17/p0b` só em `lang/python.rs` e `lang/python/tests.rs`.

## 8. O que NÃO foi verificado

- O J13 (held-out 20261013f) — é o juiz do binário integrado (INT97 + P0B + FRAG), não do P0B isolado.
- A sonda `diagnostics/p0-atributo-2026-10-09/scripts/sonda_init/` não foi re-rodada à parte (os testes cobrem as formas dela).
