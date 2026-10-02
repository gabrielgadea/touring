---
type: Research
plan_id: 2026-09-30-jev-harness-potencializacao
title: NotebookLM JEV — nlm-q1-harness-claude-code
sources_used: ["d79065e0-f8c6-4f73-acae-78ed59af61b9", "aaffea66-d39f-49ca-8d28-208f44f42afc", "f3a2f14d-5165-4725-8440-b0ad3cab2f9e", "359f586d-32eb-443a-ae4a-74d81b3f8cb0", "808a4edf-b4de-4204-8789-e5acdb35e8fa", "07964567-81ba-4eff-bb86-554b7c3eb3df", "9b733a91-a529-4cc8-912e-08339cbdf843", "3f9bf57d-e5c1-4831-976b-1b0530261287", "ee0a1a82-90e3-4696-8b0f-45896f358e9e", "19def919-5d27-4ca6-8056-a9a1041c377d", "5c195ad9-dd6b-4253-8dcf-b99b72f319e5", "e1cfa418-f58d-4a21-9e24-0fc2ad7424c5", "b3a7d75a-38cf-4973-bc56-2032916b32cc", "4d3bbd2c-3081-4c2b-88ff-3b673394e0b9", "f7e3f442-1816-48d6-ad5e-efbb4579f784", "92a50630-0334-4368-9a0c-3a71515e4fa6", "871dc0e3-ded7-4332-abd4-59cb14a0eac4"]
---

A integração do **Jev** (modelo *System 1* da TypeSafe AI) a harnesses e agentes de código como **Claude Code**, **Codex**, **Hermes** e **OpenCode** redefiniu a eficiência em workflows agênticos [1-3]. Em vez de utilizar LLMs pesados e lentos (*System 2*) para cada pequena decisão, avaliação de risco ou roteamento [4-6], o Jev atua como uma camada determinística e probabilística ultra-rápida e barata [7-9].

---

### 1. Arquitetura Fundamental: Modelo System 1 vs. System 2

* **System 2 (LLM de Fronteira)**: Modelos como Claude Opus 5.5, GPT-6 Astra ou Fable 5.1 são deliberados, lentos e focados em raciocínio, planejamento, geração de texto e código [4, 6, 7].
* **System 1 (Jev)**: Modelo treinado por **RLCD** (*Reinforcement Learning for Calibrated Decisions*) focado exclusivamente em tomar decisões estruturadas e devolver probabilidades e scores em uma única passada (*single pass*) [1, 10-12].
* **Preço e Latência**: O Jev custa **US\$ 0,042 por 1 milhão de tokens de entrada** e **US\$ 0 por tokens de saída** (saída 100% gratuita) [13-16]. A latência típica varia entre **70 ms e 500 ms** (média de 100 ms a 300 ms) [5, 17], sendo de **20x a 400x mais rápido** e **40x a 1000x mais barato** do que LLMs convencionais [4, 18, 19].
* **As 3 Primitivas Tipadas do Jev** [20-23]:
  1. `bool` (*null*): Avalia a probabilidade de uma afirmação ser verdadeira (ex: detecção de prompt injection, bugs ou comandos irreversíveis) [23-25].
  2. `choice`: Escolha múltipla entre até 255 opções predefinidas com cálculo de nível de confiança [26-29].
  3. `score`: Avaliação em uma escala/rubrica contínua (ex: 0 a 10) com pesos customizáveis [30-32].

---

### 2. Padrões Concretos de Integração em Agent Harnesses

#### A. Portões de Entrada/Saída e Guardrails de Prompt ("Crio" / "Portão de Entrada")
* **Componente / Mecanismo**: Sistema **Crio** (ou *Crivo*) implementado no Claude Code via hooks no `settings.json` [33, 34].
* **Padrão**: Intercepta cada prompt digitado pelo desenvolvedor *antes* que o Claude Code leia o contexto do projeto ou gaste tokens de raciocínio [35, 36].
* **Critérios Avaliados em Paralelo pelo Jev** [36, 37]:
  1. *Tamanho do pedido* (ajuste pequeno, feature ou refatoração inteira).
  2. *Clareza* (duas pessoas entenderiam a mesma coisa?).
  3. *Localização* (diz em qual tela/arquivo mexer?).
  4. *Definition of Done* (diz como saber se ficou pronto?).
  5. *Mistura de tarefas* (mistura vários objetivos no mesmo prompt?).
* **Regra de Decisão**: Se a probabilidade de um critério ficar abaixo de **50%**, o prompt é barrado instantaneamente [37]. O desenvolvedor recebe o feedback do Jev em milissegundos por uma fração de centavo de dólar [38, 39], evitando que o Claude Code passe 10–30 segundos analisando um prompt ambíguo [38, 40].

#### B. Guardrails de Execução de Ferramentas e Hooks Pre-Tool (`JevGuard`)
* **Componente / Mecanismo**: Pre-hook `JevGuard` em chamadas de ferramentas de terminal/escrita de arquivos [25, 41, 42].
* **Padrão**: Toda vez que o agente tenta executar comandos no terminal (ex: bash) ou editar arquivos críticos (ex: `.env`), o hook envia a alteração ao Jev [25, 43, 44].
* **Exemplo de Regra (`claude.md`)**: O Jev responde a perguntas sobre reversibilidade e intenção destrutiva [25]. Comandos como `git push --force origin main`, `rm -rf node_modules` ou modificações em arquivos de segredos são identificados como **99% irreversíveis/destrutivos** [25, 41, 43]. O Jev responde ao agente: `command was blocked by JevGuard` [41]. Se o Jev tiver **>=80% de confiança** de que uma regra do projeto foi violada, a edição é bloqueada no disco antes de acontecer [44].

#### C. Compactação Inteligente e Ultra-Rápida (`fast-jev-compaction`)
* **Componente / Mecanismo**: Plugin/Hook `fast-jev-compaction` configurado no `settings.json` com `typeSafeApiKey` [45, 46].
* **Padrão**: Substitui a compactação nativa do Claude Code (que envia todo o histórico de conversação de volta à LLM para sumarizar, gastando milhares de tokens e levando muito tempo) [47, 48].
* **Funcionamento**: O Jev analisa o histórico da conversa e as respostas recentes do modelo para decidir de forma probabilística quais turnos, arquivos lidos e decisões são realmente importantes para manter no contexto [47].
* **Métrica**: Compactação concluída em **menos de 1 segundo** [49, 50]. Só é acionada via Jev quando a sessão ultrapassa 25% de uso de tokens para otimizar chamadas de API [49].

#### D. Roteador Automático de Modelos (Model & Agent Router)
* **Componente / Mecanismo**: Hook de roteamento acionado antes de cada mensagem no Claude Code [51, 52].
* **Padrão**: A sessão principal inicia em um modelo ultraleve (ex: Haiku) [53]. O prompt é enviado ao Jev em milissegundos para classificar a complexidade e o risco [52, 54]:
  * *Tarefas simples/troca de texto*: Roteado para o modelo rápido (**Haiku**) [55].
  * *Tarefas de rotina/padrão*: Roteado para o modelo intermediário (**Sonnet**) [55].
  * *Tarefas profundas/refatorações complexas*: Roteado para o modelo forte (**Opus/Astra**) [56].
* **Gatilho de Escalada**: Se o risco calculado pelo Jev for **>70%** ou a confiança for **<60%**, o sistema escala automaticamente para o nível superior [57]. O desenvolvedor pode ignorar o roteamento iniciando o prompt com `!` [57].

#### E. Seletor de Skills ("Skill Picker" / "Skill Router")
* **Componente / Mecanismo**: Hook/Skill `Skill Picker` e gerenciador de biblioteca de skills [58, 59].
* **Padrão**: Agentes de código normais carregam o nome e a descrição de *todas* as skills instaladas na janela de contexto a cada turno, queimando de 5.000 a 10.000 tokens de graça [60-62].
* **Funcionamento**: As skills são movidas da pasta nativa do Claude Code para uma pasta de biblioteca (`.claude/skills_library`) [59]. A cada prompt, o Jev recebe a mensagem e a lista de descrições das skills (com a opção *"nenhuma das opções"*) [58, 63].
* **Métrica**: Apenas se a escolha passar de **50% de probabilidade** e **60% de confiança**, a instrução da skill específica é injetada no contexto [63]. Em testes com o catálogo Hermes (182 skills), o agente sozinho carregou a skill errada em **17% das vezes**; com a sugestão do Jev, os erros caíram para **7,3%** [64, 65].

#### F. Leituras Baratas e Busca de Arquivos em Escala (`AskJev` e `Explore`)
* **Componentes / Ferramentas**: `ask_jev_filebool`, `ask_jev_filechoice`, `ask_jev_files` [66-68].
* **Padrão**: Em vez de fazer a LLM ler arquivos inteiros (100K–500K tokens) para responder perguntas simples [66, 67], o agente chama as ferramentas do Jev [69, 70]:
  * *Substituição do agente Explore*: Executa uma busca por palavra-chave e faz o Jev ranquear 20 a 35 arquivos simultaneamente por relevância em **1,6 segundo** [70, 71]. O agente só abre os arquivos do topo da lista [70].
  * *Globbing em massa*: O Jev analisa múltiplos arquivos em paralelo em menos de 0,5s (ex: buscar TODOs, bugs ou hacks conhecidos em todos os arquivos TypeScript do repositório) [68, 72, 73].

#### G. Linters Qualitativos, Code Smells e Revisão de Código Pre-Check
* **Padrão**: O Jev é usado para criar linters qualitativos que detectam *code smells* (código duplicado, dead code, números mágicos) ou comentários inúteis [74, 75].
* **Pre-Check de Code Review**: Antes de rodar uma revisão completa por LLM (lenta e cara), o Jev avalia o diff contra 7 perguntas sim/não [71]. Se todas forem não, faz uma revisão rápida de 1 rodada; se sim ou incerto, dispara a revisão completa por LLM [76].
* **Métrica**: A Sentry implementou o Jev em pipelines de segurança e obteve desempenho **>5x mais barato e rápido** que o GPT-OSS 120B com alta precisão [77]. Um escaneamento completo de codebase de 28 milhões de tokens via Jev custou apenas **US\$ 1,19** [78].

#### H. Gateway / Proxy de Injeção de Ferramentas (`jev-gateway` / `f-codex` / `f-claude`)
* **Componente**: Proxy/Gateway de código aberto (`jev-gateway` com comandos `f-codex` e `f-claude`) [2, 79, 80].
* **Padrão**: O gateway intercepta todas as requisições do agente de código [80, 81]. Quando identifica que a chamada é uma etapa de *reasoning to call* (decidir qual ferramenta/MCP chamar), o gateway redireciona a chamada para o Jev [82].
* **Preservação de Cache (KV Cache)**: O Jev retorna a decisão e ajusta a requisição adicionando o contexto da ferramenta *sem alterar o histórico da conversa* [81, 83]. Isso garante que o **Prompt Cache** da LLM principal não seja invalidado [83, 84].

---

### 3. Arquitetura de Sistemas Operacionais Agênticos (Claude + Jev OS)

Sistemas como o **Chase AI OS** ou **Jack Roberts Agentic OS** utilizam uma arquitetura de 3 camadas integrada ao Jev [85-87]:
1. **Camada Visual / Dashboard**: Interface no navegador ou plugin no Obsidian para disparar ações e visualizar relatórios [86].
2. **Camada de Memória (Obsidian / Carpathy Vault)**: Estrutura de pastas `raw`, `wiki` e `outputs` para persistência e RAG estruturado [87, 88].
3. **Backbone de Skills + Voz Local + Roteamento de Modelos em 3 Tiers** [89-91]:
   * *Entrada de Voz*: Áudio do usuário -> Transcrição local (**Whisper**) -> Áudio de resposta local (**Kokoro / Fish Audio TTS**) [90, 91].
   * *Classificação via Jev*:
     * **Tier 1 (Comandos Simples / UI)**: Busca de relatórios ou abertura de abas. Executado em **<100 ms sem chamar LLM** [91].
     * **Tier 2 (Q&A Não-Complexo)**: Respostas rápidas usando modelos menores (**Haiku / Luna / DeepSeek**) [92].
     * **Tier 3 (Trabalho Agêntico Complexo)**: Abre o terminal com **Claude Code** ou **Codex** rodando modelos de ponta (**Opus 5.5 / Astra**) [92, 93].

---

### 4. Números Medidos: Latência, Custos e Economia (%)

#### A. Tabela de Benchmarks Medidos no `jev-gateway` (5 Execuções Reais de Tarefas)
Os testes comparativos com e sem o `jev-gateway` registraram os seguintes resultados médios [94-97]:

| Modelo LLM | Tipo de Tarefa | Economia de Output Tokens (%) | Economia de Input Tokens (%) | Redução de Requisições LLM (%) | Ganho de Tempo / Latência (%) | Fonte |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| **GPT-6 Astra** | Correção de Bugs | **57%** | **7%** | — | **39% mais rápido** | [94] |
| **GPT-5.6 Sol** | Correção de Bugs | **57%** | **40%** | **36% menos req.** | **36% mais rápido** | [95] |
| **Claude Sonnet 5** | Correção de Bugs | **41%** | **48%** | **26% menos req.** | **25% mais rápido** | [96] |
| **Fable 5.1** | Criar Novas Features | **24%** | **27%** | **19% menos req.** | **26% mais rápido** | [96, 97] |
| **GPT-5.6 Luna** | Criar Novas Features | **14%** | **51%** | **42% menos req.** | **14% mais rápido** | [97] |

#### B. Comparações Diretas de Custo e Latência em Outras Aplicações
* **Triagem de 10.000 E-mails (AI Edge)**: O processamento via Jev custou **US\$ 0,14**, comparado a **US\$ 2,20** utilizando Claude Haiku (e dezenas de vezes mais com Opus/Fable) [98].
* **Scraping e Escaneamento SEO de 584 Páginas (AI Edge)**: Custo de **US\$ 2,10** usando Jev + Opus vs. **US\$ 4,30** usando Opus puro [99].
* **Análise e Classificação de 300 Comentários no Hermes Agent (The AI Doctor)**: Realizou 2.400 decisões em **17 segundos** com **0 erros** a um custo total de **US\$ 0,015** (comparado a US\$ 5,90–US\$ 7,40 no Fable 5.1, sendo **535x mais rápido**) [100, 101].
* **Busca e Reserva de Voos no Browser Use**: Navegação completa no Google Flights executada em **7 segundos** a um custo de apenas **US\$ 0,0039** por sessão [102, 103].
* **Custo em Escala Global**: 1 milhão de execuções no Jev custam aproximadamente **US\$ 20**, enquanto a mesma carga no Fable 5.1 custaria **US\$ 11.000** (economia de 550x) [104].

---

### 5. Resumo dos Componentes e Ferramentas Mencionadas

* **Integradores e Proxy**: `langchain-typesafe` (`TypeSafeClassifier`), `jev-gateway` (`f-codex`, `f-claude`), `fast-jev-compaction` [2, 45, 51].
* **Mecanismos de Defesa e Filtro**: `JevGuard` (pre-hook de bash/escrita), `Crio`/`Crivo` (Portões de Entrada e Saída), `Skill Picker` / `Seletor de Skills` [25, 33, 41, 58, 63].
* **Ferramentas de Leitura de Arquivo**: `ask_jev_filebool`, `ask_jev_filechoice`, `ask_jev_files` e `ask_jev` meta-tool [66-68, 105].

💡 *Quer que eu elabore um script pronto em Python/Node.js ou um exemplo de configuração de hooks (`settings.json`) para integrar o Jev ao seu Claude Code?*
