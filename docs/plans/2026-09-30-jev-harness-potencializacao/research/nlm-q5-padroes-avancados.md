---
type: Research
plan_id: 2026-09-30-jev-harness-potencializacao
title: NotebookLM JEV — nlm-q5-padroes-avancados
sources_used: ["d79065e0-f8c6-4f73-acae-78ed59af61b9", "07964567-81ba-4eff-bb86-554b7c3eb3df", "9b733a91-a529-4cc8-912e-08339cbdf843", "3f9bf57d-e5c1-4831-976b-1b0530261287", "19def919-5d27-4ca6-8056-a9a1041c377d", "e1cfa418-f58d-4a21-9e24-0fc2ad7424c5", "aaffea66-d39f-49ca-8d28-208f44f42afc", "871dc0e3-ded7-4332-abd4-59cb14a0eac4", "359f586d-32eb-443a-ae4a-74d81b3f8cb0", "fcf9e995-f30c-4935-b0b8-fc6334fbe417"]
---

Esta é uma análise técnica aprofundada dos padrões avançados de integração do **Jev** (desenvolvido pela TypeSafe AI), baseada detalhadamente nas fontes especificadas.

---

### 1. Arquitetura de Engenharia: Onde o Jev Senta no Loop do Agente

O Jev é classificado como um **modelo Sistema 1** (*gut call* intuitivo, rápido e barato) em contraste com os LLMs tradicionais, que operam como **Sistema 2** (raciocínio deliberado, lento e custoso) [1-5]. Ele não gera texto autoregressivo token por token; em vez disso, avalia um estado (`state`) e responde a perguntas tipadas (`questions`) retornando probabilidades calibradas e decisões em uma única passagem paralela (sub-100 a 500 ms) [1, 5-9].

No loop de um agente de código ou automação, o Jev atua em **quatro posições estratégicas**:

1. **Pre-hook / Gatekeeper (Entrada e Roteamento):** Intercepta os prompts do usuário ou comandos antes de chamarem o LLM [10-18].
2. **Tool / Harness Middleware (Segurança em tempo de execução):** Atua dentro das chamadas de ferramentas (*pre-tool execution hook*) do agente, validando se comandos (ex: bash ou gravações em arquivo) são destrutivos antes de sua execução real [10, 19-21].
3. **Extension Tool / Agent Assist (`ask_jev`):** Atua como uma ferramenta invocada pelo próprio agente principal para filtrar, inspecionar arquivos ou validar hipóteses sem carregar o contexto do LLM com textos desnecessários [22-26].
4. **Post-hook / Verifier (Saída e Avaliação):** Valida as respostas geradas pelo LLM (ex: checagem de citação de fontes, detecção de erros) ou atua como avaliador de traces (*online evals*) [15, 20, 27-29].

#### Decisões Assumidas e Mecanismo de Fallback (*Confidence Thresholds*)
O Jev assume decisões determinísticas e classificatórias: nivelamento de severidade de bugs, detecção de *prompt injection*, risco de execução bash, seleção de rotas de modelos e categorização de arquivos [10, 13, 30-33]. 

O **mecanismo de fallback** baseia-se na **métrica de confiança (`confidence`)** retornada pelo Jev [34-37]:
- Se a probabilidade ou o índice de confiança retornado pelo Jev estiver acima do limite (*threshold*, ex: \\(\ge 0.80\\)), a ação é executada automaticamente por código determinístico (`if statement`) [34, 38-40].
- Se a confiança ficar abaixo do limite (zona cinzenta ou incerteza, ex: \\(< 0.60\\)), o sistema dispara um **fallback condicional**: degrada a decisão para um modelo Sistema 2 mais pesado (Claude/Astra) ou redireciona a tarefa para revisão humana (*Human-in-the-loop*) [31, 34, 36, 39-43].

---

### 2. Nível a Nível: "10 Levels of Jev For Agentic Engineers"

Baseado no trabalho de engenharia de *harness* de IndyDevDan [44]:

* **Level 1: Basic Decision Making (Boolean / Smart IF):** Decisão binária sim/não sobre um estado.
  * *Uso:* Detecção de *prompt injection* na API. Retorna probabilidades e grau de confiança (ex: 99% de certeza de injeção) [30, 31, 44].
* **Level 2: Multiple Choice Options (`choice`):** Seleção de uma ou mais opções a partir de uma lista definida.
  * *Uso:* Triagem automatizada de chamados de suporte/bugs em plataformas como Jira/Linear (identifica se é bug e qual a prioridade) em milissegundos sem chamar um LLM [32, 45-47].
* **Level 3: Composite Scoring (`score`):** Avaliação de critérios em escala contínua com pesos calibrados no código.
  * *Uso:* Análise de risco em revisões de código (*code review risk*), considerando se altera autenticação, entradas de usuário ou padrão de commits [34, 47-49].
* **Level 4: Confidence Gating:** Criação de portões de decisão onde erros custam caro.
  * *Uso:* Validação de segurança no comando Bash do agente (ex: identificar se `git push --force` ou `rm -rf` é irreversível ou destrutivo) [10-12, 34].
* **Level 5: Intent & Model Routing:** Uma decisão barata no Jev antes de invocar recursos caros.
  * *Uso:* Roteamento de tarefas para o agente ideal (ex: *Browser Agent*, *Fast Agent*, *Desktop Agent*) com base na intenção do comando [33, 50, 51].
* **Level 6: Guardrail Hooks (`JevGuard`):** Inserção do Jev diretamente no *harness* do agente como um *hook* de pré-execução de ferramenta.
  * *Uso:* Bloqueio automatizado de comandos perigosos no terminal e bloqueio de edições em arquivos sensíveis (ex: `.env`) [19, 52-54].
* **Level 7: Agent Context Self-Compaction:** Decisão interna do *harness* sobre o momento exato de compactar o histórico.
  * *Uso:* O Jev analisa a mudança de tarefas e o volume de tokens (ex: limites de 6K, 10K, 14K tokens) para ordenar ao agente que compacte a memória [54-57].
* **Level 8: Cheap Read (Inspecionar sem ler no LLM):** Delegação de tarefas de controle de leitura de arquivos ao Jev.
  * *Uso:* Ferramentas personalizadas (`ask_jev_filebool`, `ask_jev_filechoice`) que inspecionam arquivos no disco e respondem perguntas ao agente sem injetar o conteúdo completo no contexto do LLM [22-24, 57, 58].
* **Level 9: Files at Scale (Varredura Paralela / Globs):** Escalonamento do Nível 8 para repositórios inteiros.
  * *Uso:* Varredura em paralelo (*globs* de arquivos TypeScript) para identificar arquivos relevantes para um bug de arredondamento ou *to-dos* críticos [58-64].
* **Level 10: Agentic Jev:** Concessão de autonomia total para o LLM invocar o Jev dinamicamente via `ask_jev`.
  * *Uso:* O agente usa o Jev para autovalidação, classificação de falhas de testes em tempo real, QA e validação de premissas de código a custo quase nulo [25, 26, 65-67].

---

### 3. Arquitetura de Harness: "Building a Harness with Jev" (LangChain)

A especificação de *harness* apresentada pela LangChain contrasta o loop tradicional com a nova arquitetura baseada no Jev [1, 7, 68, 69]:

```
[Loop Tradicional]: Requisicao -> LLM (Sistema 2) -> Tool Call -> Resultado -> Loop LLM (Lento/Caro)
[Harness com Jev]:  Requisicao -> Jev (Sistema 1 / Roteador) -> Validação/Filtro -> LLM apenas quando necessário
```

#### Primitivas de Pergunta do Jev [7, 70]
1. `choice`: Escolha múltipla de uma lista (retorna a probabilidade de cada opção e o *confidence*) [70].
2. `score`: Posição em uma escala/rubrica contínua definida (ex: grau de frustração do cliente de 0 a 2) [70].
3. `noul` (ou `bool`): Resposta booleana (probabilidade de ser verdadeiro de 0 a 1) [70].

#### Casos de Uso Arquiteturais no Harness [13, 20, 27]
* **Roteamento de Modelos (*Model Routing*):** Avalia a complexidade do prompt contra critérios predefinidos para decidir instantaneamente se envia para um modelo rápido e barato ou um modelo de raciocínio pesado [13].
* **Modo Automático com Trava de Segurança (*Auto Mode / Guardrail Middleware*):** Middleware intercepta chamadas de ferramentas e consulta o Jev se a ação é arriscada (ex: deletar banco de dados). Se classificada como arriscada, a chamada é bloqueada em tempo de execução [20].
* **Avaliador Online de Traces (*Online Evals / LLM as a Judge*):** Substitui avaliadores LLM caros na avaliação de saídas de agentes em relação a rubricas (ex: correção, grounding, citações válidas) com custo irrisório e maior consistência [20, 27].

---

### 4. Conceitos do Jev para Usuários Claude: "Every Jev Concept Explained"

A integração conceitual com o ecossistema Anthropic/Claude estabelece que **o Claude é a conversa e o raciocínio; o Jev é a tubulação de infraestrutura por baixo** [4, 71]:

#### Componentes da Chamada
* **`state`**: O dado não estruturado de entrada (e-mail de suporte, diff de código, arquivo) [4].
* **`questions`**: O conjunto de perguntas tipadas (`noul`, `choice`, `score`) aplicadas sobre o `state` [35-38, 71, 72].
* **`confidence`**: Valor derivado da dispersão das probabilidades distribuídas entre as opções do `choice` e `score` [35-37, 72].
* **Paralelização em Massa (*Speculative Fan-out*):** É possível enviar múltiplas perguntas de tipos variados sobre o mesmo `state` em uma única requisição HTTP. O Jev processa todas em paralelo, cobrando o texto de entrada apenas uma vez [39, 73-75].

#### Checklist para Design de Perguntas [76-78]
1. **Listar as Ações:** Defina primeiro o que o código fará com o resultado [77].
2. **Formular a Pergunta:** Crie a frase de gatilho que define o tipo da primitiva (`noul` para agir/não agir, `choice` para rotear, `score` para ordenar/ranquear) [77, 78].
3. **Mapear o Estado (`state`):** Identifique exatamente quais campos de dados um humano precisaria analisar [78].
4. **Tratar Edge Cases:** Inclua opções de salvaguarda (ex: opção `"nenhum dos anteriores"` em escolhas) [78, 79].
5. **Definir Limites de Confiança:** Ajuste as tolerâncias (*confidence thresholds*) conforme o custo de um diagnóstico errado [78].

#### Os 4 Padrões de Integração [39, 40, 42, 43, 75]
1. **Pergunte Tudo de Uma Vez:** Envie todas as perguntas no mesmo *payload* [39, 75].
2. **Decisões com Portão de Confiança (*Confidence Gating*):** Defina limiares específicos por ação conforme o risco [39, 40].
3. **Pontuação Composta (*Composite Scoring*):** Quebre julgamentos complexos em dimensões independentes e combine os resultados com pesos no código [40, 42].
4. **Roteamento de Intenção (*Intent Routing*):** Classifique e envie para o executor ideal (código determinístico, LLM especializado ou humano) [42, 43].

---

### 5. Padrões Avançados de Integração: Claude OS e Claude Opus 5.5

#### Arquitetura "This NEW Jev + Claude OS" (3-Tier Routing & Local Voice)
O sistema opera em uma arquitetura de 3 camadas visuais/memória (UI, Carpathy Obsidian RAG com `raw/`, `wiki/`, `outputs/` e matriz de *skills*) integrada a um pipeline de voz local e roteamento em 3 Tiers [16-18, 80-85]:

```
[Voz do Usuário] -> Local Whisper (STT) 
                   -> Jev Classifier (Roteador Instantâneo)
                        ├── Tier 1 (Simples/Sem IA): Código/Interface (Sub-100ms)
                        ├── Tier 2 (Médio): Chamada para LLM Pequeno (Haiku/Luna)
                        └── Tier 3 (Complexo): Loop Agente Claude Code / Codex
                   -> Local Kokoro / Fish Audio (TTS / Resposta por Voz)
```
* **Tier 1 (Instantâneo / Sem LLM):** Ações diretas de interface/sistema (abrir relatórios, navegar no Obsidian). O Jev identifica a intenção e o código executa sem tocar em nenhum LLM [16, 17].
* **Tier 2 (Raciocínio Leve):** Tarefas simples de consulta processadas por modelos rápidos/baratos (Haiku ou Luna) [18].
* **Tier 3 (Trabalho Pesado):** Ativação completa do *loop* agente no terminal com Claude Code/Codex [18, 86].

#### Arquitetura "Claude Opus 5.5 + Jev = 10x Power" (Brain vs. Determinator)
A arquitetura divide o agente em **Cérebro (*Brain*)** e **Determinador Probabilístico (*Determinator*)** [2, 3, 87]:

* **Claude Opus 5.5 (O Cérebro):** Executa o planejamento de alto nível, raciocínio estratégico, pesquisa, geração de texto e escrita de código [3, 87].
* **Jev (O Determinador):** Assume todas as decisões intermediárias no loop — validação de gatilhos, pontuação de dados, regras de segurança e classificação de sinais [3, 87].
* **Otimização do Loop:** Em vez de fazer uma chamada custosa ao Opus a cada passo do loop para responder "devo continuar?" ou "esta entrada é válida?", o Jev responde essa decisão probabilística em ~400ms, reduzindo o consumo de tokens e custos em até 10x-400x [2, 3, 88-90].

---

### 6. Código e Pseudocódigo Citados nas Fontes

#### A. Estrutura Padrão de Payload JSON do Jev [32, 45, 46, 70, 91-93]
Requisição enviada à API do Jev contendo `state` e as perguntas com primitivas `noul`, `choice` e `score`:

```json
{
  "state": {
    "ticket_id": "BUG-404",
    "customer_plan": "Enterprise",
    "subject": "Botão de exportação quebra no Safari",
    "body": "Ao clicar em exportar no Safari a tela congela. No Chrome funciona."
  },
  "questions": [
    {
      "id": "is_bug",
      "type": "noul",
      "instructions": "O e-mail relata um defeito no software?",
      "criteria": {
        "true": "Relata comportamento inesperado, erro ou travamento.",
        "false": "Dúvida geral, pedido de recurso ou cobrança."
      }
    },
    {
      "id": "category",
      "type": "choice",
      "instructions": "Qual departamento deve tratar este chamado?",
      "options": [
        {"name": "frontend", "description": "Erros de interface, layout ou navegador"},
        {"name": "backend", "description": "Erros de servidor, banco de dados ou API"},
        {"name": "billing", "description": "Problemas de pagamento ou fatura"}
      ]
    },
    {
      "id": "priority_score",
      "type": "score",
      "instructions": "Qual a severidade da prioridade deste chamado?",
      "rubric": [
        {"score": 0, "description": "Baixa: Cosmético ou dúvida simples"},
        {"score": 1, "description": "Média: Bug com solução alternativa disponível"},
        {"score": 2, "description": "Alta: Bloqueio total sem solução alternativa"}
      ]
    }
  ]
}
```

#### B. Pseudocódigo do LangChain TypeSafe Integration (`TypeSafeClassifier`) [13]
```python
from langchain_typesafe import TypeSafeClassifier

# Inicialização do classificador
classifier = TypeSafeClassifier(typesafe_api_key="SUA_CHAVE_API")

# Definição do estado e perguntas
state = "Falha ao conectar conta do Stripe há 3 dias. Perdendo vendas."
questions = [
    {
        "id": "urgency",
        "type": "noul",
        "instructions": "A mensagem indica urgência financeira crítica?"
    },
    {
        "id": "team",
        "type": "choice",
        "options": ["billing", "tech_support", "sales"]
    }
]

# Invocação instantânea
response = classifier.invoke(state=state, questions=questions)

# Resposta estruturada retornada
# response = {
#     "urgency": {"result": True, "probability": 0.999},
#     "team": {"selected": "billing", "probability": 0.840, "confidence": 0.596}
# }
```

#### C. Pseudocódigo de Guardrail Hook Pre-Hook (`JevGuard`) [19, 20, 53]
Hook em TypeScript/Python inserido no *harness* do agente antes de executar ferramentas Bash:

```typescript
// Hook de pré-execução de ferramenta Bash no Harness do Agente
async function JevGuardPreHook(toolCall: { name: string, args: { command: string } }) {
  if (toolCall.name !== "bash") return { proceed: true };

  const jevResponse = await callJevAPI({
    state: { command: toolCall.args.command },
    questions: [{
      id: "is_destructive",
      type: "noul",
      instructions": "Este comando Bash é destrutivo ou irreversível?",
      criteria: {
        true: "Deleta arquivos permanentemente (rm -rf), faz push forçado, altera schemas de prod.",
        false: "Comandos de leitura, testes, listagem (ls, git status, npm test)."
      }
    }]
  });

  const isDestructive = jevResponse.answers.is_destructive.probability > 0.80;

  if (isDestructive) {
    // Bloqueia a ferramenta antes do LLM executá-la
    return {
      proceed: false,
      error: `[JevGuard] Comando bloqueado por segurança: O comando '${toolCall.args.command}' foi classificado como destrutivo/irreversível.`
    };
  }

  return { proceed: true };
}
```

#### D. Pseudocódigo de Roteamento Dinâmico de Tiers no Claude OS [16-18]
```python
def route_user_request(user_prompt: str):
    jev_routing = call_jev(
        state={"prompt": user_prompt},
        questions=[{
            "id": "tier",
            "type": "choice",
            "options": [
                {"name": "tier_1", "description": "Comando de UI, abrir relatórios, navegação estática"},
                {"name": "tier_2", "description": "Pergunta simples de conhecimento ou resumo leve"},
                {"name": "tier_3", "description": "Tarefa complexa de programação, refatoração ou relatório profundo"}
            ]
        }]
    )

    tier = jev_routing["tier"]["selected"]
    confidence = jev_routing["tier"]["confidence"]

    # Fallback se a confiança do Jev for baixa
    if confidence < 0.60:
        return execute_claude_agent_loop(user_prompt, model="claude-3-5-sonnet")

    if tier == "tier_1":
        return execute_local_ui_command(user_prompt)  # Sub-100ms, sem LLM
    elif tier == "tier_2":
        return execute_llm_call(user_prompt, model="claude-3-5-haiku")  # Rápido/Barato
    else:
        return execute_claude_agent_loop(user_prompt, model="claude-3-5-sonnet")  # Loop completo
```

---

💡 **Próximo passo recomendado:** Se desejar explorar a implementação de um destes padrões em código (como o *JevGuard* para pre-hooks de ferramentas ou um roteador customizado para o Claude Code), posso ajudar a estruturar o script exato para o seu fluxo!
