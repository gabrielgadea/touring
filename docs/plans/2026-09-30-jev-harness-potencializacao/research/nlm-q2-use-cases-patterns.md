---
type: Research
plan_id: 2026-09-30-jev-harness-potencializacao
title: NotebookLM JEV — nlm-q2-use-cases-patterns
sources_used: ["d79065e0-f8c6-4f73-acae-78ed59af61b9", "9b733a91-a529-4cc8-912e-08339cbdf843", "808a4edf-b4de-4204-8789-e5acdb35e8fa", "ee0a1a82-90e3-4696-8b0f-45896f358e9e", "3f9bf57d-e5c1-4831-976b-1b0530261287", "e1cfa418-f58d-4a21-9e24-0fc2ad7424c5", "359f586d-32eb-443a-ae4a-74d81b3f8cb0", "5c195ad9-dd6b-4253-8dcf-b99b72f319e5", "f7e3f442-1816-48d6-ad5e-efbb4579f784", "f3a2f14d-5165-4725-8440-b0ad3cab2f9e", "19def919-5d27-4ca6-8056-a9a1041c377d", "b6cdabb3-722d-49f6-9d2e-be3e58d47f4d", "07964567-81ba-4eff-bb86-554b7c3eb3df"]
---

O **Jev** (desenvolvido pela TypeSafe AI) é classificado nas fontes como um **System 1 Model** (modelo de julgamento e decisão rápida, análogo ao sistema intuitivo do cérebro humano) [1-3]. Ao contrário dos LLMs tradicionais (System 2, focados em raciocínio autorregressivo e geração de texto token a token) [1, 2, 4], o Jev **não gera texto nem código** e é incapaz de alucinar estruturalmente [5-7]. Ele recebe um **State** (dados brutos ou não estruturados) e um conjunto de perguntas tipadas, retornando decisões estruturadas e probabilidades calibradas em uma **única passada paralela (single pass)** com latência entre 70ms e 500ms e **custo de tokens de saída gratuito** [5, 7-11].

---

### 1. As Três Primitivas Técnicas do Jev

Todas as arquiteturas e casos de uso do Jev assentam em três primitivas fundamentais [12-16]:

1. **Noul (ou Bool / Bernoulli)** [14-16]:
   - **Definição**: Pergunta de afirmação binária (`verdadeiro` ou `falso`) [14, 15].
   - **Retorno**: Um valor de probabilidade entre `0.0` e `1.0` de a afirmação ser verdadeira [14, 17, 18]. Não possui metradado de *confidence* separado, pois a probabilidade resultante já reflete a confiança estocástica do modelo [18, 19].
2. **Choice** [14, 16, 20]:
   - **Definição**: Múltipla escolha sobre uma lista explícita e predefinida de opções categóricas (até 255 opções) [7, 20, 21].
   - **Retorno**: A opção vencedora, a distribuição de probabilidade de cada categoria e uma métrica de **Confidence** (calculada com base no grau de dispersão/entropia das probabilidades entre as opções) [16, 20, 22].
3. **Score** [14, 16, 23]:
   - **Definição**: Avaliação contínua/discreta ao longo de um espectro ou rúbrica ordenada predefinida (até 11 níveis, de 0 a 10) [23-25].
   - **Retorno**: Um valor pontuado (ex: `2.4` em uma escala de 3.0) indicando a posição relativa no espectro e uma métrica de **Confidence** [23, 26].

---

### 2. Casos de Uso e Desenhos Técnicos por Fonte

#### A. "10 Levels of Jev For Agentic Engineers" (IndyDevDan)

1. **Level 1: Basic Decision Making / Security Gate** [27-29]:
   - **Primitiva**: `Noul` [28, 30].
   - **State**: Prompt enviado pelo usuário via API [28].
   - **Thresholds**: Se `prompt_injection_prob > 0.80` (80%), bloqueia a requisição imediatamente [28, 30].
   - **Arquitetura**: *Smart IF Statement* na borda (edge) do sistema para barrar ataques de injeção de prompt em sub-segundos a custo irrisório antes de acionar modelos caros [27-29].
2. **Level 2: Support Triage** [29, 31, 32]:
   - **Primitiva**: `Choice` [29, 32].
   - **State**: Objeto JSON com ticket de suporte (título, passos de reprodução, ambiente) [29, 32].
   - **Thresholds**: Rotulagem de categoria e severidade (`Low`, `Normal`, `High`). Se `confidence < 0.60` em falha generalizada, escala para revisão humana [29, 32, 33].
   - **Arquitetura**: Roteamento ultra-rápido de chamados sem passar por LLMs [31, 33].
3. **Level 3: Composite Scoring (Risk Assessment)** [33-35]:
   - **Primitiva**: `Score` [34, 35].
   - **State**: Diff de código, arquivo de commit ou ticket de engenharia [34, 35].
   - **Thresholds**: Cálculo de score ponderado via código (`In-code Priority Weights`). Exemplo: Risco de segurança (`1.33/2.0`) + Qualidade de commit (`0.8/2.0`) [34, 35].
   - **Arquitetura**: Avaliação multidimensional onde o Jev pontua critérios individuais e o código calcula a média ponderada determinística [34, 35].
4. **Level 4: Confidence Gating (Bash Tool Gate / Danger Control)** [36-38]:
   - **Primitiva**: `Noul` + `Choice` [37, 39].
   - **State**: Comando de terminal que um agente pretende executar (`git push --force`, `rm -rf`, `ls -la`) [37, 38].
   - **Thresholds**: Se `irreversible == true` ou `destructive == true` com probabilidade `> 0.80`, o pre-hook da ferramenta intercepta e aborta a execução [37, 39, 40].
   - **Arquitetura**: Guardrail em tempo real (`JevGuard`) acoplado ao harness do agente [39, 40].
5. **Level 5: Intent & Model Routing** [41-43]:
   - **Primitiva**: `Choice` [42, 43].
   - **State**: Solicitação ou tarefa do usuário [42].
   - **Thresholds**: Seleção de agente especializado (`Browser Agent`, `Fast Agent`, `Desktop Agent`) [42, 43].
   - **Arquitetura**: Roteador na entrada de pipelines para direcionar tarefas ao modelo/agente de menor custo capaz de executá-la [41, 42].
6. **Level 6: Tool Call Safety in Agent Harness** [40, 44, 45]:
   - **Primitiva**: `Noul` / `Choice` [40, 45].
   - **State**: Chamada de ferramenta e argumentos propostos pelo agente [40, 45].
   - **Thresholds**: Bloqueio de gravações em arquivos sensíveis (ex: `.env`) ou comandos de exclusão não autorizados [45, 46].
   - **Arquitetura**: Hook síncrono no loop do agente que impede execuções nocivas sem impedir comandos seguros como `npm test` [40, 44].
7. **Level 7: Self-Compacting Agent Harness** [46-48]:
   - **Primitiva**: `Choice` / `Noul` [48].
   - **State**: Histórico da conversa, contagem atual de tokens e marcador de transição de tarefa [47, 48].
   - **Thresholds**: Avisa a 6k tokens, recomenda a 10k tokens e exige compactação a 14k tokens quando há troca de tarefa [47, 48].
   - **Arquitetura**: Padrão *Model-in-a-Model* onde o Jev monitora a janela de contexto da LLM e dispara sinal de compactação autônoma sem interferência do usuário [47, 48].
8. **Level 8: Cheap Reads / File Scouting** [49-51]:
   - **Primitiva**: `Noul` (`AskJev filebool`) e `Choice` (`AskJev filechoice`) [50, 52].
   - **State**: Conteúdo e caminho de arquivos do repositório [50, 51].
   - **Thresholds**: Avaliação se o arquivo valida tokens ou contém credenciais reais [50].
   - **Arquitetura**: Ferramenta estendida no agente. A LLM principal não lê os arquivos pesados no seu contexto; em vez disso, chama o `AskJev` para julgar os arquivos externamente por uma fração de centavo [50-52].
9. **Level 9: Parallel Files at Scale (Glob Search)** [53-55]:
   - **Primitiva**: Speculative Fan-out paralelo de `Choice` e `Noul` sobre múltiplos arquivos [54, 56].
   - **State**: Lista de caminhos de arquivos obtida via globbing (ex: todos os arquivos `.ts`) [55, 56].
   - **Thresholds**: Identificação de presença de bugs de arredondamento, `TODO` ou atalhos [56, 57].
   - **Arquitetura**: Varredura massiva em paralelo sobre o repositório em sub-segundos, filtrando os arquivos relevantes antes de entregar o resultado refinado para a LLM principal [54, 56, 57].
10. **Level 10: Agentic Jev (Self-Validation & QA Loop)** [58-60]:
    - **Primitiva**: Múltiplas perguntas simultâneas (`Noul`, `Choice`, `Score`) [59, 61].
    - **State**: Logs de saída de testes que falharam e diffs propostos [59].
    - **Thresholds**: Avaliação contínua de causa-raiz e score de risco da solução [59, 61].
    - **Arquitetura**: Loop de QA autônomo onde a LLM executa alterações e usa o Jev para validar hipóteses, diagnosticar falhas de teste e checar se restaram bugs [59, 60].

---

#### B. "Insane Jev Use Cases You Need To Use Right Now" (AI LABS)

1. **Fast Jev Compaction Plugin** [62-64]:
   - **Primitiva**: `Choice` / `Noul` [62, 63].
   - **State**: Histórico completo da sessão do agente [62].
   - **Thresholds**: Se o uso do contexto for `< 25%`, utiliza compactação padrão da LLM; se `>= 25%`, aciona o Jev [64].
   - **Arquitetura**: Hook que substitui o resumo lento de conversa por uma matriz de decisões rápidas do Jev em menos de 1 segundo [63, 64].
2. **Doc-to-Test Audit Hook (Checking Layer)** [64, 65]:
   - **Primitiva**: `Noul` [65].
   - **State**: Regras de controle de acesso nos documentos/especificações e alterações no código [64, 65].
   - **Thresholds**: Retorna `true`/`false` se existe um teste cobrindo cada regra [65].
   - **Arquitetura**: Camada de checagem interceptando edições de arquivos para garantir cobertura de testes em relação à documentação [65].
3. **Skill Picker Hook** [65, 66]:
   - **Primitiva**: `Choice` [66].
   - **State**: Nomes e descrições de todas as skills do projeto + prompt do usuário [66].
   - **Thresholds**: Seleciona exatamente 1 skill adequada ou `nenhuma` [66].
   - **Arquitetura**: Hook acionado a cada prompt que previne a injeção de dezenas/centenas de descrições de skills na janela de contexto da LLM, economizando milhares de tokens [65, 66].
4. **Search & Rank Files (Substituição do Agente Explore)** [67-69]:
   - **Primitiva**: `Score` [68].
   - **State**: Busca inicial por palavra-chave + lista de até 20 arquivos candidatos [68].
   - **Thresholds**: Score de relevância frente à pergunta do usuário [68].
   - **Arquitetura**: Substitui chamadas lentas de agentes de exploração (como Haiku/Opus), ranqueando 35 arquivos em 1.6s e abrindo apenas os mais relevantes [68, 69].
5. **Pre-Code Review Gate** [69, 70]:
   - **Primitiva**: Matriz de 7 perguntas binárias `Noul` [69].
   - **State**: Diff das alterações de código [69].
   - **Thresholds**: Se todas as 7 respostas forem `false` com alta confiança, o código passa com um review rápido de 1 rodada; se qualquer resposta for `true` ou incerta, exige o ciclo de code review completo com LLM [69, 70].
   - **Arquitetura**: Funil de validação de PRs que acelera alterações simples e reserva revisões profundas para edições arriscadas [69, 70].
6. **Browser Testing Driver** [70, 71]:
   - **Primitiva**: `Choice` [71].
   - **State**: Lista de botões, links e elementos da DOM na página atual [71].
   - **Thresholds**: Seleção da próxima ação/clique mais próxima da meta [71].
   - **Arquitetura**: O Jev decide onde clicar/navegar rapidamente enquanto a LLM principal apenas avalia a conclusão final do teste [71].
7. **CLAUDE.md / Rules Enforcement Hook** [71, 72]:
   - **Primitiva**: `Noul` [72].
   - **State**: Alteração de código proposta + regras do arquivo `CLAUDE.md` / especificações do projeto [72].
   - **Thresholds**: Se `rule_broken_prob >= 0.80` (80%), a edição de arquivo é **bloqueada antes de ser criada** e o agente é notificado do motivo [72].
   - **Arquitetura**: Pre-hook de arquivo mandatório que força conformidade estrita com diretrizes de arquitetura [72].

---

#### C. "8 Jev Use Cases That Feel Like Cheating" (Matthew Berman)

1. **AI Slop Detector**:
   - **Primitiva**: `Score` / `Noul` [73].
   - **State**: Texto do website ou escrita criativa [73].
   - **Thresholds**: Sinalização percentual de probabilidade de o texto conter "slop" de IA (ex: 26% de conteúdo gerado) [73].
2. **Unclutter (Smart Ad + Slop Blocker / DOM Cleaner)** [74]:
   - **Primitiva**: `Noul` [74].
   - **State**: Lista de elementos e seletores do DOM ao carregar uma página web [74].
   - **Thresholds**: Se `is_ad_or_cookie_banner == true`, remove o nó da árvore DOM instantaneamente [74].
3. **Zapier Integration / Workflow Decision Step** [74, 75]:
   - **Primitiva**: `Choice` / `Noul` [75].
   - **State**: Triggers de mais de 9.000 conectores (ex: novos eventos de calendário, e-mails) [75].
   - **Thresholds**: Regras personalizadas para aceitar/recusar reuniões ou encaminhar fluxos [75].
4. **Sub-half Second Inbox Sorting** [76]:
   - **Primitiva**: `Score` / `Choice` [76].
   - **State**: Lista de 100 e-mails recebidos [76].
   - **Thresholds**: Ordenação por score de prioridade em menos de 0,5 segundos [76].
5. **Fuzzy Search / Semantic Find em Navegadores** [76, 77]:
   - **Primitiva**: `Score` / `Choice` [76, 77].
   - **State**: Elementos do documento vs. query do usuário em linguagem natural [76].
   - **Thresholds**: Destaca passagens no documento baseando-se em proximidade semântica em vez de correspondência exata de palavras-chave [76].
6. **Dynamic Front-End / UI Asset Builder** [77-79]:
   - **Primitiva**: `Choice` [77].
   - **State**: Perfil e intenção do usuário + biblioteca predefinida de componentes de UI (botões, campos, fontes) [77, 78].
   - **Thresholds**: Montagem e seleção imediata do layout em menos de 1 segundo [77, 78].
7. **Clipfast (Cortador de Vídeo Automático em < 2s)** [79]:
   - **Primitiva**: `Choice` / `Score` [79].
   - **State**: Transcrição de vídeos de 90+ minutos + busca do usuário [79].
   - **Thresholds**: Identificação de carimbos de data/hora e corte imediato de clipes [79].
8. **Mapeamento de Paletas de Cores e Emojis** [79, 80]:
   - **Primitiva**: `Choice` [79, 80].
   - **State**: Conceitos abstratos ou frases (ex: "anos 80", "perder peso") [79, 80].
   - **Thresholds**: Mapeamento direto para hex de cores ou emojis correlacionados [79, 80].

---

#### D. "Every Jev Concept Explained for Claude Users" (Simon Scrapes) & TypeSafe Cookbooks

**Arquitetura Completa de Triagem e Processamento de E-mails em Cascata (50 e-mails/dia)** [81-83]:

- **State**: Objeto JSON estruturado contendo dados da mensagem (`subject`, `body`) e fatos do cliente (`customer_facts`: tempo de conta, plano atual, histórico de cobrança) [82, 84].
- **Batch de 5 Perguntas Paralelas (Single Request)** [81, 85, 86]:
  1. `primary_department`: `Choice` [`billing`, `technical`, `feature_request`, `account`, `none`] [86, 87].
  2. `asks_for_money`: `Noul` [`true` se solicita reembolso ou crédito; `false` caso contrário] [88, 89].
  3. `product_broken_severity`: `Score` [0: intocado, 1: cosmético, 2: degradado/irritante, 3: indisponível/bloqueante] [89, 90].
  4. `customer_anger`: `Score` [1: calmo, 2: irritado, 3: furioso] [23, 88, 90].
  5. `human_judgment_needed`: `Choice` [`standard`, `judgment`, `unusual`] [88, 90, 91].
- **Thresholds e Matriz de Roteamento em Código**:
  - **Rota 1 (Pessoas/Suporte Humano)**: Se `asks_for_money == true` OU `confidence < 0.60` OU `human_judgment_needed == unusual`, direciona diretamente para a fila prioritária humana [88, 91].
  - **Rota 2 (Engenharia)**: Se `product_broken_severity >= 2`, insere na fila do Jira/Linear [88, 91].
  - **Rota 3 (Logs de Produto)**: Se `primary_department == feature_request`, encaminha para o log de melhorias [88, 91].
  - **Rota 4 (Resposta Autônoma via LLM)**: Casos de rotina são enviados para rascunho com a LLM (Claude) [88, 91].
  - **Passada de Validação Pós-LLM (2º Passada com Jev)**: Checa o rascunho da LLM com duas perguntas `Noul`: *Respondeu à dúvida do cliente?* e *Prometeu desconto/reembolso não autorizado?* Se falhar, o rascunho é descartado e enviado a um operador humano [91-93].

---

#### E. "I Connected Jev to Hermes: 300 Tasks for Just \$0.001" (The AI Doctor) & TypeSafe Skill Suggestion

- **Seleção e Ranqueamento de Skills para o Agente Hermes** [94-96]:
  - **State**: Catálogo de 182 skills do agente Hermes + Prompt de instrução do usuário [94].
  - **Desenho Técnico (Cascata em 2 Estágios)**:
    - *Estágio 1*: `Choice` para ranquear as top candidatos entre as 182 skills [94].
    - *Estágio 2*: `Noul` para confirmar se a skill selecionada atende perfeitamente aos critérios antes de injetá-la no contexto do agente [94].
  - **Resultado**: Reduz a taxa de seleção da skill errada de 17% (usando LLM pura) para apenas 7,3%, economizando mais de 10.000 tokens por turno e processando 2.400 decisões de análise sobre 300 itens em 17 segundos por apenas US\$ 0,015 [94, 97, 98].

---

#### F. "Build Anything with Jev, Here’s How" (David Ondrej)

1. **Signal App (TypeForm Inteligente de Qualificação em Tempo Real)** [99-101]:
   - **State**: Respostas acumuladas do usuário no formulário em tempo real [102].
   - **Primitiva**: `Choice` / `Score` [`Highly Qualified`, `Qualified`, `Mediocre`, `Disqualified`] [100, 102].
   - **Thresholds**: Se o resultado atingir `Highly Qualified` em tempo real (< 150ms), o front-end abre imediatamente a integração com o Calendly para agendamento de reunião [102, 103].
2. **Self-Driving Car / Navegação Autônoma em Tempo Real** [104, 105]:
   - **State**: Telemetria em texto (distância dos veículos ao redor, velocidade, cor do sinal, dados de radar) [105].
   - **Primitiva**: `Choice` [`Acelerar`, `Desacelerar`, `Parar`, `Virar à Direita`, `Virar à Esquerda`] [105].
   - **Thresholds**: Execução contínua a cada 70–100ms para tomar decisões instantâneas sem depender de processamento de imagem/LLM lento [105-107].
3. **Planilhas Preditivas (Predictive Spreadsheets)** [10, 105]:
   - **State**: Linhas de uma planilha [10].
   - **Primitiva**: `Score` / `Choice` (Ex: grau de urgência de `no follow-up` a `urgent`) [10, 105].
   - **Arquitetura**: Avaliação em lote de centenas de linhas em paralelo conforme o usuário digita no cabeçalho [10, 105].

---

#### G. "Jev - The Ultimate Classification Model?" (Sam Witteveen)

1. **Linters Qualitativos de Código & Code Smells** [108-110]:
   - **Primitiva**: `Noul` / `Score` [109, 111].
   - **State**: Assinaturas de funções e corpos de código [109, 112].
   - **Thresholds**: Avalia se o nome da função descreve todos os seus efeitos colaterais ou se valores de PII/segredos estão sendo logados em texto puro [109].
2. **Roteador de Ferramentas / Agent Tool Selection** [113]:
   - **Primitiva**: `Choice` [113].
   - **State**: Solicitação do usuário e lista de ferramentas disponíveis [113].
   - **Arquitetura**: Determina qual ferramenta acionar de forma ultrarrápida (embora sem extrair argumentos complexos) [113].
3. **Doom & Bot de Jogos (System 1 Reaction Loop)** [114-116]:
   - **State**: Telemetria do jogo passada em texto (vida, munição, distância de inimigos, portas) [114, 116].
   - **Primitiva**: Matriz paralela de `Choice` / `Noul` (Mover, Atirar, Esquivar, Abrir Porta) [116, 117].
   - **Latência**: 10 decisões por segundo a \$7/hora com taxa de resposta instantânea [115].

---

### 3. Síntese dos Padrões de Engenharia Arquiteturais

1. **Gating de Confiança (Confidence Thresholding Matrix)** [36, 118, 119]:
   - Operações seguras (como rolagem de tela ou busca somente leitura) aceitam limiares de confiança baixos (`> 0.50`) [119].
   - Operações financeiras, exclusões de dados ou envios de e-mail exigem alta confiança (`> 0.85` a `0.95`). Valores abaixo da régua são interceptados e desviados para verificação humana [91, 119, 120].
2. **Fan-Out Especulativo (Speculative Fan-Out & Batching)** [119, 121, 122]:
   - Como os tokens de saída são gratuitos e o custo de entrada é de apenas **\$0,042 por milhão de tokens** [5, 10, 74], a regra é **enviar todas as perguntas possíveis em um único payload** [119, 122]. Pergunta-se severidade de bug, raiva do cliente e setor de cobrança em um único request [86, 90, 122]. O código descarta o que não for relevante com base nos resultados das escolhas primárias [119].
3. **Arquitetura em Cascata System 1 + System 2** [4, 123, 124]:
   - **Fase 1 (System 1 - Jev)**: Executa triagem massiva, filtro de regras e moderação em milissegundos [4, 124].
   - **Fase 2 (System 2 - LLM de Raciocínio)**: Apenas os casos de exceção, tarefas criativas ou de alta incerteza/risco são repassados para LLMs pesadas (ex: Opus 5.5, Claude Code ou GPT-6 Astra) [4, 124, 125].

💡 **Ideia de próximo passo**: Como o seu notebook contém diversos exemplos de integração do Jev com o Claude Code e frameworks agênticos, quer que eu monte um guia prático com snippets de código em Python/TypeScript para implementar um **roteador de intenção (Intent Router)** ou um **hook de segurança de terminal** baseado nos padrões acima?
