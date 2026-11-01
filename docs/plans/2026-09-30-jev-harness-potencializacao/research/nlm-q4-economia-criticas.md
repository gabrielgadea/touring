---
type: Research
plan_id: 2026-09-30-jev-harness-potencializacao
title: NotebookLM JEV — nlm-q4-economia-criticas
sources_used: ["d79065e0-f8c6-4f73-acae-78ed59af61b9", "f3a2f14d-5165-4725-8440-b0ad3cab2f9e", "527135c4-6225-4f77-9203-3faa4f6858d3", "19def919-5d27-4ca6-8056-a9a1041c377d", "b6cdabb3-722d-49f6-9d2e-be3e58d47f4d", "3f9bf57d-e5c1-4831-976b-1b0530261287", "ee0a1a82-90e3-4696-8b0f-45896f358e9e", "5c195ad9-dd6b-4253-8dcf-b99b72f319e5", "0113cd93-11fa-4a19-a276-3d368e67431d", "83d8b7f1-ada1-485d-92eb-df772066192c", "9b733a91-a529-4cc8-912e-08339cbdf843", "4d3bbd2c-3081-4c2b-88ff-3b673394e0b9", "8ca7e142-bada-4784-a2af-7f1268483e55", "0419ff42-6695-44fc-b8cd-05f26e73f84f", "07964567-81ba-4eff-bb86-554b7c3eb3df", "359f586d-32eb-443a-ae4a-74d81b3f8cb0", "97eb34d5-67e6-40a8-990f-2f8e5cb5b518", "e3aa73a5-5766-4179-b1fe-e2b62ecef78a", "808a4edf-b4de-4204-8789-e5acdb35e8fa", "a9f85823-36ed-424f-9d03-0892972b516d", "92a50630-0334-4368-9a0c-3a71515e4fa6", "aaffea66-d39f-49ca-8d28-208f44f42afc", "d747c98e-cb05-4ec0-a45a-6f565672fbb7", "e1cfa418-f58d-4a21-9e24-0fc2ad7424c5", "871dc0e3-ded7-4332-abd4-59cb14a0eac4"]
---

O **Jev** (desenvolvido pela TypeSafe AI) é apresentado nas fontes não como um modelo de linguagem tradicional (LLM) ou chatbot, mas sim como um **modelo de decisão/classificação de Sistema 1** (*System 1 model*) [1-5]. Ele não gera texto livre nem escreve código; em vez disso, ele processa um estado (*state*) e perguntas tipadas, retornando probabilidades e respostas estruturadas via JSON em uma única passada paralela (*single pass*) [1, 4, 6-8].

Abaixo está o detalhamento completo sobre a economia real, preços, testes medidos nas 7 fontes específicas citadas e **todas as críticas, limitações e riscos** apontados nos materiais.

---

### 1. Visão Geral da Economia de Tokens, Preço e Latência

* **Preço dos Tokens:** O Jev custa **US\$ 0,042 por milhão de tokens de entrada** (ou US\$ 42 por bilhão de tokens de entrada) [9-13]. Os **tokens de saída (*output*) são 100% gratuitos e ilimitados** [9-12, 14, 15].
* **Velocidade / Latência:** Respostas típicas entre **70 ms e 500 ms** (sub-segundo) [16-19]. As fontes relatam que ele é entre **20x a 400x mais rápido** e de **40x a 1.000x mais barato** do que LLMs convencionais (como Haiku, Fable, Opus ou GPT-6 Astra) [20-25].
* **Sem Custo de Raciocínio Sequencial:** Como o Jev avalia todas as perguntas de uma vez em paralelo e não gera texto token a token, a adição de múltiplas perguntas em uma mesma chamada praticamente não altera a latência nem gera custos de saída [8, 26, 27].

---

### 2. Detalhamento dos Números e Casos nas 7 Fontes Solicitadas

#### 1. *"I Connected Jev to Hermes: 300 Tasks for Just \$0.001 🤯"*
* **Experimento Real:** O autor conectou o Jev ao agente Hermes para analisar e classificar **300 comentários do YouTube** [28, 29].
* **Resultados e Desempenho:** O Jev processou **100% dos comentários**, realizando **2.400 decisões em apenas 17 segundos** [29].
* **Custo Medido:** O custo total do teste foi de **US\$ 0,015** (com zero erros e zero tentativas de reenvio/retries), registrando uma latência média de **586 ms** [29]. Em 24 horas de testes contínuos com Hermes, o autor gastou apenas US\$ 0,0154 [23].
* **Comparação Projetada:** 
  * Para **300 comentários**: Jev custou **US\$ 0,015** contra **US\$ 5,90 a US\$ 7,40** do Fable (o Fable foi entre 435x e 535x mais lento, levando ~3s por item) [30, 31].
  * Para **1 milhão de comentários**: O Jev custaria **US\$ 51**, enquanto o Fable custaria **US\$ 22.000** [31].

#### 2. *"Parei de queimar tokens no Claude Code com o Jev (3 recursos para você copiar)"*
* **Estratégias Implementadas:**
  1. **Compactação de Contexto (*Fast Jev Compaction*):** Substitui o resumo lento de conversas por decisões rápidas do Jev para decidir o que manter ou descartar no histórico do terminal [32, 33].
  2. **Roteador de Modelos (*Model Router*):** O Jev avalia cada prompt recebido e direciona para o modelo ideal: *Haiku* (tarefas simples), *Sonnet* (desenvolvimento padrão) ou *Opus* (tarefas profundas/difíceis, ou quando o risco de quebrar o código passa de 70% ou a confiança cai de 60%) [34-36].
  3. **Seletor de Skills:** Em vez de carregar dezenas de *skills* no contexto (gastando tokens), o Jev analisa a mensagem e carrega apenas a *skill* exata necessária (ou nenhuma) em milissegundos [37-41].
* **Números e Latência:** A tomada de decisão de roteamento pelo Jev variou de **13,91 ms a 400 ms** [41, 42].

#### 3. *"We need to talk about Jev..."*
* **Números e Comparativo de Velocidade:** Aponta redução de custos de até 400x e velocidade 200x superior [15, 43].
* **Teste do *Wiki Race* (Navegação Web):** O Jev concluiu **5 saltos de páginas em 0,5 segundos**, enquanto os modelos *GPT 5.6 Terra*, *Haiku 4.5* e *Sonnet 5* levaram entre **4 e 5 segundos** para a mesma tarefa [44].
* **Destaque:** Ausência de alucinação de estrutura/esquema (0% de erro de formato) [45, 46].

#### 4. *"Jev CEO: I made ChatGPT, now I'm building what's next"*
* **Visão do Criador (Diego Almeida, coautor do InstructGPT e RLHF na OpenAI):** Diego afirma que os LLMs atuais estão presos na "era dos assistentes" (treinados via RLHF para agradar o usuário no chat), o que obriga a presença contínua de um humano no loop (*human-in-the-loop*) [47-49].
* **O Conceito do Jev:** Foi treinado com **RLCD** (*Reinforcement Learning for Calibrated Decisions*) em vez de RLHF, visando **automação real e decisões calibradas para software**, funcionando como "encanamento" (*plumbing*) invisível sob os sistemas [50, 51].
* **Premissa do Mercado:** Diego destaca que a lição aprendida por todas as empresas de software é **não usar LLMs tradicionais para tomar decisões com riscos/estacas reais ao negócio**, pois eles foram otimizados para parecerem certos, e não para precisão matemática calibrada [48, 52].

#### 5. *"JEV: É Aqui Que Eu Vejo Dinheiro de Verdade com IA"*
* **Aplicação em Vendas Reais:** Uso do Jev em um *Sales Copilot* que acompanha reuniões de vendas e transcreve áudio em tempo real [53-60].
* **Economia Medida:** 
  * **Sem Jev (chamando LLMs diretamente em chamadas de 40-50 min):** O custo por reunião variava de **R\$ 20,00 a R\$ 30,00** (devido a chamadas contínuas de LLM para analisar objeções e contextos) [58, 61, 62].
  * **Com Jev (filtrando e classificando antes de acionar a LLM):** O custo estimado cai para **R\$ 5,00 por reunião** [62, 63].
* **Cálculo Extrapolado:** Em uma reunião de 2 horas (14.400 chamadas de classificação a US\$ 0,00128 cada), o custo do Jev fica em apenas **~US\$ 0,11** [63].
* **Latência:** Média de **300 ms a 550 ms** no Jev vs. **3,5s a 4s** das LLMs tradicionais [18, 64, 65].

#### 6. *"Por que o lançamento do Jev é uma notícia melhor do que parece"*
* **Experimentos e Dados Comparativos (QuantBrasil):**
  1. **Análise de Sentimento de Notícias Financeiras (12 chamadas):**
     * Latência: *DeepSeek V4 Flash* levou **2,6s** vs. *Jev* **0,4s** (Jev foi 6x a 7x mais rápido) [66].
     * Custo por 1.000 classificações: **US\$ 0,14** (DeepSeek) vs. **US\$ 0,02** (Jev) — uma redução de **80% no custo** [66].
  2. **Mapeamento de Empresas (*Ticker Shortlist* em 1.667 ações):**
     * Universo completo bruto (1.667 ações): Custo de **US\$ 1,15** (Jev) vs. **US\$ 5,07** (GPT-6 Astra) por 1.000 artigos [67].
     * Com pré-filtragem (*Shortlist*): Custo de **US\$ 0,02** (Jev) vs. **US\$ 5,00** (GPT-6 Astra) por 1.000 artigos — **200x mais barato** e com o mesmo resultado final [67].

#### 7. *"This Might Be the Best AI Release of 2026"*
* **Preço e Volume (The Primeagen):** Custa US\$ 42 por bilhão de tokens de entrada [13, 68].
* **Exemplo de Escala:** Uma consulta sobre *Super Smash Bros* com 345 tokens de entrada pode ser executada **aproximadamente 3 milhões de vezes por US\$ 44,00** [13].
* **Velocidade:** Execução em **~100 ms** [69]. Em um teste no jogo *Balatro*, o autor consumiu **8 milhões de tokens** em pouco tempo com custo irrisório [13, 70].

---

### 3. Toda Crítica, Ressalva, Limitação e Risco Apontados nas Fontes

Embora o Jev seja extremamente rápido e barato, as fontes trazem um conjunto rigoroso de **críticas, ressalvas e limitações técnicas**:

1. **Não é um Modelo de Linguagem nem Chatbot (Não Gera Texto ou Código):**
   * O Jev **não escreve frases, não responde chats, não gera código do zero e não conversa com o usuário** [5, 71-81]. Ele não substitui modelos de raciocínio profundo (*System 2*) como Claude Opus, GPT-6 Astra ou Fable [5, 51, 72, 73, 81-83].
2. **Capacidade Limitada de Raciocínio / Falha em Casos Ambíguos:**
   * O Jev possui menor precisão bruta do que LLMs de fronteira (em benchmarks internos da TypeSafe, o Jev obteve 67,8% de precisão contra 73,1% do Claude Opus 5 e 74,1% do GPT 5.6 Sol) [24].
   * **Exemplo Real de Erro:** No teste do QuantBrasil com a notícia *"Trump proíbe CNN, MSNBC..."*, o Jev classificou erroneamente a notícia como referente ao banco *Morgan Stanley* (ticker `MS`), pois encontrou a sigla "MS" dentro de "MSNBC" e **não teve inteligência/raciocínio suficiente** para entender o contexto do texto [84, 85]. A LLM (DeepSeek/GPT-6) entendeu o contexto e acertou [84]. Portanto, o Jev **exige engenharia de pipeline prévia** (como criar *shortlists*) para não cometer erros crassos [85].
3. **Pode Errar com Alta Confiança ("Erros Confiáveis"):**
   * O fato de o Jev ter 0% de erro na estrutura do formato (nunca quebra o JSON) **não significa que ele seja infalível na decisão** [19, 86]. Ele pode escolher a categoria errada indicando 90% de confiança [86]. A definição dos limites de tolerância (*thresholds*) é de inteira responsabilidade do desenvolvedor [87-89].
4. **Desempenho Inferior em Jogos Estratégicos e Xadrez:**
   * Quando colocado para jogar xadrez contra o Fable ou GPT-6 Astra, o Fable superou o Jev em qualidade tática (ficando com +16 de vantagem de material e promovendo duas rainhas) [46, 71]. O Jev só venceu o Fable porque o Fable estourou o tempo limite de jogada (*flagging*), levando 6–15s por lance contra 2,6s do Jev [46]. O GPT-6 Astra venceu o Jev no xadrez [46].
5. **Limites de Janela de Contexto e Opções:**
   * Possui uma janela de contexto limitada a **32K ou 64K tokens de entrada** (muito inferior aos milhões de tokens de LLMs tradicionais), necessitando de técnicas como *map-reduce* para grandes volumes de dados [24, 90, 91].
   * A precisão de escolha cai se houver muitas opções na lista (recomendado menos de 20 opções), e a escala de pontuação (*score*) é limitada a no máximo 11 critérios (0 a 10) [92-94].
6. **Ausência de Suporte Multimodal Nativo:**
   * O Jev não processa imagens, áudio ou vídeo diretamente [24, 95]. Para buscar uma imagem de um hambúrguer, por exemplo, é necessário usar um modelo de visão barato antes para gerar a descrição em texto e só então passar a descrição ao Jev [95, 96].
7. **Riscos e Efeitos Colaterais em Ferramentas de Código:**
   * **Invalidação de Cache:** Interceptadores de gateway ou reescritas de histórico podem quebrar o cache de contexto do provedor de LLM, gerando custos mais altos em vez de economia [97, 98].
   * **Ocultação de Skills:** Um roteador de *skills* via Jev pode ocultar uma *skill* necessária caso o Jev atribua confiança < 50%, tornando essencial manter gatilhos manuais de sobreposição (ex: `!`) para o desenvolvedor não ficar "refém" das decisões do Jev [99].
8. **Concorrência de Modelos Open-Source Locais (Laya / Leia):**
   * Modelos abertos locais inspirados no Jev (como o *Laya/Leia*) conseguem rodar diretamente na GPU do usuário/laptop a 33 ms com custo zero [100, 101]. Sem *fine-tuning*, modelos locais abertos têm acurácia nativa inferior ao Jev (36,9% a 72% contra 87%+ do Jev) [102, 103], mas provam que a arquitetura pode ser replicada e ajustada localmente [94, 104].

---

💡 **Resumo da aplicação:** O Jev não substitui a LLM, mas atua como um filtro ultrarrápido e barato de Sistema 1 na frente dela [63, 73, 83, 105, 106]. Ele decide *se*, *como* e *qual* LLM deve ser acionada, reduzindo drasticamente o consumo de tokens e a latência de sistemas em produção [63, 106-110].
