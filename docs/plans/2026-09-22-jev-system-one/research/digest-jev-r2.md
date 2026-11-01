---
type: ResearchDigest
title: Jev rodada 2 — contrato, LGPD, distribuição, mudanças e confiabilidade
description: Termos legais (subprocessadores nos EUA, DPA sem LGPD, MCA de 19/09), ZDR via Vercel e OpenRouter, página migrating-to-v1, SDKs, status e incidentes, com fontes.
plan_id: 2026-09-22-jev-system-one
tags: [research, jev, legal, lgpd, distribution]
timestamp: 2026-09-22T15:05:00-03:00
resource: https://typesafe.ai/legal/mca
okf_version: "0.1"
---

Parte do [bundle](/index.md); consumido pela [estratégia](/strategy-2026-09-22-jev-system-one.md). Não é parecer jurídico.

# Jev (TypeSafe AI): rodada 2, só as lacunas

> Pesquisa read-only feita em 22/09/2026. Trechos entre aspas são citação literal (em inglês) da fonte indicada.
> Brutos em `scratchpad/jev/r2/raw/` (páginas, snapshots do Wayback, JSON de APIs) e textos extraídos em `scratchpad/jev/r2/text/`.
> Convenção: **[primária]** = fonte da TypeSafe, do distribuidor ou código/dado lido direto. **[secundária]** = terceiro que não consegui conferir na origem. **[NÃO ENCONTRADO]** = lacuna. **[INFERÊNCIA]** = conta ou leitura minha, não afirmação de fonte.

---

## 0. Método

- **Diff temporal pelo Wayback Machine.** Usei a API CDX para listar snapshots desde 15/09 e baixei as versões brutas (`id_`) de `llms.txt` (15/09, 17/09, 21/09, 22/09), `models` (17/09, 18/09, 21/09, 22/09), `api` (15/09 e 21/09), `legal/mca` (16/09 e 21/09), `legal/terms` (15/09 e 21/09), `legal/data-processing` (16/09, 20/09 às 09h e às 22h) e `legal/privacy-policy` (15/09 e 19/09). Comparei frase a frase.
- **Páginas em JS.** O trust center da Vanta (`trust.typesafe.ai`) e uma página de incidente da Better Stack só renderizaram em navegador headless (Playwright). As respostas do FAQ da home (Framer) saíram do chunk JS `framerusercontent.com/.../1bDVrPYMWEZ6eWmJvyMH7WadCbl21tfR2JXCIVJyZRA.BkrK7V15.mjs`.
- **Tweets.** Li via `api.fxtwitter.com`, porque o X bloqueia leitura direta.
- **Limitação.** `status.typesafe.ai` teve timeouts de TLS intermitentes. A API do GitHub bateu rate limit (403) para issues e commits dos SDKs.
- **Controle.** O `llms-full.txt` de hoje é **byte-idêntico** (903.311 B) ao lido na rodada 1, e o `llms.txt` também. Nada na doc mudou entre a rodada 1 e esta.

---

## 1. Mudanças desde 15/09

### 1.1 `llms.txt`: páginas novas (diff por snapshot) **[primária]**

| Snapshot (UTC) | Links | Entraram | Saíram |
|---|---|---|---|
| 15/09 20:18 ([wb](https://web.archive.org/web/20260915201810/https://docs.typesafe.ai/llms.txt)) | 110 | — | — |
| 17/09 18:42 | 111 | `legal`, `model-jaggedness/jev-1.13`, `models` | `sdk/javascript/api/type-aliases/ScoreList`, `ScoreMap` |
| 21/09 09:14 | 109 | `sdk/python/api/clients/async`, `clients/sync` | `clients/async/client`, `async/models`, `sync/client`, `sync/models` |
| 22/09 03:18 | 110 | `introduction/coding-agents` | — |
| hoje ([llms.txt](https://docs.typesafe.ai/llms.txt)) | 111 | `cookbooks` (índice) | — |

- **Resultado frente à lista conhecida:** nenhuma página fora da lista que você deu. Entre 15/09 e hoje entraram `models`, `legal`, `model-jaggedness/jev-1.13`, `introduction/coding-agents` e o índice `cookbooks`. Todas já estão na lista.
- **Página "oculta":** `https://docs.typesafe.ai/migrating-to-v1` existe (HTTP 200, `.md` com 14.619 B), mas **não está** no `llms.txt` nem no `sitemap.xml` da doc, que tem 111 URLs e bate 1:1 com o llms.txt. Ela é referenciada pelo SKILL.md do repositório de skills: `| Update an older integration | [Migration guide](https://docs.typesafe.ai/migrating-to-v1.md) ... |` ([SKILL.md](https://raw.githubusercontent.com/typesafe-ai/skills/main/skills/typesafe-ai/SKILL.md)).
- **Changelog da doc ou da API:** `https://docs.typesafe.ai/changelog` e `/changelog.md` respondem **404**. Só existem changelogs dos SDKs (§1.3). **[NÃO ENCONTRADO: changelog de API/doc]**

### 1.2 O que é a "v1" da API ([migrating-to-v1.md](https://docs.typesafe.ai/migrating-to-v1.md)) **[primária]**

A página diz: "The preview endpoint (`/preview/evaluation`) is replaced by the stable **v1** endpoint (`/v1/systemone`). This is a **breaking change**: the endpoint, the request shape, and the response shape all changed." Auth continua `Authorization: Bearer <API_KEY>`.

| Área | Preview | v1 |
|---|---|---|
| Endpoint | `POST /preview/evaluation` | `POST /v1/systemone` |
| Perguntas | array `prompts` (cada item tem `key`) | mapa `questions` (a chave é o id) |
| Descritores | `criteria` / `options` / `levels`, conforme o tipo | `criteria` unificado |
| Respostas | array `responses` (mesma ordem) | mapa `answers` |
| Valor da noul | `probability` | `noul` |
| Valor da choice | `chosen` | `choice` |
| Valor do score | `expectation` | `score` |
| `probabilities` da choice | array `{option, probability}` | mapa `option → probability` |
| `probabilities` do score | não retornava | mapa por nível (novo) |
| Confidence | cálculo antigo | "new computation" |
| Usage | placeholder `usage.billing_units` | `usage.input_tokens` / `output_tokens` |
| Campo de entrada | `document` | `state` |
| SDK Python | `typesafe-client` | `typesafe-sdk` (pacote novo) |

Pontos que importam para quem integra:

- **Score:** "You can no longer skip levels". Os níveis são posições 0..n do array.
- **Noul:** ganhou `criteria?: {true?, false?}` ("New feature in v1!").
- **Confidence:** "The computation behind `confidence` changed ... Any logic your integration uses based on confidence should be carefully re-evaluated." A página traz a fórmula para reproduzir a confidence antiga: 1 − entropia de Shannon normalizada.
- **`document` → `state`:** "Preview, and the first v1 releases, called it `document`. v1 now accepts only `state`; a request with `document` fails validation."
- **SDK antigo:** "The previous `typesafe-client` package (every release, `0.1.x` and `1.0.x`) sends `document` and no longer works against the API." No SDK novo, `system_one(state, questions)` tem `model` opcional com default `jev-latest`.
- **Risco de supply chain:** no PyPI público, `typesafe-client` hoje é um **placeholder de terceiro** (0.0.0, publicado em 21/09/2026). A descrição diz: "This is not the official TypeSafe AI Python SDK. TypeSafe AI's real SDK is currently distributed only via their private, unauthenticated package index (https://pypi.typesafe.ai/) ... classic dependency-confusion setup ... This package was registered defensively" ([pypi.org/project/typesafe-client](https://pypi.org/project/typesafe-client/)). A parte "only via private index" está desatualizada: `typesafe-sdk` 0.7.1 está no PyPI público ([pypi.org/pypi/typesafe-sdk/json](https://pypi.org/pypi/typesafe-sdk/json)). E `https://pypi.typesafe.ai/` hoje responde 404.

### 1.3 Versões dos SDKs **[primária]**

**Python `typesafe-sdk`** ([changelog](https://docs.typesafe.ai/sdk/python/changelog.md); datas de upload no [PyPI](https://pypi.org/pypi/typesafe-sdk/json)):

| Versão | Data (changelog / PyPI UTC) | Conteúdo |
|---|---|---|
| 0.0.1a0 | — / 09/09 10:34 | pré-release |
| 0.5.7 | 14/09 / 11/09 23:05 | "initial public release" |
| 0.6.0 | 15/09 / 15/09 10:23 | **Breaking:** `Score.criteria` passa a ser sequência ordenada, não mais dict por inteiro. Também: tipos `Mapping`/`Sequence`, erros com detalhe HTTP, validação de `RetryPolicy`, objetos picklable |
| 0.7.0 | 18/09 / 18/09 09:12 | **Breaking:** "ser/de library has been changed from `msgspec` to `pydantic`". Corrige subclasses de `str` serializadas como listas de caracteres. Novo argumento `response_model` para modelo pydantic |
| 0.7.1 | 21/09 / 21/09 15:57 | "validate the API key early and exclude the value from logged exceptions". Adiciona exemplos com AI gateways |

**JavaScript `@typesafe-ai/sdk`**: detalhes em §6.

### 1.4 Modelos: nenhuma versão nova **[primária]**

- [models.md](https://docs.typesafe.ai/models.md): só `jev-1.13.0`. `jev-latest` e `jev-preview` apontam ambos para `jev-1.13.0`. A página diz: "`jev-preview` currently points to the same model as `jev-latest`. There is no preview build available right now." Os snapshots de 17, 18, 21 e 22/09 dizem o mesmo.
- O OpenRouter publica o endpoint como `typesafe/jev-1.13-20260917`, um slug datado, com `created` = 18/09/2026 00:01 UTC ([endpoints](https://openrouter.ai/api/v1/models/typesafe/jev-1.13/endpoints)). A resposta de exemplo do OpenRouter diz: "The `model` field in the response names the dated snapshot that served your request" ([tutorial](https://openrouter.ai/docs/guides/community/jev-tutorial.md)). Não achei fonte da TypeSafe que diga se "20260917" é uma build diferente da de 15/09. **[NÃO ENCONTRADO]**
- **jev-1.14 ou preview:** **[NÃO ENCONTRADO]**. Nenhuma menção na doc, no blog, nos distribuidores ou em busca.

### 1.5 Preço, rate limit e contexto **[primária]**

A mesma tabela aparece nos snapshots do Wayback de 17/09 17:41, 18/09, 21/09 e 22/09 16:18 ([wb 17/09](https://web.archive.org/web/20260917174134/https://docs.typesafe.ai/models)):

- "Price (per Btok / per Mtok) $42 / $0.042". Só input é cobrado: "Output tokens are free". **Não mudou.**
- "Rate limits 250,000 tokens per second / 1,200 requests per minute". **Não mudou.** Continua o aviso: "**Rate limits are adjusting dynamically.** ... the limits above can change without notice ... as upcoming large GPU deals land".
- "Context length 64k tokens per request; 32k tokens for `state` plus the longest question". Essa linha **não existe** no snapshot de 17/09 e **aparece** a partir do de 18/09 11:48. O limite pode já existir antes; o que mudou é que foi **documentado** em 18/09.
- A home diz que o preço não é subsidiado. No FAQ, à pergunta "Are these prices temporary or subsidized?", a resposta é "We can serve Jev profitably at our current prices. Our goal is to make intelligence more affordable over time" (chunk Framer da [home](https://typesafe.ai/)).

**API reference, 15/09 → 21/09** ([wb 15/09](https://web.archive.org/web/20260915201823/https://docs.typesafe.ai/api) vs [wb 21/09](https://web.archive.org/web/20260921091435/https://docs.typesafe.ai/api)):

- `instructions` e `criteria` passam a aceitar `string | object | array` ("structured instructions").
- Choice: "You can have a maximum of 255 options per Choice."
- Score: "A Score should have at least two levels; the API accepts up to 10."
- Os exemplos passam de `jev-latest` para `jev-1.13.0`, com saídas reais.
- A [API reference](https://docs.typesafe.ai/api.md) de hoje documenta também o `529 Overloaded` ("TypeSafe is temporarily overloaded. Retry after a short delay").

### 1.6 Mudança de acesso: fim da waitlist **[primária]**

- Tweet de 20/09/2026 21:30 UTC: "Jev is now available to everyone. No waitlist. Start using it here: https://console.typesafe.ai" (@typesafeai, [x.com/typesafeai/status/2101786156572823624](https://x.com/typesafeai/status/2101786156572823624), lido via fxtwitter).
- A navegação do site conta a mesma história nos snapshots: "Join Waitlist" em 16/09 → "Sign in / Join" em 20/09 22:03 → "Docs · API console · Contact sales" em 21/09 ([wb DPA 20/09](https://web.archive.org/web/20260920220310/https://typesafe.ai/legal/data-processing), [wb MCA 21/09](https://web.archive.org/web/20260921155004/https://typesafe.ai/legal/mca)).
- Números que só vi em terceiros: "$5 in credits" para contas novas ([Crypto Briefing](https://cryptobriefing.com/typesafe-jev-ai-public-access/)) e "roughly 140,000 signups in the first 36 hours" ([explainx](https://www.explainx.ai/blog/jev-general-availability-no-waitlist-2026)). **[secundária; primária NÃO ENCONTRADA]**
- A home ainda traz, no FAQ, "Join the waitlist!" como resposta a "How do I get started". Texto velho no chunk Framer.

### 1.7 Cronologia da distribuição **[primária]**

| Data (UTC) | Evento | Fonte |
|---|---|---|
| 16/09 | Jev no Vercel AI Gateway | [changelog Vercel](https://vercel.com/changelog/typesafe-ai-jev-now-available-on-ai-gateway) (`datetime` 2026-09-16) |
| 17/09 20:50 | Jev no Cloudflare AI Gateway / Workers AI | [@CloudflareDev](https://x.com/CloudflareDev/status/2100688880798159254) · [doc do modelo](https://developers.cloudflare.com/ai/models/typesafe/jev/) |
| 18/09 00:32 | Jev no OpenRouter, "in beta" | [@OpenRouter](https://x.com/OpenRouter/status/2100744709589316009) |
| 21/09 | Vercel lança API compatível com a TypeSafe (base URL `/typesafe`) e a HTTP API `/v1/evaluate` | [changelog Vercel](https://vercel.com/changelog/ai-gateway-now-supports-typesafe-clients-and-http-api-for-jev) |
| sem data | LiteLLM pass-through `/typesafe` e "JEV Auto Router" | [docs.litellm.ai](https://docs.litellm.ai/docs/pass_through/typesafe) |

---

## 2. Termos legais para uso no Brasil (LGPD)

### 2.1 Os documentos e suas datas **[primária]**

| Documento | "Last updated" | Mudou desde 15/09? |
|---|---|---|
| [DPA](https://typesafe.ai/legal/data-processing) | Apr 24, 2026 | Só a navegação do site (diff 16/09 → 20/09) |
| [Privacy Policy](https://typesafe.ai/legal/privacy-policy) | Nov 19, 2025 | Só a navegação (diff 15/09 → 19/09) |
| [MCA](https://typesafe.ai/legal/mca) | **Sep 19, 2026** (a anterior era **Aug 27, 2026**) | **Sim, substantivamente** (§2.4) |
| [Terms of Use](https://typesafe.ai/legal/terms) (do site) | **Sep 19, 2026** (a anterior era Sep 14, 2026) | Sim, pouco (§2.5) |

### 2.2 Retenção, treino, ZDR e região

- **Retenção padrão em dias: [NÃO ENCONTRADO].**
  - O DPA diz apenas: "Customer Personal Data will be retained for as long as necessary taking into account the purpose of the Processing, and in compliance with applicable laws" (Schedule I, item 8).
  - A Privacy Policy diz: "for as long as reasonably necessary to provide you with the Services, or otherwise in support of our business or commercial purposes".
  - A MCA diz, em 10.3: "TypeSafe will be under no obligation to store or retain Customer Data and may delete Customer Data at any time in its sole discretion". Dados confidenciais "may be retained in TypeSafe's standard backups".
  - O trust center lista o controle "Data retention procedures established" como passing, sem prazo ([trust.typesafe.ai](https://trust.typesafe.ai/)).
- **Treino.**
  - Privacy: "We will not train or fine tune any artificial intelligence or machine learning models on your prompts or other Input."
  - MCA 4.1: "TypeSafe will not, include Customer Data in a dataset used to train (i.e., to modify the model weights of) any artificial intelligence or machine learning models without Customer's prior consent". A versão de 27/08 tinha o mesmo texto.
  - [models.md](https://docs.typesafe.ai/models.md): "Jev is not fine-tuned or LoRA-adapted with customer data".
- **Mas há Telemetry.** A MCA 4.3 define "technical logs, hashes, summary statistics and classifications, metrics, and learnings related to Customer's use of the Services" e diz que a TypeSafe "may Process Telemetry without restriction, including to improve the Services". Desde 19/09, a licença para derivar Telemetry vale "in perpetuity" (§2.4).
- **ZDR direto.** Só para enterprise: "We also offer zero data retention (ZDR) for enterprise customers. Contact privacy@typesafe.ai" ([legal.md](https://docs.typesafe.ai/legal.md)).
- **Região de processamento e armazenamento: EUA.**
  - Privacy: "The Services are hosted in the United States ... you are transferring your personal data outside of those regions to the U.S. for storage and processing."
  - Os subprocessadores são todos "USA" (§2.3).
  - Não há região fora dos EUA nem opção de região. **[NÃO ENCONTRADO: qualquer região não-EUA]**
- **Transferência internacional.** O DPA §6 cobre só transferências da **UE** (EU SCCs 2021/914, Módulos 2 e 3, lei e foro da Irlanda), do **Reino Unido** (UK Addendum) e da **Suíça**. **Não há menção a LGPD, ANPD ou Brasil.** **[NÃO ENCONTRADO]**
  - [INFERÊNCIA] Para um controlador brasileiro, a transferência aos EUA pede um mecanismo do art. 33 da LGPD. A Resolução CD/ANPD nº 19/2024 aprovou cláusulas-padrão contratuais obrigatórias, com prazo de adaptação encerrado em 23/08/2025 ([ANPD](https://www.gov.br/anpd/pt-br/acesso-a-informacao/institucional/atos-normativos/regulamentacoes_anpd/resolucao-cd-anpd-no-19-de-23-de-agosto-de-2024); [Mayer Brown](https://www.mayerbrown.com/pt/insights/publications/2025/08/end-of-grace-period-implementation-of-brazils-standard-contractual-clauses-in-international-transfers-of-personal-data)). O DPA público não as traz, então seria preciso negociar um aditivo. Isto não é parecer jurídico.
- **Incidentes e auditoria (DPA).** Notificação "without undue delay and in any case within 72 hours". Auditoria "no more than once every 12 months", a custo do cliente. Assistência a DPIA "may charge ... a reasonable fee".
- **Papéis.** Cliente = controller/business. TypeSafe = processor/service provider (DPA 1.1).

### 2.3 Subprocessadores ([trust.typesafe.ai/subprocessors](https://trust.typesafe.ai/subprocessors), renderizado via Playwright) **[primária]**

O DPA 3.1 remete a essa página. Novos subprocessadores têm "reasonable advance notice" e o cliente pode objetar em 15 dias (DPA 3.2).

| Subprocessador | Tipo | País | Papel declarado |
|---|---|---|---|
| Amazon Web Services | Cloud provider | USA | "Customer information for live requests is stored and processed on databases, caches and compute nodes within AWS" |
| Modal | AI Infrastructure | USA | "Customer AI prompts are processed, but not stored, on compute nodes managed by Modal" |
| Nebius | Cloud provider | USA | idem ("processed, but not stored") |
| CoreWeave | Cloud provider | USA | idem |
| Slack | Collaboration | USA | suporte em canais compartilhados |
| Google Workspace | Collaboration | USA | e-mail e documentos |

O trust center anuncia **SOC 2**, com o relatório "SOC 2 Type II - 2026" disponível sob pedido de acesso ([trust.typesafe.ai](https://trust.typesafe.ai/)).

### 2.4 MCA: o que mudou em 19/09 (diff contra a versão de 27/08) **[primária]**

Fontes: [wb 16/09](https://web.archive.org/web/20260916215503/https://typesafe.ai/legal/mca) e [wb 21/09](https://web.archive.org/web/20260921155004/https://typesafe.ai/legal/mca).

1. **Caiu a proibição de publicar benchmarks.** A versão de 27/08 tinha, em 2.3(f): "publish benchmarks or performance information about the Services". A de 19/09 **removeu** essa alínea.
2. **Lei e foro: Delaware → Califórnia.** Agora vale a lei da Califórnia, com foro em San Francisco. Entrou uma seção 15 nova, "Dispute Resolution and Arbitration": arbitragem individual obrigatória na JAMS, renúncia a júri e a class action, audiência "in The City and County of San Francisco". Há exceções para small claims, liminares e PI.
3. **Licença sobre Customer Data foi ampliada e passou a sobreviver ao contrato.** Antes: "(b) Customer Data to derive and generate Telemetry and as necessary to comply with applicable Laws". Agora: "(a) during the Term ... (b) during the Term, any Customer Data to provide the Services and calculate Fees, and **(c) in perpetuity**, any Customer Data (i) to derive and generate Telemetry, (ii) to monitor for fraud and abuse of the Services, and (iii) as necessary to comply with applicable Laws". A seção 4.1 entrou na lista de "Survival".
4. **Aceite individual.** O aceite passou a valer para pessoa física ("ON BEHALF OF YOURSELF AS AN INDIVIDUAL, UNLESS ..."). Entrou também uma cláusula de "Separate Agreement".
5. **Limitações de responsabilidade.** Ganharam "TO THE FULLEST EXTENT PERMITTED BY LAWS". O teto segue sendo o maior entre os valores pagos em 12 meses e **US$ 50**.
6. **Continua igual:**
   - API pode ficar incompatível após updates, com "commercially reasonable efforts to provide advance notice" (2.5).
   - Não há SLA: a garantia é só "perform materially as described in its Documentation" (9.1), e o serviço é "AS IS".
   - Créditos expiram em 12 meses.
   - Taxas em USD, sem impostos.
   - Suspensão imediata em várias hipóteses (6).
   - Indenização pelo cliente sobre o Input (13.2).
   - Alterações do contrato entram em vigor 60 dias após o aviso (16.7).

### 2.5 Terms of Use do site (15/09 → 21/09) **[primária]**

- Removida a restrição "(vi) use the Site to develop new products and services without TypeSafe's express written permission".
- Entrou a precedência de acordo separado.
- A lei continua de **Delaware**. O teto é **US$ 100**.
- "The Site is intended for visitors located within the United States."

---

## 3. Distribuição: OpenRouter, Pydantic AI, Vercel, Cloudflare, LiteLLM

### 3.1 OpenRouter **[primária]**

- **Duas superfícies de API** ([hub Jev](https://openrouter.ai/docs/guides/community/jev.md)):
  - **Decisions API** `POST https://openrouter.ai/api/alpha/decisions`, com SDKs OpenRouter para TS, Python e Go.
  - **System One API** `POST https://openrouter.ai/api/v1/systemone`, compatível com os SDKs da TypeSafe trocando só a base URL para `https://openrouter.ai/api` ([guia](https://openrouter.ai/docs/guides/community/typesafe-sdk.md)).
- **Formato** ([OpenAPI da Decisions](https://openrouter.ai/docs/api/api-reference/alphadecisions/submit-a-decisions-request.md)):
  - Requisição: obrigatórios `model`, `state` e `questions` (noul, choice e score com o `criteria` unificado da v1). Opcionais do OpenRouter: `provider` (ProviderPreferences, que inclui `zdr` e `data_collection`), `session_id` (≤256; "never sent to the provider"), `user` e `trace`.
  - Resposta: `answers`, `model`, `usage` (`input_tokens`, `output_tokens`, `cost`) e ainda `id` e `provider`.
  - Erros próprios: 402 por créditos insuficientes e 429.
- **IDs do modelo.** `typesafe/jev-1.13` ou o alias `~typesafe/jev-latest`. A System One API mapeia `jev-1.13` → `typesafe/jev-1.13`. Ressalva: `client.models.list()` do SDK TypeSafe **quebra** no OpenRouter, que devolve outro formato.
- **Preço.** `prompt` US$ 0,000000042 por token (= US$ 0,042/Mtok) e `completion` 0 ([endpoint](https://openrouter.ai/api/v1/models/typesafe/jev-1.13/endpoints)). No exemplo, 476 tokens de input custam `0.000019992`, ou seja, 476 × 0,042e-6: o output não é cobrado.
- **Contexto: 32.000 tokens** ("That's the `state` you send plus the questions"). O direto documenta **64k** por request, com 32k para state + maior pergunta.
- **ZDR.** O endpoint `TypeSafe | typesafe/jev-1.13-20260917` **aparece** na lista pública de endpoints ZDR ([/api/v1/endpoints/zdr](https://openrouter.ai/api/v1/endpoints/zdr)). O OpenRouter define ZDR como "a provider will not store your data for any period of time". Para forçar, use `provider.zdr: true` por request ou a configuração da conta ([doc ZDR](https://openrouter.ai/docs/guides/features/zdr.md)).
- **Região.** O registro do provedor tem `"headquarters": null, "datacenters": null` ([/api/v1/providers](https://openrouter.ai/api/v1/providers)). O roteamento in-region do OpenRouter existe só para **EU e US** e só para **enterprise**: "Use `https://eu.openrouter.ai` ... or `https://us.openrouter.ai` ... only enabled for enterprise customers by request" ([provider-logging](https://openrouter.ai/docs/guides/privacy/provider-logging.md)). **Não há Brasil.**
- **Status.** "in beta" segundo o tweet de 18/09. O caminho `alpha` está no nome do endpoint.

### 3.2 Pydantic AI ([pydantic.dev/docs/ai/models/typesafe](https://pydantic.dev/docs/ai/models/typesafe/)) **[primária]**

- **Instalação e uso.** `pip install "pydantic-ai-slim[typesafe]"`, depois `Agent('typesafe:jev-latest', output_type=...)` ou `TypeSafeModel('jev-latest')`.
- **Mapeamento de tipos.** Cada campo do `output_type` vira uma pergunta:
  - `bool` vira noul, com `typesafe_boolean_threshold` padrão 0,5.
  - `Literal`/`Enum` vira choice.
  - `IntEnum` documentado vira score, com 2 a 10 níveis.
  - `str` e números sem limite dão `UserError`.
- **Tools.** Jev escolhe a rota e preenche os argumentos (`typesafe_tool_call_threshold` padrão 0,6). Argumentos não suportados geram `ToolCallProposed`, que vai para o LLM de fallback.
- **Limites citados:** "at most 255 options in one question"; "jev-1.13 takes 64k tokens for the state and questions together, with 32k for the state plus the longest question; past that the request fails with a ModelHTTPError (`max_tokens_exceeded`)". Não há streaming nem entrada de arquivo ou imagem.
- **Retries.** "The TypeSafe SDK retries connection errors, timeouts and retryable HTTP statuses twice by default".
- **Privacidade (aviso da própria página):** "The arguments go to TypeSafe before the verdict comes back, so a call is disclosed to a third party even when it is then refused."
- **Região e ZDR:** não trata. **[NÃO ENCONTRADO]**

### 3.3 Vercel AI Gateway **[primária]**

- **API compatível** ([doc](https://vercel.com/docs/ai-gateway/sdks-and-apis/typesafe)). Base `https://ai-gateway.vercel.sh/typesafe`, endpoints `POST /typesafe/v1/systemone` e `GET /typesafe/v1/models`, modelo `typesafe-ai/jev`. Autentica com API key do Gateway ou OIDC. Há BYOK para faturar direto com a TypeSafe.
- **HTTP API genérica.** `POST https://ai-gateway.vercel.sh/v1/evaluate`, também via AI SDK `gateway.evaluationModel('typesafe-ai/jev')` ([evaluation](https://vercel.com/docs/ai-gateway/modalities/evaluation)).
- **Preço.** No exemplo, `"cost": "0.00001155"` para `input_tokens 275` = 275 × 0,042e-6: só o input é cobrado e não há markup no exemplo (`surchargeCost "0"`).
- **Modelo no catálogo** ([ai-gateway.vercel.sh/v1/models](https://ai-gateway.vercel.sh/v1/models)): `context_window 32000`, `"zdr": "all"`, `"no_training": "all"`, preço de input `0.000000042`. **Não há campo `regions`**.
- **ZDR.**
  - A página de ZDR lista **TypeSafe AI ✓ ✓** com a cláusula: "Except as necessary to comply with its legal obligations, TypeSafe shall not retain (a) prompts that are Customer Data for any longer than is necessary to generate Output for Customer and (b) Output for any longer than necessary to enable TypeSafe to fulfil its obligations to Customer under the Agreement." ([zdr](https://vercel.com/docs/ai-gateway/security-and-compliance/zdr)).
  - ZDR por request (`providerOptions.gateway.zeroDataRetention: true`) não tem custo extra. Team-wide custa US$ 0,10 por 1.000 requests. Ambos exigem Pro ou Enterprise.
  - O gateway da Vercel também diz não reter prompts por padrão ([security](https://vercel.com/docs/ai-gateway/security-and-compliance)).
- **Região.** O regional inference aceita só `us` e `eu`: "a model with no `regions` field doesn't support regional routing" ([regional](https://vercel.com/docs/ai-gateway/security-and-compliance/regional-inference)). Portanto o Jev **não** pode ser fixado em região na Vercel.

### 3.4 Cloudflare **[primária]**

- **Modelo.** `typesafe/jev`, marcado "Third-party". Chama-se por `env.AI.run('typesafe/jev', …)` ou `POST .../accounts/$ID/ai/run`. Contexto "32,000 tokens". O preço fica "in the Cloudflare dashboard", ou seja, não é público na doc ([doc](https://developers.cloudflare.com/ai/models/typesafe/jev/)).
- **Dados** ([data usage](https://developers.cloudflare.com/workers-ai/platform/data-usage/index.md)): "Cloudflare does not use your Customer Content to (1) train any AI models ... or (2) improve any Cloudflare or third-party services". Os modelos "constitute Third-Party Services".
- **Região ou ZDR específicos para o Jev:** **[NÃO ENCONTRADO]**.

### 3.5 LiteLLM ([docs](https://docs.litellm.ai/docs/pass_through/typesafe)) **[primária]**

Pass-through: basta trocar `https://api.typesafe.ai` por `LITELLM_PROXY_BASE_URL/typesafe`. Tem ainda um "JEV Auto Router" (`classifier_type: jev`) para escolher o LLM de uma completion.

### 3.6 Síntese: alguém permite escolher região ou garante ZDR?

| Canal | ZDR | Região escolhível | Contexto |
|---|---|---|---|
| TypeSafe direto | só enterprise, por pedido | não (EUA) | 64k / 32k |
| OpenRouter | sim (endpoint na lista ZDR; `zdr:true`) | não para o Jev (in-region EU/US é enterprise e depende do provedor) | 32k |
| Vercel | sim (TypeSafe ✓ na lista; Pro/Enterprise) | não (Jev sem `regions`; só existem `us`/`eu`) | 32k |
| Cloudflare | [NÃO ENCONTRADO] | [NÃO ENCONTRADO] | 32k |
| Pydantic AI / LiteLLM | herdam do endpoint usado | idem | idem |

---

## 4. Latência

### 4.1 Benchmarks independentes **[primária, repositórios]**

**AbdelStark/jev-benchmarks** ([README](https://github.com/AbdelStark/jev-benchmarks); [relatório JSON](https://raw.githubusercontent.com/AbdelStark/jev-benchmarks/main/results/reports/btzsc-pilot-v1.json)):

- De onde: "Jev was called as a hosted service from France". Modelo resolvido: `jev-1.13.0`. Amostra: 300 exemplos.
- Latência do Jev:

| Dataset | p50 | p95 |
|---|---|---|
| AG News | 255,9 ms | 332,3 ms |
| Banking77 | 246,4 ms | 334,2 ms |
| DAIR Emotion | 236,3 ms | 289,4 ms |

- Comparação: GLiNER local em CPU M4 Max ficou em cerca de 44 ms de p50 nas tarefas de 4 e 6 rótulos e 295,5 ms na de 72.

**nibzard/decision-model-benchmark** ([README](https://github.com/nibzard/decision-model-benchmark); [v2.md](https://raw.githubusercontent.com/nibzard/decision-model-benchmark/main/results/v2/v2.md)):

- Latência do Jev por suíte:

| Percentil | Faixa |
|---|---|
| p50 | 264–276 ms |
| p95 | 344–481 ms |
| p99 | 553–884 ms |

- "flat from 2 to 255 options"; "At 256, 384, and 512 it rejects the request: `400 Too many choices.`"
- O p50 do gpt-oss-120b no Cerebras ficou entre 303 e 346 ms.
- **De onde mediram: [NÃO ENCONTRADO].** O repositório não declara. O perfil GitHub do autor diz "Split, Croatia" ([github.com/nibzard](https://github.com/nibzard)).

**Terceiro** ([jevaiguide](https://jevaiguide.com/is-jev-down/)): "In our tests from East Asia, healthy calls returned in 250 to 600 ms" **[secundária]**.

### 4.2 Onde ficam os servidores

- Post de lançamento: "our published evals are generally run from our laptops on the West Coast (**this is where our service is currently based**)" ([post](https://typesafe.ai/blog/introducing-system-one-models-and-jev), texto da rodada 1 em `jev/blog.raw`).
- A demo da home mostra "Completed in 0.114s", presumivelmente medida da própria West Coast ([home](https://typesafe.ai/)).
- Os subprocessadores de compute (AWS, Modal, Nebius, CoreWeave) estão todos em "USA", sem região declarada. O OpenRouter registra `datacenters: null`.
- **Região exata (datacenter ou zona de cloud): [NÃO ENCONTRADO] em fonte pública.** O coordenador informou que `api.typesafe.ai` resolve para AWS **us-west-2** (Oregon) e que o TCP de lá levou ~210–230 ms. Não reproduzi a medição nesta rodada. Se estiver certa, o RTT fica perto do Azure Brazil South → West US 2 (177 ms) mais o caminho de acesso.

### 4.3 RTT estimado a partir de São Paulo

A fonte é a tabela da Microsoft com o P50 mensal de RTT no backbone Azure: janela de 30 dias terminada em 30/07/2026, linha = origem, coluna = destino ([Azure network latency](https://learn.microsoft.com/en-us/azure/networking/azure-network-latency)). **Brazil South** (estado de SP) para:

| Destino | Estado | RTT P50 |
|---|---|---|
| West US | Califórnia | **169 ms** |
| West US 2 | Washington | 177 ms |
| West US 3 | Arizona | 157 ms |
| West Central US | — | 157 ms |

Na mesma tabela, France Central → West US = 145 ms.

**[INFERÊNCIA]** Estimativa para uma chamada direta a partir de SP:

1. O p50 medido da França (236–256 ms) menos o RTT França → West US (145 ms) dá cerca de 90–110 ms. Esse é o tempo de serviço mais overhead do Jev, supondo conexão reaproveitada.
2. Somando o RTT SP → West Coast (157–177 ms), o **p50 esperado de SP fica em ~250–290 ms**. O p95 deve ficar na faixa de ~330–480 ms, pela dispersão observada nos dois benchmarks.
3. Uma conexão fria soma o handshake TCP+TLS (1 a 2 RTT, ~160–350 ms).
4. Gateways (OpenRouter, Vercel, Cloudflare) acrescentam um salto de valor não publicado.

Premissas: backbone Azure como proxy da internet pública, servidor na costa oeste dos EUA, uma única ida e volta por request.

---

## 5. Blog e manifesto

O sitemap do site lista 5 posts ([sitemap](https://typesafe.ai/sitemap.xml)). **Nenhum post foi publicado em typesafe.ai/blog depois de 15/09** (o CDX do Wayback confirma). Os posts do blog:

- **"The Bitterest Lesson"** (Sep 10, 2026, [link](https://typesafe.ai/blog/bitterest-lesson)).
  - Tese: "The bitterest lesson in ML is that doing the right task > data > compute > algorithms."
  - Argumento: Sutton vale onde a tarefa é óbvia e os dados são gerados por self-play, como nos jogos.
  - Números, citando o InstructGPT: modelos "GPT-2-sized ... (>100x smaller than GPT-3) trained on the right task ... destroyed GPT-3"; escalar o pré-treino "would need to reach roughly GPT-7 level to beat even that baseline, and GPT-9 to beat InstructGPT built on GPT-3".
  - Fecho: "You get what you optimize for and the bitterest lesson in ML is that the most important part of it isn't ML at all." O post não traz o texto das notas de rodapé 1–3.
- **"Lies, Damned Lies, and Benchmarks"** (Sep 11, 2026, [link](https://typesafe.ai/blog/antibenchmaxxing)).
  - Tese: "I'm against benchmaxxing"; "many of the spikes in 'jagged intelligence' are the benchmarks".
  - Exemplos:
    - Llama 4 no LMArena, "27 private variants".
    - Revisões do índice da Artificial Analysis para o "GPT-6 Astra".
    - GLM-5.2 contra Fable 5: "25% more code, took twice as long".
    - MMLU: "34.5%" para humanos não especializados.
  - Compromisso: "no standard benchmark table in our model releases. New evals will be dated snapshots and immediately retired once posted rather than hill-climbed ... publish our evolving internal evals ... alongside the caveats, any cherry-picking, and evidence that looks bad for us."
  - [INFERÊNCIA] Até 19/09, a MCA **proibia** os clientes de publicar benchmarks (§2.4); a TypeSafe removeu a proibição depois.
- **"AI: too good to be true, too bad to be useful"** (Jun 19, 2026) e **"Diogo Almeida - Founders You Should Know"** (Mar 31, 2026) aparecem no índice, mas as páginas não têm corpo próprio (parecem links externos).
- **Manifesto** "Composable AI: Build Prod, Not God" ([link](https://typesafe.ai/manifesto)).
  - Missão: "pave the shortest path to an AI-based economic revolution by making intelligence composable".
  - Tese: "the bottleneck isn't raw intelligence. It's that today's intelligence is hard to build on"; a IA atual é uma "horseless carriage"; "branch on common sense, understanding, and intent"; "Intelligence today is like databases before SQL"; "Safety is a precondition for layering".
  - Três passos: (1) "Ship the shape of machine-native composable AI with the highest possible intelligence-per-dollar"; (2) "Make our AI reliable enough to transform the economy via real automation"; (3) abstrações de nível mais alto.
  - Métrica de sucesso, no rodapé: "global TFP growth reaching 3% within five years and holding at that level for ten".
- **Fora do blog, depois de 15/09:** notas de Diogo Almeida (21/09) sobre um coding agent centrado no Jev e a "tirania do KV cache", resumidas pela [explainx](https://www.explainx.ai/blog/typesafe-coding-agent-kv-cache-notes-diogo-almeida-2026) **[secundária]**. **[NÃO ENCONTRADO: documento primário.]**

---

## 6. SDK JavaScript `@typesafe-ai/sdk` **[primária]**

- **Repositório e versões.** Repo [typesafe-ai/typesafe-sdk-js](https://github.com/typesafe-ai/typesafe-sdk-js), MIT. Criado em 04/09, último push em 15/09; 225 estrelas e 13 issues abertas em 22/09.
  - Versões no npm ([registry](https://registry.npmjs.org/@typesafe-ai/sdk)): `0.0.0-bootstrap.0` (placeholder, 12/09), `0.5.7` (12/09 04:13 UTC; o changelog diz 2026-09-11) e `0.6.0` (15/09 18:17 UTC).
  - **Nada depois de 15/09.**
  - O [changelog](https://docs.typesafe.ai/sdk/javascript/changelog.md) da 0.6.0 traz uma única entrada, breaking: "accept `Score.criteria` as an ordered sequence instead of a dictionary keyed by integers".
- **Requisitos.** "Node.js 20 or newer". O pacote traz ESM, CommonJS e tipos. O repo tem `jsr.json` ([README](https://raw.githubusercontent.com/typesafe-ai/typesafe-sdk-js/main/README.md)).
- **API.**
  - `new TypeSafeClient(config)`.
  - `client.systemOne<Q>(request, options?) → APIPromise<SystemOneResult<Q>>`. As respostas vêm tipadas pelo nome e pelos `criteria` da pergunta.
  - Helpers `choice()`, `noul()`, `score()` e `client.models.list()`.
  - Classes de erro: `APIError`, `RateLimitError`, `APITimeoutError`, `APIConnectionError`, `APIUserAbortError` e outras.
  - Fontes: [TypeSafeClient](https://docs.typesafe.ai/sdk/javascript/api/classes/TypeSafeClient.md), [llms.txt](https://docs.typesafe.ai/llms.txt).
- **Configuração** ([TypeSafeClientConfig](https://docs.typesafe.ai/sdk/javascript/api/interfaces/TypeSafeClientConfig.md)):

| Opção | Default |
|---|---|
| `apiKey` | `TYPESAFE_API_KEY` |
| `baseURL` | `TYPESAFE_BASE_URL`, senão `https://api.typesafe.ai` |
| `defaultModel` | `TYPESAFE_DEFAULT_MODEL`, senão `jev-latest` |
| `timeout` | "Timeout per attempt in milliseconds, without a total retry budget. Default: 10000" |
| `dangerouslyAllowBrowser` | `false` |
| `logLevel` | `warn` |
| `fetch` | customizável |
| `defaultHeaders` | — |

  - Sobre logs em `debug`: "Known credential headers are redacted; bodies are not". Ou seja, o state vai para o log.
- **Retries** ([RetryPolicy](https://docs.typesafe.ai/sdk/javascript/api/interfaces/RetryPolicy.md); [retry.ts](https://raw.githubusercontent.com/typesafe-ai/typesafe-sdk-js/main/src/retry.ts)):
  - `maxRetries: 2`.
  - Backoff de 500 ms, dobrando até 5.000 ms, com jitter de 0,25.
  - `httpStatuses: 408, 429, 500–599`, o que inclui o 529.
  - Honra `retry-after-ms` e `Retry-After` até `maxRetryAfterMs: 60000`.
  - Retenta `APIConnectionError` e `APITimeoutError`.
  - Todas as opções podem ser sobrescritas por chamada via `RequestOptions`.
- **Comparação com Python** ([retries](https://docs.typesafe.ai/sdk/python/api/retries.md)): mesmos defaults (`max_retries=2`, `backoff_initial=0.5` s, `backoff_max=5.0` s, `http_statuses={408, 429, *range(500, 600)}`). O `RetryPolicy` do Python tem `timeout: float | None = 30.0`.

---

## 7. Produção, incidentes e status

### 7.1 Uso nomeado **[secundária: imprensa com citação direta]**

Todas as citações abaixo são da [TechCrunch](https://techcrunch.com/2026/09/18/a-new-kind-of-ai-model-from-a-chatgpt-inventor-is-thrilling-developers/) de 18/09/2026, assinada por Tim Fernholz.

- **Vercel.** Pranit Sharma, engenheiro, contou que a empresa usava "OpenAI's ChatGPT Luna 5.6 to run a classifier to review commands for safety. When Vercel replaced OpenAI's Luna with Jev, it got results five to 18 times more quickly and with greater accuracy."
- **Bryo AI.** O CTO Nikhil Mudholkar testou a classificação de e-mails comerciais: "Gemini was slightly more accurate, but 10 to 20 times more expensive"; e Jev "is the only one that hands back a real probability".
- **Earendil.** Armin Ronacher, CTO, comentou usos como roteamento de modelos. É opinião, não relato de produção.
- **Capacidade.** A reportagem diz: "the company briefly lost the ability to serve users from its API because demand was so high". Também: "trained exclusively on synthetic data", e observadores externos suspeitam de base em LLM open-weight.
- **Estudo de caso formal ou cliente com volume declarado: [NÃO ENCONTRADO].** O diretório Jevable lista 194 projetos, mas o [digidai](https://digidai.github.io/2026/09/21/typesafe-jev-jevable-decision-models/) observa que "the reviewed public material does not establish retained usage or customer spending" **[secundária]**.

### 7.2 Página de status ([status.typesafe.ai](https://status.typesafe.ai/), Better Stack, lida 22/09 17:15 UTC) **[primária]**

- **Disponibilidade em 90 dias:** `api.typesafe.ai` "99.839% uptime"; `console.typesafe.ai` "99.988%".
- **Downtimes diários registrados desde 15/09:** 16/09 (5 min), 19/09 (18 min), 20/09 ("API issues", 2 min). O console aparece "unavailable" em 20/09.
  - Um guia de terceiros, lendo a página em 19/09, reportou 99.854% e o downtime de 5 min em "Sep 17" ([jevaiguide](https://jevaiguide.com/is-jev-down/)). A diferença de data provavelmente vem de fuso ou do alinhamento das barras. [INFERÊNCIA]
- **Incidentes formais** ([/incidents](https://status.typesafe.ai/incidents)). Julho e agosto: "No incidents reported". Setembro:
  - **"Console is unavailable."** (20/09), resolvido 21/09 08:16 UTC: "Issues with TypeSafe console and API are fully resolved." ([incident/1070098](https://status.typesafe.ai/incident/1070098)).
  - **"API issues"** (21/09), resolvido 21/09 23:40 UTC, afetando `api.typesafe.ai`: "We are seeing intermittent downtime and system instability. We are actively investigating." ([incident/1070670](https://status.typesafe.ai/incident/1070670)).
  - [INFERÊNCIA] Os dois vieram horas depois do fim da waitlist (20/09 21:30 UTC).
- **Rate limit.** A doc não mudou desde 17/09 (§1.5). Não há anúncio de mudança. **[NÃO ENCONTRADO]**
- **SLA contratual: não existe na MCA.** Há só a "Service Warranty" (9.1–9.2) e o serviço é "AS IS" (9.3).

---

## 8. O que muda a decisão de adoção

1. **Contrato e LGPD.**
   - Todo processamento é nos EUA.
   - O DPA público não tem cláusulas ANPD e só cobre EU/UK/Suíça.
   - Não há prazo de retenção declarado.
   - A MCA de 19/09 dá licença **perpétua** sobre Customer Data para Telemetry, fraude e compliance.
   - Lei da Califórnia, arbitragem JAMS em SF e teto de responsabilidade com piso de US$ 50.
   - ZDR direto só para enterprise.
   - Para dado pessoal de brasileiros, é preciso aditivo (SCC ANPD) ou limitar o state a dados não pessoais ou pseudonimizados.
2. **ZDR sem ser enterprise existe, mas custa contexto.** Pela **Vercel** (Pro/Enterprise, ZDR por request sem custo; a TypeSafe tem cláusula de não retenção listada) ou pelo **OpenRouter** (endpoint na lista ZDR). Nos dois, o contexto cai para **32k**, contra 64k direto. Nenhum canal permite fixar região para o Jev.
3. **Estabilidade de API e SDK.**
   - A migração preview → v1 foi breaking. `document` já foi recusado.
   - Os SDKs estão abaixo de 1.0 e tiveram breaking changes em 15/09 e 18/09 (Python msgspec → pydantic).
   - Recomenda-se fixar versões e usar `jev-1.13.0`, não `jev-latest`.
   - Instalar `typesafe-sdk`, **nunca** `typesafe-client`: no PyPI público este é um placeholder de terceiro.
4. **Preço e limites estáveis desde 17/09:** US$ 0,042/Mtok de input, output grátis, 250k tok/s e 1.200 rpm. A TypeSafe diz servir com lucro. Mas os limites "can change without notice".
5. **Latência a partir de SP.** Estimo p50 de ~250–290 ms direto: West Coast, RTT de 157–177 ms, e cerca de 100 ms de serviço inferidos do benchmark francês. É plana até 255 opções e dá erro 400 com 256.
6. **Confiabilidade inicial.** 99.839% em 90 dias e dois incidentes em 20–21/09 logo após abrir o acesso. Não há SLA. Use timeout, fallback e fila.
7. **Evidência de produção.** Só anedotas de imprensa (Vercel, Bryo AI). Nenhum caso formal.
8. **A proibição de publicar benchmarks caiu em 19/09.** Evals próprias agora podem ser publicadas.

## 9. Lacunas **[NÃO ENCONTRADO]**

- Prazo de retenção de requests e logs em dias.
- Cláusulas LGPD ou ANPD.
- Região exata dos servidores.
- Build por trás de `jev-1.13-20260917` no OpenRouter.
- jev-1.14 ou preview.
- Changelog da API.
- Local de medição do benchmark nibzard.
- Preço e ZDR do Jev na Cloudflare.
- Primária dos "$5 de crédito" e dos "140.000 signups".
- Documento primário das notas de coding agent de 21/09.
- Casos de produção formais.
- Anúncio de mudança de rate limit.
