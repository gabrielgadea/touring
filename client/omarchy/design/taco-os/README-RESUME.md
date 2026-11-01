# TACO OS — design da interface unificada (retomada)

> Estado salvo em 23/08/2026 22:10 (sessão c8861ab0). Estes são os FONTES do
> design canvas; o artifact publicado é gerado a partir deles.

## O que é

Design da interface **TACO OS** — plugin/widget do Omarchy que unifica o
notebook inteiro: Claude Code, Touring, Omarchy, projetos, workflows.
Base visual: **tema atual da máquina (Retro 82** — navy `#00172e`, âmbar
`#faa968`, teals) + referências HUD orbital de `~/Downloads`
(`e44c7f90…jpg`, `videoframe_18823.png`). Tipografia: Michroma + Space Mono.

## Artifact publicado (design canvas editável)

**https://claude.ai/code/artifact/51b9a8a2-d391-46f5-a857-c5665e751694**

Página 1 "TACO OS": `Main` (núcleo orbital + satélites-projeto com DAGs reais +
dock de 9 módulos) · `BarWidget` (widget na barra) · `GeoDeck` (kazuba-geo-engine:
HDM-4/IRI, Dijkstra, R-Tree, Earth Engine, KML/KMZ) · `MissionDeck` (DAGs por
projeto, claim, Wayfinder) · `ForgeDeck` (documentos ANTT, **LexHub** 269
aditivos, conteúdo). Página 2 "Direções": alternativas Lattice e Monolith.

## Como retomar (numa sessão Claude Code)

1. Editar os `.dc.html`/`canvas.json` deste diretório (formato Design Components).
2. Re-seed + republicar NA MESMA URL: invocar `/design` e pedir para atualizar o
   canvas existente a partir destes fontes (o helper `seed-canvas.mjs` da skill
   monta o HTML; publicar com a URL acima para manter o link).
   - Se o canvas foi editado no navegador depois desta data, LER o artifact
     primeiro (`--extract`) e partir do extraído, não destes arquivos.

## Decisões tomadas

- Direção líder: **núcleo orbital** (reactor Touring + projetos-satélite) — o
  Stack Elevator anterior evoluiu para isso após as referências de downloads.
- Base visual = tema ATUAL do Omarchy (ordem de Gabriel: "nada aprovado antes;
  do zero, com o tema atual"); o design deve acompanhar troca de tema.
- Todos os dados exibidos são REAIS (DAG ids, contagens, skills, LexHub).

## Próximos passos possíveis

- Gabriel escolher um deck para protótipo clicável completo.
- Transpor o design para o plugin real (`client/omarchy/skills-deck` →
  `Panel.qml`/`BarWidget.qml` Quickshell) ou para o command centre
  (`client/omarchy/bin/cc_build.py`, que hoje usa o visual RUBRIC).
