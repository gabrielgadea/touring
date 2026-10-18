---
type: Brief
title: touring-4f (MiniMax) — concluir os corpora semeados TS/TSX/JS/HTML (VL0 §7)
plan_id: 2026-10-08-vgp-motor-base
timestamp: 2026-10-09T19:40:00-03:00
autor: touring-09 (coordenadora)
tags: ["#kind:plan", "#status:active", "#domain:harness"]
---

# Brief touring-4f — corpora semeados

Ordem do Gabriel (09/10): «prossiga com o mesmo, tanto delegando tarefas para agentes […] quanto para as sessões
kimi e minimax ativas em projects/touring». B = `/home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base`.
Contrato: `B/VL0-multilinguagem-spec.md` §4 (classes) e §7 (corpus e juiz por linguagem).

## Ponto de partida (leia antes de qualquer comando)

- `B/diagnostics/transferencia/analise-68-RELATORIO.md`: o estado deixado pela analise-68 (MiniMax do analise), o
  comando de retomada e os defeitos que os gates dela pegaram. Instrumentos dela: `…/transferencia/analise-68-instrumentos/`.
- Gerador: `B/scripts/corpus_semeado.py` (sha256 `0f913bc856934e33`), determinístico, semente 20261009.
- A analise-25 (outra MiniMax) também trabalhou no semeado; o relatório dela **não chegou**. Não dependa dele.

## Tarefas

1. **Inventário em disco (REGRA ZERO)**: por lang × classe, positivos e negativos em `~/.claude/touring/vgp/corpus_semeado/`
   contra o alvo; qualquer artefato de semeado no disco fora do `entradas.jsonl` (caminho + mtime). Ausente ≠ zero.
2. **HTML**: completar `ancora` ≥ 20, `cdn` ≥ 20, `cdn_pin` ≥ 20 e negativos ≥ 200 pelo comando de retomada do §3 do
   relatório da analise-68. Teto real da fonte abaixo do alvo → declarado com o denominador, nunca preenchido por suposição.
3. **JavaScript — decisão da coordenadora**: o oráculo passa a ser o **DELTA de diagnósticos** do `tsc --checkJs` do
   projeto (ou `deno check`): o mutado tem **exatamente um** diagnóstico novo, com o código esperado, na linha da
   mutação e nomeando o nome esperado, e o resto do conjunto é idêntico ao do original. Isso cumpre «acusa o mutado
   e não o original» para o erro semeado mesmo com a linha de base suja. Alvo ≥ 20 positivos por classe aplicável a
   JS (§4) e negativos ≥ 200 ou teto declarado. Selftest com mutante do oráculo (um oráculo que aceita QUALQUER
   diagnóstico novo precisa morrer).
4. **Determinismo (aceite «d»)**: `--confere-determinismo` em typescript e html no mínimo; o resultado é o sha de
   `entradas.jsonl` idêntico entre as duas gerações.
5. **Gates do script**: `--selftest` rc 0, `ruff check`, `ruff format --check`, `pyright` 0 erros; `classe_erro` do juiz
   (`B/scripts/juiz_motor_vgp.py`, só leitura) classifica 100% dos positivos novos (zero `nao_classificado`).

**Aceite**: VL0 §7 — todo positivo com oráculo independente do motor, ≥ 20 positivos por classe (ou teto declarado),
≥ 200 negativos por lang (ou teto declarado), determinismo medido, selftest com mutante por operador; esquema que o
juiz lê (`entradas.jsonl` + `snap/` + `estado_no_instante/manifesto.json`), 0 snap/`disk_file` ausente, ids únicos.

## Arquivos exclusivos

`B/scripts/corpus_semeado.py` · `~/.claude/touring/vgp/corpus_semeado/**` · `$HOME/.cache/vgp-semeado/` (sombra de
trabalho; mesmo device da home — `/tmp` é outro device e quebra o hardlink) · o seu relatório.

## Ambiente

- Rede (CDN) e escrita fora do workspace não existem no sandbox do `touring run`: rode nativo.
- `tsc`/geração pesada por `scripts/hw-slot -- python3 …` (até 2 por máquina). Máquina a 87 °C.

## Proibido

commit/push; `update-touring`/`propagate`; `kill`/`pkill` em processo touring (REGRA #19; o seu próprio `python3` por
pid é aceitável); `git stash`; `rm -rf`; `~/.claude/settings.json`; `crates/**`; `B/scripts/juiz_*.py`,
`shell_inline.py`, `corpus_v0d.py`, `corpus_v5_inline_neg.py`; o corpus `~/.claude/touring/vgp/corpus_v0d*`; instalar pacote.

## Relatório (um só, ao final ou num bloqueio)

`B/RELATORIO-corpus-semeado-touring-4f.md`, roteiro de 8 seções (identidade · feito com evidência · estado por item ·
processos · dados com sha · decisões/riscos · git · o que NÃO foi verificado) + mensagem curta à touring-09
apontando o arquivo. A entrega é hipótese até o juiz da coordenadora.
