---
type: ADR
title: "ADR-007 — Não instalar o plugin TokenEconomizer; adotar as ideias com implementação própria"
description: "O plugin escreve no settings.local.json, faz side-calls de modelo e duplica mecanismos que já temos; APROVEITAR fica restrito a ideias e constantes, com a licença Apache-2.0 citada."
timestamp: 2026-11-01T18:00:00-03:00
status: proposta
---

# ADR-007 — Não instalar o plugin

## Contexto (FATO 1.0, lido em `~/.claude/touring/ext/TokenEconomizer@c53a4a6dcdeb`)

- Hooks: SessionStart, UserPromptSubmit, PostToolUse(+Failure), Stop, SessionEnd
  (`plugin/hooks/hooks.json`); estado em `~/.tokeneconomizer/`.
- A poda escreve uma deny list em `.claude/settings.local.json` (`tokeneconomizer/plugin/denylist.py`) —
  um segundo escritor do settings ao lado dos nossos hooks, e uma mudança de settings proibida no AB34.
- Side-calls com modelo `haiku` por padrão no plugin (`plugin/sidecall.py:164`), enquanto o paper usou
  o modelo do próprio agente — o número do paper não é o do plugin como distribuído.
- Micro-skills e RepoMap vêm DESLIGADOS por padrão (`config.py:148-151`, `:162`); o que liga sozinho
  é a poda.
- Avaliado só em SWE-Bench (277 tarefas, 3 seeds), streams de correções independentes, não sessões de
  usuário (limite declarado, §6).

## Decisão

**Não instalar** (confiança 0,9). Adotar as ideias pelos ADR-001 a 004 com implementação própria;
reaproveitar constantes como PONTO DE PARTIDA de calibração, nunca como valor final, citando a
fonte (Apache-2.0).

## Reversibilidade

Total. Reavaliar se a Microsoft publicar avaliação em sessões reais de usuário.
