---
type: Report
title: ECC × Touring — comparativo por funcionalidade
timestamp: 2026-10-17T19:45:00Z
pedido: "ainda sobre a exploração do ecc, quero uma analise comparativa detalhada por funcionalidade com o touring, agregando tudo que já foi explorado + tudo que ainda falta"
tags: ["#kind:report", "#domain:harness", "#process:research", "#status:active"]
---

# ECC × Touring — comparativo por funcionalidade

Este texto complementa o `RELATORIO.md`, que traz os defeitos C1–C12, as inspirações I1–I21 e as anti-lições. Aqui o
recorte é por funcionalidade: o que cada lado faz, quem faz melhor e com que evidência. A tabela linha a linha (156
linhas) está em `COMPARATIVO-matriz.md`, gerada por `python3 -I scripts/gera_matriz.py achados COMPARATIVO-matriz.md`.
Este documento é a leitura dessa tabela.

Marcação: **[V]** indica fato reverificado por mim nesta sessão, por execução. **[A]** indica afirmação de agente com
evidência citada (arquivo:linha ou comando). As referências do tipo `G4:3` apontam para a linha 3 do arquivo
`achados/G4-*.json`. Os ids `A1`…`E11` são da primeira rodada.

## 1. Como ler o placar

| Rodada | Linhas | Resultado | Viés |
|---|---|---|---|
| Lacunas (A–E, 5 agentes: hooks, memória, contexto e custo, orquestração, segurança) | 51 | ECC melhor 36 · só ECC 12 · Touring melhor 2 · a verificar 1 | Os agentes procuraram lacunas do Touring por desenho, e o estado «parcial» do Touring entra como «ECC melhor» |
| Comparativa (G1–G5: catálogo e inteligência de código, operação, multi-harness, workflows e CI, documentação e domínios) | 105 | Touring melhor 41 · ECC melhor 31 · equivalente 19 · só Touring 9 · só ECC 5 | Cada linha descreve os dois lados com evidência |
| Soma | 156 | ECC melhor 67 · Touring melhor 43 · equivalente 19 · só ECC 17 · só Touring 9 · a verificar 1 | Mistura duas perguntas diferentes; não serve como placar |

A leitura que os dados sustentam: **o ECC é mais largo e o Touring é mais fundo.** O ECC vence quando o assunto é
superfície (catálogo, número de harnesses, instalação seletiva, docs) e no método de avaliar os próprios gates. O
Touring vence quando o assunto é mecanismo executado (índice de código, sandbox, juiz, claim, roteamento, release). Os
defeitos reais que a comparação revelou no Touring se concentram em hooks e segurança (§4).

## 2. Placar por domínio

| Domínio | Linhas | Quem vence | O que decide | Ação para o Touring |
|---|---|---|---|---|
| Instalação e distribuição | 8 | Touring no núcleo, ECC nas bordas | O Touring tem toolchain imutável com lock e rollback, e o `propagate-release.sh` tem gates (G3:1, G5:1). O ECC instala por módulo e perfil com custo declarado, desinstala pelo install-state e testa a instalação do artefato exato em 3 SO (G3:2, G3:3, G3:12, G4:15) | Manifesto por projeto para desinstalar sem resto; teste de `update-touring` num HOME vazio; modo de inspeção com custo antes do `component add` |
| Multi-harness | 7 | ECC | São 17 harnesses, com fonte única, cópias geradas e matriz de paridade (G3:6, G3:7). O Touring é o harness do Claude Code. As sessões irmãs são outro eixo, que o ECC não tem (G3:11) | Não portar. Adotar só a matriz de paridade honesta (hook-backed × instruction-backed) para Anthropic × Kimi × MiniMax (I18) e o gate de conteúdo para skill distribuída: `curl \| sh`, segredo, caminho absoluto (G3:5) |
| Catálogo (agentes, skills, commands, regras) | 12 | ECC em amplitude, com volume como anti-lição | O ECC tem 71 agents, 311 skills e 97 commands; o Touring, 11, 187 e 25. O próprio ECC chama o catálogo de «a liability as much as an asset» (G5:3), e o registro gerado dele não pegou o drift entre dois docs escritos à mão (G4:17) | Só o que serve ao workspace: reviewers de Rust e Python, um build-resolver de Rust que siga o `update-touring`, e checklists de falha silenciosa (`Result` descartado, `unwrap_or` que esconde erro) e de revisão de segurança (G1:1-6, G1:19). O `hookify` só se compilar para o gate Rust e entrar em sombra (G4:18) |
| Hooks e enforcement | 11 | Misto: o Touring no executor, o ECC na cobertura das formas | O Touring tem handler em Rust, deny em JSON, prova de disparo por trace e gates por rajada (G5:6, G1:7). O ECC tem parser recursivo fail-closed, guarda de `--no-verify`, proteção de config, direção da falha por classe de hook e ID estável por hook (A1-A5, A8) | É aqui que estão os defeitos C1, C2 e C4 e a decisão D2 (§4) |
| Segurança | 10 | Misto | O Touring tem sandbox no kernel (CEG com Landlock, seccomp e rlimit), risco por tool call que bloqueia e conta (G2:9), e `redact_secrets`. O ECC tem linter dos próprios workflows (G4:14), gate de release por evidência da API (E2), scan de Unicode invisível (E3) e sanitizador pré-publicação (E10) | Linter de workflow e scan de Unicode no CI, ambos baratos; sanitizador antes de publicar, que teria achado os 172 arquivos de estado versionados |
| Memória e aprendizado | 14 | O Touring no mecanismo, o ECC no ciclo de vida | O Touring tem recall RRF sobre memória, regra e código, laço de reward e facetas (G1:10-11, G5:8). O ECC tem TTL e prune, promoção de projeto para global com confirmação humana, proveniência, gatilho observável e fronteira «unreviewed» (B2, B4, B9, B1, G3:9). O decaimento e a confiança do ECC são prosa para o LLM, e a votação adversarial refutou a semântica fixa deles | Sanitizar na escrita e no recall (C6); `memory prune --dry-run`; decaimento por limite de Wilson sobre reward medido; `#source` e `#author` automáticos (I9, I14, I15) |
| Contexto, tokens e custo | 15 | O ECC na apresentação, o Touring na medida | O Touring mede o recurso escasso (a franquia, em `rate_limits.json`) e o próprio orçamento de injeção (G2:6, G2:16). Mas o KPI `context_budget` está em FAIL: 12.109 B por turno contra teto de 2.000, com 82,6% na família «other» **[V]** (C10). O ECC tem injeção tipada na emissão, aviso por tier, cota na statusline e relatório de custo legível (C1, C2, G2:5, G1:12) | Carimbar `family` e `dedupe_key` na emissão (C10, em curso). Levar 5h/7d à statusline: o `status_monitor.sh` grava o `rate_limits.json`, mas não mostra a cota **[V]**. Aviso por tier (I7) |
| Roteamento de modelo | 4 | Touring | O gate mora no executor (`agent_model_gate.py`), com KPI e consentimento do Fable; o ECC tem variável global e frontmatter (C9, G1:13, G5:10). Fraqueza própria conhecida: o gate exige só a presença de `model`, e o haiku teve 0,1% dos tokens (memória `roteamento-de-modelo-so-exige-presenca`) | Escada de escalonamento (I20). Avisar quando `CLAUDE_CODE_SUBAGENT_MODEL` estiver definida, porque ela cala as regras de adequação |
| Orquestração e paralelismo | 14 | Touring | O ADW tem journal com fsync e `--resume-run`, e gates por código; há claim atômico com lease, `hw-slot`, sessões irmãs e CEG (G1:8, G1:26, G2:10, G4:3, G4:21). Os `orch-*` do ECC são prompts com gate humano, e o deconflito TCAS é só desenho (G2:10) | Nó `repro_test` no ADW `bugfix`: o teste novo deve falhar antes do fix (G4:3). Aceite rodável no cartão do `decompose` (I5) |
| Planejamento | 5 | Touring, com uma ideia do ECC | O Touring tem `taco-planning` e DAG com `frontier` e névoa. O `/plan` do ECC captura um exemplo de padrão do código por categoria antes de planejar (G4:1) | Seção «Padrões a espelhar» (categoria, arquivo:linha) no Pln2. Invariante de teste do `factory route`: só emitir comando que carregue o escopo do passo (G4:2) |
| Verificação e qualidade | 16 | O Touring no juiz, o ECC no método de avaliar o gate | O Touring tem juiz por exit code com `judge_attest`, `tdd-enforcer` que bloqueia, 17.113 `#[test]`, proptest, fuzz e pré-registro (G4:8-11, G4:22). O ECC tem sonda de aderência em 3 níveis de prompt, ablação pré-registrada, pré-voo do juiz com decoy, corpus rotulado de cenários e review fail-closed (D1-D3, A6, D9). A única medição de efeito do ECC deu nula (p = 0,210) | I1-I4 e I13. Lacunas internas em §4: mutação declarada sem execução e nenhum piso de cobertura no CI |
| Observabilidade e operação | 15 | Misto | O Touring tem o painel ATZILUTH na abertura da sessão, diário por agente, KPI com piso e `doctor`/`e2e` (G2:12, G2:14, G2:17). O ECC tem uma linha de decisão (`readiness` + `attentionCount`), detector de sessão travada e notificação externa (G2:1, G2:3, G2:8). O Touring emite 4 números de saúde sem reconciliação: 0,4939, 0,859, 0,536 e 26/46 (G2:2) | Agregador `attention_count` com exit 2; detector de sessão travada (I11); política de retenção dos journals (I19) |
| Inteligência de código | 7 | Touring (diferencial central) | Índice de símbolos, blast, wiring, `explore --until-dry` e scout com VGP (G1:16, G1:18, G4:23, G5:15). O `codemaps` do ECC é um mapa de pastas | Só o formato `.tour`, como saída de `touring map` para quem chega ao código (G1:17, baixa prioridade) |
| Governança do repositório | 8 | O Touring no release, o ECC no vigia agendado | O release do Touring tem verify, SBOM, cosign keyless e atestado SLSA, e o template de PR exige evidência (G4:15, G4:16). O ECC tem cron de supply chain a cada 6 h e manutenção semanal (G4:13), além de ADR numerado (G1:21). Nenhum workflow do Touring tem `schedule:` **[V]** | Cron semanal de `cargo deny check advisories`. Correção ao G5:16: o repo já tem `SECURITY.md` e `CODE_OF_CONDUCT.md` **[V]** |
| Documentação | 4 | Equivalente, em eixos diferentes | O Touring checa a verdade (a doc não cita crate inexistente). O ECC checa a estrutura: link que escapa do projeto, symlink para fora, ADR superado sem sucessor (G4:12) | Auditoria de links e symlinks em `docs/plans`; atualizar a seção manual do guia de instalação, datada de 23/05 (G3:16). Tradução sem gate de sincronia é anti-lição (G5:17) |
| Domínios fora de código | 6 | Nenhum (eixos disjuntos) | O ECC cobre marketing, saúde, conteúdo e pagamento entre agentes. O Touring cobre o domínio jurídico-administrativo brasileiro (G1:25, G5:18) | Ignorar, nos dois sentidos |

## 3. Só Touring ou Touring superior (manter, não trocar)

| Capacidade | O que faz | Evidência | Contraparte no ECC |
|---|---|---|---|
| Inteligência de código | Índice de símbolos, blast radius, wiring e órfãos em menos de 10 ms | `touring index find`, `ast blast`, `wiring` (G5:15) | Mapa de pastas (G1:16) |
| Sandbox no kernel | CEG X0..X9 com Landlock, seccomp, rlimit e tmp privado por run | `touring run` nesta sessão: `landlock: KernelEnforced` **[V]** | Snippet de docker-compose |
| ADW durável | Workflows declarativos com journal fsync, resume, gates com `verdict_contract` e `NEW_FINDINGS` contado pelo runner | `touring adw explain bugfix` (8 nós); 23 specs (G4:3) | `orch-*` como prompts; `autonomous-loops` marcada para remoção desde a v1.8.0 e ainda viva na 2.2.3 |
| Claim atômico | UPDATE condicional com lease; `frontier` separa decisão de implementação | `touring decompose --help` (G4:7) | `github-coordination.js` lê e grava, com corrida admitida |
| Juiz por exit code | `loop_converged.py` com 8 cláusulas, precedido por `judge_attest` | `~/.claude/CLAUDE.md:221` **[V]** | `verification-loop` em prosa; `santa-method` descreve o princípio |
| TDD que bloqueia | `tdd-enforcer` nega Edit em produção sem teste vermelho | skill `tdd-enforcer` (G4:8) | `tdd-guide` aconselha |
| Roteamento julgado | `agent_model_gate.py`, KPI e consentimento do Fable | §17 do `CLAUDE.md` do repo | Variável global e frontmatter |
| Toolchains rustup-like | Versão por projeto, lock, rollback e shim que verifica o dono do binário (C2) | `scripts/test_touring_hook_shim.py`: 8 passed **[V]** | install-state e reinstalação |
| Recall RRF | BM25 + TF-IDF + ANN sobre memória, regra e código | `touring memory recall` | Arquivos de instinct, ranking lexical |
| Painel de abertura | ATZILUTH injeta o estado classificado antes do 1º turno | G2:12 | Dashboard sob demanda |
| Sessões irmãs | Kimi e MiniMax com arquivos exclusivos e aceite escrito | `sessoes_irmas.py` (G3:11) | `multi-*` via runtime npx externo, sem escrita |
| Governador de hardware | `hw-slot`: flock no kernel (rust=1, heavy=2), exit 70 em machine check | `scripts/hw-slot --status` (G4:21) | Nada |
| Pré-registro | Ordem ABBA, A/A antes de A/B, IC95 com semente fixa, INCONCLUSIVO = exit 3 | `PREREGISTRO-P95-PAREADO-2026-10-17.md` (G4:22) | Só o estudo do GateGuard |
| Escala de testes | 17.113 `#[test]`, 358 `#[tokio::test]`, nextest sem retries, proptest e fuzz | G4:11 | Cerca de 3.049 `test()` em JS; nenhum teste de mutação |
| Supply chain do release | verify → build → SBOM CycloneDX → cosign keyless → atestado SLSA | `release.yml` (G4:15) | Pack-installer testado em 3 SO: mais largo em SO, sem assinatura |

## 4. Defeitos e lacunas do Touring revelados, com o estado da correção

| Item | Defeito | Evidência | Estado |
|---|---|---|---|
| C1 | O guarda da REGRA #11 só bloqueia a forma direta; `bash -c`, `eval`, heredoc para shell e `xargs` passam | `scripts/sonda_git_aninhado.py` **[V]** | Em curso (ecc-py-eng) |
| C2 | O shim de hooks dava `exec` no 1º `.touring/bin/touring-hook` da árvore, sem verificar nada | Shim, camada 2 **[V]** | **Feito e implantado** |
| C3/C5 | `id-token` no nível do workflow; release sem conferir CI verde no SHA | `release.yml` **[V]** | **Feito** no main e no int97; vale depois do push |
| C4 | `read_stdin` devolve `{}` em timeout, vazio ou erro; a guarda vê comando vazio e libera | `hook_runtime_ext.rs:445,465,474` **[V]** | Em curso (ecc-hk-eng) |
| C6 | `memory store` grava sem varrer segredo, bidi ou controle, e o recall injeta o texto de volta | **[V]** | Em curso (ecc-mem-eng) |
| C7 | Gotchas: 388.840 acertos e 0 prevenidos (campo sem produtor) | `touring gotcha stats` **[V]** | Em curso (ecc-mem-eng) |
| C8 | `TierManager` só tem consumidor de teste | Índice + `git grep` **[V]** | Em curso (ecc-mem-eng: ligar ou relatar) |
| C9 | A constituição dizia 6 cláusulas; o juiz tem 8 | `loop_converged.py:10-17` **[V]** | **Feito** |
| C10 | `context_budget` em FAIL, com 82,6% de bytes sem família | `scripts/familias_context_budget.py` **[V]** | Em curso (ecc-hk-eng) |
| C11 | 2 U+FEFF literais na regex de frontmatter | `agent_model_gate.py:126` **[V]** | Em curso (ecc-py-eng) |
| C12 | `touring scan` dá 0 CWE onde F2.1 acusa CWE-78 | [A] | Em curso (ecc-qa-eng) |
| C13 | Um doctest grava `test.db` dentro do crate `touring-intelligence` | `analyzer.rs` **[V]** | Em curso (ecc-mem-eng) |
| C14 | `touring serve --help` sobe o servidor | **[V]** | Em curso (ecc-qa-eng) |
| C15 | `touring status -j` devolve `resource_monitor: ""` | **[V]** | Em curso (ecc-qa-eng) |
| Estado versionado | 172 arquivos de runtime (29,8 MB, 0 segredos, 10.749 caminhos `/home/`) no repo público, desde `a38ae33e` | `scripts/estado_versionado.py` **[V]** | `.gitignore` feito; a retirada do índice está no script do commit |
| D2 | `--no-verify` e `core.hooksPath` contornam os hooks; houve 1 bypass real no analise em 29/09 | **[V]** | O texto da REGRA #11 aguarda o seu OK; implementação com o ecc-py-eng |
| D3 | O scan de diretório segurava o lock e os arquivos únicos esperavam | **[V]** | **Feito** (36 testes; 2 mutantes mortos) |
| D4 | O KPI grava snapshots de todos os projetos dentro do repo do touring | `kpi.rs:895` **[V]** | `.gitignore` feito; mover o escritor para fora está em curso (ecc-qa-eng) |
| N1 (novo) | Mutação declarada e não executada: `.cargo/mutants.toml` promete «incremental (--in-diff) in CI, sharded full scans weekly», mas nenhum workflow roda `cargo mutants`; o passo do `ci.yml:271-279` só testa a config | **[V]** | Sem dono. Decidir entre agendar o job e retirar a promessa |
| N2 (novo) | Nenhum piso de cobertura: o job `coverage` gera lcov sem `--fail-under` | `ci.yml:95-152` **[V]** | Sem dono |
| N3 (novo) | Nenhum vigia agendado: um advisory novo em dependência parada só aparece no próximo push | 0 workflows com `schedule:` **[V]** | Sem dono (cron semanal de `cargo deny`) |
| N4 (novo) | A cota está em disco, mas fora da barra de status | `status_monitor.sh:41-47` **[V]** | Sem dono |
| N5 (novo) | Quatro números de saúde sem reconciliação e sem uma linha de decisão | G2:2 [A] | Sem dono |
| N6 (novo) | Journals sem política de retenção (logs de ciclo de setembro seguem no disco) | G2:15 [A] | Sem dono (junta com I19) |
| Fora do ECC | O konverter tem `.pre-commit-config` e 0 hooks instalados; falta conferir o que o commit `--no-verify` do analise de 29/09 pulou | **[V]** | Pendente |

Uma afirmação de agente caiu na conferência: o G2 citou `touring web`, que responde «Unknown subcommand», e nenhuma doc
fora de `docs/plans` ensina esse comando **[V]**. Não é defeito.

## 5. Onde o ECC é melhor e vale adotar (fila priorizada)

Custo: P = pequeno, M = médio, G = grande. Os itens I* estão detalhados no `RELATORIO.md` §3.

| Prio | Item | Origem | Custo |
|---|---|---|---|
| Alta | Linter de segurança dos workflows: pytest que reprove `pull_request_target`, `workflow_run` com checkout do head e `write-all` (parte em curso nas regras novas do F4.7) | G4:14, E1 | P |
| Alta | Scan de Unicode invisível no CI e no `propagate-release`, sobre skills, rules, agents, CLAUDE.md e `client/` | I8, E3 | P |
| Alta | Proteção de config de lint e dos insumos do juiz contra edição pelo agente, primeiro em sombra | I6, A3, E4 | P-M |
| Alta | Cron semanal de `cargo deny check advisories` (N3) | G4:13 | P |
| Alta | Cota 5h/7d na statusline (N4) | G2:5 | P |
| Alta | Sanitizador pré-publicação, só leitura (segredo, `/home/<user>`, arquivo de estado) | I17, E10 | M |
| Média | Sonda de aderência por regra em 3 níveis de rigor do prompt | I1, D1 | M |
| Média | Corpus rotulado de cenários de gate, com replay contra o release anterior | I4, A6, E8 | G |
| Média | Pré-voo do juiz: referência PASS, decoy FAIL, alerta de saturação, regressão em pass^k | I3, D3 | M |
| Média | Ablação pré-registrada dos gates G1/G10/G11 e do nudge, com custo medido | I2, A7, D2 | G |
| Média | Nó `repro_test` no ADW `bugfix` | G4:3 | P |
| Média | Aceite rodável no cartão do `decompose` e recusa de escopo sobreposto | I5, D4 | M |
| Média | Linha única de decisão (`attention_count`, exit 2) e detector de sessão travada (N5) | G2:1, I11, D6 | P-M |
| Média | Ciclo de vida da memória: prune, decaimento medido, promoção com 2+ projetos | I9, B2-B4 | M |
| Baixa | Reviewers de Rust e Python; checklists de falha silenciosa e de segurança | G1:1-6, G1:19 | P |
| Baixa | `hookify` compilando para o gate Rust, em sombra (D8: texto e predicado da mesma fonte) | G4:18 | M |
| Baixa | Manifesto de desinstalação por projeto e teste de instalação em HOME vazio | G3:3, G3:12 | M |
| Baixa | Auditoria de links e symlinks em `docs/plans` | G4:12 | P |
| Baixa | «Padrões a espelhar» no Pln2 e invariante do `factory route` | G4:1-2 | P |
| Baixa | Notificação externa só para pedido de aprovação e franquia perto do fim | G2:8 | P |
| Baixa | Gate com data para toda promessa de remoção ou de execução (N1 é o caso do Touring) | lição do G4 | P |
| Baixa | ID estável e perfis de hook (I10), predicado de shell único (I21), escritores JSONL (I19), proveniência (I14), gatilho (I15), telemetria por skill (I16), paridade de backend (I18), escada de roteamento (I20), worktrees (I12), review fail-closed (I13) | `RELATORIO.md` §3 | P-M cada |

## 6. Anti-lições (não copiar)

As da primeira rodada estão no `RELATORIO.md` §5: GateGuard, prosa como enforcement, confiança autodeclarada, import com
a confiança da fonte, A/B com n = 2, `LANE-RULES.md`, `harness-audit` por contagem, `git stash` como snapshot, resumo por
LLM no PreCompact e processo Node por hook. A rodada comparativa acrescentou:

- **Verde por presença de texto.** `observability-readiness.js:143-149` dá verde se o arquivo cita `--watch`, e o teste
  de doc `antigravity-guide.test.js:29-40` confere a frase. Os dois provam a declaração, não o comportamento.
- **Linhas de código não são capacidade.** A TUI do `ecc2` tem 15.460 linhas, e o README dela diz «Do not market ecc2/
  as done».
- **Desenho contado como capacidade.** O deconflito TCAS existe só como desenho (`TCAS-HOOK.md:3`).
- **Depreciação sem gate.** A `autonomous-loops` foi «mantida por uma release» desde a v1.8.0 e segue viva na 2.2.3.
  O Touring comete o mesmo erro no `mutants.toml` (N1).
- **Registro gerado não cobre o que fica fora dele.** O `COMMAND-AGENT-MAP.md` lista como vivos `/tdd`, `/e2e` e
  `/orchestrate`, aposentados no `COMMANDS-QUICK-REF.md`, mesmo com `catalog:check` no CI.
- **Nota numérica raspada da prosa de um LLM como juiz** (o GAN harness extrai o TOTAL do texto do feedback).
- **Catálogo por contagem, tradução sem gate de sincronia e o mesmo bloco de defesa colado em todo agente.**

## 7. O que ainda falta cobrir

**No ECC**, não lido ou lido só pelo cabeçalho:

- O corpo das 311 skills, dos 71 agents e dos 97 commands (o G1 leu o frontmatter e o 1º parágrafo).
- O `ecc2/` além de grep (12.830 linhas no `main.rs`), além de `pi/`, `plugins/`, `integrations/` e a UI do
  control-pane.
- Os adaptadores `.kiro`, `.opencode`, `.cursor` e `.agents`, arquivo a arquivo (só houve inventário; não se sabe se são
  gerados).
- `tdd-workflow`, `prp-plan`, `prp-implement`, `santa-loop`, `e2e-runner`, `iterative-retrieval`,
  `PLAN-PRD-PATTERN.md`, `the-security-guide.md` e `docs/releases/`.
- `tests/lib` (171 arquivos) e `tests/scripts` (88), só contados.
- Pesquisa web: 25 das 95 afirmações extraídas passaram pela votação.

**No Touring**, não verificado:

- 20 dos 23 specs da `adw-library` (só `bugfix`, `plan-excellence` e `release-gate` foram explicados).
- O painel ATZILUTH, lido pelo docstring e não executado.
- Duplicidade de hook entre o global e o do projeto em runtime (G3:10).
- Se o recall trata a própria saída como dado (G3:9); entra na correção C6.
- Co-alteração de arquivos no git como sinal para skill (G4:19).

É improvável que essas lacunas mudem o placar por domínio. As de maior chance de achado são o `ecc2/` (teto de orçamento
agregado, merge queue) e os 20 specs ADW do próprio Touring, que seriam auditoria interna, não comparação. Vale fechá-las
quando for implementar o I5 ou o teto de orçamento.

## 8. Arquivos

- `COMPARATIVO-matriz.md`: matriz gerada, 156 linhas em 19 domínios.
- `achados/*.json`: 11 conjuntos (A–E, G1–G5, W).
- `RELATORIO.md`: defeitos, inspirações, anti-lições e o estado das correções (§8).
- `scripts/gera_matriz.py`: gera a matriz e separa as duas rodadas no resumo.

## 9. Rodada H — aprofundamento (17–18/10/2026)

Pedido do Gabriel: «faça mais uma rodada de exploração de todas as funcionalidades do ecc e complemente os relatórios
analíticos de comparação com o touring, e aprofunde as dimensões já analisadas».

**Método.** Primeiro código, depois leitores, por último juízes:

1. **Inventário determinístico** (`scripts/inventario_ecc.py` → `inventario/`): 601 entradas de catálogo classificadas,
   adaptadores pareados arquivo a arquivo com a fonte, quatro lotes de leitura.
2. **Sete leitores, cada um com escopo próprio:**
   - **H1:** regras;
   - **H2a:** skills de processo;
   - **H2b:** skills de segurança, contexto e linguagem;
   - **H3:** agents e commands;
   - **H4:** o runtime `ecc2`, `src/llm`, `schemas` e `manifests`;
   - **H5:** `docker/context-profiles`, `examples`, docs e adaptadores;
   - **H6:** o lado Touring das linhas não verificadas.

   Cinco rodaram em haiku e dois em sonnet. Cada um leu corpos, verificou o Touring por comando e escreveu
   `achados/H*.json`; uma linha que revisa outra (`aprofunda`) substitui a antiga no placar, e a antiga fica marcada.
3. **Juízes:**
   - `scripts/cobertura_ecc.py`, com piso de 85%;
   - `scripts/confere_evidencias.py`, que verifica cada `caminho:linha`;
   - uma amostra manual de 49% das linhas H, mais a reconferência por comando de 5 afirmações de alto impacto.

**O que os juízes mediram.**

| Medida | Antes da rodada | Depois |
|---|---|---|
| Arquivos funcionais do ECC (sem traduções e imagens) | 3.174 | 3.174 |
| Corpo lido ou citado (L1) | 49,7% | **67,5%** |
| Classificado ou lido (L0 ∪ L1) | 74,6% (juiz rc=1) | **89,9% (juiz rc=0)** |
| Referências `caminho:linha` das linhas H | — | 308, das quais 4 inválidas (3 faixas além do fim do arquivo, 1 sonda efêmera); nenhum caminho inventado |

Ficaram sem cobertura 320 arquivos, listados por nome em `inventario/cobertura.json`: fixtures de teste, prompts e tools
do `.opencode` e templates de issue.

**Desvios declarados pelos leitores:**
- o H1 leu o corpo de 27 das 122 regras: as cerca de 95 de outras linguagens ficaram só classificadas, e o escopo
  comparável é Rust e Python;
- o H2b leu as primeiras ~60 linhas de cada um dos 43 corpos.

**Placar.** A matriz tem 292 linhas de 17 fontes; 256 seguem vigentes depois de 42 revisões. Placar vigente: Touring
melhor 85 · ECC melhor 73 · equivalente 47 · só ECC 21 · a verificar 19 · só Touring 11. Só a rodada H (136 linhas):
Touring melhor 49 · equivalente 29 · ECC melhor 28 · a verificar 19 · só ECC 9 · só Touring 2.

**As 42 revisões:**
- 9 viraram de «ECC melhor» para «Touring melhor»;
- 7 viraram de «ECC melhor» para «equivalente»;
- 9 foram reconfirmadas como «ECC melhor», com evidência mais funda;
- 3 subiram de «só ECC» para «ECC melhor»;
- **2 viraram de «Touring melhor» para «ECC melhor»**: o G4:8 (o `tdd-enforcer` se declara gate, mas nenhum hook o
  registra) e o G4:3 (o `bugfix.toml` não exige teste vermelho antes do conserto). Foram afirmações da rodada G sobre a
  descrição, não sobre o executor.

Dos 31 «ECC melhor» da rodada G:
- 9 caíram (4 para «Touring melhor», 5 para «equivalente»);
- 7 foram reconfirmados;
- 1 ficou «a verificar»;
- 14 seguem sem revisão.

**Alvo consolidado:** 94 linhas vigentes em que o ECC está na frente. Delas, 9 já foram corrigidas nesta análise e 15 são
«ignorar» ou anti-lição, o que deixa **70 acionáveis**, em `ALVO-ECC.md`, gerado por `scripts/gera_alvo.py`.

**Achados novos de maior peso:**

- **Integridade do juiz**, o maior tema novo, onde o ECC está metodologicamente na frente:
  - proteção das configs de lint e formatação contra o agente que afrouxa o juiz para passar (H1:5, E:E4). A REGRA #21
    sem essa barreira induz o atalho;
  - registro imutável de candidatos, avaliações e promoções, com trigger SQLite que aborta UPDATE e DELETE (H4:9);
  - medida de se o agente cumpre a skill, em três níveis de rigor (H2a:21);
  - fixture de reward hacking (H5:16).
- **Disciplina de teste:** o `tdd-enforcer` não está registrado em nenhum `settings.json` (reconferido: 0 ocorrências); o
  bugfix sem teste vermelho primeiro (H2a:14); a falha silenciosa sem detector de `except: pass` nem de `catch` vazio
  (H2b:12, H3:5, H6:2); o Python sem cobertura no F3.1 (H2b:11); a cobertura comportamental do diff (H3:21).
- **Operação multi-sessão:**
  - sessão travada detectada pelo transcript (H6:4) e pelo heartbeat (H4:11);
  - contrato de conclusão de delegação (H1:7): a resposta final é o entregável, nunca «aguardando subagentes»;
  - fila de merge dos branches de worktree com predição de conflito (H4:4).
- **Custo:** `touring cost-report` não existe (reconferido) e o `budget-verify` mede o orçamento do DAG, não tokens
  (corrige a G1:12).
  - **Defeito do ECC a evitar:** o `ecc2` soma linhas cumulativas do `costs.jsonl`, o que infla o custo em cerca de
    (N+1)/2 vezes. O mesmo arquivo, lido pelo lado JS, usa só a linha mais nova de cada sessão (H4:2).
- **O que o Touring faz melhor, agora com prova de código:**
  - **exclusão mútua:** o `hw-slot` usa flock; o `ecc2` não tem pidfile nem lock (H4:13);
  - **claim de tarefa:** o `decompose claim` é atômico; o inbox do ECC não é (H4:12);
  - **risco antes de executar:** o CEG decide antes de rodar; o «Block» do `compute_risk` do ECC não bloqueia nada (H4:14);
  - **testes no CI:** o CI do Touring roda `cargo`; os 495 testes do `ecc2` nunca rodam no CI dele (H4:17);
  - **cópias por harness:** o espelho `client/` tem teste de drift; as cópias do ECC derivaram em metade da amostra,
    desde o commit único de 18/05 (H5:1);
  - **saúde estrutural:** o Touring tem `ast tdg` e as 50 dimensões; o ECC só aponta para um serviço pago (H6:3).

**Defeitos do Touring citados pelos leitores:** 38, listados em `ALVO-ECC.md`; 8 foram reconferidos por comando:
- config de lint sem proteção;
- `/checkpoint` aponta para um script inexistente;
- `SECURITY.md` e `SUPPORT.md` se citam em círculo, sem canal privado de vulnerabilidade;
- `budget-verify` mal descrito na matriz;
- `cost-report` inexistente.

Os outros 30 são hipóteses até a confirmação. Achados do orquestrador durante a integração:
- o `pre_tool_validator.rs` trata o corpo de um heredoc como comando: o `del` do Python foi negado como «Windows delete»
  (corrigido no int97 em 18/10, gates verdes, `correcoes/val/`);
- o hook `WorktreeCreate` retorna sucesso sem `worktreePath`, o que quebra `Agent` com `isolation: worktree`.
