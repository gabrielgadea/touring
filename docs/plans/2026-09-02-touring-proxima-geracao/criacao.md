---
type: Criacao
title: "Touring próxima geração — harness modular com reinjeção medida"
plan_id: 2026-09-02-touring-proxima-geracao
tags: [briah, criacao, harness, sistema-1-2, modularizacao, reinjecao, telemetria]
timestamp: 2026-09-02T09:25:00Z
dominio: harness
intensidade: I2
ratio_entrada: 0.286
---
# Touring próxima geração — a concepção (Briah)

> Rito Briah executivo (I2) em 02/09/2026. Ratio de entrada 0,286 (presentes: Telos, Imagem fraca).
> Gabriel enunciou a Emanação no espelho invertido; o rito cobriu Fronteira, Cadeia, Custo Invisível e Pronto.
> Modo profundo (I3) ofertado e recusado: vale para a criação inteira.

## A Emanação

O que eu quero é **evoluir o Touring à próxima geração de harness do mercado**: a intenção é
reproduzir, no par harness↔modelo, a relação entre os Sistemas 1 e 2 da mente humana. O harness é
o Sistema 1: processa contexto de forma imediata e constante e o reinjeta na janela do modelo (o
Sistema 2), como os hooks do Touring e do Claude Code já fazem. Quero que os efeitos dessa
reinjeção sobre o comportamento do agente sejam conhecidos, medidos em intensidade e potência,
e governados. Cinco estruturas para compreender, conceber, diagnosticar, planejar e evoluir sem fim:

1. **Telemetria padrão** de toda interação agente↔harness (Claude Code + Touring) e agente↔usuário.
2. **Telemetria estratégica**: pensada, com finalidade específica, que registra o evento certo no
   momento certo, trata, recombina, cruza e transforma a informação e a reinjeta na janela de contexto.
3. **Memória**: o que fica registrado e como, arquitetado pela finalidade e pelas restrições ambientais.
4. **Processamento**: cruzamento de dados, inteligência, aprendizado e registro do aprendizado.
5. **Reinjeção**: com registro, monitoramento e avaliação de efetividade.

Modelo de modularização: o DeepSeek Harness (dsh, clone em
`~/references/code-mode-2026-08-23/deepseek-harness`), onde "everything is a plugin" sobre uma
taxonomia tipada de eventos (Cordis) com quatro modos de despacho (waterfall, serial, parallel,
emit); o próprio loop do agente é um plugin substituível e o mapa feature→mecanismo é obrigação
de prova. Touring deve ter capacidade de modularização exponencial dessa natureza.

## O Telos

Para quê: para que o harness deixe de ser um conjunto de hooks ad hoc e vire uma plataforma em
que módulos de reinjeção entram, são medidos e evoluem sem tocar o núcleo. Para quem: primeiro
para o TACO operando o projeto **analise**, que precisa de módulos de processamento cujo conteúdo
reinjetado é o do próprio domínio — o pipeline integrado com o lexhub, sistemas de design,
elaboração e produção de documentos, relatórios, apresentações, artefatos e peças de GeoAI, e a
análise de conteúdo regulatório, legal, contratual e administrativo. Depois, para a comunidade que
adota o Touring como harness de próxima geração. O propósito de fundo é a finalidade
epistêmica: saber o que a reinjeção faz ao agente, e não apenas fazê-la.

## A Imagem

Quando pronto, o resultado final se parece com isto: uma sessão do Claude Code no projeto analise
em que o Touring carrega um módulo `analise-regulatorio` pelo mesmo contrato de qualquer outro
módulo. O módulo escuta os eventos do harness, consulta o lexhub e a memória facetada, transforma
o que encontrou e reinjeta um contexto denso e curto com um id de injeção. No turno seguinte, o
outcome da ferramenta é atribuído a esse id. `touring kpi -j` deve apresentar, por módulo, a
efetividade por injeção, o volume de bytes injetados por turno contra o teto declarado, e a lista
de injeções desligadas por evidência. O entregável é o Touring com um contrato de módulo em cinco
estágios (telemetria padrão → telemetria estratégica → memória → processamento/aprendizado →
reinjeção+medição), o registro de módulos, a régua por injeção, e a primeira filha viva no analise.
O repositório público deve ficar legível para quem chega de fora: um módulo de terceiro entra
lendo o contrato e o mapa feature→mecanismo.

## A Fronteira

Anti-goals declarados por Gabriel (o que esta criação NÃO é; fora do escopo):

1. **Não é reescrita do Touring.** A próxima geração nasce modular SOBRE os crates, hooks,
   memória e KPIs existentes. Big-bang fica fora do escopo. Não quero tocar o núcleo além do
   necessário para o contrato existir.
2. **Não é o módulo do analise em si.** O módulo regulatório/lexhub é a PRIMEIRA FILHA, a prova
   viva da modularidade, não a criação-mãe. Esta criação é a mãe.
3. **Não é telemetria sem finalidade.** Registrar tudo sem reinjeção medida não é o caso. Toda
   telemetria estratégica nasce com a própria régua de efetividade.
4. **Não substitui o Claude Code.** Touring é o Sistema 1 ao redor do modelo, não outro agente ou
   runtime. Não reimplementa o loop do CC; os limites do Touring são os hooks e o daemon.

## A Cadeia

Forma escolhida: **medição-primeiro** (a régua nasce antes do primeiro módulo). Encadeamento:

1. Se a telemetria PADRÃO registra cada evento agente↔harness↔usuário com identidade (sessão,
   turno, hook, bytes injetados) → então existe o corpus bruto que qualquer módulo consome.
2. Se cada reinjeção carrega um id e o outcome do turno seguinte é atribuído a esse id → então a
   efetividade é mensurável POR INJEÇÃO, nunca por sensação.
3. Se existe um contrato de módulo (registrar → tratar → reinjetar → medir) com interface única →
   então um módulo novo entra sem tocar o núcleo (modularização exponencial, à maneira do dsh).
4. Se contrato + régua existem → então o primeiro módulo (analise/lexhub/regulatório) nasce já medido.
5. Se a régua mostra efeito baixo → então o módulo é refinado ou desligado por evidência; o
   aprendizado fica registrado (laço Sistema 1/Sistema 2 fechado).
6. Se o laço fecha em um módulo → então o harness de próxima geração é o portfólio de módulos,
   cada um com KPI de efetividade.

Substrato já existente que a cadeia reaproveita (evolução, não folha em branco): trace de hooks
(`TOURING_HOOK_TRACE_FILE`), `gate-metrics`, `run_journal`; signal layers e `sdk_signal_mirror`
com KPI `hooks_complement`; memória facetada; `learning reward` e evolution; o laço real
`context_utility_bonus` (auditoria S-00 de 04/07/2026) que já correlaciona sucesso da tool com o
arquivo de contexto injetado; `pillar_induction_ratio` como primeira régua de nudge.

## O Custo Invisível

Pré-mortem (daqui a seis meses, a criação fracassou). A manchete que Gabriel teme de verdade:
**"Morreu de contexto"** — reinjeção demais saturou a janela, o agente piorou, STR negativo: o
Sistema 1 gritando por cima do Sistema 2. Os riscos secundários enumerados e não escolhidos
(morrer de telemetria sem consumidor, morrer de reinjeção sem régua, morrer de refactor) ficam
como trade-offs vigiados pela cadeia: a régua por injeção (elo 2) e o contrato (elo 3) existem
antes de qualquer módulo exatamente para que o que pode dar errado seja visto no número.
Antídoto do risco principal, a ser exigido em Yetzirah: teto de bytes injetados por turno
declarado e aplicado no executor (não no texto), e desligamento automático da injeção cuja régua
não mostra efeito. Alternativas descartadas no rito: módulo-primeiro (a filha puxa a mãe) e
contrato-primeiro (destilar o dsh antes de tudo); descartei ambas porque a medição precisa
existir antes para o "morreu de contexto" ser detectável.

## O Pronto

Critérios declarados por Gabriel, convertidos em medida (nunca sensação). O critério de aceitação
é o exit code de cada verificação; declaro pronto quando os três valem:

1. **Primeira filha viva no analise.** O módulo `analise-regulatorio` reinjeta em sessão REAL do
   projeto analise e `touring kpi -j` expõe, para esse módulo, n ≥ 30 injeções atribuídas e
   efeito medido acima do piso declarado no contrato do módulo. Métrica intermediária obrigatória
   (Cadeia, elo 2): o campo de efetividade por injeção existe e é consultável por comando.
2. **Repositório hypado no GitHub.** Medido por `gh repo view --json stargazerCount,forkCount`
   no repositório público do Touring. Números propostos (Gabriel calibra): ≥ 1.000 stars,
   ≥ 100 forks, ≥ 10 contribuidores externos com PR mesclado.
3. **Viralização mundial.** Medido por evento verificável, não por impressão: ≥ 3 módulos de
   terceiros publicados pelo contrato (a modularização saiu de casa), contribuidores de ≥ 5
   países, ≥ 10.000 instalações. Números propostos (Gabriel calibra).

Gates de processo em toda fase de Yetzirah: `loop_converged.py --task <id> --scope <path>`
exit 0, `cargo check` + clippy verdes, `touring e2e -j` sem regressão do baseline.

## O Prompt Perfeito

Evoluir o Touring à próxima geração de harness: um harness modular em que a reinjeção de contexto é medida por injeção, no modelo Sistema 1 (harness) / Sistema 2 (modelo). Contexto: o Touring já tem hooks (PreToolUse/PostToolUse via Claude Code), signal layers, sdk_signal_mirror, KPI hooks_complement, memória facetada, learning reward, trace de hooks e o laço context_utility_bonus (S-00, 04/07) que correlaciona sucesso da tool com contexto injetado; o modelo de modularização é o DeepSeek Harness em ~/references/code-mode-2026-08-23/deepseek-harness (everything is a plugin sobre taxonomia tipada de eventos Cordis com despacho waterfall/serial/parallel/emit, loop como plugin substituível, mapa feature→mecanismo como obrigação de prova). Contrato de saída: (1) um contrato de módulo em cinco estágios, telemetria padrão → telemetria estratégica → memória → processamento/aprendizado → reinjeção+medição, com interface única e registro de módulos, tal que um módulo novo entre com diff zero no núcleo; (2) toda reinjeção carrega um id e o outcome do turno seguinte é atribuído a esse id, expondo em touring kpi -j a efetividade por injeção, por módulo, com n mínimo declarado; (3) teto de bytes injetados por turno declarado e aplicado no executor, e desligamento por evidência da injeção sem efeito, porque o risco principal nomeado é morrer de contexto; (4) a primeira filha: módulo analise-regulatorio que consulta lexhub e memória facetada e reinjeta contexto do domínio regulatório/legal/contratual/administrativo e do pipeline de documentos, relatórios, apresentações e GeoAI do projeto analise, medido em sessão real com n ≥ 30 e efeito acima do piso. Ordem obrigatória: medição-primeiro, a régua antes do primeiro módulo. Exemplo de aceite: uma sessão no analise em que touring kpi -j mostra o módulo com n=30, efeito acima do piso, bytes/turno abaixo do teto, e uma injeção desligada por evidência. Critérios de aceitação: loop_converged.py exit 0 por fase, cargo check e clippy verdes, touring e2e -j sem regressão, contrato provado por diff zero no núcleo ao entrar o módulo. Anti-goals: não é reescrita do Touring (evolução sobre o existente), não é o módulo do analise em si (ele é a filha, esta é a mãe), não é telemetria sem finalidade (toda telemetria estratégica nasce com régua), não substitui o Claude Code (Touring é o Sistema 1 ao redor do modelo, hooks e daemon são os limites).
