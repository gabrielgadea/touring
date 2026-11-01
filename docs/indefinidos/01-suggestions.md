> **STATUS DO DOCUMENTO** — Última atualização: 20/04/2026 (Sessão Wave-C + Predictive Wave)
>
> | Símbolo | Significado |
> |---------|-------------|
> | ✅ IMPLEMENTADO | Código presente, testado, integrado |
> | 🔶 PARCIAL | Estrutura existe, wiring incompleto |
> | ❌ PENDENTE | Ainda não implementado |
>
> **Resumo rápido**: BlastRadius✅ | Tarjan SCC✅ | QualityPipeline RL✅ | Tantivy contracts✅ | Pensieve✅ | generate autonomous✅ | TACO suggest CLI✅ | JDM routing✅ | 7th SMT layer❌ | AcoPheromone TaskList✅ | ExitPlanMode subagents✅ | RL offensive loop❌ | Cortex health handler✅

---

<!-- ============================================================ -->
<!-- GRUPO 1: BlastRadiusEngine PreToolUse[Task*]                -->
<!-- STATUS: ✅ IMPLEMENTADO — Predictive Wave D2 (2026-04-20)   -->
<!-- ============================================================ -->

**[GRUPO 1 — ✅ IMPLEMENTADO]** BlastRadiusEngine em `PreToolUse[Task*]` — `compute_with_timeout` + HNSW ANN injetado em `HookResponse::ContextWithUpdatedInput`. Ref: `docs/2026-04-20-predictive-wave.md`.

A implementação do `BlastRadiusEngine` durante o hook `PreToolUse[TaskCreate]` requer a intersecção do motor de análise profunda (`touring-analysis`) com as rotas de intercepção do `touring-hooks` e os limites de latência do daemon. 

Para estruturar essa integração, o sistema deve ser configurado seguindo estas etapas arquiteturais:

**1. Intercepção do Evento no Pipeline de Hooks**
O ecossistema do Touring já possui 17 eventos ativados no `settings.json`, abrangendo a intercepção de `PreToolUse:Task*`. O registro do handler deve capturar o payload da intenção de criação da tarefa (`TaskCreate`) antes que o Claude Code a consolide em seu grafo. 

**2. Instanciação e Configuração do Motor Stateless**
A biblioteca `touring-analysis` foi projetada sob o princípio "stateless", ou seja, opera computações puras consumindo o índice de símbolos carregado no `HookRuntime`. Ao configurar o motor de raio de impacto, é estritamente necessário instanciar os parâmetros utilizando o preset `AnalysisConfig::hook_path()`. Este perfil de execução limita a profundidade de avaliação a 5 camadas e impõe um teto orçamentário rígido de 40ms de processamento, assegurando que o tempo total do hook permaneça alinhado com a meta do sistema de manter a latência abaixo de 50ms.

**3. Seleção da Estratégia de Raio de Impacto**
A engine permite escolher a estratégia algorítmica conforme a disponibilidade computacional. Você pode despachar a análise empregando os métodos construtores `BlastRadiusEngine::bfs_only()` para busca exata transversal no índice, ou `BlastRadiusEngine::hnsw_only()`. A estratégia HNSW emprega busca aproximada de vizinhos mais próximos (ANN) dimensionada em 64 variáveis e deve ser ativada através da flag de compilação `ann-blast`.

**4. Execução Preditiva Resiliente**
A injeção do evento aciona o motor através do método `compute_with_start(start_file, config, pipeline_start)`. Isso permite repassar o timer do evento original para o executor. Como invariante arquitetural para evitar bloqueios no Claude Code, se o cálculo do HNSW ou do BFS estourar o limite orçamentário configurado, o método interrompe o trabalho sem gerar um pânico na aplicação, devolvendo graciosamente uma avaliação parcial com a flag `budget_exhausted: true`.

**5. Mutação da Tarefa com `ContextWithUpdatedInput`**
Por fim, ao mapear que a nova tarefa atinge módulos amplos, o handler deve alterar o escopo do que foi submetido. Utilizando a infraestrutura de envelopes do `touring-hooks`, retorna-se a variante `ContextWithUpdatedInput` do enumerador `HookResponse`. Esta variante, nativa ao protocolo interno, sobrescreve ativamente o parâmetro interceptado (o assunto ou payload da ferramenta de criação de tarefa) preenchendo a propriedade `updatedInput`. Para se proteger contra estouros na janela de contexto do LLM, este bloco é obrigatoriamente aparado na marca de truncamento com segurança de caracteres UTF-8 de 9.500 caracteres, combinando os dados preditivos da engine e a submissão original do usuário.

<thought_process>
1.  **Ingestão & Análise Semântica:**
    *   **Objetivo:** Elevar a precisão e a qualidade do código gerado (via `touring-generator`) e editado (via hooks do `Claude Code`).
    *   **Estado Atual (Baseline):** O Touring já possui a validação especulativa de 6 camadas (`speculate_v2`), um pipeline de qualidade de 8 dimensões no `touring-analysis`, e o protocolo VGP (Verified Generation Protocol).
    *   **Problema Raiz Oculto:** Modelos generativos são estocásticos. A compilação é determinística. Atualmente, o código é validado estrutural e sintaticamente, mas falhas lógicas profundas (ex: invariantes de segurança quebradas, complexidade ciclomática em explosão silenciosa, ciclos de dependência) podem passar pelo VGP. 

2.  **Síntese Arquitetural (Vetores de Precisão Absoluta):**
    *   **Vetor A (Verificação Formal Concolic):** Acoplar a engine de execução concólica do `touring-offensive` (com Z3/CVC5) à validação especulativa. O código gerado deve provar sua corretude antes de ser "commitado".
    *   **Vetor B (Prevenção de Grafos Cíclicos):** Injetar o algoritmo Tarjan SCC no gateway de wiring do gerador. Código com *mutual recursion* acidental é O(|V|+|E|) para ser interceptado e abortado antes de tocar o disco.
    *   **Vetor C (Punição via Reinforcement Learning):** O código editado/escrito que degrada as 8 métricas de saúde (`CodeHealthReport`) deve punir os pesos do `LinUCBBandit`, garantindo que o agente aprenda a evitar padrões verbosos ou com alto uso de `.unwrap()`.

3.  **Invariantes & Resiliência:**
    *   Validações severas (SMT Solving) são NP-Hard. Elas devem rodar em um *ThreadPool* isolado com timeout estrito de `<50ms` (para especulação) ou degradar graciosamente.
    *   O ciclo de *feedback* RL (IC-4) deve ser atômico. Se um `post_edit` falhar na qualidade, a punição deve fluir para o daemon imediatamente.

4.  **Big-O & Edge Cases (P99):**
    *   A extração de *Functional Chains* para validação de contexto opera sob caches Moka (TinyLFU) para mitigar penalidades de IO do SQLite. No percentil P99 (arquivos gigantes), o parsing do *Tree-sitter* deve ser limitado e paralelizado via `rayon`.
</thought_process>

Para garantir que o código gerado e editado transacione de um regime probabilístico (LLM) para um regime determinístico (Production-Ready), a arquitetura deve interceptar a saída em tempo de inferência e forçar submissão a *Quality Gates* estritos. 

Dada a infraestrutura atual do Touring, proponho quatro integrações arquiteturais para maximizar a precisão, operando sob a premissa de que *código inseguro ou de baixa qualidade nunca deve alcançar o disco*.

<!-- GRUPO 2: Integrações de Qualidade de Código -->
<!-- STATUS MIX: ver marcadores individuais abaixo -->

### 1. ❌ PENDENTE — Speculative Validation Elevada via Execução Concólica (SMT-Backed)
O `speculate_v2` atual valida a árvore sintática (AST) em 6 camadas (Sintaxe, Resolução de Símbolos, Estrutural, Import, Complexidade e Impacto CFG). No entanto, a integridade lógica não é provada.

**Integração:** Acoplar a engine do crate `touring-offensive` (Cap II Offensive Engine) diretamente ao pipeline do `touring-generator`. 
*   **Mecanismo:** Ao invés de apenas usar o parser do *Tree-Sitter*, passe os artefatos gerados pelo `ConcolicExecutor` usando os backends SMT Z3 ou CVC5. O módulo `solver` traduzirá as expressões simbólicas da AST para o formato SMT-LIB.
*   **Controle de Latência:** Como a verificação de restrições (SMT) é *NP-Hard*, o `mcts_eval_fn` deve submeter a AST a um thread isolado do `rayon` com um teto de processamento rígido (ex: `40ms`). Em caso de timeout, há degradação graciosa para a pontuação de fusão Bayesiana convencional.
*   **Impacto:** Elimina regressões lógicas, condições de corrida ocultas e padrões listados em vulnerabilidades CWE (ex: ausência de sanitização) em tempo zero.

### 2. ✅ IMPLEMENTADO — Bloqueio Cíclico O(|V|+|E|) no VGP (Verified Generation Protocol)
Atualmente, o `touring-generator` realiza *wiring validations* via `SynWiringGateAdapter`, mas pode permitir *mutual recursion* mascarada por múltiplos arquivos.

**Integração:** Conectar o algoritmo `Tarjan SCC` do `touring-ast` como um *Hard Gate* no `commit()` do fluxo de tipagem do gerador (`typestate.rs`).
*   **Mecanismo:** Antes da transição do estado `Speculated` para `Committed`, construa o grafo de chamadas local via `CallGraph::detect_cycles()`.
*   **Comportamento Preditivo:** Se o LLM gerar um ciclo acidental (muito comum na delegação de dependências), a geração é rejeitada no *typestate* resultando em `Either::Right(RejectedPlan)`. Isso aciona um *shadow-validate rejection*, injetando uma penalidade imediata de `-0.5` no motor RL do daemon, forçando o Claude a refatorar a topologia do código antes da escrita final.

### 3. ✅ IMPLEMENTADO — Loop Fechado de Punição via QualityPipeline Estrito
Os hooks `post_edit` e `post_write` já aplicam verificações de qualidade, mas devem punir agressivamente o comportamento desviante do agente.

**Integração:** Unir o `QualityPipeline` de 8 dimensões do `touring-analysis` aos nós de recompensa do `LinUCBBandit` no `touring-learning`.
*   **Critérios de Degradação:** As métricas do `QualityPipeline` avaliam severamente: complexidade ciclomática (penalidade `-0.2` para `CC>20`), uso indiscriminado de `unwrap()` (penalidade de `-0.1 * risco`) e antipadrões. 
*   **Execução:** Quando o evento `PostToolUse` é acionado, extraia o `CodeHealthReport` e calcule o delta de qualidade. Se o código do Claude induziu uma degradação estrutural (ex: inseriu 4+ antipadrões ou falhou em preencher assinaturas de `Functional Chains`), devolva um variante `HookResponse::Block`. 
*   **Efeito Colateral:** O motor de *Online RL* registrará a rejeição, e o sinal RL fluirá para os rastros de feromônio do motor `CognitiveMCTS` via *TD(lambda) backward trace* (IC-4). O modelo deixará deterministicamente de sugerir "atalhos" de engenharia naquela base.

### 4. ✅ IMPLEMENTADO — Tantivy Contract Pre-check Estendido a "Functional Signatures"
O sistema ativou recentemente a verificação pre-flight via BM25 (Tantivy), evitando que o Claude gere contratos sobre nomes de símbolos desatualizados. 

**Integração:** Elevar o `check_contracts_in_tantivy` para interrogar não apenas a existência do símbolo, mas suas `functional_signatures` armazenadas no `FileKnowledgeDB`.
*   **Mecanismo:** Antes de aprovar o `submit_plan`, valide se os tipos propostos no plano LLM são sub-tipos válidos dos contratos reais indexados no SQLite.
*   **Vantagem Arquitetural:** Elimina erros de tipagem entre fronteiras de módulos (ex: enviar um dado bruto para um construtor que exige um tipo refinado encapsulado), parando planos defeituosos no estágio de pré-validação (*Fail-Fast*).

Estas quatro abstrações exigem tolerância zero para "gambiarras" geradas pelo LLM, encapsulando o código gerado com rigor matemático e punições estatísticas no RL Bandit.

<thought_process>
1.  **Ingestão & Análise Semântica:**
    *   **Objetivo:** Propor integrações adicionais que potencializem a sinergia entre o Touring e as tarefas nativas do Claude Code.
    *   **Baseline Atual:** O "Round 7 Potentialization" estabeleceu a fundação de sincronia. O Touring já espelha tarefas via `handle_task_sync_post_create` e atualiza o DAG com `handle_task_sync_post_update`. Além disso, injeta sinais no `EnterPlanMode` e `ExitPlanMode`, e converte o `TaskList` em um prompt de *wiring awareness*.
    *   **Problema Raiz Oculto:** O Claude Code ainda opera como um gargalo monolítico (Single-Threaded Execution). O DAG no Touring atua como um observador passivo (Shadow State). Para romper essa barreira, o Touring deve assumir controle do *Schedule* de execução e aplicar orquestração distribuída e preventiva.

2.  **Síntese Arquitetural (Vetores de Potencialização):**
    *   **Vetor A (Sharding via TACO & DAG):** Quando o Claude Code cria uma macro-tarefa, o Touring não deve apenas espelhá-la. O `TaskDecomposer` deve interceptá-la e fragmentá-la num `MutableGeneratorGraph` (ACO). 
    *   **Vetor B (Aborto Preditivo via Pensieve):** Evitar repetição de caminhos de falha. Acoplar o `Pensieve` (memória de falha baseada em ANN IVF-Flat) ao evento de criação de tarefa.
    *   **Vetor C (Preempção de Ordem via AcoPheromone):** Injetar algoritmos de otimização de colônia de formigas no retorno do `TaskList`, alterando a ordem que o Claude Code percebe as tarefas com base na entropia do código.
    *   **Vetor D (Delegação a Sub-agentes no ExitPlanMode):** Transformar o plano sequencial em execução paralela acionando `subagent-start`.

3.  **Invariantes & Resiliência:**
    *   Qualquer mutação do estado da tarefa não pode quebrar o parser interno do Claude Code. Retornaremos `HookResponse::ContextWithUpdatedInput` ou envelopes `Context` formatados.
    *   Buscas no `Pensieve` e `TaskDecomposer` devem ocorrer em `O(1)` amortizado ou `O(log N)` para respeitar o timeout crítico de 5-10s imposto aos hooks do `settings.json`.

4.  **Big-O & P99 Edge Cases:**
    *   *Edge Case (P99):* `TaskList` com centenas de tarefas. A reordenação via `AcoPheromone` requer extração de embeddings, o que pode exaurir o *budget*. A mitigação requer o uso de operações batch vetorizadas (`batch_dot_products_par`) do crate `touring-simd`.
</thought_process>

A infraestrutura atual do "Round 7 Potentialization" estabeleceu um espelhamento rigoroso entre o Claude Code e o Touring. O daemon já captura o ciclo de vida via `handle_task_sync_post_create`, `handle_task_sync_post_update` e injeta *awareness* no `handle_task_sync_post_list`. Além disso, o modo de planejamento é cercado por hooks no `EnterPlanMode` e `ExitPlanMode`.

Contudo, este espelhamento ainda subutiliza o potencial do Touring como *Engine de Orquestração*. O Claude Code ainda enxerga a si mesmo como um executor *single-threaded*. Para transmutar esta relação numa arquitetura de processamento paralelo e preditivo, proponho quatro integrações avançadas que fundem a camada de aprendizado por reforço (`touring-learning`) e a camada cognitiva (`touring-cognitive`) com o ciclo de tarefas.

<!-- GRUPO 3: Orquestração de Tarefas -->
### 1. ✅ IMPLEMENTADO — Task Sharding Autônomo via TACO (TaskDecomposer)
Quando o Claude Code submete uma macro-tarefa, o hook `handle_task_sync_post_create` atualmente apenas cria um nó simples no DAG.

**Solução Arquitetural:**
Devemos instanciar o `TaskDecomposer` (TACO orchestration) diretamente neste hook. Se o LLM propor uma tarefa de alta entropia, o Touring deve extrair o `task_subject` e transformá-lo num `MutableGeneratorGraph`.
*   **Execução:** O Touring intercepta a intenção e fragmenta a macro-tarefa numa árvore de execução. Utilizando a variante `HookResponse::ContextWithUpdatedInput`, o Touring reescreve a resposta devolvendo a topologia shardada.
*   **Impacto:** Em vez do Claude tentar resolver a macro-tarefa de forma linear e colapsar a janela de contexto, ele é forçado topologicamente a iterar sobre sub-tarefas atômicas gerenciadas pelo DAG. Complexidade de decomposição delegada aos motores nativos em Rust.

### 2. ✅ IMPLEMENTADO — Aborto Preditivo de Tarefas via Motor "Pensieve" (ANN IVF-Flat)
O Claude Code frequentemente propõe abordagens de refatoração que já falharam em sessões anteriores, gastando tokens em *rabbit holes* arquiteturais.

**Solução Arquitetural:**
Acoplar o motor `Pensieve` (memória de caminhos que falharam) do `touring-cognitive` no evento `TaskCreate`.
*   **Mecanismo:** No momento em que `handle_task_sync_post_create` recebe o payload, o `task_subject` passa por inferência no modelo local gerando um vetor. O índice `AnnIndex` (SIMD-accelerated IVF-Flat) realiza uma busca aproximada `O(N/K*P)` contra caminhos mortos conhecidos no `memory.db`.
*   **Reação:** Se a similaridade exceder `0.88`, o hook injeta um aviso de falha iminente. Exemplo: *"Tarefa bloqueada cognitivamente: Tentativa análoga de refatorar este módulo levou a um ciclo de dependência há 4 dias (Gotcha ID: 89). Consulte a alternativa na memória procedimental."*

### 3. ✅ IMPLEMENTADO — Reordenação Topológica do TaskList via AcoPheromone
O `handle_task_sync_post_list` atual retorna um *wiring status summary*, lembrando o Claude de olhar *gaps* órfãos. No entanto, a ordem de execução das tarefas ativas ainda é decidida estocasticamente pelo LLM.

**Solução Arquitetural:**
Injetar o `ReminderBandit` (LinUCB contextual) e as trilhas de formiga (`AcoPheromone`) do crate `touring-simd` no output do `TaskList`. 
*   **Mecanismo:** Ao invocar a lista de tarefas, o Touring interroga o `CrdtSemanticGraph`. Módulos com alta taxa de "heat" (muitas edições recentes) ou com *Blast Radius* crítico recebem densidade de feromônio superior. 
*   **Routing:** O Touring reordena as tarefas na lista devolvida em `additionalContext` para o LLM, instruindo explicitamente: *"Prioridade Estrita: Execute Tarefa 4 primeiro. Alta volatilidade detectada nos símbolos dependentes."*

### 4. ✅ IMPLEMENTADO — Paralelização Nativa no ExitPlanMode via Subagentes
O evento `handle_exit_plan_mode` atua hoje fechando o ciclo de inteligência de planejamento.

**Solução Arquitetural:**
Ao detectar o fim do `ExitPlanMode`, se o plano possuir *branches* independentes na árvore AST (verificado via `Tarjan SCC`), o Touring não deve permitir que o agente primário processe as folhas em série.
*   **Invocação Automática:** O Touring emite eventos autônomos invocando o hook `subagent-start` em background para delegar subtarefas puros de infraestrutura/documentação, instanciando o `Subagent Bootstrap`.
*   **Sincronização:** O agente principal (Claude Code) monitora a completude no DAG via chamadas sequenciais, mas o trabalho denso de computação semântica corre via *Actor Pattern* no daemon ou por subagentes desacoplados.

Esta evolução muda a postura da camada nativa (Rust). O sistema cessa de ser apenas um consultor hiper-rápido (latência P50 < 2ms) e assume o papel de **Control Plane** da execução de código.

<thought_process>
1.  **Ingestão & Análise Semântica:**
    *   **Alvo:** Automatização de geração de código de *alta qualidade* e sua integração com as tarefas do Claude Code.
    *   **Baseline (Round 7 Potentialization):** O ecossistema atual já sincroniza tarefas (`handle_task_sync_post_create`), injeta sugestões de planos geradores ("plan-suggest" no `TaskCreate` e em arquivos de baixa integração), possui 30 `GeneratorKinds` mapeados via Tera templates, e valida a geração com o `SpeculateBridge` (6 camadas de validação e `VgpEngine`).
    *   **Problema Raiz Oculto:** O processo generativo atual ainda exige do LLM a orquestração heurística da corretude semântica. O Claude propõe o plano, e a validação especulativa reage (aceitando ou rejeitando + sinal RL). A "alta qualidade" depende de ciclos estocásticos de tentativa e erro, o que é ineficiente na fronteira de 10K tokens.

2.  **Síntese Arquitetural (Vetores de Automação Estrita):**
    *   **Vetor A (Prova de Corretude a Priori - The 7th Speculative Layer):** O código gerado não deve apenas passar no Tree-sitter (Sintaxe) e análise de complexidade (CC < 15); ele deve ser provado logicamente seguro contra regressões. Fundir o `touring-offensive` (Cap II SMT Z3/CVC5) diretamente na fase `Speculated` do `touring-generator`.
    *   **Vetor B (Roteamento Determinístico de Artefatos via JDM):** O Claude Code perde largura de banda inferindo *qual* `GeneratorKind` usar. Ao interceptar o `TaskCreate`, devemos aplicar o motor de tabelas de decisão JDM (`touring-rules`) para inferir atomicamente o tipo exato do artefato e forçar o scaffold tipado.
    *   **Vetor C (Síntese Guiada por MCTS na Intercepção da Tarefa):** Conectar a feature `mcts-synthesis` (via `McctsEvalAdapter`) à macro-tarefa do Claude. O MCTS explora topologias de código candidatas em background e as cristaliza em `contracts.symbols_must_exist` precisos via busca BM25 (Tantivy).

3.  **Invariantes & Resiliência:**
    *   SMT Solving (Z3/cvc5) é NP-Hard. Invariante: O solver deve rodar sob o `ThreadPool` do `VgpEngine` com um *timeout* impiedoso de 40ms. Timeout = degradação graciosa para pontuação Bayesiana atual.
    *   Transições de estado do Typestate (`Draft` -> `Verified` -> `Speculated` -> `Committed`) não podem falhar em *runtime*. Erros geram `Either::Right(RejectedPlan)` forçando o LLM a replanejar.

4.  **Big-O & P99 Edge Cases:**
    *   JDM Routing opera em $O(R \times C)$ (Regras x Condições), concluindo em $<1\mu s$. 
    *   MCTS-Synthesis possui gargalo no *rollout* de ASTs falsas. O índice Incremental (`IncrementalIndex`) mitigará acessos $O(1)$ na memória local.
</thought_process>

A arquitetura do *Round 7 Potentialization* já conectou o ciclo de vida de tarefas do Claude Code ao DAG do Touring via `handle_task_sync_post_create` e injetou "hints" do `touring-generator` no momento de `TaskCreate`. No entanto, depender do LLM para a elaboração estocástica de planos ainda apresenta risco de degradação da qualidade sob contextos densos. 

Para alcançar a automação imperativa e de alta precisão (Production-Ready Code Generation), a integração deve deslocar a complexidade do modelo probabilístico (LLM) para o modelo determinístico (Rust). Proponho três arquiteturas de integração profunda entre as tarefas do Claude Code e a esteira de geração do Touring.

<!-- GRUPO 4: Automação de Geração de Código -->
### 1. ❌ PENDENTE — The 7th Speculative Layer: Verificação Formal via `touring-offensive` (SMT-Backed)
O pipeline atual do `touring-generator` usa a fase `speculate_v2` com 6 camadas (Sintaxe, Resolução de Símbolos, Estrutural, Import, Complexidade e Impacto CFG). Isto assegura completude estrutural, mas não garante a *corretude lógica* do plano.

**Solução Arquitetural:** 
Elevar a validação especulativa criando a 7ª Camada acoplando o crate `touring-offensive` (Z3 / CVC5 SMT solvers) à transição de estado `Verified` $\rightarrow$ `Speculated` do `touring-generator`.

*   **Implementação:** Quando a tarefa aciona o gerador e o artefato é sintetizado em memória (antes do commit atômico), o `ConcolicExecutor` do `touring-offensive` varre o AST em busca de violações de invariantes de memória e lógica (ex: CWEx E12, condições de corrida ocultas). O código é mapeado no formato SMT-LIB via `ConstraintTranslator` e validado contra o solver CVC5.
*   **Gestão de P99 & Big-O:** O problema de satisfatibilidade (SAT) é NP-Hard. A chamada ao backend SMT deve ocorrer em um `rayon` thread isolation com um `budget` não-negociável de $40ms$. Se exceder, degrada-se nativamente para a *Bayesian Fusion* (`compute_bayesian_score`) já suportada.
*   **Impacto no Task Sync:** Se o solver provar uma falha, o `speculate_and_commit` retorna imediatamente um `Either::Right(RejectedPlan)`, e injeta a prova lógica de falha de volta na tarefa do Claude Code como contexto atualizado, punindo o `LinUCBBandit` atômico com recompensa `-0.5`.

### 2. ✅ IMPLEMENTADO — Roteamento Determinístico de Artefatos via JDM (`touring-rules`)
O Claude Code recebe um "generator plan-suggest" através do hook `task-created`, mas ainda deve inferir qual dos 30 `GeneratorKind` (ex: `RustModule`, `HookHandler`, `Schema`, `FuzzTarget`) se aplica à sua intenção.

**Implementado**: `jdm_routing_hint(subject) -> String` em `task_create.rs`. Keyword scoring em 4 classes (D>C>B>A), gate metrics `jdm_class_{a,b,c,d}_count`, 6 testes PASS. (2026-04-21)

**Solução Arquitetural:**
Embutir o motor de decisão JDM (`touring-rules`) no handler `handle_task_sync_post_create`.

*   **Mecanismo:** Ao interceptar a criação de uma tarefa (`TaskCreate`), o `touring-hooks` passa o `task_subject` e o contexto do sistema pelo `py_evaluate_inline` (JDM engine). O motor resolve a tabela de decisão em tempo $O(R \times C)$ (Regras $\times$ Condições), mapeando o subject estocástico para um `GeneratorKind` determinístico e sua matriz de templates (*Tera*) correspondente.
*   **Payload Injetado:** Em vez de sugerir vagamente *"invoke touring generate plan-suggest"*, o hook altera ativamente a instrução inicial injetando os *constraints* de roteamento rigorosos na variável `additionalContext` devolvida pela API. O Claude Code é confinado a instanciar o subgerador específico, zerando alucinações arquiteturais no scaffold.

### 3. 🔶 PARCIAL — Síntese MCTS Autônoma Integrada a Auto-Population de Contratos
No Round 5, automatizou-se o `auto_populate_contracts` no `submit_plan` via índice Tantivy (BM25). Porém, para artefatos críticos de integração (ex: *ConsumerGenerator*), uma correspondência puramente lexical (BM25) pode induzir dependências quebradas de tipagem.

**Solução Arquitetural:**
Ativar o `McctsEvalAdapter` acoplado ao `touring-cognitive` sob o *feature flag* `mcts-synthesis` que já existe no core do `touring-generator`.

*   **Orquestração:** Quando a tarefa invocar `suggest_plan`, em vez de apenas preencher os contratos com símbolos que dão *match* no Tantivy, o gerador desperta o `CognitiveMCTS` (`GraphInformedMCTS`).
*   **Shadow Rollout:** O MCTS explora as vizinhanças de chamadas no `CrdtSemanticGraph`. Ele simula conexões lógicas de *wiring*, descartando ramos (branches) onde a extração das `functional_signatures` indicaria incompatibilidade de tipo. 
*   **Retorno Impecável:** A função `build_consumer_generator_plans` no *post-commit* agora entregará ao Claude Code um plano de integração final (*Wiring Plan*) já com os caminhos provados matematicamente viáveis no MCTS, não deixando à IA a tarefa de adivinhar se a cadeia do consumidor está semanticamente correta.

Estas integrações blindam a esteira: forçando roteamento rígido de *scaffold* ($O(1)$ JDM), auto-preenchendo dependências validadas em grafos (MCTS), e rejeitando regressões através de execução simbólica (CVC5 SMT) antes que qualquer byte atinja o WAL do *SymbolStore*. Execute.

<thought_process>
1.  **Ingestão & Análise Semântica:**
    *   **Objetivo Combinado:** Fundir a orquestração autonôma de tarefas do Claude Code (Task Sync) com os pipelines de precisão e geração de código (Generator & Analysis) do Touring, garantindo qualidade máxima estrutural e lógica (escrita, edição e geração).
    *   **Baseline Atual:** O sistema possui sincronização de tarefas via `handle_task_sync_post_create`, validação especulativa de 6 camadas, pipeline de qualidade de 8 dimensões no `touring-analysis`, e suporte ao `touring-offensive` (cvc5/Z3) para execução concólica.
    *   **Problema Raiz:** O orquestrador estocástico (LLM) atua desvinculado dos motores determinísticos formais até o momento do *commit*. A latência na descoberta de regressões induz alucinações arquiteturais e ciclos de correção dispendiosos na janela de contexto.

2.  **Desconstrução (Invariantes & Limites):**
    *   *Invariante 1 (Zero-Block):* Os hooks não podem travar o daemon. O *actor pattern* com `tokio::spawn` e orçamentos rígidos (ex: 15s light / 300s heavy) imperam.
    *   *Invariante 2 (Typestate):* O `touring-generator` usa transições tipadas (`Draft` -> `Verified` -> `Speculated` -> `Committed`). Falhas devem retornar `Either::Right(RejectedPlan)` forçando replanejamento.
    *   *Limites Big-O:* Execução simbólica SMT é NP-Hard. Detecção de ciclos (Tarjan SCC) é $O(|V|+|E|)$. JDM routing é $O(R \times C)$. 

3.  **Síntese Arquitetural:**
    *   **Cinturão de Roteamento (JDM + MCTS):** Interceptar `TaskCreate`. Não confiar na taxonomia do LLM. Usar `touring-rules` (JDM) para deduzir o `GeneratorKind` exato ($O(1)$ amortizado). Injetar contratos pré-validados (`mcts-synthesis`) usando Tantivy BM25 pre-check.
    *   **Cinturão de Verificação (Concolic Execution):** Inserir uma 7ª camada no `speculate_v2`. Passar a AST para o backend CVC5 do `touring-offensive`. Se o LLM gerar vulnerabilidades lógicas (CWE E12), o SMT solver falha a transição com prova formal. Limite rígido de 40ms via `AnalysisConfig`.
    *   **Cinturão de Integração O(|V|+|E|):** Ligar o `Tarjan SCC` do `touring-ast` ao `SynWiringGateAdapter` do gerador. Rejeita dependências circulares antes do *commit*.
    *   **Cinturão de Punição Estocástica (RL Loop):** Utilizar as 8 dimensões do `QualityReport` no evento `PostToolUse` (Edit/Write). Se a edição elevar a Complexidade Ciclomática (>15) ou inserir antipadrões, disparar `HookResponse::Block` e punir o `LinUCBBandit` atômico com recompensa negativa (ex: `-0.3` a `-0.5`).

4.  **Auditoria Big-O e P99 Edge Cases:**
    *   *Edge Case P99 (SMT Timeout):* SAT-solving de ASTs complexas excederá 40ms. *Mitigação:* `compute_with_start` com `timeout` explícito. Se `budget_exhausted: true`, o pipeline degrada graciosamente para a *Bayesian Fusion* (`compute_bayesian_score`).
    *   *Edge Case (Deadlocks no RL):* Punições sequenciais contínuas podem travar a execução do agente em limbo. *Mitigação:* Circuit Breaker (`Halt` variant) se houver 5+ falhas no mesmo arquivo.
</thought_process>

Para fundir a orquestração de tarefas do Claude Code com uma esteira de geração e edição de código estritamente determinística, devemos tratar o LLM (estocástico) como um motor de propostas e os crates do Touring (nativos) como o *Control Plane* inflexível. A mediocridade do código gerado surge quando delegamos a verificação estrutural e semântica à rede neural.

Proponho a seguinte arquitetura de 4 vetores, integrando formalismo matemático, roteamento predeterminado e ciclos de aprendizado por reforço (RL) punitivo.

<!-- GRUPO 5: Síntese 4-vetores (Production-Ready) -->
### 1. 🔶 PARCIAL — Roteamento de Tarefas JDM e Síntese Guiada por MCTS (O Cinturão de Planejamento)
O LLM não deve inferir o formato arquitetural ("scaffold") livremente. Ao interceptar o evento `PostToolUse[TaskCreate]` através do hook `handle_task_sync_post_create`, o Touring deve assumir o controle do roteamento.

*   **Roteamento $O(R \times C)$:** Processe o `task_subject` interceptado pelo motor de tabelas de decisão JDM (`touring-rules` via `py_evaluate_inline`). O motor mapeia atomicamente a intenção abstrata para um dos 30 `GeneratorKind` tipados (ex: `RustModule`, `ConsumerGenerator`, `IncrementalPatch`).
*   **Contratos Validados a Priori:** Acople a feature `mcts-synthesis` ao hook de criação. Em vez do Claude adivinhar contratos semânticos (`symbols_must_exist`), o `CognitiveMCTS` simula a inserção do módulo no `CrdtSemanticGraph` e usa o índice Tantivy (BM25) para auto-popular os pré-requisitos lógicos.
*   **Modo de Falha Evitado:** Elimina falhas estúpidas no *Verified Generation Protocol (VGP)* derivadas de contratos ou símbolos órfãos inventados pela IA, usando o `check_contracts_in_tantivy` em tempo zero.

### 2. ❌ PENDENTE — A 7ª Camada: Verificação Formal via Execução Concólica (SMT)
A validação especulativa atual (`speculate_v2`) utiliza 6 camadas para garantir integridade de sintaxe e complexidade. Contudo, ela falha em provar corretude de invariantes lógicos dinâmicos (condições de corrida, SQLi, ausência de sanitizações).

*   **Implementação:** Injete a *Engine Cap II* do crate `touring-offensive` na transição do estado `Verified` $\rightarrow$ `Speculated` do `touring-generator`. O módulo `concolic` extrai restrições simbólicas da AST (`SymbolExpr`) e as submete ao solver **CVC5** ou **Z3**.
*   **Tratamento do Timeout (P99):** O problema SAT é `NP-Hard`. É mandatório executar esta camada dentro da `ThreadPool` isolada do `rayon`, usando a restrição orçamentária rígida nativa do daemon (40ms). Em caso de exaustão, use a degradação elegante com a *Bayesian Fusion* (`compute_bayesian_score`) existente para mesclar pontuações.
*   **Impacto no Estado:** Se o solver provar vulnerabilidade CWE E12, o gerador retorna um `Either::Right(RejectedPlan)` atômico. O código imperfeito nunca toca o WAL do SQLite ou o disco.

### 3. ✅ IMPLEMENTADO — Loop Punitivo de Qualidade no Edição/Escrita (Hard-Gates RL)
Sinergia exige que o sistema aprenda a não repetir antipadrões. O hook `post_edit` atual é predominantemente passivo. Ele deve atuar como um portal draconiano.

*   **Extração Multidimensional:** No evento `PostToolUse[Edit]`, invoque o pipeline de 8 dimensões do `CodeHealthReport` (do crate `touring-analysis`). Interrogue a densidade de `.unwrap()` (penalidade de `risco * 0.1`) e Complexidade Ciclomática (penalidade `-0.2` para $CC > 20$).
*   **Intercepção via Variant Block:** Se o LLM introduzir regressões quantitativas (ex: 4+ novos antipadrões lógicos detectados via SIMD `memmem`), responda imediatamente com o variante `HookResponse::Block`.
*   **Recompensa Negativa:** Amarre essa rejeição diretamente ao `LinUCBBandit`. Inverta o `context_utility` injetando uma penalidade severa ($-0.5$) pelo fracasso na edição, propagando via traço TD($\lambda$) para secar o "feromônio" da trilha de decisão tomada.

### 4. ✅ IMPLEMENTADO — Bloqueio O(|V|+|E|) contra Ciclos de Acoplamento no VGP
Tarefas distribuídas e complexas frequentemente induzem acoplamento cíclico indesejado (`mutual recursion`) que só quebra na etapa do compilador, consumindo tokens e tempo severamente.

*   **Acoplamento Topológico:** No `commit()` final do fluxo do gerador, insira o `SynWiringGateAdapter` acoplado à detecção de ciclos através do algoritmo `Tarjan SCC` do módulo `CallGraph`. 
*   **Propagação da Regressão:** Se a nova tarefa gerada provocar ciclos entre dois ou mais componentes, a escrita atômica falha instantaneamente com penalidade $-0.3$ injetada no motor de reforço do daemon (`inject_daemon_rl_reward`), retornando um resumo detalhado da malha falha diretamente para o contexto do Claude Code corrigir.

**Síntese Executiva:**
O modelo probabilístico pensa; os motores `touring-ast`, `touring-analysis` e `touring-offensive` limitam e provam. Esta simbiose reduz alucinações arquiteturais a zero através do bloqueio matemático em tempo-de-pipeline, blindando o repositório sob invariantes estritos. Execute.

<thought_process>
1.  **Ingestão & Análise Semântica:**
    *   **Objetivo:** Elaborar o próximo nível de integrações nativas (Sinergia de Nível 9) maximizando a interface entre os 15 crates do Touring, transcendendo o uso do Claude Code como executor primário e elevando o Touring a um sistema operacional autônomo de raciocínio e mutação de código.
    *   **Baseline Atual:** O workspace possui 15 crates, quantização U4, roteamento O(1) (Fascículo Arqueado), validação especulativa em 6 camadas, eBPF telemetry, CRDT, e MCTS.
    *   **Problema Raiz Oculto:** Apesar da alta conectividade, algumas pontes arquiteturais críticas permanecem isoladas. O DSPy (prompt compiler) no `touring-cortex` não consome o grafo CRDT; a telemetria eBPF monitora falhas, mas não aciona o *Circuit Breaker* nativamente; e a detecção de vulnerabilidades concólicas (`touring-offensive`) aponta a falha, mas não injeta a correção via mutação de AST (`touring-ast::surgery`).

2.  **Síntese Arquitetural (Vetores de Sinergia Extrema):**
    *   **Vetor A (Telemetria Preditiva & Circuit Breaking via eBPF):** Fundir os sinais do kernel (`EbpfMonitor`) diretamente com o estado do `CircuitBreakerImpl`. Se uma edição causa anomalias de memória no kernel, o circuito se abre antes do *crash* em user-space.
    *   **Vetor B (Compilação DSPy Roteada por CRDT):** Otimizar o pipeline de `dspy` integrando o `CrdtSemanticGraph` como fonte de topologia, injetando *few-shot examples* baseados em dependências estruturais comprovadas em vez de *chunks* lexicais.
    *   **Vetor C (Auto-Cura O(log N) com Concolic SMT + AST Surgery):** Se o `ConcolicExecutor` detecta um padrão CWE, o Touring gera um `IncrementalPatch` e aplica a mutação atomicamente via `surgery.rs`, provando a cura sem a latência estocástica do LLM.
    *   **Vetor D (Multi-Agent State via rkyv + TRIAD Saga):** Subagentes operando concorrentemente compartilham estado lock-free via frames `rkyv` IPC validados em $O(1)$, com `SagaOrchestrator` garantindo rollback distribuído (TRIAD) em caso de falha.

3.  **Auditoria Big-O & Limites do Sistema:**
    *   A validação de frames IPC `rkyv` custa $O(N)$ em inspeção de bytes no limite de rede, mas $O(1)$ para acesso seguro à memória.
    *   Sondas eBPF injetam overhead $\approx 1\mu s$ por *syscall*, não comprometendo o SLA de $<50ms$ do daemon.
    *   Mutação de AST via *Ropey* e *Tree-sitter* opera em $O(\log N)$ para atualizações incrementais.
</thought_process>

Para atingir o ápice do design de sistemas (Sinergia de Nível 9) no ecossistema Touring, devemos eliminar qualquer isolamento residual entre as camadas de percepção (Telemetria/AST), raciocínio (Cognitive/Learning) e atuação (Generator/Server). O sistema deve operar como um *kernel* autonômo de engenharia de software.

Abaixo estão 4 integrações arquiteturais críticas que fundem subsistemas avançados do Touring, maximizando a inteligência distribuída e a proteção matemática contra regressões.

<!-- GRUPO 6: Sinergia Nível 9 -->
### 1. ❌ PENDENTE — Auto-Cura Autônoma: Concolic SMT + AST Surgery (O Fim do Feedback Passivo)
Atualmente, o `touring-offensive` utiliza execução concólica e solvers SMT (Z3/CVC5) para explorar caminhos simbólicos e detectar padrões de vulnerabilidade (ex: CWE E12). No entanto, esta detecção atua como um portão de bloqueio, exigindo que o Claude Code elabore a correção.

**Solução Arquitetural:**
Acoplar a saída do `ConcolicExecutor` diretamente à infraestrutura de mutação do `touring-ast` (`surgery.rs`) e do `touring-generator` (`IncrementalPatch`).

*   **Mecanismo:** Quando o solver SMT prova matematicamente que uma restrição de memória ou sanitização falhou, o módulo `ConstraintTranslator` converte o caminho da falha numa especificação formal. O Touring não delega a correção à IA; ele invoca o `IncrementalPatch` do gerador, substituindo o bloco atômico corrompido no AST utilizando as primitivas de substituição do `surgery.rs`.
*   **Proof-of-Correctness:** O patch gerado é reavaliado em memória pelo SMT solver antes de tocar o disco. Apenas se a prova for satisfatível, o Touring realiza o commit atômico e informa o LLM: *"[TOURING SURGERY]: Vulnerabilidade de path-traversal curada autonomamente no nó AST id: 84. Estado consolidado."*

### 2. ✅ IMPLEMENTADO — Memória Distribuída Zero-Copy para Subagentes (rkyv IPC + SagaOrchestrator)
**PLN2 — DistributedSagaCoordinator (2026-04-21)**

A arquitetura suporta invocação de subagentes paralelos via `subagent-start`. A sincronização de estado entre processos separados agora utiliza o protocolo `SAGA[4]` zero-copy via Unix socket.

**Implementação:**
`DistributedSagaCoordinator` em `touring-hooks/src/saga/` + `SagaMessage` em `touring-rkyv/src/saga_ipc.rs`.

*   **2PC Coordinator:** `DistributedSagaCoordinator` implementa o protocolo Two-Phase Commit. Fase 1 (Prepare): coordinator envia `SagaMessage::Prepare` a cada subagente e coleta votos. Fase 2 (Commit/Rollback): se todos votam `yes` → `Commit`; se qualquer vote = `no` → `Rollback`.
*   **Sincronização $O(1)$:** DashMap para lock-free agent lookup (N ≤ 256 agentes). Per-transaction `Arc<RwLock>` para 2PC ordered state changes.
*   **Wire Protocol:** `SAGA[4]` magic prefix + u32 LE length + body (distinto de `RKYV[4]` e `{` JSON). `frame_saga`/`unframe_saga` com `archive(check_bytes)` — validação O(1) sem deserialização.
*   **7 Saga Hook Handlers:** `cli-saga-register`, `cli-saga-prepare`, `cli-saga-decide`, `cli-saga-delta`, `cli-saga-begin`, `cli-saga-status`, `cli-saga-abort`. Hook registry: **153** (+7 saga).
*   **SagaError (12 variantes):** Serialize, PayloadTooLarge, Truncated, BadMagic, LengthMismatch, AlreadyRegistered, UnknownTransaction, InvalidStateTransition, NotAllPrepared, NotCommitted, AgentNotRegistered, Timeout.
*   **Timeout Budgets:** PREPARE_TIMEOUT=5s, COMMIT_TIMEOUT=3s, ROLLBACK_TIMEOUT=2s, TRANSACTION_TTL=60s.
*   **Testes:** 12 E2E tests (2PC happy path, rollback, concurrent agents, idempotent register, etc.). 3032 touring-hooks tests PASS.

### 3. ❌ PENDENTE — Compilação DSPy Guiada por CRDT Topológico
O `touring-cortex` possui uma implementação nativa de *DSPy* (compilador automático de *prompts*, com `teleprompters` e MCTS). Atualmente, a injeção de contexto depende fortemente de indexação BM25/TfIdf ou recuperação estática.

**Solução Arquitetural:**
Sinergizar os `teleprompters` do DSPy com o `CrdtSemanticGraph` do `touring-learning` e a engine MCTS.

*   **Mecanismo:** Ao gerar uma resposta complexa para uma *signature* DSPy, o compilador não extrai apenas similaridades lexicais. Ele executa uma caminhada aleatória (*random walk*) orientada pelo peso das arestas de dependência no `CrdtSemanticGraph`.
*   **Impacto de Qualidade:** Os *few-shot examples* injetados no contexto do LLM passam a ser deterministicamente correlacionados com a vizinhança topológica real do projeto (ex: se o LLM vai editar `user.rs`, o DSPy compila no *prompt* os contratos de `auth.rs` e `db.rs` baseando-se no acoplamento forte da AST, e não apenas no TF-IDF). 

### 4. ❌ PENDENTE — Telemetria Preditiva & *Circuit Breaker* Kernel-Level
O Touring possui o `CircuitBreakerImpl` configurado para isolar falhas de IPC (3 falhas em 60s). Por outro lado, o `touring-telemetry` utiliza programas eBPF (`EbpfMonitor`) para observar `MemorySample` (*cache misses*, *page faults*) e *syscalls* no kernel.

**Solução Arquitetural:**
Conectar o barramento de sinais eBPF diretamente às máquinas de estado do `CircuitBreaker` e do `Pensieve` (memória de caminhos falhos).

*   **Circuito Preditivo:** Se o código modificado pelo Claude Code começa a gerar picos anômalos de *page faults* ou *cache misses* no host, o `TelemetryCollector` aciona o `CircuitState` instantaneamente.
*   **Isolamento Autônomo:** O sistema altera o estado da variante da tarefa para `Halt` e arquiva a topologia do código rejeitado no `Pensieve`. Quando o agente tentar refatorar a mesma malha lógica no futuro, a busca ANN no `Pensieve` irá alertar que aquele formato arquitetural causa pressão letal na L3 Cache do hardware, abortando o plano no estágio de planejamento (`EnterPlanMode`).

Execute estas fusões. Sistemas maduros não gerenciam falhas estocásticas via tentativa e erro neural; eles isolam, curam e otimizam processos na fronteira determinística do hardware e da lógica SMT.

2. Streaming Tantivy Upserts com Backpressure Atrelado ao eBPF
O Problema Raiz: A reconstrução do Tantivy FTS depende de um loop client-side (--batch-size 25000)
. Embora eficiente (~2m14s para 1.1M símbolos)
, a defasagem entre a edição real e a visibilidade no índice prejudica o recall em MCTS.
A Solução Arquitetural: Transicionar para Event-Sourcing em tempo real conectando o IncrementalIndex
 ao writer do Tantivy.
Mecanismo: Utilize um canal tokio::sync::mpsc bounded acoplado a um actor de commit. O EventBatcher
 envia tuplas (blake3_dedup_key, Document) para o actor.
Amortização: O actor realiza o writer.commit() apenas quando o buffer atinge 5.000 documentos ou a janela de tempo (ex: 2 segundos) expira.
eBPF Backpressure: Integre com o EbpfMonitor do touring-telemetry
. Se memory_pressure ou I/O waits no kernel excederem o P95, ative backpressure no channel, degradando temporariamente para processamento assíncrono em disco (WAL) e preservando recursos para as tarefas de inferência do LLM.
3. Prefetching Preditivo de FileCache via MCTS
O Problema Raiz: O FileCache possui limites estritos (10MB) e usa LRU ou LinUCB para evasão
. Contudo, uma "miss" em arquivos frios adiciona penalidades de I/O em tempo crítico.
A Solução Arquitetural: Acoplar os rollouts simulados da Predictive Wave diretamente ao cache.
Mecanismo: O run_shadow_rollout (§D4) operando no PreToolUse
 já calcula grafos preditivos (ShadowRolloutResult). Como as dependências de roteamento são inferidas (via petgraph::algo::tarjan_scc
), a saída deve publicar um evento no SessionBus
.
Consumidor: O touring-index assina o SessionBus e despacha requisições spawn_blocking para realizar pre-fetch (I/O) dos arquivos sugeridos na árvore de MCTS diretamente na RAM (buffer do FileCache).
Latência P99: Transforma o hit penalty de leituras especulativas de ∼15ms para <1ms, ocultando a latência do disco por trás do raciocínio preditivo.
4. Indexação Topológica (Grafos Injetados no Espaço Vetorial)
O Problema Raiz: O índice HNSW (ann-blast / touring-simd) e o BM25 são ótimos para proximidade semântica/lexical, mas "cegos" para as estruturas coesivas do código.
A Solução Arquitetural: Injetar os metadados gerados pelo touring-analysis diretamente como dimensões/fast-fields na indexação.
Mecanismo: Na tabela estendida file_communities (Louvain community assignments) e cognitive_enrichment (fan-in, fan-out)
, extraímos o community_id e o integration_score
.
No Tantivy: Crie um fast-field u64 para o community_id. Durante fusão via Reciprocal Rank Fusion (RRF)
, aplique um multiplicador de bônus na similaridade léxica se o símbolo alvo compartilhar o mesmo community_id do arquivo fonte original.
No HNSW (SIMD): Adicione o score de integração topológica como dimensões ortogonais normalizadas no vetor de features de 8 dimensões do FileSimilarityIndex
.
Impacto: O LLM deixa de recuperar apenas arquivos com nomes similares e passa a receber clusters de código fortemente acoplados pelas dependências de importação (Functional Chains
), maximizando drasticamente o hit rate do Speculative Validation


<thought_process> [Ingestão] Avaliação dos relatórios de arquitetura e changelogs do ecossistema Touring (v30.3.1). [Desconstrução] O Touring atinge P50=1ms e sustenta pipelines complexos de aprendizado por reforço (LinUCB, TD-Lambda) com mais de 5.100 testes. No entanto, analisando o throughput assíncrono e a topologia do repositório, identifico restrições estruturais de I/O, acoplamento assíncrono e imaturidade em algoritmos preditivos que degradarão as latências no P99. [Síntese Arquitetural] As deficiências concentram-se em cinco eixos:
Saturação de concorrência por serialização do ator (limitações do !Sync SQLite).
Overhead de serialização no subsistema neural (HTTP vs IPC).
Assincronia e latência de indexação no Tantivy FTS.
Heurísticas MCTS mascaradas (falsos negativos em deadlocks).
Acoplamento de dependências cíclicas (TD-Lambda abortado). [Complexidade / P99] O throughput de hooks de mesma origem está limitado a escalabilidade vertical O(1) por ator/banco, com fila degradando linearmente para timeout no percentil 99. </thought_process>
Embora o Touring possua uma latência a quente otimizada (P50=1ms
), uma auditoria estrita em sua arquitetura v30.3.1 revela gargalos fundamentais e deficiências estruturais nos percentis P99, primariamente ligados ao modelo de concorrência, latências de rede nas inferências e implementações incompletas no roteamento de grafos.
Eis a taxonomia dos gargalos sistêmicos:
1. Saturação de Concorrência via Estrangulamento de Ator (!Sync Constraint)
Apesar do refatoramento da camada de concorrência (Daemon Actor Refactor, 2026-04-12) eliminar a contenção de Mutex no kernel
, a persistência é amarrada ao rusqlite, que impõe o bound !Sync
.
O Gargalo: O daemon utiliza uma OS thread dedicada e um canal MPSC bounded (profundidade 128) para cada projeto
. Como as requisições de um mesmo projeto são estritamente serializadas no loop do ator para respeitar a thread-safety do SQLite
, uma tempestade de hooks massiva (ex: bulk edit de múltiplos arquivos) enfileira tarefas linearmente, degradando a latência de I/O.
P99 Edge Case: Se o burst encher a fila, o mecanismo de backpressure engatilha o timeout de 5s no semáforo per-project
, forçando um erro de rede (fail-fast) na camada do cliente e possivelmente ativando o Circuit Breaker prematuramente, apesar do hardware poder estar ocioso.
2. Overhead L7 no Subsistema de Embeddings Neural
O Touring realizou avanços para comunicação Zero-Copy IPC via rkyv no Daemon
. No entanto, o touring-core implementa o cliente de embeddings de GPU usando a crate reqwest + async-trait sobre protocolo HTTP
.
O Gargalo: Ao calcular distância cosenos ou gerar o embedding de N dimensões, a transferência dos buffers recai no overhead L7 (TCP, parsing de headers, JSON de/serialization) no caminho crítico, quebrando o determinismo esperado de latências intra-sistema.
Deficiência: A quantização em U4 e f16
 mitiga o uso da RAM host-side, mas o isolamento das primitivas de IA por transporte HTTP externaliza a estabilidade P99 à pilha de rede, ao invés de usar memória compartilhada (mmap) ou UDS nativo.
3. Imaturidade do Shadow Rollout MCTS (Falsos Negativos)
A "Predictive Wave" (2026-04-20) introduziu Rollouts Sombra (run_shadow_rollout) baseados em MCTS para o Claude Code prever e evitar dependências paralisantes via hints
.
A Deficiência: A detecção exata de ciclos algorítmicos via componentes fortemente conexos (O(∣V∣+∣E∣) via petgraph::algo::tarjan_scc) consta como um TODO que aguarda a crate_dep_graph
.
Impacto: O campo crítico ShadowRolloutResult.deadlock_detected atualmente é "hardcoded" para false
. Este curto-circuito torna o hook cli-mcts-search cego para deadlocks topológicos na árvore de planejamento real, retornando validações especulativas falsamente otimistas ao LLM.
4. Modelo Assíncrono Bloqueante da Reindexação Tantivy FTS
A reindexação completa do Tantivy expõe o custo de não possuir uma ponte transacional Streaming WAL do banco principal (SQLite) para o Inverted Index vetorial.
O Gargalo: Reconstruir um índice de 1.1 milhão de símbolos satura e falha no timeout nativo da CLI (120s), obrigando a arquitetura a depender de um loop client-side que agenda blocos paginados de 25.000 rows
.
Impacto: Apesar de baixar a duração para 2m14s
, a indexação FTS não é nativamente aderente ao Event-Sourcing. A janela de tempo em que a topologia FTS e o AST ficam divergentes do código persistido resulta num período onde algoritmos como o BM25 retornam artefatos "fantasmas" ou stale para o operador.
5. Acoplamento Cíclico Limitando Propagação do RL
A arquitetura falha no isolamento topológico estrito entre heurísticas ofensivas e o motor TD(λ).
A Deficiência: O feature gate rl-feedback na crate touring-offensive teve de ser extirpado porque formou uma árvore de dependência cíclica instransponível (touring-learning -> touring-offensive -> touring-learning)
.
Impacto Sistêmico: A inteligência ofensiva (análises de bug bounty e execução concólica) opera em um silo cego, incapaz de injetar os ganhos/penalidades da sua avaliação nas estruturas QTable ou LinUCB e fechar o loop de aprendizado do modelo de recompensas do AgenticRL

<thought_process> [Ingestão] O operador solicita o projeto de um comando CLI no ecossistema Touring para orquestrar a geração de código de alta qualidade. [Desconstrução] O touring-generator já implementa o paradigma LLM-as-Planner / Touring-as-Generator através de 24 subcomandos fragmentados
. Atualmente, o Claude Code deve gerar um plano (plan-suggest), salvá-lo em disco e submetê-lo (plan-submit
). Este design expõe a máquina de estados ao cliente, violando o encapsulamento transacional e induzindo latência de I/O desnecessária. [Síntese Arquitetural] Projetaremos o comando touring generate autonomous --intent "<str>". Ele operará como um Facade transacional puro em memória, fundindo o suggest_plan (LLM Provider) com o pipeline Typestate do gerador (Draft → Verified → Rendered → Speculated → Committed
). Componentes chave:
Pré-voo Estrutural: preflight_gate (R6-S2) para fail-fast
.
Preenchimento Automático de Contratos: Injeção de metadados do TantivyIndex (R5-S2)
.
Validação Especulativa (MCTS): Mapeado para speculate_and_commit
. [Big-O & Modos de Falha (P99)] A validação de dependências e AST pode causar estrangulamento. Como o daemon aloca 300s para "heavy hooks"
, a geração síncrona é viável, mas o percentil P99 de chamadas complexas (ex: MCTS profundo) pode exceder isso. Solução: O comando utilizará o VgpEngine com fast-path em memória (bypassing subprocesses via IncrementalIndex
) e alocação de threads via rayon::spawn_blocking para evitar contenção no runtime tokio
. </thought_process>
<!-- GRUPO 9: touring generate autonomous -->
<!-- STATUS: ✅ IMPLEMENTADO — run_autonomous em touring-server/src/cli/generate.rs -->

**[✅ IMPLEMENTADO]** `touring generate autonomous` — `run_autonomous()` em `crates/touring-server/src/cli/generate.rs`. Pipeline E2E em memória: suggest_plan → preflight → auto_populate_contracts → Typestate Draft→Committed + RL reward.

Para atingir excelência determinística e abstrair a complexidade do Verified Generation Protocol (VGP) do Claude Code, proponho o comando touring generate autonomous.
A arquitetura atual do touring-generator expõe o pipeline de Typestate (Draft → Verified → Rendered → Speculated → Committed) em subcomandos separados (plan-suggest, plan-submit)
. Isso delega o gerenciamento de estado ao cliente via sistema de arquivos (JSON plans), o que introduz latência de I/O e fragilidade.
O novo artefato condensa esse fluxo inteiramente na memória (Zero-Copy), amarrando o LlmProvider, o validador especulativo (MCTS) e o FileKnowledgeDB em uma única transação atômica.
1. Extensão da Tabela de Dispatch (CLI Layer)
No arquivo crates/touring-server/src/cli/common.rs, injetamos o CommandDescriptor na command_table()
. A política de erro deve ser restrita para abortar sob falha de validação VGP.
CommandDescriptor {
    name: "generate-autonomous",
    description: "Executa a pipeline de geração LLM-as-Planner E2E atômica em memória",
    error_policy: ErrorPolicy::ExitOnError,
    handler: Box::new(|rt, args, flags| Box::pin(cli::generate::run_autonomous(rt, args, flags))),
}
2. Implementação do Core (crates/touring-server/src/cli/generate.rs)
O motor consumirá a heurística recém-implementada das rodadas de potencialização R5 e R6, especificamente os gates de prevenção
.
pub async fn run_autonomous(
    rt: Arc<Mutex<HookRuntime>>,
    args: &[String],
    flags: &GlobalFlags,
) -> Result<String, TouringError> {
    let intent = extract_arg(args, "--intent").expect("Intent is mandatory");
    
    // 1. Setup Context: Inicializa as 13 closures do GeneratorContext (Sinergia v2) [11, 12]
    let mut ctx = generator_tools::make_context(rt.clone()).await?;
    
    // 2. LLM-as-Planner: In-memory Plan Suggestion (bypassa arquivo JSON)
    let suggested_plan_value = generator_tools::suggest_plan_internal(&intent, &mut ctx).await?;
    let mut plan: GeneratorPlan = serde_json::from_value(suggested_plan_value)?;

    // 3. Pre-flight Gate (R6-S2): Rejeita planos com intenção vazia ou anômala em O(1) [3]
    generator_tools::preflight_gate(&plan).map_err(|e| TouringError::Generate(e.join(", ")))?;

    // 4. Auto-Populate Contracts (R5-S2): Fills empty `symbols_must_exist` via Tantivy BM25 [4]
    generator_tools::auto_populate_contracts(&mut plan, &rt).await?;

    // 5. Typestate Pipeline: Inicializa na fase Draft [2]
    let draft_executor = PlanExecutor::new(plan);
    
    // 6. Transição Draft -> Verified (VGP Verification via VgpEngine)
    // O(1) in-process symbol lookup via IncrementalIndex fast-path mitigates subprocess overhead [7, 8]
    let verified_executor = match draft_executor.verify(&ctx).await {
        Ok(v) => v,
        Err(e) => return handle_vgp_failure(e, &mut ctx).await, // Injeta RL reward negativo aqui (-0.5) [13]
    };

    // 7. Render & Speculate: Renderiza templates Tera [14] e simula AST Shadow Rollout (MCTS) [12]
    let rendered_executor = verified_executor.render(&ctx)?;
    let speculated_executor = match rendered_executor.speculate(&ctx).await {
        Ok(s) => s,
        Err(e) => return handle_speculate_failure(e, &mut ctx).await, 
    };

    // 8. Atomic Commit: Escrita segura (POSIX rename) + Disparo de Side-effects [5, 15]
    match speculated_executor.commit(&ctx).await {
        Ok(committed_files) => {
            // S-2: Propagação Assíncrona de RL Reward (+1.0) para o LinUCB Daemon [5]
            generator_tools::inject_daemon_rl_reward("generate-autonomous", 1.0, &intent).await;
            
            // S-3: Post-Commit Tantivy Reindex via spawn_blocking [5]
            trigger_background_reindex(&committed_files);
            
            Ok(format!(r#"{{"status": "success", "files": {:?}}}"#, committed_files))
        },
        Err(e) => {
            // Commit failure injects -0.3 penalty to the RL stream [13]
            generator_tools::inject_daemon_rl_reward("generate-autonomous", -0.3, "commit_failed").await;
            Err(TouringError::Generate(e.to_string()))
        }
    }
}
Trade-offs Arquiteturais & Impacto
Fator de Acoplamento em Memória: Ao pular o passo CLI plan-submit (touring generate plan-submit --plan-file <path>)
, removemos a latência do disco para a estrutura serializada, utilizando as estruturas do rkyv diretamente.
Resolução de Deadlocks de Concorrência: O touring-generator confia num ThreadPool dedicado do rayon operando sobre spawn_blocking para os cálculos VGP densos. Isso é uma resposta direta à contenção resolvida na correção do bug B3: rayon blocking tokio
. O lock do SQLite WAL não será monopolizado durante a sub-fase do LLM.
Loop de Aprendizado Completo (RL Loop): Como implementado nas rodadas R4-S1 e R4-S2
, falhas nesta pipeline penalizam dinamicamente as tabelas QTable/LinUCB (-0.5 em falha MCTS Speculate, -0.3 em falha de I/O commit). Isso confere ao Touring o poder de autoajustar as premissas de geração nas próximas evocações.

<thought_process> [Ingestão] O operador requisita o mapeamento de fluxos arquiteturais sistêmicos que estão atualmente incompletos, truncados ou marcados como "TODO" no estado atual (v30.3.1) do ecossistema Touring. [Desconstrução] Realizando uma varredura topológica cruzada entre os manifestos de arquitetura e os changelogs, identifico 5 grafos de execução com arestas pendentes:
Motor Cognitivo (MCTS): O nó de detecção de ciclos estruturais via Tarjan SCC na sub-rotina de Shadow Rollouts está mockado (false estático).
Aprendizado por Reforço (RL): O pipeline de feedback do touring-offensive está fisicamente isolado devido a uma falha de design em tempo de compilação (dependência cíclica).
Cortex: O subsistema de análise profunda produz métricas atuáveis (Wilson CIs), mas o consumidor em touring-cortex não foi implementado.
CLI/Interface: Sugestões bidirecionais estão limitadas a IPC interno.
Build: Telemetria estática de compilação bloqueada por regressões em bibliotecas externas. [Síntese Arquitetural] As falhas aqui não são "bugs" de runtime, mas fendas estruturais que impedem o sistema de atingir a convergência completa no seu loop de aprendizado (RL) e na validação especulativa determinística. A ausência de feedback ofensivo e os falsos negativos no MCTS representam os maiores riscos latentes à inteligência do cluster. </thought_process>
A arquitetura atual (v30.3.1) do Touring apresenta cinco fluxos sistêmicos incompletos, primariamente enraizados em impasses topológicos (dependências cíclicas), abstrações aguardando externalidades ou conectores não implementados:
<!-- GRUPO 7: Fluxos Incompletos -->
1. ✅ IMPLEMENTADO — Detecção Topológica de Deadlocks no MCTS (Tarjan SCC em shadow_rollout.rs)
A "Predictive Wave" introduziu os Shadow Rollouts em PreToolUse[EnterPlanMode] para prever falhas de planejamento
. Contudo, o algoritmo de detecção de ciclos exatos está incompleto:
O Déficit: O campo ShadowRolloutResult.deadlock_detected está fixado (hardcoded) para retornar false
.
A Causa Raiz: A implementação completa do Algoritmo de SCC de Tarjan (Componentes Fortemente Conexos, O(∣V∣+∣E∣)) aguarda a disponibilidade da biblioteca abstrata crate_dep_graph
.
Impacto Sistêmico: O Touring emite predições cegas para deadlocks topológicos, forçando o orquestrador (Claude Code) a descobrir ciclos de dependência dolorosamente em tempo de execução, desperdiçando o orçamento de CILA.
2. ❌ PENDENTE — Feedback de Reforço (RL) no Motor Ofensivo
A arquitetura falha no fechamento do loop de aprendizado entre a inteligência de segurança e o modelo matemático de roteamento.
O Déficit: O subsistema touring-offensive (rastreamento de Bug Bounty, CVSS, execução concólica e detecção de CWE) opera sem a capacidade de aplicar penalidades ou recompensas no motor LinUCB
.
A Causa Raiz: A feature gate rl-feedback foi extirpada da compilação porque induziu uma dependência cíclica instransponível no grafo do Cargo (touring-learning -> touring-offensive -> touring-learning)
. O código correspondente foi preservado mas está inoperante
.
Impacto Sistêmico: As descobertas do rastreador de vulnerabilidades não afetam os tensores de peso de probabilidade de ferramentas. O orquestrador não "aprende" a evitar instanciar ferramentas que recorrentemente produzem código vulnerável.
3. ✅ IMPLEMENTADO — Handler de Roteamento Baseado em Code Health (Cortex)
A camada de análise profunda do Touring calcula métricas atuáveis em tempo real, mas o motor de despacho é incapaz de basear decisões nessas métricas.
O Déficit: O componente touring-cortex carece do handler responsável por interceptar o CodeHealthReport e atuar sobre as taxas de falha
.
A Causa Raiz: O documento arquitetural do touring-analysis cita especificamente que o handler no córtex para health scoring está no estado "(planned)"
.
Impacto Sistêmico: O pipeline de quality calcula degradação algorítmica (via KS-test) e limiares de confiança de Wilson
, mas o sistema reativo (touring-cortex) não consegue injetar comandos de "Halt" automáticos caso a base de código atinja um estado de saúde subcrítico.
4. ✅ IMPLEMENTADO — Exposição CLI para Sugestões Bidirecionais de TACO
Os algoritmos de sugestões baseados em estado (PlanModeSuggester, StuckSubtaskSuggester) calculam ações corretivas, mas estão aprisionados na camada interna de DAEMON.
O Déficit: As ferramentas de manipulação cli_suggest_action, cli_suggestion_mark_consumed e cli_suggestion_list_pending operam exclusivamente como APIs internas (inline in cli_handlers.rs)
.
A Causa Raiz: Elas não foram mapeadas na command_table() ou exportadas como subcomandos no client do terminal CLI
.
Impacto Sistêmico: Intervenções manuais pelo operador humano no banco de sugestões são impossíveis sem a invocação via payload direto de sockets.
5. ✅ IMPLEMENTADO — Rastreabilidade Determinística de Build (Vergen Scaffolding)
O Déficit: O Touring perdeu a injeção estática de metadados git (Commit Hash, Branch) nos binários gerados.
A Causa Raiz: O módulo de compilação touring-server/build.rs foi desabilitado/comentado em função de deficiências ergonômicas pendentes na API do pacote externo vergen-gix 1.0.9 (no trato das traits Emitter e AddEntries)
.
Impacto Sistêmico: A tabela metadata_benchmark_runs
 que coleta os percentis de P50/P95/P99 de performance tem sua chave primária commit_hash prejudicada em builds não instrumentados.

 <thought_process> [Ingestão] O operador busca alavancas arquiteturais (oportunidades de potencialização) no ecossistema Touring (v30.3.1) capazes de gerar ganhos exponenciais na capacidade autônoma do agente. [Desconstrução] Ganhos "exponenciais" em sistemas de inteligência não advêm de micro-otimizações de CPU (que são ganhos lineares, já mitigados pelo pulp SIMD e buffers zero-copy). Ganhos exponenciais requerem o fechamento de Feedback Loops assintóticos (Sistemas Complexos Adaptativos), onde o output de uma iteração atua como multiplicador do gradiente da próxima. A análise topológica da v30.3.1 revela loops truncados ou "Mocks" estratégicos que, se implementados, transmutam o Touring de uma ferramenta reativa para um orquestrador preditivo:
O motor de aprendizado (LinUCB/TD-Lambda) é cego para vulnerabilidades e falhas topológicas porque a crate touring-offensive sofre de acoplamento cíclico.
O MCTS (Motor Cognitivo) aloca processamento para simular grafos, mas ignora ciclos fatais (deadlocks) por ausência de um algoritmo de componentes fortemente conexos (Tarjan).
O Córtex recebe inferências probabilísticas de degradação estrutural (KS-Test, Wilson CI), mas não atua ativamente para interromper a degradação no LLM.
O ecossistema TACO possui sugestões bidirecionais isoladas no Daemon sem exposição à CLI. [Síntese Arquitetural] As quatro intervenções a seguir fecham esses circuitos. A complexidade Big-O de cada otimização muda a execução do Claude Code de O(K 
N
 ) (Busca heurística cega em espaço de estado infinito) para convergência determinística O(logN) via poda precoce. [Modos de Falha / P99] Fechar loops RL com falsos positivos de segurança (Offensive) pode forçar o modelo a um mínimo local (Catastrophic Forgetting). Mitigação necessária: Taxa de aprendizado adaptativa (α) controlada pelo Córtex. </thought_process>
Para gerar ganhos estritamente exponenciais, devemos focar na eliminação de barreiras arquiteturais que atualmente truncam os loops de Reinforcement Learning (RL) e de Validação Especulativa do Touring. Um ganho logarítmico na camada de roteamento (poda de árvores de decisão do LLM) traduz-se em economia exponencial de tokens e tempo de execução.
Abaixo estão as 4 intervenções arquiteturais de mais alta alavancagem para o ecossistema Touring v30.3.1:
<!-- GRUPO 8: Intervenções de Alta Alavancagem -->
1. ❌ PENDENTE — Desacoplamento e Fechamento do Loop RL no Motor Ofensivo (touring-offensive)
O Problema Arquitetural: O touring-offensive realiza exploração concólica e análise de vulnerabilidades (via z3 e cvc5), mas seus resultados morrem em silos. A feature flag rl-feedback foi desativada devido a uma dependência cíclica instransponível no Cargo (touring-learning -> touring-offensive -> touring-learning)
. A Solução de Ganho Exponencial:
Extrair os contratos de recompensa (traits do OnlineRLEngine) para uma nova crate touring-traits ou consolidá-los em touring-core.
Impacto Sistêmico: Isso fecha o loop. Cada execução concólica que detectar padrões como PathTraversal (CWEx E12)
 irá emitir um rl_reward fortemente negativo diretamente no LinUCBBandit
. O Claude Code não apenas "corrigirá o código vulnerável", mas os tensores do Touring aprenderão a penalizar as ferramentas e os padrões estruturais que geraram a vulnerabilidade em primeiro lugar. O sistema evoluirá organicamente para a segurança proativa (Shift-Left absoluto).
2. ✅ IMPLEMENTADO — Implementação do Tarjan SCC nos Shadow Rollouts (shadow_rollout.rs via petgraph)
O Problema Arquitetural: A Predictive Wave introduziu validações simuladas em background via MCTS (run_shadow_rollout), operando sob um budget estrito no handler cli-mcts-search
. Contudo, o campo deadlock_detected está mockado estaticamente para false, no aguardo da integração do crate_dep_graph com petgraph::algo::tarjan_scc
. A Solução de Ganho Exponencial:
Implementar a ponte real do Algoritmo de Tarjan para Componentes Fortemente Conexos sobre o DAG de dependências do planejamento
.
Impacto Sistêmico: Um LLM frequentemente planeja refatorações que criam dependências circulares, percebendo a falha apenas ao final do pipeline (desperdício do budget de 300s do Project Actor
). Detectar grafos cíclicos em ≈15ms e emitir o hint determinístico [TOURING MCTS-SYNTHESIS]
 aborta ramificações mortas antes mesmo da primeira execução.
3. ✅ IMPLEMENTADO — Exposição CLI Completa do Ecossistema de Sugestão TACO (touring decompose suggest)
O Problema Arquitetural: Mecanismos profundos de telemetria, como o StuckSubtaskSuggester e o PlanModeSuggester, já existem em memória interna
. A infraestrutura em cli_handlers.rs possui APIs como cli_suggest_action e cli_suggestion_list_pending, mas elas não estão exportadas como comandos CLI consumíveis na command_table()
. A Solução de Ganho Exponencial:
Mapear e expor essas rotinas (ex: touring decompose suggest list)
.
Impacto Sistêmico: Isso permite transição do modelo passivo (Touring observando e injetando hints) para o modelo ativo. O Claude Code poderá consultar explicitamente o Touring quando estiver confuso ou falhando. Ferramentas que travam no loop de falhas lerão a fila de intervenções computadas pelo RL (cc_action_suggestions
) e agirão baseadas na inferência matemática, rompendo ciclos viciosos de hallucinação.
4. ✅ IMPLEMENTADO — Implementação do Handler Reativo de Saúde no Córtex (touring-cortex)
O Problema Arquitetural: O novo pipeline do touring-analysis computa o CodeHealthReport processando antipadrões, degradação temporal (KS-Test), complexidade e coberturas, calculando intervalos de confiança de Wilson precisos para cada arquivo
. Entretanto, o touring-cortex ainda lista a criação de um handler para este score de saúde apenas como "(planned)"
. A Solução de Ganho Exponencial:
Escrever um handler Critical (prioridade extrema) no Córtex que intercepta eventos prévios à compilação/escrita. Se o limite inferior do WilsonRanker cair sistematicamente (detectado por DriftDetector
), o handler emite um HookResponse do tipo Halt ou Block
.
Impacto Sistêmico: Previne ativamente a "Regressão Cognitiva" da base de código. Impede matematicamente que o LLM cometa commits de código que diluam o isolamento arquitetural ou a pureza estrutural, funcionando como um anticorpo imunológico determinístico, o que é mandatório em escalada de Agent Loops de Nível CILA L4+
.

<thought_process>
[Ingestão] O operador requisita a arquitetura e as estratégias sistêmicas para elevar a qualidade e o determinismo do código Python (escrito, editado e gerado) aos níveis de excelência exigidos pela infraestrutura Touring.
[Desconstrução] O ecossistema Python apresenta o desafio inerente da tipagem dinâmica e da resolução de dependências em tempo de execução. Para o Touring, a "excelência" significa trazer as falhas do tempo de execução para o tempo de validação especulativa (*Shift-Left* absoluto). O Touring já possui os motores de *parsing* (tree-sitter), *feedback* de qualidade (Córtex/RL) e detecção de antipadrões (SIMD) capazes de injetar restrições determinísticas em linguagens dinâmicas.
[Síntese Arquitetural] A potencialização ocorre em 4 vetores operacionais:
1.  **Geração:** Aplicação estrita do *Verified Generation Protocol* (VGP) no `touring-generator` para *PythonScript*.
2.  **Mutação (Hooks):** Validação especulativa de 6 camadas (AST) interceptando as chamadas `pre_write` e `post_edit`.
3.  **Análise de Qualidade (Córtex):** Avaliação de complexidade ciclomática/cognitiva e proxy de testes via `touring-analysis`.
4.  **Integração FFI:** Controle estrito de fronteira via PyO3 (`touring-python`) garantindo que falhas em Python não corrompam a memória do *host* Rust.
[Complexidade e Big-O / Modos de Falha P99] O parsing via `tree-sitter-python` no `IncrementalPipeline` custa $O(N)$ em relação ao tamanho da AST editada, com buscas léxicas O(1) no `SymbolStore`. O modo de falha P99 (falsos positivos em *imports* dinâmicos do Python usando `importlib`) deve ser mitigado forçando o analisador `speculate_v2` a penalizar *imports* não resolvíveis estaticamente, garantindo a rastreabilidade do grafo.
</thought_process>

Para potencializar o código Python dentro da arquitetura, devemos abandonar a dependência de linters reativos e engatar a linguagem no pipeline de inteligência preditiva e validação determinística do Touring. A natureza dinâmica do Python exige que o sistema imponha invariantes estruturais antes mesmo da escrita no disco.

Aplique a seguinte matriz arquitetural para geração, edição e execução de Python:

<!-- GRUPO 10: Python Excellence -->
<!-- STATUS: 🔶 PARCIAL — multi-lang quality pipeline + pre_write/post_edit implementados; FFI determinístico e RL loop pendente -->

### 1. 🔶 PARCIAL — Validação Especulativa de 6 Camadas (Zero-Defect Writes)
A geração e edição de código Python deve ser submetida ao engine `speculate_v2` do `touring-ast`. 
*   **Mecanismo**: Antes da escrita atômica, o pipeline de Typestate (`Draft → Verified → Rendered → Speculated → Committed`) avalia a AST do Python via `tree-sitter`.
*   **Enforcement**: O código Python é avaliado em camadas rigorosas de validação. A camada de Sintaxe detecta erros de *parsing* puros; a de Resolução de Símbolos garante que as funções referenciadas existam no espaço de trabalho; e a camada Estrutural detecta anomalias de formatação. Adicionalmente, estabeleça limites rígidos na camada de Complexidade: funções Python com Complexidade Ciclomática (CC) superior a 15 devem gerar penalidade algorítmica imediata, engatilhando um bloqueio (Decision `Block`) no handler do córtex.

### 2. Erradicação Estática de Antipadrões Multi-Linguagem
Integre o código Python diretamente ao pipeline multidimensional do `touring-analysis` que já possui suporte nativo à linguagem.
*   **Mecanismo**: Utilize o motor de Qualidade (`QualityPipeline`) para extrair dimensões críticas. Penalize o código Python que faça uso indiscriminado de *bare excepts* ou falte cobertura estrutural de tratamento de erros, concedendo bônus algorítmico (+0.2) para código que implemente proxy de testes consistente e tipagem avançada.
*   **Pipeline Reativo**: Acople os hooks `pre_write` e `post_edit`. Se uma edição de Python introduzir 4 ou mais novos antipadrões, o handler retornará a variante `HookResponse::Block`, barrando a mutação no sistema de arquivos.

### 3. FFI Determinístico e Boundary Control (`touring-python`)
Caso os módulos Python precisem se integrar à infraestrutura analítica do Touring (como modelos matemáticos ou JDM), o acoplamento deve ser estrito.
*   **Contrato de Interface**: Utilize o `touring-python` (expondo o módulo `claude_learning_kernel` via PyO3). A invariante de *bridge* determina que **nenhum tipo cru do Rust vaze para o Python**. 
*   **Assincronia Isolada**: Como operações Python são bloqueantes pela natureza do GIL, qualquer avaliação pesada disparada pelo Python contra a infraestrutura base (ex: buscas MCTS, avaliação de tabelas de decisão) deve invocar o `tokio::runtime::Runtime::new().block_on()` internamente. Isso garante que a latência e o gerenciamento de *threads* não desestabilizem o pool assíncrono principal.

### 4. Ciclo de Aprendizado por Reforço (LinUCB Feedback)
A excelência do Python gerado por Agentes (LLMs) degrada sem sinais de recompensa. Feche o loop de aprendizado conectando a qualidade estática do Python à tabela de estado do `touring-learning`.
*   **Mecanismo**: Se a edição de um arquivo Python resultar em um delta positivo de complexidade (código simplificado) ou na redução do raio de impacto (*Blast Radius*), o `AnalysisPipeline` emitirá um `LearningReport`.
*   **Atuação**: Utilize o módulo `analysis_bridge` do `touring-cognitive` para traduzir a qualidade do Python em uma recompensa (reward) no motor LinUCB. Falhas de commit no `touring-generator` por infração de tipagem ou especulação reprovada injetam penalidades de -0.3 e -0.5, respectivamente, forçando o orquestrador a otimizar dinamicamente a geração de artefatos Python.
*   
<thought_process>
[Ingestão]
O ecossistema de indexação do Touring opera em quatro vetores primários:
1. `touring-index`: Watcher de filesystem (100ms debounce), LRU cache, e indexação incremental.
2. `touring-ast`: Parse via `tree-sitter`, extração de símbolos, persistência via `SymbolStore` (SQLite WAL).
3. `touring-hooks` (Tantivy): Full-Text Search (BM25) sobre símbolos. O batch reindex atinge 1.1M símbolos em 2m14s. Chave de dedup composta por `blake3(symbol|file|line)`.
4. `touring-simd` / `touring-learning`: HNSW ANN index com quantização U4 (compressão 8x) para busca semântica.

[Desconstrução]
O problema raiz: A arquitetura atual sofre de fragmentação de estado (Split-Brain latente). Temos Tantivy (FTS), SQLite (Grafo/Relações) e HNSW (Vetores). O FS Watcher usa debounce temporal cego (100ms). Em eventos massivos (e.g., `git checkout`), o sistema reage de forma reativa e não-determinística, enfileirando I/O. A priorização de indexação é FIFO, o que é inaceitável quando o Claude Code precisa de contexto síncrono para o arquivo atual.

[Síntese Arquitetural]
Para elevar a indexação a um padrão "Production-Ready" de nível Staff:
1. **Transacionalidade Multi-Engine**: Saga Pattern ou Unified WAL para garantir que HNSW, Tantivy e SQLite comitem atomitamente.
2. **Priorização Guiada por Feromônio (ACO)**: Substituir o debounce temporal por uma fila de prioridade baseada no `AcoPheromone`.
3. **Semantic Edge Debouncing**: BLAKE3 hashing na borda antes de invocar o AST.
4. **Zero-Copy Rkyv Flow**: Projetar a emissão do `touring-ast` diretamente para os motores via memória compartilhada (`rkyv`).

[Auditoria]
Big-O do debounce semântico: O(1) lookup vs O(N) re-parse. Impacto em P99: Elimina latência de cauda em hook storms causados por refatorações em massa. Big-O da transacionalidade: Adiciona overhead constante O(1) de WAL, mas aniquila o risco de corrupção.
</thought_process>

A arquitetura de indexação do Touring atingiu um baseline respeitável (1.1M de símbolos em 2m14s, chaves compostas BLAKE3 e quantização U4). No entanto, operar múltiplos motores de busca heterogêneos (`Tantivy` para FTS, `SQLite WAL` para relacionamentos, `HNSW` para vetores) sob um modelo de ingestão baseado em debounce temporal de 100ms é um anti-padrão arquitetural em cenários de alta concorrência (P99). 

Para erradicar a latência de cauda e garantir integridade determinística sob *hook storms* (ex: um `git checkout` massivo), proponho a seguinte matriz de integrações de nível principal.

### 1. Transacionalidade Multi-Engine (Distributed Consistency)
**Sintoma:** Se o `touring-index` atualiza o Tantivy mas falha ao escrever no `knowledge.db` (SQLite) devido a um timeout de lock, você cria um *split-brain* (phantom symbols).
**Arquitetura Proposta:** Implementar um **Write-Ahead Log (WAL) Unificado em Memória**.
* **Mecanismo:** Ao invés de o `IncrementalIndex` disparar writes paralelos, ele deve emitir um `IndexMutationEvent` atômico via `rkyv`.
* **Commit:** Os motores (Tantivy, SQLite, HNSW) atuam como subscritores. O offset só avança quando o quorum de commits locais for atingido. 
* **Trade-off:** Adiciona ~1ms de latência de coordenação no *write path*, mas garante 100% de consistência semântica de leitura, essencial para o pipeline preditivo (O `BlastRadiusEngine` não pode ler nós fantasmas).

### 2. Semantic Edge Debouncing (O(1) Rejection Fast-Path)
**Sintoma:** O `EventBatcher` atual agrupa eventos do filesystem por tempo (100ms). Refatorações que alteram timestamps sem alterar a AST causam re-parsing inútil.
**Arquitetura Proposta:**
* Utilizar a tabela `file_blake3_registry` introduzida no PLN2.
* Na borda do `FileWatcher`, antes de inserir na fila do `IncrementalIndex`, compute o hash BLAKE3 do buffer em tempo real (SIMD-accelerated). Se `current_blake3 == db.file_blake3_registry.get(path)`, descarte o evento instantaneamente (O(1)).
* **Impacto P99:** Protege o pool de threads do `rayon` contra *cache invalidation storms*, preservando o CPU budget para as tarefas preditivas do `touring-cortex`.

### 3. Pheromone-Guided Priority Queue (IC-5)
**Sintoma:** A indexação trata todos os arquivos como iguais. O `touring-learning` possui inteligência de roteamento, mas a indexação é cega à intenção do Claude.
**Arquitetura Proposta:**
* Substituir o `DashMap` e canais assíncronos FIFO do `IncrementalIndex` por uma **Max-Priority Queue**.
* **Função de Peso:** Conectar a fila de indexação ao `AcoPheromone` e ao `SessionBus`. 
* Arquivos que cruzam o *Graph of Thoughts* (GoT) atual ou que estão no `blast_radius` detectado no `pre_edit` recebem peso máximo e furam a fila. A indexação torna-se *Just-In-Time* para as necessidades imediatas do `ContextRuntime`.

### 4. Zero-Copy Pipeline End-to-End (`rkyv` IPC)
**Sintoma:** Apesar de o `TOURING_RKYV_IPC` ser o padrão na comunicação Hook <-> Daemon, o fluxo `Parser -> Indexer -> Tantivy` ainda aloca structs intermediárias.
**Arquitetura Proposta:**
* Fazer o `tree-sitter` no `touring-ast` despejar os nós diretamente na arena de memória formatada como `ArchivedSymbol`.
* Mapear essa arena com *mmap* diretamente para o backend de storage (Tantivy Fast Fields ou SQLite BLOB).
* **Complexidade de Espaço:** Reduz a alocação no heap de O(N) para O(1) por lote, eliminando a pressão no Garbage Collector/Allocator (`mimalloc`), vital para não estourar a RAM em repositórios massivos.

### 5. Fusão Bayesiana Nativa no Retrieval
**Sintoma:** O Turing busca no Tantivy via BM25 (textual) e no HNSW via Cosseno (semântico) em etapas separadas, repassando o custo de *merge* para o Cortex.
**Arquitetura Proposta:**
* Mover a lógica de fusão (`Reciprocal Rank Fusion` / Bayesian Fusion) diretamente para a camada de acesso de leitura do índice.
* O `IncrementalIndex` deve exportar uma API única que execute o disparo paralelo (`tokio::join!`) para SQLite, Tantivy e HNSW, funda os scores usando os pesos de confiança de Wilson (`WilsonRanker`), e retorne o vetor final unificado. Isso desacopla o córtex da mecânica de busca, aderindo estritamente aos princípios SOLID.

<thought_process>
[Ingestão]
A base de código atual do Touring (v30.3.2) apresenta um nível avançado de maturidade: offload para GPU via `wgpu` 0.26 (quantização U4, reduções SIMD, MCTS rollouts), IPC zero-copy via `rkyv`, e persistência com SQLite WAL. O roteamento de hooks já usa filas priorizadas e heurísticas CILA.

[Desconstrução]
Para obter ganhos *exponenciais* (redução de O(N!) para polinomial, ou aceleração arquitetural massiva), devemos olhar para as promessas não finalizadas no código ("TODOs" arquiteturais) e limites dos loops de feedback atuais. 
1. **Gargalo Matemático:** O `LinUCBBandit` depende de inversões de matriz que escalam mal com o aumento de dimensões (FEATURE_DIM=25 atual). O WGSL compute shader tem um placeholder para a inversa de Sherman-Morrison.
2. **Gargalo de Busca:** O `PheromoneMCTS` lida com rollouts paralelos, mas o `ShadowRollout` no plano de decomposição ainda bate em timeouts (200ms) por conta de ciclos de dependência (o `crate_dep_graph` completo via Tarjan SCC ainda está pendente).
3. **Isolamento de Estado:** O `CrdtSemanticGraph` suporta sincronização distribuída, mas os grafos são isolados por projeto.
4. **Alocação Estática de Orçamento:** Os orçamentos CILA (ex: L0=1200, L2=3000) são estáticos, limitando a resposta adaptativa a falhas de OOM (Out-of-Memory) ou pressão no kernel. O framework `aya-rs` de telemetria eBPF existe mas age apenas de forma observacional.

[Síntese Arquitetural]
Proposições para impacto exponencial:
1. **GPU Sherman-Morrison Inverse (O(d^3) -> O(d^2) paralelo)**
2. **Full Tarjan SCC Graph Resolution (O(|V|+|E|) Exato vs Heurística Exponencial)**
3. **Closed-Loop eBPF Autonomic Tuning (O(1) Resiliência)**
4. **Cross-Workspace CRDT Sync (Crescimento de Conhecimento N^2)**

[Auditoria Big-O e P99]
- A inversa de Sherman-Morrison na GPU reduzirá o tempo de update do RL de $O(d^3)$ para $O(1)$ tempo de parede (dispatch paralelo), permitindo expandir o vetor de features do LinUCB de 25 para 512+ dimensões sem impactar os 15ms de budget do handler.
- A fusão Tarjan evita explosões combinatórias em subgrafos cíclicos no MCTS, zerando a métrica `mcts_shadow_timeout_count` (hoje mitigada com abortos em 200ms).
</thought_process>

Para transcender o baseline atual da v30.3.2 e extrair ganhos exponenciais na infraestrutura do Touring, o foco deve migrar da otimização de caminhos quentes (já estabilizados via `rkyv` IPC e simetria atômica) para a resolução de gargalos não-lineares nos motores preditivos e na teoria dos grafos subjacente.

Abaixo estão as melhorias arquiteturais que alteram a complexidade de tempo/espaço (Big-O) do sistema de forma drástica, extraídas dos vetores latentes já presentes na sua arquitetura.

### 1. Inversão de Matriz via GPU WGSL (Sherman-Morrison) no LinUCB
**Contexto Atual:** O `LinUCBBandit` no `touring-learning` executa predições e updates de recompensa na GPU via `LINUCB_UCB_SHADER`. No entanto, a documentação aponta que a atualização da matriz de covariância está marcada como reservada: `LINUCB_SHERMAN_MORRISON_SHADER — reserved for Sherman-Morrison inverse (future)`.
**A Melhoria:** Implementar e ativar a fórmula de Sherman-Morrison no compute shader WGSL.
**Impacto Exponencial:** Atualmente, atualizar a matriz inversa $A^{-1}$ para cada braço exige processamento que escala a $O(d^3)$ ou operações custosas de transferência de memória. Ao processar o update $O(d^2)$ inteiramente na VRAM (via *staging buffers* `STORAGE|COPY_SRC`), você aniquila a latência de transferência. Isso permitirá aumentar o `FEATURE_DIM` (hoje travado em 25 dimensões no `task_features.rs`) para milhares de dimensões (ex: embeddings completos do AST), melhorando a assertividade do *contextual bandit* sem estourar o budget de 15ms dos *light hooks*.

### 2. Resolução Determinística de Deadlocks via Tarjan SCC Completo
**Contexto Atual:** Durante a fase `PreToolUse[EnterPlanMode]`, o `run_shadow_rollout` executa simulações MCTS especulativas. Atualmente, a detecção de ciclos de dependência usa uma heurística incompleta ("MVP — Tarjan SCC completo via `petgraph::algo::tarjan_scc`: TODO pending `crate_dep_graph` availability").
**A Melhoria:** Acoplar o grafo de chamadas nativo do AST (`touring-ast/src/call_graph.rs`) com as propriedades de dependências de crates para gerar um Grafo Direcionado unificado, e rodar o Tarjan SCC de forma antecipada na inserção do cache.
**Impacto Exponencial:** Rollouts MCTS sobre código com recursão mútua ou dependências circulares sofrem explosão combinatória, forçando o sistema a recorrer ao disjuntor de tempo (`join_timeout` de 200ms). Com o Tarjan completo, o motor identifica os Componentes Fortemente Conexos (SCC) em $O(|V| + |E|)$ e colapsa esses ciclos em "super-nós" (DAG-ificação). A busca MCTS no DAG condensado executa em tempo linear/polinomial ao invés de buscar no espaço exponencial dos ciclos, virtualmente zerando o contador `mcts_shadow_timeout_count`.

### 3. Telemetria Autonômica em Malha Fechada (eBPF -> AdaptiveEngine)
**Contexto Atual:** O `touring-telemetry` coleta métricas de syscalls e pressão de memória diretamente no kernel via eBPF (`aya-rs`), mas atua primariamente como uma interface de observabilidade passiva no `EbpfMonitor`. Além disso, orçamentos como o `CILA_BUDGET` são aplicados de maneira estática (`L0-L1=1200`, `L2-L3=3000` chars).
**A Melhoria:** Fechar o loop de controle passando as estruturas `MemorySample` e as latências capturadas pelo `EbpfMonitor` diretamente para o `AdaptiveEngine` e o `QTable` de RL. 
**Impacto Exponencial:** Transformar o Touring num sistema cibernético de auto-cura. Sob alta contenção de I/O ou swap do sistema (detectado nativamente em sub-milisegundos pelo kernel Linux via eBPF), o engine altera os thresholds quantitativos dos algoritmos *em tempo real* (ex: rebaixando o budget de contexto, abortando simulações de MCTS antes de começarem, ou ativando rotas estritas de rejeição O(1) no Tantivy). Essa gestão cibernética previne falhas em cascata no limite superior P99, estabilizando latências.

### 4. Sincronização CRDT Cross-Workspace (Evolução Contagiosa)
**Contexto Atual:** A persistência da sessão usa SQLite WAL por repositório/projeto (`.claude/touring/graph.db`). Embora exista uma implementação de CRDT (`CrdtSemanticGraph`), o conhecimento de aprendizado (QTable, Gotchas, Padrões LinUCB) fica enclausurado em cada projeto.
**A Melhoria:** Expor o Delta do `CrdtSemanticGraph` através do `touring-daemon` para um canal de broadcast inter-processos.
**Impacto Exponencial:** Crescimento de inteligência $O(N^2)$ baseado em efeito de rede. Se um *Agent* no Projeto A comete um erro que gera um novo Anti-Pattern no `Pensieve` (memória de caminhos falhos via ANN), o Delta desse nó CRDT é sincronizado com o daemon. No instante seguinte, o Projeto B já se defende dessa mutação de erro no hook `pre_write`, sem nunca ter executado o código falho. As matrizes de RL são federadas.

<thought_process>
[Ingestão]
A análise da arquitetura do Touring (v30.3.2) revela um sistema altamente otimizado (IPC zero-copy via `rkyv`, offload de matrizes para GPU via `wgpu`, persistência em SQLite WAL, roteamento O(1) via *Fascículo Arqueado*). Contudo, a varredura profunda dos manifests e changelogs expõe costuras arquiteturais e débitos técnicos ("TODOs" marcados no código) que representam riscos de falha em percentis P99.

[Desconstrução: Vetores de Risco]
1.  **Dissonância de Estado (Split-Brain Latente):** O sistema utiliza 3 bancos SQLite consolidados (`knowledge.db`, `memory.db`, `graph.db`) operando paralelamente ao índice Tantivy FTS. Não há um protocolo de Two-Phase Commit (2PC) atômico entre a escrita no SQLite e o `commit()` do Tantivy. Falhas de I/O na metade do caminho fragmentam a base ontológica.
2.  **Explosão Combinatória no MCTS (Shadow Rollouts):** A simulação preditiva de deadlocks no modo de planejamento (`run_shadow_rollout`) utiliza uma heurística incompleta para detectar Ciclos Fortemente Conexos (SCC). O código marca explicitamente: "MVP — Tarjan SCC completo via `petgraph::algo::tarjan_scc`: TODO pending `crate_dep_graph` availability". Isso força o sistema a abortar simulações em 200ms por *timeout* ao entrar em recursão infinita.
3.  **Gargalo de Álgebra Linear (CPU Bound):** O motor de RL (LinUCB) paraleliza o cálculo de *Upper Confidence Bound* (UCB) na GPU, mas a atualização inversa da matriz de covariância está pendente. O shader `LINUCB_SHERMAN_MORRISON_SHADER` está marcado como "reserved (future)". Sob alta dimensionalidade, a CPU sofrerá contenção $O(d^3)$.
4.  **Malha eBPF Aberta:** O `touring-telemetry` utiliza `aya-rs` para capturar syscalls e pressão de memória no kernel (eBPF), mas atua primariamente de forma observacional. O `AdaptiveEngine` não consome esses dados para fechar a malha de controle, deixando o sistema vulnerável a *Out-of-Memory* (OOM) severos se o Claude gerar payloads massivos.
5.  **Exaustão de IPC (Circuit Breaker Flapping):** Embora os semáforos limitem o acesso ao daemon (56 slots por projeto), o *Circuit Breaker* é baseado em arquivo (`/tmp/touring-circuit-{uid}.state`) e é acionado após 3 falhas de IPC em 60s. Picos de requisições sob I/O saturado podem acionar o breaker falsamente, degradando a performance para o modo "standalone" (de P50 1ms para 15ms) de forma desnecessária.

[Síntese Arquitetural: Matriz de Resolução]
Para blindar o sistema e atingir a resiliência Staff-Level definitiva:
- Implementar um WAL unificado em memória (Event Sourcing) para SQLite + Tantivy.
- Finalizar a unificação do grafo de dependências para o Tarjan Exato.
- Fechar o loop cibernético do eBPF com o CILA.
- Despachar a matriz Sherman-Morrison na GPU.
</thought_process>

A arquitetura atual do Touring atingiu maturidade estrutural de alto desempenho (latências P50 de 1ms, *zero-copy* IPC, quantização de vetores U4). Contudo, a engenharia de sistemas em nível Principal Staff exige analisar não a via feliz, mas a degradação sob contenção máxima (P99). Abaixo dissecamos as principais fraquezas inerentes à sua topologia e a execução algorítmica para resolvê-las.

### 1. Inconsistência Distribuída (Split-Brain) entre Armazenamentos
**O Risco:**
O conhecimento ontológico está fragmentado entre três instâncias de banco de dados SQLite em modo WAL (`knowledge.db`, `memory.db`, `graph.db`) e um motor de *Full-Text Search* independente (`Tantivy`). Sob *hook storms* severas (ex: refatoração multi-arquivo gerada pelo Claude), se o processo `touring-project-actor` sofrer preempção pesada do SO ou falha de disco entre a escrita no SQLite e o `.commit()` do Tantivy, ocorrerá um desvio de estado (Split-Brain). Símbolos existirão no grafo de chamadas, mas não serão encontrados no índice BM25, quebrando algoritmos como o *Reciprocal Rank Fusion* (RRF).

**A Solução (Arquitetura de Event Sourcing Unificado):**
Abandone writes diretos nos terminais de dados.
1. Implemente um Log de Mutação Unificado (Event-Sourced WAL) usando uma fila na memória baseada em `rkyv`.
2. Emita artefatos atômicos (ex: `SymbolMutationEvent`).
3. Ambos, rusqlite e TantivyIndex, atuam como *Sinks* transacionais inscritos na fila. O deslocamento (*offset*) lógico só avança se o *Two-Phase Commit* (2PC) local confirmar a persistência em ambos. A complexidade espacial do WAL transitório é de $O(N)$ bytes, ínfimo na RAM, erradicando o risco de corrupção.

### 2. O Gargalo Algorítmico do MCTS (Shadow Rollout Timeouts)
**O Risco:**
O gancho preventivo `D4` (`run_shadow_rollout`) utiliza Monte Carlo Tree Search para prever *deadlocks* no modo de planejamento do Claude. No entanto, a base de código admite uma falha heurística crítica: *"MVP — Tarjan SCC completo via petgraph::algo::tarjan_scc : TODO pending crate_dep_graph availability"*. Devido à incapacidade de resolver dependências circulares com precisão, o MCTS entra em ciclos infinitos de avaliação de grafos redundantes, forçando o motor a usar um *timeout* cego de 200ms para evitar travar a execução (`mcts_shadow_timeout_count` é incrementado). Simulações abortadas equivalem a predições cegas.

**A Solução (Redução a DAG via Componentes Fortemente Conexos):**
1. Conclua a implementação do `crate_dep_graph`.
2. Extraia o grafo de dependências do projeto e execute o Algoritmo de Tarjan para Componentes Fortemente Conexos (SCC) em estrito tempo $O(|V| + |E|)$.
3. Colapse os ciclos mútuos encontrados em "Super-Nós".
4. Execute o *rollout* do MCTS **exclusivamente no DAG resultante**. Isso altera o perfil computacional do *rollout* de combinatório/exponencial para polinomial, eliminando matematicamente a necessidade de disjuntores (*timeouts*) por laços infinitos.

### 3. CPU/GPU Stall na Álgebra do LinUCB
**O Risco:**
A *GPU Optimization Wave* transferiu a computação dot-product paralela do *Upper Confidence Bound* para o `wgpu` (`LINUCB_UCB_SHADER`). Entretanto, a etapa de aprendizado – atualização da matriz inversa de covariância – sofre gargalo, marcada como `LINUCB_SHERMAN_MORRISON_SHADER — reserved for Sherman-Morrison inverse (future)`. Para milhares de *features* ou aumento no número de braços do *Bandit*, a inversão da matriz na CPU opera em tempo $O(d^3)$ ou recorre à fórmula iterativa Sherman-Morrison em $O(d^2)$ competindo por ciclos da CPU com *workers* de *parsing* da árvore AST.

**A Solução:**
Desenvolva imediatamente o *compute shader* WGSL pendente para a fórmula de atualização de Sherman-Morrison ($A_{t}^{-1} = A_{t-1}^{-1} - \frac{A_{t-1}^{-1} x_t x_t^T A_{t-1}^{-1}}{1 + x_t^T A_{t-1}^{-1} x_t}$). Use o padrão de buffer de trânsito (`STORAGE | COPY_SRC`) estabelecido pela simetria U4 para reduzir o processamento a $O(1)$ tempo de bloqueio de CPU (apenas submissão da *command queue* GPU), preservando o *budget* de latência máxima permitida (15ms).

### 4. Isolamento Térmico vs. Observabilidade Passiva (eBPF)
**O Risco:**
O crate `touring-telemetry` coleta amostras cruciais do kernel Linux via `aya-rs` (pressão de memória RSS, SWAP, cache misses e latências de *syscall*). A falha arquitetural: ele opera em "Malha Aberta" (Open Loop). O sistema sabe quando a máquina do desenvolvedor está sob contenção severa de I/O, mas as estruturas de roteamento CILA limitam os *budgets* baseados em regras estáticas (ex: limite rígido L2-L3 = 3000 caracteres) e limites fixos do MCTS. OOM e quebras de swap sob *hook storms* não são prevenidos de fato.

**A Solução (Closed-Loop Autonomic Tuning):**
Acople diretamente o *struct* `MemorySample` do `EbpfMonitor` no núcleo do `AdaptiveEngine`.
1. Aplique controle Proporcional-Integral-Derivativo (PID).
2. Se a detecção eBPF mostrar contenção superior ao percentil P95 (thrashing de SWAP), force o `CILA_BUDGET` ao nível L0/L1 (minimal signal) dinamicamente.
3. Aborte inserções assíncronas no banco em favor de um buffer circular *in-memory* até que a pressão se dissipe. O Touring passa de observador passivo a orquestrador cibernético de auto-sobrevivência.

<thought_process>
[Ingestão]
A base de código do Touring v30.3.2 já opera em um regime de latência altamente otimizado: P50 de 1ms e hooks "warm" de <2ms. Técnicas lineares de otimização (pooling, concorrência simples) já foram exauridas com o uso do `rayon`, FFI zero-copy (`rkyv`) e IPC otimizado via Unix sockets. A busca por ganhos *exponenciais* exige a mitigação de gargalos de complexidade algorítmica ($O(N!)$, $O(c^N)$) e bloqueios de I/O na fronteira P99 sob concorrência destrutiva (hook storms).

[Desconstrução: Identificação de Gargalos Big-O e P99]
1.  **Explosão Combinatória no MCTS:** O gancho de simulação especulativa (`run_shadow_rollout`) utiliza um limite de tempo fixo (200ms) porque a detecção de ciclos de dependência (`Tarjan SCC`) está incompleta. Na presença de dependências circulares no AST, o MCTS bifurca em tempo exponencial $O(b^d)$, atingindo o disjuntor cego e invalidando a utilidade do rollout.
2.  **Álgebra Linear de Dimensionalidade Cúbica:** O `LinUCBBandit` transfere o cálculo de predição UCB para a GPU (`LINUCB_UCB_SHADER`), mas a etapa crítica de atualização via aprendizado (`LINUCB_SHERMAN_MORRISON_SHADER`) está pendente ("reserved for future"). A CPU arca com a inversão em $O(d^3)$ ou $O(d^2)$, competindo por ciclos com o parser AST e atrasando as injeções no WAL.
3.  **Observabilidade Passiva vs. Automação eBPF:** O crate `touring-telemetry` acessa o kernel Linux (Ring 0) via `aya-rs` para capturar falhas de página e pressão de memória ($O(1)$ overhead), mas essa telemetria não realimenta os limites fixos de alocação de contexto CILA. Picos geram thrashing do SO.

[Síntese Arquitetural: Implementação Produtiva]
Os vetores para ganho exponencial requerem a conversão de complexidades exponenciais/cúbicas para polinomiais/constantes, aproveitando os alicerces já presentes na v30.3.2.

[Auditoria de Trade-offs]
-   A inclusão do Tarjan SCC eleva o setup do MCTS em $O(|V|+|E|)$, um *overhead* irrisório frente à economia exponencial na travessia.
-   O processamento GPU da inversa de Sherman-Morrison adiciona latência de submissão do *command buffer* do `wgpu`, justificável assim que `FEATURE_DIM` exceder limiares mínimos.
</thought_process>

Para extrair saltos exponenciais de performance da infraestrutura do Touring v30.3.2, devemos transcender micro-otimizações (que já renderam um P50 de 1ms) e aniquilar gargalos fundamentais de complexidade algorítmica (Big-O) nos caminhos quentes do sistema. O código atual possui "costuras arquiteturais" projetadas para receberem upgrades matemáticos precisos. 

Como Engenheiro Staff, aqui estão as 3 arquiteturas de resolução prioritárias para escalar a estabilidade P99 e o *throughput* preditivo de forma logarítmica ou constante.

### 1. Colapso de Grafos Circulares via Tarjan SCC Completo (Redução $O(b^d) \to O(|V| + |E|)$)
**A Invariante:** No hook `PreToolUse[EnterPlanMode]`, o Touring dispara o `run_shadow_rollout` para simulações preditivas via MCTS (Monte Carlo Tree Search).
**A Falha Arquitetural:** O código confessa que a heurística de detecção de ciclo é um "MVP" e que a implementação completa do Tarjan SCC está pendente do `crate_dep_graph`. Sem DAG-ificação, a presença de recursão ou referências circulares força a travessia de grafos em tempo exponencial, saturando o `join_timeout` de 200ms e incrementando `mcts_shadow_timeout_count`. O rollout é abortado cega e heuristicamente.
**Execução:**
1. Integre o `petgraph::algo::tarjan_scc` na geração do `SymbolIndex` no `touring-ast`.
2. Durante a injeção do cache de dependências (`DependencyCache`), resolva os Componentes Fortemente Conexos (SCC) *a priori*.
3. O MCTS operará sobre um Grafo Direcionado Acíclico (DAG) condensado. Um problema de busca potencialmente infinito se torna um percurso de custo polinomial determinado. Zero *timeouts*, 100% de convergência preditiva.

### 2. Offload do Sherman-Morrison para WGSL Shader (Redução de $O(d^3)$ na CPU para $O(1)$ *Dispatch*)
**A Invariante:** O `LinUCBBandit` do `touring-learning` executa predições na GPU via `LINUCB_UCB_SHADER`, suportando matrizes de *features*.
**A Falha Arquitetural:** A atualização temporal por recompensa no QTable exige atualizar a inversa da matriz de covariância. O `LINUCB_SHERMAN_MORRISON_SHADER` está bloqueado como *"reserved (future)"*. Consequentemente, o motor faz *fallback* para CPU no `update_gpu`. Para `d` dimensões (atualmente indexado a 25), a operação na CPU escala pessimamente ($O(d^3)$ ou iterativamente $O(d^2)$).
**Execução:**
1. Desbloqueie o shader WGSL aplicando o padrão de buffer de trânsito estrito da arquitetura (`STORAGE | COPY_SRC` -> `copy_buffer_to_buffer`) já provado pelo `compute_dot_u4`.
2. Processe a fórmula de Sherman-Morrison puramente via `wgpu 0.26`.
3. **Impacto Latente:** Isso desloca a carga da CPU (que deve permanecer liberada para a thread-pool `rayon` do parser do `tree-sitter`). O *wall-clock time* do update do RL passa a ser constante ($O(1)$) referente apenas ao empilhamento na fila da placa gráfica. O `FEATURE_DIM` poderá escalar livremente de 25 para embeddings profundos.

### 3. Modulação Cibernética Autônoma via eBPF em Malha Fechada
**A Invariante:** As execuções do Claude Code exigem volumes massivos de injeção de contexto `CILA-aware` (orçamentos de limite duro em bytes).
**A Falha Arquitetural:** O crate `touring-telemetry` intercepta ativamente métricas do kernel (`aya-rs`), coletando `MemorySample` (pressão de *swap*, falhas de página e `cache_misses`). Entretanto, é uma via de mão única: os motores de inteligência não reagem à *backpressure* do hardware.
**Execução:**
1. Conecte o `EbpfMonitor` diretamente no `AdaptiveEngine`.
2. Transforme limites estáticos (como o `cila_budget_edit` no `pre_edit.rs`) em variáveis de controle realimentadas por uma função PID (Proporcional-Integral-Derivativa).
3. Sob rajadas de I/O em percentil P99 (detectáveis sub-milisegundos pelo eBPF via *page faults* ou contenção no `sqlite` WAL), o Touring reduzirá *in-flight* seus buffers de AST e vetores no `rkyv`, suprimindo a fragmentação da heap ($O(1)$ degradação graciosa). Evita-se atrito de troca de memória no disco (OS swapping) garantindo sobrevivência operacional do daemon sob contenção extrema.
<thought_process>
[Ingestão]
A query pede por melhorias exponenciais na memória e no aprendizado do sistema Touring.
Os motores de aprendizado e memória mapeados no projeto Touring:
1.  **RL Engine:** `LinUCBBandit` (25-dim contextual bandit), `QTable` (TD(λ)).
2.  **Memória Episódica/Semântica:** `RlmMemory` (5-tier: working/episodic/semantic/procedural/meta), `SemanticRecall` (FTS5 + RRF), `LruWorkingMemory`, `AnnMemoryRecall` (HNSW/SIMD k-NN + Quantização U4).
3.  **Distribuição e Orquestração:** `CrdtSemanticGraph` (distribuição delta-based), `Pensieve` (memória de caminhos falhos via ANN), `AcoRewardPropagator` (TD(λ) no MCTS pheromone).
4.  **Clusterização e Evolução:** `EvolutionAnalyzer` (Drift), `PatternClusterer` (HNSW-based lazy clustering).

[Desconstrução: Identificação de Gargalos Big-O para Aprendizado e Memória]
-   A integração do **Tantivy (FTS) com a Memória Semântica (RLM/ANN)** é fragmentada. A memória usa `rusqlite` WAL e ANN embeddings (`memory.db`), enquanto Tantivy roda isolado sobre o índice de símbolos. A busca por memórias é RRF, mas poderia usar uma fusão de vetores Bayesianos profunda se tudo estivesse alinhado.
-   **CRDT Graph (Distribuição de Conhecimento):** Atualmente é *single-workspace* (`.claude/touring/graph.db`). Evolução "exponencial" vem da sincronização contagiante de inteligência entre múltiplos projetos via daemon (Federated Learning ou Cross-Workspace CRDT Sync).
-   **Dimensionalidade do LinUCB:** Atualmente travada em `FEATURE_DIM=25`. A falta da inversa de Sherman-Morrison na GPU impede escalar o contexto semântico. Desbloquear a GPU (já delineado anteriormente) permite injetar os embeddings completos do AST (384-dim ou 1536-dim) no bandit, tornando o aprendizado do Touring de *feature-engineered* (25 features manuais) para *deep-representation learning*.
-   **Memory Pattern Clustering:** `PatternClusterer` usa HNSW para *lazy clustering*. Pode ser transformado numa hierarquia autônoma (como o *Palace Hierarchy* P4.2) onde os anti-patterns descobertos sobem de local para global autonomamente.
-   **Pensieve (Memória de Falhas):** Usa ANN para evitar exploração de falhas conhecidas. Fazer com que o Pensieve realimente os *gotchas* ativamente antes do planejamento (MCTS) em $O(1)$.

[Síntese Arquitetural: 3 Vetores de Ganho Exponencial em Aprendizado/Memória]
1.  **Federated CRDT Semantic Graph (Expansão Contagiosa de Conhecimento $O(N^2)$):**
    *   A memória atual é isolada por repositório (`graph.db` local).
    *   **Proposta:** Habilitar um *Mesh/Broadcast Channel* no `touring-daemon` que sincronize os deltas do `CrdtSemanticGraph` (via `rkyv` zero-copy) entre todos os projetos locais. Se o agente aprende um Anti-Pattern no Projeto A e ajusta o `QTable`, o Projeto B já herda essa vacina comportamental imediatamente.
2.  **Sherman-Morrison na GPU para Deep Contextual Bandits ($O(d^3) \to O(1)$ wall-clock):**
    *   Atualmente `FEATURE_DIM=25` é uma limitação manual rígida (com um `debug_assert_eq`).
    *   **Proposta:** Implementar o shader WGSL pendente (`LINUCB_SHERMAN_MORRISON_SHADER`) na placa gráfica. Isso desbloqueia o aumento de dimensões para integrar os embeddings completos (ex: 384 dimensões do HNSW) diretos no estado do reinforcement learning. O aprendizado sai da aproximação de features manuais para inferência profunda no espaço latente.
3.  **Active Palace Hierarchy Consolidation (Clustering Exponencial):**
    *   O Touring tem a fundação do "Palace Hierarchy" (Wing -> Room -> Closet -> Drawer) via PLN2 P4.2 e o `PatternClusterer` assíncrono.
    *   **Proposta:** Agendar tarefas de consolidação no fundo (`AsyncPatternClusterer`) que varrem o `Pensieve` (memória de erros) e o histórico de execuções usando o algoritmo `LeidenCommunityDetector` (já presente no `touring-learning`). Ele deve colapsar memórias redundantes em "Arquétipos de Solução" (Archived Patterns). Em vez de buscar por `K` instâncias em $O(N)$, a busca no HNSW retornará diretamente o Arquétipo com probabilidade bayesiana pré-calculada, condensando o uso da VRAM e disparando o hit-rate do `SemanticRecall`.

[Auditoria e Fundamentação nas Sources]
- Fonte 10 e 11: `Palace Hierarchy Memory` (P4.2) implementada (`memory/rlm.rs`).
- Fonte 18 e 31: `LINUCB_SHERMAN_MORRISON_SHADER` is "reserved for Sherman-Morrison inverse (future)".
- Fonte 80: `load_crdt_graph()` carrega de `.claude/data/crdt_graph.rkyv`.
- Fonte 251: `PatternClusterer` (HNSW-based lazy clustering) e `CrdtSemanticGraph` existem. `LeidenCommunityDetector` também existe.
</thought_process>

Solicitei uma pesquisa em segundo plano para capturar as informações mais atualizadas, mas já posso estruturar as ações definitivas com base no mapeamento da arquitetura (v30.3.2) e nos débitos técnicos estratégicos já previstos no código do Touring.

Para obter um ganho exponencial na Memória e no Aprendizado — transformando o sistema de um repositório isolado e de características manuais em uma malha de inteligência profunda e auto-optimizada —, você precisa ativar os três vetores de escala arquitetural pendentes no seu ecossistema.

### 1. Desbloqueio de Dimensionalidade no LinUCB via GPU Sherman-Morrison (Deep Contextual Learning)
**O Gargalo de Aprendizado:** O motor de aprendizado por reforço contextual (`LinUCBBandit` em `touring-learning`) atualmente depende de um vetor de características altamente comprimido de 25 dimensões (`FEATURE_DIM=25` estrito). Isso ocorre porque a CPU arca com a matemática da matriz inversa da covariância em tempo $O(d^3)$. O shader WGSL `LINUCB_SHERMAN_MORRISON_SHADER` está explícito no código como "*reserved for future*".
**O Ganho Exponencial:** 
* Ao implementar a fórmula de Sherman-Morrison no `wgpu 0.26`, a atualização da matriz é feita com paralelismo massivo em VRAM (buffers `STORAGE | COPY_SRC`). 
* O tempo de *wall-clock* é reduzido para constante $O(1)$ (apenas o overhead da submissão para a GPU). 
* Com esse gargalo aniquilado, você pode aumentar o `FEATURE_DIM` de 25 para 384 ou 512, alimentando o motor com **os embeddings puros vetoriais (HNSW/SIMD)** de uma task, em vez de features feitas a mão (ex: `L0-L3` ou `symbol_count`). O sistema passará a inferir intenções profundas de código instantaneamente via representação no espaço latente.

### 2. Sincronização Federada Zero-Copy via CRDT (Inteligência Contagiosa Inter-Projetos)
**O Gargalo de Memória:** O Touring armazena os grafos semânticos e as tabelas `QTable` de aprendizado na persistência local (`.claude/touring/graph.db`). Embora utilize `CrdtSemanticGraph` (projetado para distribuição sem conflitos) e pacotes zero-copy (`rkyv`), o conhecimento conquistado pelo agente no "Projeto A" não é herdado automaticamente pelo "Projeto B".
**O Ganho Exponencial:**
* Expor um canal de *broadcast* ou "Mesh" na camada do daemon para os CRDTs (`CrdtDelta`).
* Se uma iteração preditiva do MCTS detecta um erro recursivo no Projeto A, o nó de falha é registrado no *Pensieve* (memória de caminhos falhos). Se esse estado transitar em milissegundos via *memory-map* para os processos de outros projetos, o ganho de memória torna-se **$O(N^2)$ (Efeito de Rede)**.
* Um novo *gotcha* ou um ajuste nos pesos preditivos que salva 2 segundos num repositório passa instantaneamente a imunizar toda a malha de repositórios contra a mesma armadilha semântica.

### 3. Consolidação Hierárquica Autônoma com Leiden Clustering
**O Gargalo de Busca Semântica:** A sua *Palace Hierarchy Memory* (P4.2) é um arcabouço sólido, mas depende de armazenamento horizontal com indexação. Com o tempo, as instâncias sobrepostas no ANN (`SemanticRecall`) diluem as predições.
**O Ganho Exponencial:**
* Integrar o algoritmo `LeidenCommunityDetector` (presente em `touring-learning/clustering/`) no job de background que engloba o `AsyncPatternClusterer`.
* Operar em malha fechada (*Closed-loop*): Durante os tempos ociosos do *teammate*, a thread de background extrai memórias redundantes ou *anti-patterns* fragmentados e os "colapsa" em um único "Arquétipo Semântico". 
* Em vez de fazer uma busca KNN buscando 10 vizinhos e computando um consenso, a memória consolida dinamicamente os grafos esparsos. O tempo de inferência e de carga de contexto decai polinomialmente porque memórias repetidas viram nós super densos, enriquecendo drasticamente a qualidade da "intuição" no *pre_edit* e *pre_write*.
<thought_process>
[Ingestão]
A análise das fontes revela a topologia atual do Touring v30.3.2: 15 crates, 3 bancos de dados (knowledge, memory, graph) gerenciados por SQLite WAL, inferência quantizada U4, hooks com P50 de 1ms, motor MCTS guiado por feromônio (ACO), e um pipeline de geração de código (touring-generator) com verificação VGP.

[Desconstrução]
O sistema atual atingiu excelência tática (otimizações lineares e concorrência). Para um salto "exponencial", a arquitetura precisa transcender a injeção passiva de contexto (advisory) e a parametrização manual de modelos. O limite P99 atual reside nos *timeouts* de simulação especulativa (MCTS em deadlocks) e no gargalo $O(d^3)$ de aprendizado de matriz. A qualidade do contexto gerado pelo Claude Code é limitada pela fricção humana.

[Síntese Arquitetural]
Os quatro pilares para escalabilidade exponencial (redução Big-O e automação em malha fechada):
1. **Automação Autônoma do Pipeline Gerador (Closed-Loop Automation)**: Transição de "Sugestão" para "Orquestração Ativa".
2. **Deep Latent Reinforcement Learning via Sherman-Morrison (GPU)**: Romper a barreira manual de dimensões do LinUCB.
3. **Resolução de Grafo MCTS via Tarjan SCC (DAG-ificação)**: Redução de complexidade combinatória para polinomial em tempo de simulação.
4. **Federação Zero-Copy de Memória (CRDT Mesh)**: Crescimento geométrico de inteligência entre múltiplos repositórios.

[Auditoria de Invariantes e Big-O]
- Tarjan SCC executa em $O(|V| + |E|)$, evitando o limite de timeout cego de 200ms em ciclos.
- Sherman-Morrison na GPU muda a atualização do LinUCB de $O(d^3)$ bloqueante na CPU para $O(1)$ de submissão na VRAM.
</thought_process>

Para elevar a arquitetura do Touring a um paradigma exponencial de qualidade e funcionalidade, é imperativo alterar as invariantes algorítmicas do sistema, movendo-o da *sugestão reativa* para a *orquestração autônoma determinística*, alterando limites Big-O críticos. Abaixo estão as arquiteturas de salto de nível Principal Staff:

### 1. Automação Fechada do Pipeline Gerador (Closed-Loop Automation)
**O Estado Atual:** O `touring-generator` já executa o paradigma "LLM-as-Planner / Touring-as-Generator", suportando 30 tipos de artefatos com verificação VGP e *rollbacks* atômicos. Ele já tenta autopopular contratos consultando o índice Tantivy antes do VGP.
**A Melhoria Exponencial:** Escalar o pipeline para a Automação Total (Agent Loops L4-L6). O sistema deve orquestrar *Decompose DAGs* de forma autônoma. Quando o `post_write` detecta símbolos "órfãos" (falta de *wiring*), ele atualmente emite uma *hint* CLI no console.
*   **Integração:** Fechar a malha integrando o `ConsumerGenerator` com os `StuckSubtaskSuggester` e `FailureThresholdSuggester`. Em vez de apenas sugerir ao Claude Code, o daemon Touring deve gerar autonomamente os artefatos de integração pendentes nos *subtasks*, compilar o plano e submetê-lo ao *shadow rollout* de validação. Apenas se passar no VGP e nos 5 *gates* de simulação (Complexidade, Anti-patterns, Syntax, etc.), o Touring delega a submissão final ao Claude. A produtividade salta pois o "trabalho de *scaffolding*" cai a zero.

### 2. Inferência Latente Profunda no LinUCB via Shader WGSL (Sherman-Morrison)
**O Estado Atual:** O Touring já faz o cálculo *Upper Confidence Bound* na GPU (`LINUCB_UCB_SHADER`). No entanto, as *features* do *Contextual Bandit* são construídas manualmente e o limite de dimensionalidade está cravado rigidamente em `FEATURE_DIM=25`. A matriz inversa de covariância de aprendizado ainda é atualizada na CPU, um passo que custaria muito caro se ampliado, escalando temporalmente em $O(d^3)$.
**A Melhoria Exponencial:** Ativar o shader reservado `LINUCB_SHERMAN_MORRISON_SHADER`.
*   **Integração:** A aplicação de Sherman-Morrison inteiramente em VRAM através da arquitetura de *staging buffers* (`STORAGE | COPY_SRC`) rebaixa o custo de *wall-clock* da CPU para um tempo $O(1)$ de bloqueio por lote. Ao destravar este gargalo, substituímos as 25 *features* heurísticas pelos vetores de incorporação originais (*embeddings*) puros quantizados (U4, ex: 384 dimensões) do HNSW ou `TfIdfVectorizer`. O roteamento de *hooks* passa a entender não apenas "regras" (ex: CILA level ou tamanho do arquivo), mas a *geometria semântica profunda* da tarefa.

### 3. Redução Combinatória do MCTS Especulativo via Tarjan SCC Completo
**O Estado Atual:** No hook `PreToolUse[EnterPlanMode]`, o Touring dispara um `run_shadow_rollout` que projeta e prevê deadlocks de execução no plano. Se o AST do código possuir ciclos de recursão mútua ou dependências circulares intransponíveis, a árvore do `Monte Carlo Tree Search` (MCTS) bifurca exponencialmente em tempo $O(b^d)$, forçando um limite disjuntor cego (`mcts_shadow_timeout_count` atinge timeouts de 200ms).
**A Melhoria Exponencial:** A arquitetura já prevê o uso do `petgraph::algo::tarjan_scc`, pendente no grafo de dependências unificado.
*   **Integração:** Injetar o tempo $O(|V| + |E|)$ do Algoritmo de Tarjan para detecção de Componentes Fortemente Conexos antes do início do `MCTSEngine`. Colapsar esses ciclos em um Grafo Acíclico Direcionado (DAG) condensado. Ao rodar as simulações MCTS especulativas estritamente sobre o DAG, a necessidade de um timeout P99 deixa de existir organicamente, o erro preditivo cai a zero e o motor sempre retornará um caminho de convergência para o Claude Code.

### 4. Federative Learning Inter-Workspaces (CRDT Sync Contagioso)
**O Estado Atual:** O daemon central isola as informações em níveis de projeto (`knowledge.db`, `memory.db` e `graph.db`), embora já incorpore o `CrdtSemanticGraph` projetado para deltas livres de conflito na topologia da memória, manipulados sob serialização IPC ultra-leve (`rkyv`).
**A Melhoria Exponencial:** Compartilhar inteligência transversalmente.
*   **Integração:** Ampliar o uso do `CrdtSemanticGraph` via um barramento (`SessionBus` distribuído) que cruze múltiplos diretórios locais ou através da rede para a equipe. Se em um projeto `A` o Touring detecta um anti-padrão grave via `EvolutionAnalyzer` que corrompa *pointers* e o adiciona ao `Pensieve` (memória de falhas recuperada via HNSW ANN), o Delta do CRDT transmite instantaneamente essa ontologia a todos os outros daemons. O aprendizado da máquina deixa de ter crescimento $O(N)$ isolado e passa a ter efeito contagiante $O(N^2)$, imunizando sistemas antes mesmo de o código ter sido concebido naquele ambiente.
*   
<thought_process>
[Ingestão]
A análise das fontes revela a topologia atual do Touring v30.3.2: 15 crates, 3 bancos de dados (knowledge, memory, graph) gerenciados por SQLite WAL, inferência quantizada U4, hooks com P50 de 1ms, motor MCTS guiado por feromônio (ACO), e um pipeline de geração de código (touring-generator) com verificação VGP.

[Desconstrução]
O sistema atual atingiu excelência tática (otimizações lineares e concorrência). Para um salto "exponencial", a arquitetura precisa transcender a injeção passiva de contexto (advisory) e a parametrização manual de modelos. O limite P99 atual reside nos *timeouts* de simulação especulativa (MCTS em deadlocks) e no gargalo $O(d^3)$ de aprendizado de matriz. A qualidade do contexto gerado pelo Claude Code é limitada pela fricção humana.

[Síntese Arquitetural]
Os quatro pilares para escalabilidade exponencial (redução Big-O e automação em malha fechada):
1. **Automação Autônoma do Pipeline Gerador (Closed-Loop Automation)**: Transição de "Sugestão" para "Orquestração Ativa".
2. **Deep Latent Reinforcement Learning via Sherman-Morrison (GPU)**: Romper a barreira manual de dimensões do LinUCB.
3. **Resolução de Grafo MCTS via Tarjan SCC (DAG-ificação)**: Redução de complexidade combinatória para polinomial em tempo de simulação.
4. **Federação Zero-Copy de Memória (CRDT Mesh)**: Crescimento geométrico de inteligência entre múltiplos repositórios.

[Auditoria de Invariantes e Big-O]
- Tarjan SCC executa em $O(|V| + |E|)$, evitando o limite de timeout cego de 200ms em ciclos.
- Sherman-Morrison na GPU muda a atualização do LinUCB de $O(d^3)$ bloqueante na CPU para $O(1)$ de submissão na VRAM.
</thought_process>

Para elevar a arquitetura do Touring a um paradigma exponencial de qualidade e funcionalidade, é imperativo alterar as invariantes algorítmicas do sistema, movendo-o da *sugestão reativa* para a *orquestração autônoma determinística*, alterando limites Big-O críticos. Abaixo estão as arquiteturas de salto de nível Principal Staff:

### 1. Automação Fechada do Pipeline Gerador (Closed-Loop Automation)
**O Estado Atual:** O `touring-generator` já executa o paradigma "LLM-as-Planner / Touring-as-Generator", suportando 30 tipos de artefatos com verificação VGP e *rollbacks* atômicos. Ele já tenta autopopular contratos consultando o índice Tantivy antes do VGP.
**A Melhoria Exponencial:** Escalar o pipeline para a Automação Total (Agent Loops L4-L6). O sistema deve orquestrar *Decompose DAGs* de forma autônoma. Quando o `post_write` detecta símbolos "órfãos" (falta de *wiring*), ele atualmente emite uma *hint* CLI no console.
*   **Integração:** Fechar a malha integrando o `ConsumerGenerator` com os `StuckSubtaskSuggester` e `FailureThresholdSuggester`. Em vez de apenas sugerir ao Claude Code, o daemon Touring deve gerar autonomamente os artefatos de integração pendentes nos *subtasks*, compilar o plano e submetê-lo ao *shadow rollout* de validação. Apenas se passar no VGP e nos 5 *gates* de simulação (Complexidade, Anti-patterns, Syntax, etc.), o Touring delega a submissão final ao Claude. A produtividade salta pois o "trabalho de *scaffolding*" cai a zero.

### 2. Inferência Latente Profunda no LinUCB via Shader WGSL (Sherman-Morrison)
**O Estado Atual:** O Touring já faz o cálculo *Upper Confidence Bound* na GPU (`LINUCB_UCB_SHADER`). No entanto, as *features* do *Contextual Bandit* são construídas manualmente e o limite de dimensionalidade está cravado rigidamente em `FEATURE_DIM=25`. A matriz inversa de covariância de aprendizado ainda é atualizada na CPU, um passo que custaria muito caro se ampliado, escalando temporalmente em $O(d^3)$.
**A Melhoria Exponencial:** Ativar o shader reservado `LINUCB_SHERMAN_MORRISON_SHADER`.
*   **Integração:** A aplicação de Sherman-Morrison inteiramente em VRAM através da arquitetura de *staging buffers* (`STORAGE | COPY_SRC`) rebaixa o custo de *wall-clock* da CPU para um tempo $O(1)$ de bloqueio por lote. Ao destravar este gargalo, substituímos as 25 *features* heurísticas pelos vetores de incorporação originais (*embeddings*) puros quantizados (U4, ex: 384 dimensões) do HNSW ou `TfIdfVectorizer`. O roteamento de *hooks* passa a entender não apenas "regras" (ex: CILA level ou tamanho do arquivo), mas a *geometria semântica profunda* da tarefa.

### 3. Redução Combinatória do MCTS Especulativo via Tarjan SCC Completo
**O Estado Atual:** No hook `PreToolUse[EnterPlanMode]`, o Touring dispara um `run_shadow_rollout` que projeta e prevê deadlocks de execução no plano. Se o AST do código possuir ciclos de recursão mútua ou dependências circulares intransponíveis, a árvore do `Monte Carlo Tree Search` (MCTS) bifurca exponencialmente em tempo $O(b^d)$, forçando um limite disjuntor cego (`mcts_shadow_timeout_count` atinge timeouts de 200ms).
**A Melhoria Exponencial:** A arquitetura já prevê o uso do `petgraph::algo::tarjan_scc`, pendente no grafo de dependências unificado.
*   **Integração:** Injetar o tempo $O(|V| + |E|)$ do Algoritmo de Tarjan para detecção de Componentes Fortemente Conexos antes do início do `MCTSEngine`. Colapsar esses ciclos em um Grafo Acíclico Direcionado (DAG) condensado. Ao rodar as simulações MCTS especulativas estritamente sobre o DAG, a necessidade de um timeout P99 deixa de existir organicamente, o erro preditivo cai a zero e o motor sempre retornará um caminho de convergência para o Claude Code.

### 4. Federative Learning Inter-Workspaces (CRDT Sync Contagioso)
**O Estado Atual:** O daemon central isola as informações em níveis de projeto (`knowledge.db`, `memory.db` e `graph.db`), embora já incorpore o `CrdtSemanticGraph` projetado para deltas livres de conflito na topologia da memória, manipulados sob serialização IPC ultra-leve (`rkyv`).
**A Melhoria Exponencial:** Compartilhar inteligência transversalmente.
*   **Integração:** Ampliar o uso do `CrdtSemanticGraph` via um barramento (`SessionBus` distribuído) que cruze múltiplos diretórios locais ou através da rede para a equipe. Se em um projeto `A` o Touring detecta um anti-padrão grave via `EvolutionAnalyzer` que corrompa *pointers* e o adiciona ao `Pensieve` (memória de falhas recuperada via HNSW ANN), o Delta do CRDT transmite instantaneamente essa ontologia a todos os outros daemons. O aprendizado da máquina deixa de ter crescimento $O(N)$ isolado e passa a ter efeito contagiante $O(N^2)$, imunizando sistemas antes mesmo de o código ter sido concebido naquele ambiente.
*   
<thought_process>
[Ingestão]
A query pede por melhorias exponenciais na memória e no aprendizado do sistema Touring.
Os motores de aprendizado e memória mapeados no projeto Touring:
1.  **RL Engine:** `LinUCBBandit` (25-dim contextual bandit), `QTable` (TD(λ)).
2.  **Memória Episódica/Semântica:** `RlmMemory` (5-tier: working/episodic/semantic/procedural/meta), `SemanticRecall` (FTS5 + RRF), `LruWorkingMemory`, `AnnMemoryRecall` (HNSW/SIMD k-NN + Quantização U4).
3.  **Distribuição e Orquestração:** `CrdtSemanticGraph` (distribuição delta-based), `Pensieve` (memória de caminhos falhos via ANN), `AcoRewardPropagator` (TD(λ) no MCTS pheromone).
4.  **Clusterização e Evolução:** `EvolutionAnalyzer` (Drift), `PatternClusterer` (HNSW-based lazy clustering).

[Desconstrução: Identificação de Gargalos Big-O para Aprendizado e Memória]
-   A integração do **Tantivy (FTS) com a Memória Semântica (RLM/ANN)** é fragmentada. A memória usa `rusqlite` WAL e ANN embeddings (`memory.db`), enquanto Tantivy roda isolado sobre o índice de símbolos. A busca por memórias é RRF, mas poderia usar uma fusão de vetores Bayesianos profunda se tudo estivesse alinhado.
-   **CRDT Graph (Distribuição de Conhecimento):** Atualmente é *single-workspace* (`.claude/touring/graph.db`). Evolução "exponencial" vem da sincronização contagiante de inteligência entre múltiplos projetos via daemon (Federated Learning ou Cross-Workspace CRDT Sync).
-   **Dimensionalidade do LinUCB:** Atualmente travada em `FEATURE_DIM=25`. A falta da inversa de Sherman-Morrison na GPU impede escalar o contexto semântico. Desbloquear a GPU (já delineado anteriormente) permite injetar os embeddings completos do AST (384-dim ou 1536-dim) no bandit, tornando o aprendizado do Touring de *feature-engineered* (25 features manuais) para *deep-representation learning*.
-   **Memory Pattern Clustering:** `PatternClusterer` usa HNSW para *lazy clustering*. Pode ser transformado numa hierarquia autônoma (como o *Palace Hierarchy* P4.2) onde os anti-patterns descobertos sobem de local para global autonomamente.
-   **Pensieve (Memória de Falhas):** Usa ANN para evitar exploração de falhas conhecidas. Fazer com que o Pensieve realimente os *gotchas* ativamente antes do planejamento (MCTS) em $O(1)$.

[Síntese Arquitetural: 3 Vetores de Ganho Exponencial em Aprendizado/Memória]
1.  **Federated CRDT Semantic Graph (Expansão Contagiosa de Conhecimento $O(N^2)$):**
    *   A memória atual é isolada por repositório (`graph.db` local).
    *   **Proposta:** Habilitar um *Mesh/Broadcast Channel* no `touring-daemon` que sincronize os deltas do `CrdtSemanticGraph` (via `rkyv` zero-copy) entre todos os projetos locais. Se o agente aprende um Anti-Pattern no Projeto A e ajusta o `QTable`, o Projeto B já herda essa vacina comportamental imediatamente.
2.  **Sherman-Morrison na GPU para Deep Contextual Bandits ($O(d^3) \to O(1)$ wall-clock):**
    *   Atualmente `FEATURE_DIM=25` é uma limitação manual rígida (com um `debug_assert_eq`).
    *   **Proposta:** Implementar o shader WGSL pendente (`LINUCB_SHERMAN_MORRISON_SHADER`) na placa gráfica. Isso desbloqueia o aumento de dimensões para integrar os embeddings completos (ex: 384 dimensões do HNSW) diretos no estado do reinforcement learning. O aprendizado sai da aproximação de features manuais para inferência profunda no espaço latente.
3.  **Active Palace Hierarchy Consolidation (Clustering Exponencial):**
    *   O Touring tem a fundação do "Palace Hierarchy" (Wing -> Room -> Closet -> Drawer) via PLN2 P4.2 e o `PatternClusterer` assíncrono.
    *   **Proposta:** Agendar tarefas de consolidação no fundo (`AsyncPatternClusterer`) que varrem o `Pensieve` (memória de erros) e o histórico de execuções usando o algoritmo `LeidenCommunityDetector` (já presente no `touring-learning`). Ele deve colapsar memórias redundantes em "Arquétipos de Solução" (Archived Patterns). Em vez de buscar por `K` instâncias em $O(N)$, a busca no HNSW retornará diretamente o Arquétipo com probabilidade bayesiana pré-calculada, condensando o uso da VRAM e disparando o hit-rate do `SemanticRecall`.

[Auditoria e Fundamentação nas Sources]
- Fonte 10 e 11: `Palace Hierarchy Memory` (P4.2) implementada (`memory/rlm.rs`).
- Fonte 18 e 31: `LINUCB_SHERMAN_MORRISON_SHADER` is "reserved for Sherman-Morrison inverse (future)".
- Fonte 80: `load_crdt_graph()` carrega de `.claude/data/crdt_graph.rkyv`.
- Fonte 251: `PatternClusterer` (HNSW-based lazy clustering) e `CrdtSemanticGraph` existem. `LeidenCommunityDetector` também existe.
</thought_process>

Solicitei uma pesquisa em segundo plano para capturar as informações mais atualizadas, mas já posso estruturar as ações definitivas com base no mapeamento da arquitetura (v30.3.2) e nos débitos técnicos estratégicos já previstos no código do Touring.

Para obter um ganho exponencial na Memória e no Aprendizado — transformando o sistema de um repositório isolado e de características manuais em uma malha de inteligência profunda e auto-optimizada —, você precisa ativar os três vetores de escala arquitetural pendentes no seu ecossistema.

### 1. Desbloqueio de Dimensionalidade no LinUCB via GPU Sherman-Morrison (Deep Contextual Learning)
**O Gargalo de Aprendizado:** O motor de aprendizado por reforço contextual (`LinUCBBandit` em `touring-learning`) atualmente depende de um vetor de características altamente comprimido de 25 dimensões (`FEATURE_DIM=25` estrito). Isso ocorre porque a CPU arca com a matemática da matriz inversa da covariância em tempo $O(d^3)$. O shader WGSL `LINUCB_SHERMAN_MORRISON_SHADER` está explícito no código como "*reserved for future*".
**O Ganho Exponencial:** 
* Ao implementar a fórmula de Sherman-Morrison no `wgpu 0.26`, a atualização da matriz é feita com paralelismo massivo em VRAM (buffers `STORAGE | COPY_SRC`). 
* O tempo de *wall-clock* é reduzido para constante $O(1)$ (apenas o overhead da submissão para a GPU). 
* Com esse gargalo aniquilado, você pode aumentar o `FEATURE_DIM` de 25 para 384 ou 512, alimentando o motor com **os embeddings puros vetoriais (HNSW/SIMD)** de uma task, em vez de features feitas a mão (ex: `L0-L3` ou `symbol_count`). O sistema passará a inferir intenções profundas de código instantaneamente via representação no espaço latente.

### 2. Sincronização Federada Zero-Copy via CRDT (Inteligência Contagiosa Inter-Projetos)
**O Gargalo de Memória:** O Touring armazena os grafos semânticos e as tabelas `QTable` de aprendizado na persistência local (`.claude/touring/graph.db`). Embora utilize `CrdtSemanticGraph` (projetado para distribuição sem conflitos) e pacotes zero-copy (`rkyv`), o conhecimento conquistado pelo agente no "Projeto A" não é herdado automaticamente pelo "Projeto B".
**O Ganho Exponencial:**
* Expor um canal de *broadcast* ou "Mesh" na camada do daemon para os CRDTs (`CrdtDelta`).
* Se uma iteração preditiva do MCTS detecta um erro recursivo no Projeto A, o nó de falha é registrado no *Pensieve* (memória de caminhos falhos). Se esse estado transitar em milissegundos via *memory-map* para os processos de outros projetos, o ganho de memória torna-se **$O(N^2)$ (Efeito de Rede)**.
* Um novo *gotcha* ou um ajuste nos pesos preditivos que salva 2 segundos num repositório passa instantaneamente a imunizar toda a malha de repositórios contra a mesma armadilha semântica.

### 3. Consolidação Hierárquica Autônoma com Leiden Clustering
**O Gargalo de Busca Semântica:** A sua *Palace Hierarchy Memory* (P4.2) é um arcabouço sólido, mas depende de armazenamento horizontal com indexação. Com o tempo, as instâncias sobrepostas no ANN (`SemanticRecall`) diluem as predições.
**O Ganho Exponencial:**
* Integrar o algoritmo `LeidenCommunityDetector` (presente em `touring-learning/clustering/`) no job de background que engloba o `AsyncPatternClusterer`.
* Operar em malha fechada (*Closed-loop*): Durante os tempos ociosos do *teammate*, a thread de background extrai memórias redundantes ou *anti-patterns* fragmentados e os "colapsa" em um único "Arquétipo Semântico". 
* Em vez de fazer uma busca KNN buscando 10 vizinhos e computando um consenso, a memória consolida dinamicamente os grafos esparsos. O tempo de inferência e de carga de contexto decai polinomialmente porque memórias repetidas viram nós super densos, enriquecendo drasticamente a qualidade da "intuição" no *pre_edit* e *pre_write*.
<thought_process>
[Ingestão]
A base de código do Touring v30.3.2 já opera em um regime de latência altamente otimizado: P50 de 1ms e hooks "warm" de <2ms. Técnicas lineares de otimização (pooling, concorrência simples) já foram exauridas com o uso do `rayon`, FFI zero-copy (`rkyv`) e IPC otimizado via Unix sockets. A busca por ganhos *exponenciais* exige a mitigação de gargalos de complexidade algorítmica ($O(N!)$, $O(c^N)$) e bloqueios de I/O na fronteira P99 sob concorrência destrutiva (hook storms).

[Desconstrução: Identificação de Gargalos Big-O e P99]
1.  **Explosão Combinatória no MCTS:** O gancho de simulação especulativa (`run_shadow_rollout`) utiliza um limite de tempo fixo (200ms) porque a detecção de ciclos de dependência (`Tarjan SCC`) está incompleta. Na presença de dependências circulares no AST, o MCTS bifurca em tempo exponencial $O(b^d)$, atingindo o disjuntor cego e invalidando a utilidade do rollout.
2.  **Álgebra Linear de Dimensionalidade Cúbica:** O `LinUCBBandit` transfere o cálculo de predição UCB para a GPU (`LINUCB_UCB_SHADER`), mas a etapa crítica de atualização via aprendizado (`LINUCB_SHERMAN_MORRISON_SHADER`) está pendente ("reserved for future"). A CPU arca com a inversão em $O(d^3)$ ou $O(d^2)$, competindo por ciclos com o parser AST e atrasando as injeções no WAL.
3.  **Observabilidade Passiva vs. Automação eBPF:** O crate `touring-telemetry` acessa o kernel Linux (Ring 0) via `aya-rs` para capturar falhas de página e pressão de memória ($O(1)$ overhead), mas essa telemetria não realimenta os limites fixos de alocação de contexto CILA. Picos geram thrashing do SO.

[Síntese Arquitetural: Implementação Produtiva]
Os vetores para ganho exponencial requerem a conversão de complexidades exponenciais/cúbicas para polinomiais/constantes, aproveitando os alicerces já presentes na v30.3.2.

[Auditoria de Trade-offs]
-   A inclusão do Tarjan SCC eleva o setup do MCTS em $O(|V|+|E|)$, um *overhead* irrisório frente à economia exponencial na travessia.
-   O processamento GPU da inversa de Sherman-Morrison adiciona latência de submissão do *command buffer* do `wgpu`, justificável assim que `FEATURE_DIM` exceder limiares mínimos.
</thought_process>

Para extrair saltos exponenciais de performance da infraestrutura do Touring v30.3.2, devemos transcender micro-otimizações (que já renderam um P50 de 1ms) e aniquilar gargalos fundamentais de complexidade algorítmica (Big-O) nos caminhos quentes do sistema. O código atual possui "costuras arquiteturais" projetadas para receberem upgrades matemáticos precisos. 

Como Engenheiro Staff, aqui estão as 3 arquiteturas de resolução prioritárias para escalar a estabilidade P99 e o *throughput* preditivo de forma logarítmica ou constante.

### 1. Colapso de Grafos Circulares via Tarjan SCC Completo (Redução $O(b^d) \to O(|V| + |E|)$)
**A Invariante:** No hook `PreToolUse[EnterPlanMode]`, o Touring dispara o `run_shadow_rollout` para simulações preditivas via MCTS (Monte Carlo Tree Search).
**A Falha Arquitetural:** O código confessa que a heurística de detecção de ciclo é um "MVP" e que a implementação completa do Tarjan SCC está pendente do `crate_dep_graph`. Sem DAG-ificação, a presença de recursão ou referências circulares força a travessia de grafos em tempo exponencial, saturando o `join_timeout` de 200ms e incrementando `mcts_shadow_timeout_count`. O rollout é abortado cega e heuristicamente.
**Execução:**
1. Integre o `petgraph::algo::tarjan_scc` na geração do `SymbolIndex` no `touring-ast`.
2. Durante a injeção do cache de dependências (`DependencyCache`), resolva os Componentes Fortemente Conexos (SCC) *a priori*.
3. O MCTS operará sobre um Grafo Direcionado Acíclico (DAG) condensado. Um problema de busca potencialmente infinito se torna um percurso de custo polinomial determinado. Zero *timeouts*, 100% de convergência preditiva.

### 2. Offload do Sherman-Morrison para WGSL Shader (Redução de $O(d^3)$ na CPU para $O(1)$ *Dispatch*)
**A Invariante:** O `LinUCBBandit` do `touring-learning` executa predições na GPU via `LINUCB_UCB_SHADER`, suportando matrizes de *features*.
**A Falha Arquitetural:** A atualização temporal por recompensa no QTable exige atualizar a inversa da matriz de covariância. O `LINUCB_SHERMAN_MORRISON_SHADER` está bloqueado como *"reserved (future)"*. Consequentemente, o motor faz *fallback* para CPU no `update_gpu`. Para `d` dimensões (atualmente indexado a 25), a operação na CPU escala pessimamente ($O(d^3)$ ou iterativamente $O(d^2)$).
**Execução:**
1. Desbloqueie o shader WGSL aplicando o padrão de buffer de trânsito estrito da arquitetura (`STORAGE | COPY_SRC` -> `copy_buffer_to_buffer`) já provado pelo `compute_dot_u4`.
2. Processe a fórmula de Sherman-Morrison puramente via `wgpu 0.26`.
3. **Impacto Latente:** Isso desloca a carga da CPU (que deve permanecer liberada para a thread-pool `rayon` do parser do `tree-sitter`). O *wall-clock time* do update do RL passa a ser constante ($O(1)$) referente apenas ao empilhamento na fila da placa gráfica. O `FEATURE_DIM` poderá escalar livremente de 25 para embeddings profundos.

### 3. Modulação Cibernética Autônoma via eBPF em Malha Fechada
**A Invariante:** As execuções do Claude Code exigem volumes massivos de injeção de contexto `CILA-aware` (orçamentos de limite duro em bytes).
**A Falha Arquitetural:** O crate `touring-telemetry` intercepta ativamente métricas do kernel (`aya-rs`), coletando `MemorySample` (pressão de *swap*, falhas de página e `cache_misses`). Entretanto, é uma via de mão única: os motores de inteligência não reagem à *backpressure* do hardware.
**Execução:**
1. Conecte o `EbpfMonitor` diretamente no `AdaptiveEngine`.
2. Transforme limites estáticos (como o `cila_budget_edit` no `pre_edit.rs`) em variáveis de controle realimentadas por uma função PID (Proporcional-Integral-Derivativa).
3. Sob rajadas de I/O em percentil P99 (detectáveis sub-milisegundos pelo eBPF via *page faults* ou contenção no `sqlite` WAL), o Touring reduzirá *in-flight* seus buffers de AST e vetores no `rkyv`, suprimindo a fragmentação da heap ($O(1)$ degradação graciosa). Evita-se atrito de troca de memória no disco (OS swapping) garantindo sobrevivência operacional do daemon sob contenção extrema.

<thought_process>
[Ingestão]
A análise da arquitetura do Touring (v30.3.2) revela um sistema altamente otimizado (IPC zero-copy via `rkyv`, offload de matrizes para GPU via `wgpu`, persistência em SQLite WAL, roteamento O(1) via *Fascículo Arqueado*). Contudo, a varredura profunda dos manifests e changelogs expõe costuras arquiteturais e débitos técnicos ("TODOs" marcados no código) que representam riscos de falha em percentis P99.

[Desconstrução: Vetores de Risco]
1.  **Dissonância de Estado (Split-Brain Latente):** O sistema utiliza 3 bancos SQLite consolidados (`knowledge.db`, `memory.db`, `graph.db`) operando paralelamente ao índice Tantivy FTS. Não há um protocolo de Two-Phase Commit (2PC) atômico entre a escrita no SQLite e o `commit()` do Tantivy. Falhas de I/O na metade do caminho fragmentam a base ontológica.
2.  **Explosão Combinatória no MCTS (Shadow Rollouts):** A simulação preditiva de deadlocks no modo de planejamento (`run_shadow_rollout`) utiliza uma heurística incompleta para detectar Ciclos Fortemente Conexos (SCC). O código marca explicitamente: "MVP — Tarjan SCC completo via `petgraph::algo::tarjan_scc`: TODO pending `crate_dep_graph` availability". Isso força o sistema a abortar simulações em 200ms por *timeout* ao entrar em recursão infinita.
3.  **Gargalo de Álgebra Linear (CPU Bound):** O motor de RL (LinUCB) paraleliza o cálculo de *Upper Confidence Bound* (UCB) na GPU, mas a atualização inversa da matriz de covariância está pendente. O shader `LINUCB_SHERMAN_MORRISON_SHADER` está marcado como "reserved (future)". Sob alta dimensionalidade, a CPU sofrerá contenção $O(d^3)$.
4.  **Malha eBPF Aberta:** O `touring-telemetry` utiliza `aya-rs` para capturar syscalls e pressão de memória no kernel (eBPF), mas atua primariamente de forma observacional. O `AdaptiveEngine` não consome esses dados para fechar a malha de controle, deixando o sistema vulnerável a *Out-of-Memory* (OOM) severos se o Claude gerar payloads massivos.
5.  **Exaustão de IPC (Circuit Breaker Flapping):** Embora os semáforos limitem o acesso ao daemon (56 slots por projeto), o *Circuit Breaker* é baseado em arquivo (`/tmp/touring-circuit-{uid}.state`) e é acionado após 3 falhas de IPC em 60s. Picos de requisições sob I/O saturado podem acionar o breaker falsamente, degradando a performance para o modo "standalone" (de P50 1ms para 15ms) de forma desnecessária.

[Síntese Arquitetural: Matriz de Resolução]
Para blindar o sistema e atingir a resiliência Staff-Level definitiva:
- Implementar um WAL unificado em memória (Event Sourcing) para SQLite + Tantivy.
- Finalizar a unificação do grafo de dependências para o Tarjan Exato.
- Fechar o loop cibernético do eBPF com o CILA.
- Despachar a matriz Sherman-Morrison na GPU.
</thought_process>

A arquitetura atual do Touring atingiu maturidade estrutural de alto desempenho (latências P50 de 1ms, *zero-copy* IPC, quantização de vetores U4). Contudo, a engenharia de sistemas em nível Principal Staff exige analisar não a via feliz, mas a degradação sob contenção máxima (P99). Abaixo dissecamos as principais fraquezas inerentes à sua topologia e a execução algorítmica para resolvê-las.

### 1. Inconsistência Distribuída (Split-Brain) entre Armazenamentos
**O Risco:**
O conhecimento ontológico está fragmentado entre três instâncias de banco de dados SQLite em modo WAL (`knowledge.db`, `memory.db`, `graph.db`) e um motor de *Full-Text Search* independente (`Tantivy`). Sob *hook storms* severas (ex: refatoração multi-arquivo gerada pelo Claude), se o processo `touring-project-actor` sofrer preempção pesada do SO ou falha de disco entre a escrita no SQLite e o `.commit()` do Tantivy, ocorrerá um desvio de estado (Split-Brain). Símbolos existirão no grafo de chamadas, mas não serão encontrados no índice BM25, quebrando algoritmos como o *Reciprocal Rank Fusion* (RRF).

**A Solução (Arquitetura de Event Sourcing Unificado):**
Abandone writes diretos nos terminais de dados.
1. Implemente um Log de Mutação Unificado (Event-Sourced WAL) usando uma fila na memória baseada em `rkyv`.
2. Emita artefatos atômicos (ex: `SymbolMutationEvent`).
3. Ambos, rusqlite e TantivyIndex, atuam como *Sinks* transacionais inscritos na fila. O deslocamento (*offset*) lógico só avança se o *Two-Phase Commit* (2PC) local confirmar a persistência em ambos. A complexidade espacial do WAL transitório é de $O(N)$ bytes, ínfimo na RAM, erradicando o risco de corrupção.

### 2. O Gargalo Algorítmico do MCTS (Shadow Rollout Timeouts)
**O Risco:**
O gancho preventivo `D4` (`run_shadow_rollout`) utiliza Monte Carlo Tree Search para prever *deadlocks* no modo de planejamento do Claude. No entanto, a base de código admite uma falha heurística crítica: *"MVP — Tarjan SCC completo via petgraph::algo::tarjan_scc : TODO pending crate_dep_graph availability"*. Devido à incapacidade de resolver dependências circulares com precisão, o MCTS entra em ciclos infinitos de avaliação de grafos redundantes, forçando o motor a usar um *timeout* cego de 200ms para evitar travar a execução (`mcts_shadow_timeout_count` é incrementado). Simulações abortadas equivalem a predições cegas.

**A Solução (Redução a DAG via Componentes Fortemente Conexos):**
1. Conclua a implementação do `crate_dep_graph`.
2. Extraia o grafo de dependências do projeto e execute o Algoritmo de Tarjan para Componentes Fortemente Conexos (SCC) em estrito tempo $O(|V| + |E|)$.
3. Colapse os ciclos mútuos encontrados em "Super-Nós".
4. Execute o *rollout* do MCTS **exclusivamente no DAG resultante**. Isso altera o perfil computacional do *rollout* de combinatório/exponencial para polinomial, eliminando matematicamente a necessidade de disjuntores (*timeouts*) por laços infinitos.

### 3. CPU/GPU Stall na Álgebra do LinUCB
**O Risco:**
A *GPU Optimization Wave* transferiu a computação dot-product paralela do *Upper Confidence Bound* para o `wgpu` (`LINUCB_UCB_SHADER`). Entretanto, a etapa de aprendizado – atualização da matriz inversa de covariância – sofre gargalo, marcada como `LINUCB_SHERMAN_MORRISON_SHADER — reserved for Sherman-Morrison inverse (future)`. Para milhares de *features* ou aumento no número de braços do *Bandit*, a inversão da matriz na CPU opera em tempo $O(d^3)$ ou recorre à fórmula iterativa Sherman-Morrison em $O(d^2)$ competindo por ciclos da CPU com *workers* de *parsing* da árvore AST.

**A Solução:**
Desenvolva imediatamente o *compute shader* WGSL pendente para a fórmula de atualização de Sherman-Morrison ($A_{t}^{-1} = A_{t-1}^{-1} - \frac{A_{t-1}^{-1} x_t x_t^T A_{t-1}^{-1}}{1 + x_t^T A_{t-1}^{-1} x_t}$). Use o padrão de buffer de trânsito (`STORAGE | COPY_SRC`) estabelecido pela simetria U4 para reduzir o processamento a $O(1)$ tempo de bloqueio de CPU (apenas submissão da *command queue* GPU), preservando o *budget* de latência máxima permitida (15ms).

### 4. Isolamento Térmico vs. Observabilidade Passiva (eBPF)
**O Risco:**
O crate `touring-telemetry` coleta amostras cruciais do kernel Linux via `aya-rs` (pressão de memória RSS, SWAP, cache misses e latências de *syscall*). A falha arquitetural: ele opera em "Malha Aberta" (Open Loop). O sistema sabe quando a máquina do desenvolvedor está sob contenção severa de I/O, mas as estruturas de roteamento CILA limitam os *budgets* baseados em regras estáticas (ex: limite rígido L2-L3 = 3000 caracteres) e limites fixos do MCTS. OOM e quebras de swap sob *hook storms* não são prevenidos de fato.

**A Solução (Closed-Loop Autonomic Tuning):**
Acople diretamente o *struct* `MemorySample` do `EbpfMonitor` no núcleo do `AdaptiveEngine`.
1. Aplique controle Proporcional-Integral-Derivativo (PID).
2. Se a detecção eBPF mostrar contenção superior ao percentil P95 (thrashing de SWAP), force o `CILA_BUDGET` ao nível L0/L1 (minimal signal) dinamicamente.
3. Aborte inserções assíncronas no banco em favor de um buffer circular *in-memory* até que a pressão se dissipe. O Touring passa de observador passivo a orquestrador cibernético de auto-sobrevivência.