---
type: ADR
title: "ADR-003 — Superfície de skills e agentes guiada por uso medido, com a regra corrigida para itens raros"
description: "327 skills anunciadas e 7 usadas em 250 sessões; adotar poda por evidência, mas sem a regra do TokenEconomizer para agentes, que erraria 18 vezes."
timestamp: 2026-11-01T18:00:00-03:00
status: proposta
---

# ADR-003 — Superfície de skills e agentes guiada por uso medido

## Contexto

- **Medido (F4d, `medicao/f4d_prune.py`, 250 transcritos):**
  - skills: 327 anunciadas, **7 usadas ao menos uma vez**, 1,93 MB de `skill_listing` (~7,7 KB por
    sessão); a regra deles podaria 324 com 9 podas erradas;
  - agentes: 28 anunciados, 9 usados, 1,38 MB de `agent_listing_delta`; a regra podaria os 28 com
    **18 podas erradas** — agente é raro e necessário, e a regra p/m/s não distingue os dois;
  - tools adiadas: 265 anunciadas, 23 usadas; 243 podadas, 23 erradas.
  - Limite: uso de skill contado só pela tool `Skill` (um `/skill` digitado pode não virar
    `tool_use`) — o uso real está subestimado.
- **Nosso análogo (FATO 1.0):** a fachada MCP é curadoria ESTÁTICA (`crates/touring-server/src/server/mod.rs:73`,
  `CURATED_TOOLS`, descoberta sob demanda por `touring_search`); `client/hooks/skill_picker_hook.py`
  SUGERE skills por prompt (gate τ 0,55). Nenhum dos dois usa histórico de uso.
- **Externo (FATO 1.0):** `tokeneconomizer/plugin/fold.py:97-106` (`enough_evidence`: p=10 prompts em
  ≥ m=2 sessões, ou s=5 sessões; alto risco ×3) e a lista vai para `.claude/settings.local.json`
  (`denylist.py`). Ablação: poda sozinha −13,2% / −17,7% (Tabela 2a). Limite declarado (§6): "a pruned
  tool may become necessary".

## Opções

1. Instalar o plugin deles — escreve no settings (proibido no AB34; e fora dele, um segundo dono do
   settings ao lado dos nossos hooks).
2. **Relatório de uso da superfície no `touring kpi`** (anunciado × usado × bytes, por skill, plugin e
   agente) e a decisão de desligar plugin/skill fica com o Gabriel, item a item.
3. Poda automática própria, com a regra corrigida: agentes e itens de alto risco fora da poda (ou s
   muito maior), e recuperação barata (o item podado continua descobrível por busca).

## Decisão recomendada

**Opção 2 agora, opção 3 só se o relatório confirmar** (confiança 0,7). O número que importa é o
custo real: o `skill_listing` vai para o prefixo cacheado da sessão, então bytes ≠ custo. Antes de
podar, medir o custo em tokens com cache (ADR-005).

## Reversibilidade

Opção 2: total (só leitura). Opção 3: alta se o item podado seguir descobrível.

## Como medir

BACKLOG B3: bytes de anúncio por sessão e taxa de "precisei e não estava" (falso positivo), com teto
declarado antes.
