# Brief 5 — D-8: população html (touring-c0, MiniMax)

Decisão do Gabriel (10/11/2026): «Aprovar a lista e clonar».

## Contexto
O adaptador html do VGP não tem prova estatística: a população html do acervo tem **1.704** arquivos e a prova
pede **n ≥ 3.700** (`docs/plans/2026-11-04-correcoes-aprofundamento/RETOMADA.md:212`). O held-out web tem 0
positivo natural de html (`docs/plans/2026-10-08-vgp-motor-base/PROPOSTA-positivos-html-heldout.md`). Plano e
contrato das entradas: `docs/plans/2026-10-08-vgp-motor-base/PLANO-ADJUDICADOR-HTML.md`; montador:
`docs/plans/2026-10-08-vgp-motor-base/scripts/monta_camada_html_oss.py` (BASE = `~/.claude/touring/vgp`).
Em 05/11 já houve clones preparados (`dom-examples`, `html5-boilerplate`) — procure-os em `~/.claude/touring/vgp`
antes de clonar de novo.

## Etapa 1 — lista (antes de qualquer download)
Escreva `docs/plans/2026-11-10-decisoes-pendentes/LISTA-HTML-OSS.md`: repositórios públicos com html escrito à
mão (sites, docs, exemplos; NÃO html gerado/minificado, NÃO vendored), com colunas: url, licença (só
permissivas: MIT, Apache-2.0, BSD, CC0, CC-BY, MPL), tamanho estimado, nº estimado de arquivos `.html`, commit
fixo (sha) para clonar. Meta: somar ≥ 2.000 arquivos html novos (acervo 1.704 + 2.000 ≥ 3.700) com folga.
Diversidade: nenhum repo com mais de 25% do total novo.

## Etapa 2 — clonar
- Destino: `~/.claude/touring/vgp/oss-html/<nome>` (NUNCA `/tmp` ou `~/.cache`: um hook nega, e o conteúdo
  sumiria). `git clone --filter=blob:none` + checkout do sha fixo.
- Teto: 5 GB no total; `df -h ~` antes (há ~126 GB livres, e o disco já encheu num release — pare se cair
  abaixo de 100 GB).
- Contar os `.html` efetivos por repo com um script (não estimar).

## Arquivos exclusivos seus
`LISTA-HTML-OSS.md`, `RELATORIO-5.md` (nesta pasta) e `~/.claude/touring/vgp/oss-html/`. Não edite os scripts
do bundle VGP nem o montador — integrar a camada ao montador é o passo seguinte, desta sessão.

## Aceite (o juiz desta sessão confere)
- Lista com licença verificada no próprio repo (arquivo LICENSE lido), sha fixo e contagem medida.
- Soma medida de html novos ≥ 2.000, maior repo ≤ 25%.
- Tamanho em disco medido (`du -sh`).
- Um único relatório ao terminar, via SendMessage para `touring-b4`.
