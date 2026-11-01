# Alvo consolidado depois da rodada H (gerado por `scripts/gera_alvo.py`)

94 linhas vigentes em que o ECC é melhor ou o único: 9 já corrigidas, 15 declaradas ignorar ou anti-lição, **70 acionáveis**.

## Instalação, atualização e distribuição (4)

| Fonte | Funcionalidade | Veredito | Ação | O que fazer |
|---|---|---|---|---|
| E:E5 | Instalação seletiva declarativa: perfis, módulos, install-state, doctor/repair | ECC melhor | adaptar | Manifesto declarativo de componentes (o catálogo hoje vem do filesystem) com validador que rejeita *.old/*.bak. Entregar 3 perfis (minimal/dev/full) com cost/stability. Gravar install-state com digest para o doctor/repa… |
| G3:3 | Desinstalacao | ECC melhor | adaptar | Gravar um manifesto por projeto (arquivos criados) para desinstalar sem resto. |
| G3:4 | Atualizacao automatica | ECC melhor | adaptar | Aceitar atualizacao so com consentimento gravado e gate verde; nunca silenciosa, dado o historico de regressao do Touring. |
| H5:3 | Teste de instalacao limpa: pacote gerado, instalado num consumidor isolado com rede desligada e verificado pela CLI pub… | ECC melhor | adaptar | Confirma G3:12. Adaptar: a propagacao de toolchain nova deveria rodar update-touring num HOME vazio, com rede desligada, e depois o doctor. O isolamento de HOME ja existe no teste de daemon e basta reaproveitar. |

## Multi-harness e multi-modelo (2)

| Fonte | Funcionalidade | Veredito | Ação | O que fazer |
|---|---|---|---|---|
| E:E7 | Scorecard de paridade entre harnesses com estados honestos | só ECC | adotar | docs/backend-parity.json com os 4 estados por backend (hooks, prose gate, CEG, matriz de modelos) e um --check no CI. |
| G3:5 | Empacotamento de skills para Pi (pi/core) e o que e o pi core | ECC melhor | adaptar | Adotar o gate de conteudo (curl/sh, segredo, caminho absoluto) para qualquer skill distribuida. |

## Catálogo: agentes (2)

| Fonte | Funcionalidade | Veredito | Ação | O que fazer |
|---|---|---|---|---|
| H3:1 | revisão especializada por linguagem: checklist de conteúdo Rust e Python | ECC melhor | adaptar | Mudança vs G1:1: o lado Touring estava 'parcial' por contar agents que não são revisores; corrigido para 'ausente'. Importar só o checklist de conteúdo (Rust e Python), ligado ao gate cargo check + clippy -D warnings (p… |
| H3:2 | privilégio mínimo do revisor: ferramentas só de leitura | ECC melhor | adaptar | Um auditor que pode editar o que audita perde independência. Criar um revisor só-leitura, ou separar auditor de executor, sem trocar o touring-auditor existente. |

## Catálogo: commands (2)

| Fonte | Funcionalidade | Veredito | Ação | O que fazer |
|---|---|---|---|---|
| G4:18 | hookify (regras declarativas a partir da conversa) | só ECC | adaptar | Só vale se a regra compilar para o gate Rust existente (texto e predicado da mesma fonte, D8) e entrar primeiro em sombra. |
| G5:5 | Slash commands de atalho | ECC melhor | adaptar | Adaptar a ideia do wrapper de CLI sem MCP, que o ECC mesmo recomenda. Os touring masters cobrem a parte. |

## Catálogo: regras e contextos (2)

| Fonte | Funcionalidade | Veredito | Ação | O que fazer |
|---|---|---|---|---|
| G1:4 | regras por linguagem (coding-style, testing, security, hooks) e modos de contexto | ECC melhor | adaptar | A divisao por arquivo (style/testing/security) vale como padrao. Os contexts nao compensam: o Touring ativa por hook e skill. |
| H1:4 | Aviso de console.log após edição e auditoria de console.log antes de encerrar a sessão (JS/TS) | ECC melhor | adaptar | Com dois lados evidenciados, o ECC tem executor e o Touring não. O valor é baixo porque console.log é JS e o Touring é Rust/Python. O princípio portável é um detector de saída de depuração (dbg!, println!, print(), cons… |

## Hooks e enforcement (6)

| Fonte | Funcionalidade | Veredito | Ação | O que fazer |
|---|---|---|---|---|
| A:A10 | Detector de laço de ferramenta e cobertura de PostToolUseFailure | ECC melhor | adotar | Anel de 5 chamadas por sessão no post-tool-batch com limiar 5 e aviso consultivo com debounce. Ampliar o matcher de PostToolUseFailure para `^mcp__/Skill/Agent`, só com contador. |
| A:A8 | ID estável por hook com fingerprint, lista única de desativação, perfis e dry-run | ECC melhor | adaptar | ID e fingerprint no sidecar da fonte versionada. `TOURING_DISABLED_HOOKS=id,…` e `TOURING_HOOK_PROFILE=minimal/standard/strict` lidos pelo shim. `touring-hook explain <payload>` em dry-run. O perfil minimal substitui a … |
| A:A9 | Consolidar processos e manter um único predicado | ECC melhor | adaptar | Levar a lógica do block_git e o masker para o handler Rust (bash_ast_validator.rs:215 já tem 'git reset --hard' como Warn), com um shim fino. Isso elimina a cópia Python e cerca de 40 forks, e entrega A1 de quebra. Port… |
| E:E4 | Proteção de arquivos de config/lint contra edição pelo agente | só ECC | adotar | Handler pre_edit/pre_write com conjunto protegido: deny.toml, clippy.toml, rustfmt.toml, rust-toolchain.toml, .cargo/config.toml, .github/workflows/*, .ruff.toml e juízes (loop_converged.py). Começar em sombra, depois a… |
| H2a:17 | Regras de hook declaradas em markdown do usuario, com regex, evento e acao warn ou block | só ECC | adaptar | Vale testar uma camada declarativa para regras locais de projeto, sem tocar os gates de codigo. Custo: mais uma superficie para manter |
| H2a:22 | Mensagem de integridade de commit: formato Conventional Commits que segue o historico do repositorio | só ECC | adaptar | Baixo custo: um gate que confere o prefixo contra o historico antes do commit |

## Segurança (4)

| Fonte | Funcionalidade | Veredito | Ação | O que fazer |
|---|---|---|---|---|
| E:E3 | Gate de entrada contra Unicode oculto (ASCII smuggling) | só ECC | adotar | scripts/check_unicode_safety.py (mesmas faixas) em CI e propagate-release. Trocar os 2 U+FEFF literais por escape \ufeff. Adicionar a classe 'invisível' ao prose gate em sombra. Escopo: skills, rules, agents, CLAUDE.md,… |
| G4:14 | Linter de segurança dos próprios workflows | só ECC | adaptar | Barato e preventivo: um pytest que reprove pull_request_target, workflow_run com checkout de head e write-all em .github/workflows. |
| H1:5 | Proteção de configs de lint e format: bloquear o agente que enfraquece o linter para passar no check | ECC melhor | adotar | Este é o achado mais forte para o Touring. Ele tem REGRA #21 (sem falha conhecida aberta) e não tem nenhuma barreira contra o atalho de enfraquecer o juiz. Proposta: o pre-edit/pre-write bloqueia edição em config de lin… |
| H2b:2 | modos de proteção contra operação destrutiva: Careful (bloqueia rm -rf, push --force, DROP), Freeze (trava escrita a um… | ECC melhor | adaptar | Adotar o modo Freeze como restrição de escopo por sessão: trava de Write/Edit fora de um diretório declarado. O Touring tem blast radius por símbolo, mas não trava por diretório. |

## Memória e aprendizado contínuo (10)

| Fonte | Funcionalidade | Veredito | Ação | O que fazer |
|---|---|---|---|---|
| B:B1 | Instinct atômico (trigger+ação+confiança+escopo) com frontmatter fixo | ECC melhor | adotar | Lições novas levam a 1ª linha 'Use when <condição observável>' e a faceta #trigger. O `memory store` valida o formato, como o learn.md valida a descoberta. Assim o recall casa pela situação, não só por similaridade text… |
| B:B10 | Detecção de padrões: captura determinística + análise por LLM com contrato de conclusão | ECC melhor | adotar | Adicionar detectores determinísticos (sem LLM): sequência de ferramentas repetida 3+ vezes e reversão/correção do usuário. Entram como lição untrusted, subindo por medição (B7). |
| B:B2 | TTL e prune de instincts pendentes | só ECC | adotar | `memory prune --unmeasured --older-than 30d --dry-run` aposenta por tombstone (padrão `--supersedes` já existente): entradas outcome:* sem reward e nunca recuperadas. Chamar o prune no cron e no loop_phase_close. |
| B:B3 | Decaimento temporal de confiança | ECC melhor | corrigir | Ligar o TierManager a um sweep e acrescentar a classe 'envelhecida' ao rerank por valor. Decidir pelo limite inferior de Wilson (rl/ranking/wilson.rs), não por incrementos fixos. Medir antes e depois com o kpi. |
| B:B4 | Promoção projeto->global, e import/export por escopo | só ECC | adotar | `memory promote`: id/tag em 2+ projetos com limite de Wilson sobre o reward medido (não a média da confiança autodeclarada). `memory export --tag --min-reward` e import que entra untrusted, com confiança limitada (ver a… |
| B:B7 | Medir se o aprendizado ajudou: telemetria por skill e 'não medido != saudável' | ECC melhor | adotar | Tracker de Skill com skill_version e bucket 'unmeasured' no kpi. Dar produtor ao prevented_errors dos gotchas (hoje 0 contra 388834 acertos). |
| B:B8 | Evolve: agrupar instincts em skill/comando/agente | ECC melhor | adotar | `memory moc --emit-skill`: comunidade com N lições de reward positivo vira rascunho de skill com `evolved_from` listando as chaves. Humano aprova, como o /learn. |
| B:B9 | Proveniência (quem/onde/quando gerou) por unidade aprendida | ECC melhor | adotar | Faceta automática #source (modelo/sessão/harness) e #author em todo `memory store`, lida pelo recall. Sem isso a lição de sessão irmã é indistinguível da curada. |
| H2a:7 | Aprendizado por instincts com escopo de projeto e promocao para global | ECC melhor | adaptar | G1:11 dizia touring_melhor em aprendizado por instincts. Vale para o aprendizado medido, mas a promocao projeto para global e o TTL sao lacuna do Touring. A mecanica do ECC foi conferida pela matriz (COMPARATIVO-matriz.… |
| H6:10 | Fronteira de confiança da memória: recall tratado como dado, escrita sem revisão, rejeição de segredo | ECC melhor | adaptar | Corrige G3:9: o Touring ganha em aprendizado e perde na fronteira. Adaptar só o que é barato e já tem peça pronta (REGRA #0): chamar redact_secrets/scan_secrets em cli_memory_store e RECUSAR (não redigir calado) valor c… |

## Contexto, tokens e custo (9)

| Fonte | Funcionalidade | Veredito | Ação | O que fazer |
|---|---|---|---|---|
| C:C2 | Aviso escalonado por tier, hoje em $ estimado | ECC melhor | adotar | Aviso do tipo `rate_limit` com dedupe por tier de janela: 7d ≥ 70/85/95% e 5h ≥ 90%. Emite uma vez por tier e por resets_at, com o remédio (parar fanout, preferir irmã). O Touring tem o sinal real, que o ECC só aproxima… |
| C:C3 | Re-lembrete por crescimento de contexto, tool count rebaixado | ECC melhor | adaptar | Tratar o intervalo como parâmetro pré-registrado. ECC usa 60k, billion-context 50K, o plano 100K, e nenhum foi medido. Escolher pelo ~/.claude/touring/compactions.jsonl (702 KB): perda do resumo × intervalo. Manter o es… |
| C:C5 | Teto da superfície eager com ledger por entrada | ECC melhor | adotar | Transformar a medição de junho em KPI report-only: estimativa por entrada (ranking dos mais pesados) mais o 1º call observado como verdade. Calibrar o estimador contra o observado antes de fixar qualquer teto. Só depois… |
| C:C8 | Estimativa e observado separados no mesmo registro | ECC melhor | adaptar | Calibrar bytes/token por família com o delta de `usage` entre chamadas consecutivas nos transcripts e publicar o fator no KPI. Rotular cada teto como «bytes (proxy)» ou «tokens (observado)». Evita um teto de 2000 B que … |
| E:E6 | Perfil de contexto com orçamento de tokens bloqueante | ECC melhor | adaptar | Ligar o guard ao propagate-release (o lado vivo existe lá). Medir e gatear o total de tokens da listagem eager no KPI, com piso bloqueante como lean@1. |
| G2:5 | Statusline: contexto, custo, cota | ECC melhor | adaptar | Levar 5h/7d e o countdown para a barra é barato: o dado já está em disco. O gatilho SALVE/COMPACT por marcador de sessão é melhor que o do ECC. |
| H2b:5 | relatório de custo de uso a partir do log de sessões | ECC melhor | adaptar | O relatório com a regra de deduplicação por session_id é o que o T3 de DECISOES-E-PLANO-ULTRAPASSAR-ECC.md propõe. Usar a regra de deduplicação do ECC ao implementar, porque somar todas as linhas conta o custo várias ve… |
| H2b:7 | coleta de lixo da configuração do agente: skills, memória, hooks, permissões, MCP, caches | ECC melhor | adaptar | Um 'touring config-gc --dry-run' que mede linhas do CLAUDE.md, rules e hooks órfãos, com soft-delete, daria ao Touring o que o ECC tem e o Touring não tem. Antes de afirmar 'ausente', confirmar que a regra 16 não tem de… |
| H3:10 | relatório de custo por dia, modelo e sessão a partir de ledger | ECC melhor | adaptar | O Touring cobra economia de créditos (CLAUDE.md regra 17) sem medir gasto por sessão. O ECC mostra o desenho: ledger de uma linha cumulativa por sessão, deduplicado por session_id. Adaptar com o mesmo desenho. |

## Roteamento de modelo (1)

| Fonte | Funcionalidade | Veredito | Ação | O que fazer |
|---|---|---|---|---|
| C:C7 | Escada de escalonamento e confiança no roteamento | ECC melhor | adotar | Acrescentar à matriz a coluna «se falhar no juiz → sobe para». Juntar o gate log (agent_id) ao desfecho do subagente no transcript para gerar rótulos decisão→desfecho. Isso alimenta a Laya (~1.000 rótulos) sem classific… |

## Orquestração e paralelismo (4)

| Fonte | Funcionalidade | Veredito | Ação | O que fazer |
|---|---|---|---|---|
| D:D10 | Classificador determinístico do ciclo de vida de worktree | só ECC | adotar | scripts/worktree-report só de leitura, alinhado à REGRA #11: dirty nunca limpa, stale vira candidato a branch safety/ e só 'merged' sugere remoção, com predição via merge-tree. |
| D:D5 | Risco de colisão multi-canal entre sessões vivas (TCAS) | ECC melhor | adotar | PreToolUse de Edit em modo sombra: lê o working set (git diff) de outras sessões e usa o grafo de blast do Touring como canal de dependência. Só avisa, com right-of-way determinístico, e mede antes de qualquer bloqueio. |
| H1:7 | Contrato de conclusão da delegação: a resposta final de um agente é o entregável; nunca encerrar com 'aguardando agente… | ECC melhor | adotar | Sem medição nos dois lados, o ECC só tem o motivo declarado (uma observação). Ainda assim, o Touring não tem a regra e a falha é do mesmo tipo do que a memória do projeto já registra (resultado sem coleta). Proposta: um… |
| H4:4 | Fila de merge dos branches de sessões em worktree, com ordem e predição de conflito | ECC melhor | adaptar | Resolve um problema que o Touring tem hoje (19 worktrees de agentes, integração manual). Adaptar só o núcleo, como `scripts/worktree-queue` de LEITURA: lista os branches/worktrees a integrar, ordena por `git merge-tree … |

## Planejamento e decomposição (1)

| Fonte | Funcionalidade | Veredito | Ação | O que fazer |
|---|---|---|---|---|
| H3:14 | plano que para e espera confirmação antes de tocar código | ECC melhor | corrigir | O Touring tem planejamento mais forte (taco-planning, DAG, VGP), mas o comando de entrada não roda. Corrigir o comando antes de comparar conteúdo, e adotar a pausa para confirmação. |

## Verificação, avaliação e qualidade (13)

| Fonte | Funcionalidade | Veredito | Ação | O que fazer |
|---|---|---|---|---|
| A:A6 | Corpus de cenários rotulados (mustDeny, redundant, expect) com comparação contra refs do git | ECC melhor | adotar | scripts/gate_replay.py mais fixtures JSON para G1-G11, block_git e o deny do pre-bash. Métricas: negações, redundantes, bypass de must-deny (=0), divergência contra a ref anterior. Ligar ao gate do propagate-release. Se… |
| A:A7 | Medir o efeito e o custo do gate com braços contrafactuais pré-registrados | ECC melhor | adaptar | Um experimento pré-registrado e pareado, com G1, G10 e G11 ligados contra desligados, em tarefas com custo medido. Reportar turnos e tokens ao lado do acerto. O fato decisivo fica fora do repositório. Reusar o método, n… |
| D:D11 | Journal com hash encadeado, receipts offline e não-afirmações explícitas | ECC melhor | adaptar | parent_hash em cada linha do journal e 'adw verify-run'. Receipt (digest da entrega mais veredito do gate) para entregas de sessão irmã. Cada artefato de juiz ganha um campo 'non_claims', como o gate.js:11-17. |
| D:D3 | Pré-voo de juiz: reference=1, naive e decoys<1, e detecção de saturação | ECC melhor | adaptar | judge_attest exige por gate um fixture reference (PASS) e um decoy (FAIL), e alerta o gate com 100% de aprovação nas últimas N rodadas. Gate de regressão em pass^k, não pass@k (eval-harness SKILL.md:292). |
| E:E8 | Eval de gate com controle negativo contra baseline e invariante 'nunca allow' | ECC melhor | adaptar | Teste que varre as saídas de todos os PreToolUse e afirma 'sem allow'. No gate 5.5, rodar os cenários also contra o binário do release anterior como controle negativo. |
| H2a:14 | Correcao de bug que comeca por reproduzir o defeito como teste falhando | ECC melhor | adaptar | G4:3 dizia touring_melhor para a familia orch-*. Para o passo de correcao de bug, o ECC e melhor: o Touring nao impoe o teste vermelho antes do Edit. Adaptar: um no red antes do fix, que exija o comando de teste falhar … |
| H2a:21 | Medida de se o agente cumpre a skill, em tres niveis de rigor do prompt | ECC melhor | adaptar | Confirmo D1. Relevante para o Touring: o proprio CLAUDE.md registra que nudges MUST foram ignorados na sessao que os emitiu. O skill-comply mede exatamente essa lacuna. Adaptar como ADW de medida, sem substituir a aford… |
| H2b:11 | padrões idiomáticos e de teste em Python e Rust: idiomas, tratamento de erro, cobertura e TDD | ECC melhor | adaptar | G1:3 disse 'ausente' para padrões por linguagem. Corrigido: idiomas e unwrap já são detectados. A lacuna é a cobertura Python, que F3.1 não lê, e o detector de catch vazio (ver a linha de error-handling). A camada de pr… |
| H2b:12 | erro nunca silencioso: todo catch trata, relança ou registra | ECC melhor | adaptar | G1:19 deixou a revisão de falha silenciosa como ecc_melhor sem evidência do lado Touring. Confirmado com a leitura de F1.6: não há detector de catch vazio nem de Python 'except: pass'. Adicionar um detector ao F1.6 (Rus… |
| H3:21 | cobertura comportamental dos testes do PR | ECC melhor | adaptar | Pede um passo de revisão de cobertura por diff antes do cross-audit. Não medi se o ECC acerta; a vantagem aqui é de cobertura de função, não de medição. |
| H3:5 | detecção de falha silenciosa: catch vazio, fallback perigoso, perda de erro | ECC melhor | adaptar | Medido do lado Touring (probe), declarado do lado ECC (prompt sem teste): o ECC cobre o que o Touring não cobre. Adaptar como detector determinístico na camada quality do audit (ExceptHandler com corpo pass ou continue … |
| H4:9 | Registro imutável de candidatos, avaliações e promoções de configuração (harness-eval) | ECC melhor | adaptar | Dois lados: a estatística do Touring (Wilson sobre taxa pré-registrada) é mais forte que delta médio e win-rate; o ledger do ECC é mais forte em integridade (imutabilidade no banco, promoção atômica). Adaptar só o ledge… |
| H5:16 | Fixture de reward hacking com avisos sintaticos de inspecao estatica | ECC melhor | adaptar | Veredito limitado: o ECC tem a ideia e o caso de teste, mas sem medicao. Adaptar como gate do juiz do Touring so depois de medir falso positivo num corpus. A licao de juiz gravavel pelo julgado ja esta na memoria do Tou… |

## Observabilidade e operação (5)

| Fonte | Funcionalidade | Veredito | Ação | O que fazer |
|---|---|---|---|---|
| D:D7 | Ledger de execuções por skill com 'sem medida ≠ saudável' | só ECC | adotar | Ledger JSONL de skill/ADW com desfecho e correção do Gabriel, e bloco 'skills' no kpi com 'unmeasured' explícito. Mesma lei L2 (ausência ≠ zero), aplicada às skills. |
| G2:1 | Painel de estado de sessões e filas (pull) | ECC melhor | adaptar | Falta um agregador `attention_count` com exit 2, que o Stop hook e o Gabriel possam ler. O score composto atual não diz o que fazer. |
| G2:2 | Números de saúde concorrentes | ECC melhor | adaptar | Um nome, grandezas diferentes (lição já registrada). Publicar uma linha só no painel, com as outras quatro como detalhe. |
| H4:11 | Detecção de sessão travada pelo heartbeat do runner | ECC melhor | adaptar | Reforça D6: o heartbeat dentro do orquestrador não substitui o detector por transcript (tool_use sem tool_result, wakeup vencido). O ECC mesmo precisou do script separado. Para o Touring, o detector é um subcomando de s… |
| H6:4 | Detecção de sessão travada: tool call pendente e ScheduleWakeup vencido, lidos do transcript | ECC melhor | adaptar | Confirma G2:3 com evidência. Adaptar como um script `touring run` sobre ~/.claude/projects/*/<id>.jsonl que usa a idade do último evento e o pareamento só com timestamp (o registro de sessões sozinho engana, como analis… |

## Inteligência de código (1)

| Fonte | Funcionalidade | Veredito | Ação | O que fazer |
|---|---|---|---|---|
| G1:17 | tour e onboarding de codebase | ECC melhor | adaptar | O formato .tour e util como saida de touring map para novato. change-impact ja e coberto por ast blast. |

## Governança do repositório e do catálogo (3)

| Fonte | Funcionalidade | Veredito | Ação | O que fazer |
|---|---|---|---|---|
| H2a:20 | Auditoria completa do catalogo de skills com cache de resultados e modo de varredura rapida | ECC melhor | adaptar | O Touring refina skill a skill e confere afirmacoes estaticas. Falta o inventario com cache e a varredura de mudados |
| H3:11 | criação de PR com validações, template e título pelo tipo dominante | só ECC | adaptar | Adaptar as 4 validações (base, commits à frente, PR duplicado, árvore limpa) como passo de gate antes de gh pr create. O ECC não mede o ganho. |
| H6:7 | Canal privado e prazos para relatar vulnerabilidade | ECC melhor | corrigir | A lacuna que G5:16 chamou 'de forma, não de conteúdo' é de conteúdo: hoje um pesquisador que ache uma fuga do sandbox do CEG não tem como avisar em privado. Correção de 3 passos, todos do Gabriel: ligar o private vulner… |

## Documentação e guias (1)

| Fonte | Funcionalidade | Veredito | Ação | O que fazer |
|---|---|---|---|---|
| H5:11 | Proveniencia e colocacao de skills com validadores | ECC melhor | adaptar | Adaptar: validar origin e tamanho de SKILL.md e rodar o validador sobre client/skills. Sem isso o campo muda na fonte e nunca chega ao espelho, que e o que aconteceu com o ECC. |

## Defeitos do Touring citados pelos leitores

Hipótese até confirmação por comando. Os marcados foram reconferidos pelo orquestrador.

| Lote | Defeito | Evidência do leitor | Estado |
|---|---|---|---|
| H1 | Nenhum executor de regra de linguagem roda ao editar: pós-edição não formata nem avisa de depuração (dbg!, println!, print()); a medição de unwrap (F1.6) existe mas não bloqueia | `grep -n 'Command::new' crates/touring-hook-handlers/src/hooks/post_edit.rs crates/touring-hook-handlers/src/hooks/post_write.rs: 0; crates/…` | D01 · confirmado (triagem 18/10): Pós-edição não roda formatter (sem rustfmt/prettyplease) nem detecta dbg!/println!/print(): o detector de antipadrões não tem esses padrões. F1.6 não está na l… |
| H1 | Config de lint/format sem proteção: o agente pode enfraquecer o linter para passar no check (REGRA #21 pode induzir isso) | `grep -rln 'eslintrc\/ruff.toml\/clippy.toml\/rustfmt.toml\/prettierrc' client/hooks crates/touring-hook-handlers/src crates/touring-hook-ru…` | confirmado: nenhum hook protege ruff.toml, clippy.toml, rustfmt.toml ou eslintrc (grep em client/hooks e nos handlers) |
| H1 | Nenhuma regra nem detector contra delegação órfã: resposta final 'aguardando subagentes' não é barrada | `~/.claude/skills/Touring/references/TACO-subagent-rule.md (sem regra de conclusão de delegação); grep -rln 'fire-and-forget\/run_in_backgro…` | D02 · confirmado (triagem 18/10): Confirmado por leitura de código, sem disparo real do Stop. Os matches de 'fire-and-forget' são comentários de registro no daemon, não regra de delegação. Nenh… |
| H1 | Teto de contexto: sem regra de 20% para refatoração grande (ECC common/performance.md) no fluxo TACO; o aviso único existe aos 55% | `~/.claude/CLAUDE.md 'Context Management' (aviso aos 55%); rules/common/performance.md §Context Window Management (ECC)` | D03 · confirmado (triagem 18/10): Nenhuma regra de teto de 20% para refatoração grande no fluxo TACO. O aviso único aos 55% é o único limiar de contexto. O conteúdo do ECC performance.md não fo… |
| H2a | loop_converged.py declara 'fail-open on daemon errors' no cabecalho, mas a Lei L2 do CLAUDE.md global diz 'sinal ausente != zero (fail-closed)'. As duas regras nao batem | `~/.claude/skills/loop-engineering/scripts/loop_converged.py:1-6; ~/.claude/CLAUDE.md, secao MODUS OPERANDI (tabela de Leis, L2)` | D04 · corrigido 18/10: o docstring do loop_converged dizia fail-open; diz o que o código faz (todas as cláusulas fail-closed) |
| H2a | tdd-enforcer se declara gate BLOQUEANTE de Edit/Write, mas nenhum hook o registra em settings.json, client/claude/hooks.settings.json, ~/.claude/hooks, client/hooks ou crates/touring-hook* | `~/.claude/skills/tdd-enforcer/SKILL.md:10; ~/.claude/settings.json:456 (unico pre-write registrado: touring-hook pre-write); grep -rli 'tdd…` | confirmado e corrigido no texto (18/10): 0 registros de hook; SKILL.md e loop-engineering deixam de dizer que bloqueia; registrar = T1 |
| H2a | pre_edit_gate.py (docstring: 'Bundles every Reflex #1/#2/#4') nao tem chamador encontrado em hook nem em crate: touring index find pre_edit_gate retornou count 0 | `~/.claude/skills/Touring/scripts/pre_edit_gate.py:1-17; grep -rln pre_edit_gate ~/.claude/skills crates client -> so docs, scripts e harnes…` | D06 · refutado (triagem 18/10): pre_edit_gate.py tem chamador: o master CLI `touring guard` o roteia por script_for. O count 0 do index find não prova ausência, porque o índice é de símbolos … |
| H2a | vocabulario: 'touring e2e' e analise de codigo, nao teste E2E. A regra 3 do CLAUDE.md do workspace usa esse comando como gate de pronto, o que e facil de ler como 'teste de ponta a ponta passou' | `touring e2e --help ('Comprehensive E2E code analysis'); ~/projects/touring/CLAUDE.md regra 3` | D07 · corrigido 18/10: a regra 3 do CLAUDE.md do workspace diz que `touring e2e` é análise composta, não teste ponta a ponta |
| H2b | touring scan não reporta o segredo AKIA literal que touring audit bloqueia: scan só listou dois unwrap (P2) e p0_block=false; audit deu verdict block com kind F2.4 | `scratchpad/h2b/probe.rs:1 (AKIA literal); touring scan scratchpad/h2b/probe.rs; touring audit scratchpad/h2b/probe.rs` | D08 · confirmado (triagem 18/10): Reproduzido: scan não reporta o segredo AKIA literal e só lista o unwrap como P2. audit bloqueia com F2.4. Fixture em $SCR/d08/probe.rs. |
| H2b | SQLi por format! tem três vereditos: scan P1 (p0_block=false), audit 'info', hook pre-write advisory [0.5] | `scratchpad/h2b/probe2.rs:2; touring scan e touring audit no mesmo arquivo; saída do hook pre-write nesta rodada` | D09 · confirmado (triagem 18/10): Reproduzidos 2 dos 3 vereditos (scan P1 sem p0_block; audit info sem finding). O advisory [0.5] do hook pre-write não foi disparado nesta rodada: não reproduzi… |
| H2b | Command::new('sh').arg('-c').arg(c) em Rust, com c vindo de parâmetro, não é detectado por scan nem por audit | `scratchpad/h2b/probe2.rs:6; touring audit scratchpad/h2b/probe2.rs -> verdict info` | D10 · confirmado (triagem 18/10): Reproduzido: shell -c com argumento vindo de parâmetro não é detectado por scan nem por audit. O fixture não tem segredo, para o veredito não ser mascarado. |
| H2b | COMPARATIVO-matriz.md (G1:12) atribui a 'touring budget-verify' a função de orçamento de contexto e custo; o comando verifica o orçamento do DAG | `docs/plans/2026-10-17-analise-ecc/COMPARATIVO-matriz.md:150; touring budget-verify --help` | confirmado: `touring budget-verify` verifica o orçamento do DAG, não tokens |
| H2b | touring cost-report é citado no plano T3 e no G1:12 como existente, mas não existe no binário 30.4.96 | `touring cost-report -> Unknown subcommand; docs/plans/2026-10-17-analise-ecc/DECISOES-E-PLANO-ULTRAPASSAR-ECC.md:251` | confirmado: `touring cost-report` não existe |
| H2b | stop.rs documenta registro de tokens e sumário, sem gate de encerramento; a regra do CLAUDE.md sobre Stop que recusa encerrar sem manifesto não tem implementação localizada neste arquivo | `crates/touring-hook-handlers/src/hooks/stop.rs:1-8; ~/projects/touring/CLAUDE.md, regra 17 (work-outer) e seção Referências` | D11 · refutado na referência (18/10): o gate de Stop existe (loop_stop_guard → loop_outer_gate); «regra 17 (work-outer)» não aparece em nenhum CLAUDE.md |
| H2b | F3.1 só lê cobertura LCOV do cargo llvm-cov: projeto Python não recebe medição de cobertura | `crates/touring-quality/src/verifications/f3_1_coverage.rs:4-6` | D12 · confirmado (triagem 18/10): Confirmado por leitura, não executado: a única medição real é LCOV do cargo llvm-cov. Sem artefato cai na razão de presença de #[test] (Rust). Não há caminho p… |
| H3 | ~/.claude/commands/plan.md: o caminho de saída depende de ~/.claude/hooks/plan-detector.sh, que não existe; a linha de execução é '/plan ou skill taco-planning ...' (shell inválido) | `~/.claude/commands/plan.md:44-52; ls ~/.claude/hooks/plan-detector.sh → No such file` | D13 · corrigido 18/10: /plan roda ground_truth_collector + plan_scaffolder reais; o gate plan-detector.sh, que nunca existiu, saiu do texto |
| H3 | ~/.claude/commands/checkpoint.md aponta para ~/.claude/tools/Touring-native tooling/workflows/checkpoint.sh, inexistente | `~/.claude/commands/checkpoint.md:35; ls '~/.claude/tools/Touring-native tooling/workflows/checkpoint.sh' → No such file` | confirmado: ~/.claude/commands/checkpoint.md aponta para workflows/checkpoint.sh, que não existe |
| H3 | ~/.claude/agents/_shared-touring-base.md manda rodar ~/.claude/lib/plan_generator/checkpoint_validator.py, inexistente | `~/.claude/agents/_shared-touring-base.md:191; ls ~/.claude/lib/plan_generator/checkpoint_validator.py → No such file` | confirmado: ~/.claude/commands/checkpoint.md aponta para workflows/checkpoint.sh, que não existe |
| H3 | touring audit não reporta bare except com pass (silent failure): só o segredo literal foi reportado no mesmo arquivo | `touring audit scratchpad/h3probe/probe_silent.py → único finding F2.4 (linha 8), exit 2; linhas 4-5 sem finding` | D14 · confirmado (triagem 18/10): Confirmado: bare except com pass não é reportado por audit. Controle positivo: no mesmo tipo de arquivo (Python) audit pega eval (F2.1), então a ferramenta fun… |
| H3 | touring audit não reporta unwrap() nem unsafe sem // SAFETY: em arquivo .rs; verdict info com zero findings | `touring audit scratchpad/h3probe/probe_rust.rs (linhas 2-3) → {"verdict":"info","findings":[]}, exit 0` | D15 · confirmado (triagem 18/10): Confirmado: audit não reporta unwrap nem unsafe sem // SAFETY. scan pega o unwrap mas não o unsafe. Controle: o F2.4 do audit dispara no mesmo tipo de arquivo … |
| H3 | touring-auditor, que audita, tem Edit e Write: o papel de auditoria pode alterar o que audita | `~/.claude/agents/touring-auditor.md:11` | D16 · confirmado (triagem 18/10): Auditor tem Edit e Write. Confirmado por leitura do frontmatter; não há bloqueio de escrita no corpo. |
| H3 | touring-scouter grava saída em /tmp fixo, sem mktemp nem permissão restrita; o santa-loop do ECC usa mktemp com umask 077 | `~/.claude/agents/touring-scouter.md:31-32; ECC commands/santa-loop.md:107-108` | D17 · corrigido 18/10: touring-scouter grava num mktemp -d com umask 077 (vivo e espelho) |
| H4 | O daemon global roda um binário apagado: `touring daemon-ctl status` mostra exe `…/target/release/touring-daemon (deleted)` com a nota «rebuild ran without restart» (PID 731793, 18/10 02:56). Relatado, não corrigido: re… | `touring daemon-ctl status (18/10 02:56)` | D18 · confirmado (triagem 18/10): Daemon segue com binário apagado. Não reiniciado, conforme REGRA #19 e o próprio brief. |
| H4 | O registro de sessões do `touring status -j` está morto: `.sessions` lista 20 registros, 17 sem objetivo, ids numéricos de 14 a 31 e nenhum atualizado depois de 01/09 além dos fixtures wave24-test e wave24-warmup. Com 6… | `touring status -j → .sessions (18/10 02:53); `.resource_monitor` segue string vazia, como em G2:1` | D19 · confirmado (triagem 18/10): Fixtures wave24-test e wave24-warmup são os únicos atualizados em 18/10. O resource_monitor não foi reexaminado. |
| H4 | `prose_integrity_gate.py --criterion` sem o arquivo de rótulos termina em IndexError (traceback) em vez de mensagem de uso. | `python3 -I ~/.claude/skills/Touring/scripts/hooks/prose_integrity_gate.py --criterion → IndexError em prose_integrity_gate.py:473 (argv[2])` | D20 · corrigido 18/10: --criterion sem arquivo legível devolve uso e exit 2 (teste vermelho→verde, 49 passed) |
| H4 | Nenhuma tabela de evidência do Touring é imutável no banco: os 14 CREATE TRIGGER de crates/scripts/client são de sincronia FTS e `RAISE(` tem 0 acertos. Log de experimentos e veredictos de juiz dependem de disciplina de… | `grep 'CREATE TRIGGER/RAISE[(]' em crates scripts client (18/10): 14 e 0` | D21 · confirmado (triagem 18/10): Nenhum trigger de imutabilidade. Os 14 são de sincronia FTS. Os 14 incluem o trigger de teste em handlers/tests.rs. |
| H4 | Integração de worktrees de agentes sem ferramenta: 20 worktrees hoje, entrega por cópia de arquivos entre worktree, vivo e int97, com duas ocorrências registradas de quebra (10/10 e 17/10). | `git worktree list / wc -l = 20; memórias copiar-worktree-de-agente-vivo.md e entrega-partida-entre-vivo-e-integracao.md` | D22 · parcial (triagem 18/10): Número citado de 20 não bate: são 22 linhas (21 de agente). A parte de integração por cópia com duas quebras registradas está nas memórias. |
| H5 | Estados contraditorios sobre o gate de modelo: a memoria de 17/10 diz que client/hooks/agent_model_gate.py aceita qualquer model e so confere presenca. O docstring do proprio gate descreve julgamento de ajuste por class… | `client/hooks/agent_model_gate.py:45-59 (docstring com julgamento de ajuste); memoria roteamento-de-modelo-so-exige-presenca (linhas 1-14, '…` | D23 · corrigido (17/10): o gate confere modelo × tipo; a memória roteamento-de-modelo-so-exige-presenca foi atualizada em 18/10 |
| H5 | Regra 13 de higiene de skills citada como vigente, com arquivo de apoio marcado como em redacao. A regra existe no CLAUDE.md global mas nao tem corpo nem validador ligado. | `~/.claude/CLAUDE.md, regra 13 ('skill-hygiene.md em redacao').` | D24 · corrigido 18/10: a linha #13 do CLAUDE.md global aponta para os executores reais (test_skill_size_guard.py, package_skill.py) |
| H5 | Campo com_par_na_fonte do inventario ECC (inventario-ecc.json, chave adaptadores) usa pareamento por nome-base e aceita o primeiro candidato. Gera pares falsos, como .agents/skills/*/agents/openai.yaml apontando para sk… | `docs/plans/2026-10-17-analise-ecc/inventario/inventario-ecc.json, chave adaptadores; contagem find .agents -name openai.yaml = 46 (executad…` | D25 · corrigido 18/10: o inventário declara que o par é por nome e conta os homônimos ambíguos; números inalterados |
| H6 | `touring ast quality <arquivo inexistente>` (e diretório) devolve overall_score 1.0, antipatterns [], language 'rust' e exit 0, e injeta recompensa de RL 1.0 ('measured_overall_score'): read_to_string(...).unwrap_or_def… | `crates/touring-cli/src/cli/handlers/index.rs:1917-1926 (quality), :1953-1954, :2141-2142, :2289-2290; executado: `touring ast quality /tmp/…` | D26 · confirmado (triagem 18/10): Confirmado por leitura do código. Não rodei 'ast quality' (grava reward, vedado pelo brief). O ast tdg tem o erro explícito, como descrito. |
| H6 | `analyze_quality` conta o token do antipadrão por substring no arquivo inteiro, sem excluir comentário, string nem #[cfg(test)] (count_occurrences), enquanto a F1.6 e o `ast tdg` são região-cientes. Controle: arquivo co… | `crates/touring-code/src/ast/quality.rs:792-803 (count_occurrences), :364-420 (detect_rust_safety); crates/touring-cli/src/cli/ast.rs:300-31…` | D27 · confirmado (triagem 18/10): Substring no arquivo inteiro e fallback 0.0 confirmados. Controle com 'ast quality' não executado (grava reward); a evidência é de código. |
| H6 | Os eventos de ciclo de vida do hook (notification, stop-failure, session-end, subagent-start, post-compact, config-change, instructions-loaded, worktree-*, setup, elicitation*, cwd-changed, file-changed, post-tool-failu… | `crates/touring-hooks/src/main.rs:290-312 e :605-650 (run_lifecycle_event, record_bash_outcome em :633); crates/touring-cortex/src/handlers/…` | confirmado: nenhum hook protege ruff.toml, clippy.toml, rustfmt.toml ou eslintrc (grep em client/hooks e nos handlers) |
| H6 | Instrumento que mede a configuração: com o SLIM como default, o SessionStart nunca injeta lição (0 de 8.967 candidatos em 266 sessões), mas decision.recall_curated_share sai com status 'measured' e curated_share 0.0, e … | `client/hooks/session_startup_intelligence.py:78-84,223-256,338-348,806; crates/touring-cli/src/cli/kpi/decision.rs:355-358; `touring kpi -j…` | confirmado: nenhum hook protege ruff.toml, clippy.toml, rustfmt.toml ou eslintrc (grep em client/hooks e nos handlers) |
| H6 | `touring memory store` não varre segredo: parse_memory_store_payload só exige chave e valor não vazios. Só o minerador de transcript redige (redact_secrets). Qualquer chamada direta, de hook ou de agente grava um token … | `crates/touring-hook-runtime/src/ceg_impls.rs:112-190,192-300; crates/touring-server/src/ingest/transcript_miner.rs:520-533 (a única redação)` | D28 · corrigido no int97 (C6: sanitize_memory_text redige segredos no store); chega com o R97 |
| H6 | O repositório público não tem canal privado para relatar vulnerabilidade: SECURITY.md aponta para SUPPORT.md e SUPPORT.md aponta de volta para SECURITY.md, sem e-mail nem link; private-vulnerability-reporting, secret sc… | `SECURITY.md:9-18; SUPPORT.md:28; .github/CODEOWNERS:1-5; `gh api repos/gabrielgadea/touring/private-vulnerability-reporting` -> enabled:fal…` | confirmado: SECURITY.md:11 manda ao SUPPORT.md e SUPPORT.md:28 devolve ao SECURITY.md; não há canal privado |
| H6 | O hook_trace.jsonl (202 MB, 904.218 linhas) não grava tool_use_id nem hash do payload, só stdin_bytes: execução duplicada de hook não se distingue de chamadas paralelas de mesmo tamanho (936 coincidências em 14 dias, 0,… | `chaves de ~/.claude/touring/hook_trace.jsonl (elapsed_ms exit_reason hook pid ppid route session_id stdin_bytes stdin_state ts_ms); scratch…` | D29 · confirmado (triagem 18/10): Direção confirmada. A razão agora é 3,9x, não 2,96x: o log cresceu. Sem tool_use_id, duplicata não se distingue de chamada paralela. |
| H6 | A F1.6 (tratamento de erro) mede só o ruído alto (unwrap/expect/panic) e recompensa result_ratio: um arquivo Rust com 7 padrões de falha silenciosa (let _ =, .ok();, unwrap_or*, braços Err vazios) recebe 0,850 Gold, e u… | `crates/touring-quality/src/verifications/f1_6_error_handling.rs:55-80; `touring-quality score scratchpad/h6_silent/{rust_ctl.rs,ts_ctl.ts} …` | D30 · parcial (triagem 18/10): Rust reproduz 0,850. O TS não reproduz 0,850: o catch vazio ficou em 1,000 (Diamond). A falha do F1.6 está confirmada: o silencioso passa. |
