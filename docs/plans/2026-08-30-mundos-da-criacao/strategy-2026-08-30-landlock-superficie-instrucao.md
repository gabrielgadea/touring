---
type: Strategy
title: "SEG-2 — superfície de instrução legível no sandbox"
description: "Estratégia aprovada em execução: grant read-only por subcaminho de ~/.claude/{skills,rules,agents,commands}+CLAUDE.md no sandbox_read_roots(), nunca ~/.claude inteiro; aceite por predicado positivo (rglob>0) na prova 5.5; release 30.4.28 propagada aos projetos pinados."
plan: /docs/plans/2026-08-30-mundos-da-criacao/
content_blake2b: e585d7c518ab75ee7ef8c234dcc1ad93
timestamp: 2026-08-30T23:22:05.192304-03:00
---

## PROBLEMA — o deny mudo

Landlock (SEG-1, 28/08) negava ~/.claude inteiro no sandbox do CEG. Caso real (peer analise-94, 30/08): pathlib.rglob engole PermissionError e devolve 0 com exists()=True — listar profiles/ da skill analysis-loop deu [] com o perfil de 15.824 bytes presente e em uso; só o controle positivo impediu a conclusão falsa. O gate de rajada empurra exatamente essa varredura para dentro do sandbox, então negar ali quebra o caminho sancionado.

## ESTRATÉGIA — grant cirúrgico, nunca o diretório inteiro

sandbox_read_roots() ganha 5 subcaminhos explícitos: ~/.claude/skills, ~/.claude/rules, ~/.claude/agents, ~/.claude/commands, ~/.claude/CLAUDE.md (path_beneath_rules ajusta o access do grant de arquivo por tipo — doc do crate landlock). O que CONTINUA negado: settings.json (env com chaves), .credentials.json, projects/ (transcripts), history.jsonl, hooks/. Alternativas descartadas: TOURING_SANDBOX_READ_WIDE=1 (reabre credenciais; fail-safe humano-only), espelho client/ como rota primária (drift e desvio da afordância natural), declarar leituras NATIVAS (colide com os gates G1/T3 que fundem rajadas para o sandbox).

## ENFORCEMENT E ACEITE

Teste estrutural the_instruction_surface_is_readable_but_claude_secrets_stay_out (positivo nos grants, negativo em 8 portadores de segredo). Prova comportamental prova_instrucao no gate 5.5 do propagate-release: head de SKILL.md com stdout não-vazio, ls de rules exit 0, cat settings.json NEGADO, e o predicado positivo no idioma que enganou — rglob('*.md') > 0. Aceite final: re-sonda da analise-94 no daemon per-project do analise (antes/depois com os mesmos controles).

## TENSÃO ABERTA (decisão de Gabriel)

G10 conta leituras nativas de alvo fora das read roots como rajada python-inline e nega na 5ª — antes do SEG-2, o caminho correto (ler fora do sandbox o que o sandbox não alcançava) era penalizado pelo gate que empurra para dentro. O SEG-2 remove o caso skills; a regra geral (isentar da rajada o comando cujo alvo está fora das read roots) fica registrada como candidata, não aplicada. ADENDO (30/08, retratação da fonte): a própria analise-94 corrigiu o dado — dos 8 usos de TOURING_GATE_OK dela, só os primeiros tinham base factual; após o SEG-2 o bypass foi comodidade, não bloqueio. O caso dela NÃO é evidência para afrouxar o G10; a candidata fica sem caso de suporte vivo.

## Cadeia causal

Elos derivados do que este documento já afirma — a cadeia existia na prosa e não
na forma que o próprio rito cobra (dogfood do `loop_doc_link_gate`, 04/09/2026).
Cada elo mantém `se … então` na MESMA linha: o `.` do regex do gate não casa
quebra de linha, então um elo que envolve não é contado.

1. **se** o Landlock nega `~/.claude` inteiro no sandbox (SEG-1, 28/08) **então**
   `pathlib.rglob` ali dentro engole `PermissionError` e devolve `0` — sem exceção.
2. **se** `rglob` devolve `0` com `exists()=True` **então** a varredura conclui
   "não há perfis" sobre um perfil de 15.824 bytes presente e em uso (caso real,
   peer analise-94, 30/08).
3. **se** o gate de rajada empurra essa varredura para o sandbox **então** negar
   ali quebra o caminho sancionado — o gate e a contenção passam a trabalhar um
   contra o outro.
4. **se** o grant fosse o diretório inteiro **então** `settings.json` (env com
   chaves), `.credentials.json`, `projects/` e `history.jsonl` voltariam a ser
   legíveis — a correção compraria a leitura ao preço do segredo.
5. **se** o grant for por subcaminho explícito **então** a superfície de
   instrução abre (`skills`, `rules`, `agents`, `commands`, `CLAUDE.md`) e os
   oito portadores de segredo continuam negados — o que o teste estrutural fixa
   nas duas direções.
6. **se** o aceite for "não deu erro" **então** o deny mudo passa despercebido;
   **se** for predicado POSITIVO (`rglob('*.md') > 0`) **então** só a leitura que
   de fato aconteceu satisfaz — o idioma que enganou vira o idioma que prova.
