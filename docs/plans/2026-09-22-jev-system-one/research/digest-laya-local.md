---
type: ResearchDigest
title: Laya e alternativas locais ao Jev — viabilidade na RTX 4060
description: Laya upstream e laya-mlx, MLX em Linux/CUDA, português, calibração, fine-tuning RLCD, clones compatíveis com /v1/systemone e latência do Jev a partir do Brasil, com fontes.
plan_id: 2026-09-22-jev-system-one
tags: [research, laya, local, mlx, rtx4060]
timestamp: 2026-09-22T14:35:00-03:00
resource: https://github.com/NandhaKishorM/laya
okf_version: "0.1"
---

Parte do [bundle](/index.md); consumido pela [estratégia](/strategy-2026-09-22-jev-system-one.md) e complementado pelo [experimento local](/experiments/laya-local/report.md). Caminhos `scratchpad/…` citados abaixo eram temporários da sessão; as URLs são a fonte durável.

# Jev hospedado × Laya local — digest técnico (22/09/2026)

Escopo: saber se um modelo **aberto e local** cumpre o papel do Jev (TypeSafe, decisões tipadas
`choice`/`score`/`noul`) numa máquina Linux (Omarchy/Arch) com **RTX 4060 Laptop 8 GB**, sem Apple Silicon.
Regra deste documento: cada número traz a URL de origem; lacunas estão marcadas **[NÃO ENCONTRADO]**;
deduções minhas a partir de código/fonte estão marcadas **[INFERÊNCIA]**. Snapshots de API (estrelas etc.)
foram tirados em 22/09/2026.

---

## 0. Sumário para a decisão

| Eixo | Jev hospedado | Laya local (RTX 4060) | Fonte |
|---|---|---|---|
| Latência | p50 236–276 ms (França/?), 382 ms (Texas), 650 ms (Alemanha, estados longos). **Do Brasil: nenhuma medição publicada**; medi daqui um piso de rede de **~210–230 ms por RTT** até `api.typesafe.ai` (AWS us-west-2, Oregon) | 14–57 ms por chamada de 1 pergunta em GPUs de consumo NVIDIA equivalentes; nenhuma medição em RTX 4060 específica | §4, §5 |
| Português | Sem benchmark publicado por idioma **[NÃO ENCONTRADO]** | MASSIVE-intent `pt` (pt-PT, 20 opções): **0,45–0,49** zero-shot, contra 0,78–0,82 em inglês; XNLI não tem português; pt-BR nunca medido | §1.2 |
| Zero-shot geral | Mais forte em comparações com itens idênticos (≠ README do Laya) | Checkpoints base "perto do acaso" em typed-decisions (0,36/0,35); o autor diz: base para especializar, não motor zero-shot | §1.2, §1.7 |
| Alta cardinalidade | Até 255 opções; Banking77 0,76–0,80 com rótulos crus | Banking77 0,38–0,54 (0,61 com shortlist) | §1.6 |
| Calibração | ECE 0,02–0,25 conforme estudo | Multilingual sai **sem temperatura**; English com bucket `choice:11+`=0,1006 (agora com clamp); ajuste de temperatura por (tipo, nº de opções) é obrigatório | §1.2, §1.3 |
| Licença/custo | API fechada, US$ 0,042 / 1M tokens de entrada | Apache-2.0 (código e pesos), US$ 0 por chamada | §1.1 |
| Rota técnica | — | **PyTorch upstream (`pip install laya` + torch CUDA)**. O laya-mlx não suporta Linux. Para falar `/v1/systemone`: arbiter (CUDA 13, Docker), PR #31 (não mergeado) ou o binário GGUF `ggmlc` sm89 para Linux. O upstream não tem servidor HTTP no main | §2, §3, §5 |

**Leitura:** o Laya ganha claramente em latência, privacidade e custo marginal. A troca é qualidade zero-shot
(principalmente em PT, em cardinalidade alta e em `noul`/`score`) e calibração, que vêm frágeis. Para
se equiparar ao Jev numa carga PT-BR, o próprio projeto indica **fine-tuning + ajuste de temperatura com
dados do domínio**.

---

## 1. Laya upstream (github.com/NandhaKishorM/laya)

### 1.1 Identidade, maturidade e licença

- Repo criado em **18/09/2026**, último push em 21/09. **15.470 estrelas, 1.275 forks, 102 issues abertas**, 67 watchers, Apache-2.0 (API GitHub: https://api.github.com/repos/NandhaKishorM/laya).
- PyPI `laya`: 15 versões de 0.1.0 (18/09 04:38) a **0.3.5 (21/09 18:20)**. Python ≥3.10, licença Apache-2.0 (https://pypi.org/pypi/laya/json).
- Pesos no HF `convaiinnovations/laya`: criado em 18/09/2026, **2.368 likes**, `license: apache-2.0`, tag `commercial-use`. São 421.293.827 parâmetros em F16. O repo reúne os três checkpoints: raiz = English, subpastas `multilingual/` e `typed-decisions/` (https://huggingface.co/api/models/convaiinnovations/laya e https://huggingface.co/convaiinnovations/laya/raw/main/README.md).
- **Licença dos pesos: Apache-2.0**, declarada no YAML do card ("Apache 2.0 · Convai Innovations"). O card do multilingual também declara `license: apache-2.0` (https://huggingface.co/convaiinnovations/laya-multilingual/raw/main/README.md).

| checkpoint | encoder | params | contexto | uso | fonte |
|---|---|---|---|---|---|
| `laya` (raiz) | ModernBERT-large | 421M | 512 (`head_max_len` 192) | inglês | README upstream |
| `laya-multilingual` | mmBERT-base (22 camadas, vocab 256k) | 322M | 1024 (até 8k no encoder) | 100+ idiomas | card HF |
| `laya-typed-decisions` | ModernBERT-large | 421M | 1024 (`head_max_len` 256) | os 4 workflows typed-decisions | README upstream |

Downloads: English ~808 MB, multilingual ~647 MB (card HF). Desde a 0.3.5 baixa só o checkpoint pedido (PR #98, https://github.com/NandhaKishorM/laya/pull/98).

### 1.2 BENCHMARKS.md — completo, com foco em **Português**

Fonte: https://github.com/NandhaKishorM/laya/blob/main/BENCHMARKS.md, mais os JSONs crus
`research/results/t4_colab_benchmark.json` e `research/results/cpu_51_language_sweep.json`
(https://github.com/NandhaKishorM/laya/tree/main/research/results).

**Headline (Laya × Jev "publicado")**: typed-decisions 0,766 × 0,727 · AG News 0,953 × 0,910 · DAIR Emotion 0,600 × 0,480 · ECE pós-temperatura 0,081 × 0,246 · p50 1 pergunta (T4) 32,8 ms × 236–276 ms. O próprio arquivo avisa: "Jev figures are third-party published, never measured here".

**As 51 línguas** do sweep CPU (MASSIVE intent, 20 opções, acaso = 0,05, 100 casos/língua, seed 13, CPU 4 threads, torch 2.8.0, laya 0.2.0, fonte `cpu_51_language_sweep.json`):
`af am ar az bn cy da de el en es fa fi fr he hi hu hy id is it ja jv ka km kn ko lv ml mn ms my nb nl pl pt ro ru sl sq sv sw ta te th tl tr ur vi zh-CN zh-TW`.
Macro: `laya` 0,2269 (ECE 0,7331, 23/51 línguas acima de 3× o acaso) contra `laya-multilingual` **0,3661** (ECE 0,3869, 45/51).

**Português — tudo o que existe:**

| corrida | checkpoint | n | acurácia | ECE | observação | fonte |
|---|---|---|---|---|---|---|
| CPU sweep, MASSIVE intent `pt`, 20 opções | laya (English) | 100 | **0,470** | 0,512 (conf. média 0,972) | | `cpu_51_language_sweep.json` / BENCHMARKS.md |
| idem | laya-multilingual | 100 | **0,450** | 0,342 (conf. média 0,786) | Δ = −0,020: único idioma românico em que o English empata ou ganha | idem |
| T4 Colab, MASSIVE **intent** `pt` | laya | 300 | **0,457** (F1 macro 0,450) | 0,526 com as temps embarcadas; 0,345 cru | a temperatura embarcada `choice:11+`=0,1006 **piorou** o ECE | `t4_colab_benchmark.json` |
| idem | laya-multilingual | 300 | **0,493** (F1 0,488) | 0,342 | sem temperaturas (1,0) | idem |
| T4 Colab, MASSIVE **scenario** `pt` | laya | 300 | **0,420** | 0,568 embarcada / 0,375 cru | | idem |
| idem | laya-multilingual | 300 | **0,463** | 0,400 | | idem |
| reajuste de temperatura (150 held-out), intent `pt` | laya | 150 | 0,467 | **0,524 → 0,180** (T=2,58) | NLL 12,2 → 1,84 | idem, `calibration_repair` |
| idem | laya-multilingual | 150 | 0,513 | **0,341 → 0,114** (T=2,58) | | idem |
| reajuste, scenario `pt` | laya | 150 | 0,447 | **0,542 → 0,097** (T=2,71) | | idem |
| idem | laya-multilingual | 150 | 0,467 | **0,411 → 0,114** (T=2,92) | | idem |
| **XNLI português** | — | — | **não existe** | — | as 15 línguas do XNLI (en de fr es ru tr ar hi ur vi th el bg zh sw) não incluem pt (`XNLI_LANGS` em `research/scripts/build_benchmark_nb.py`) | https://github.com/NandhaKishorM/laya/blob/main/research/scripts/build_benchmark_nb.py |
| **pt-BR** | — | — | **[NÃO ENCONTRADO]** | — | o config `pt` do `mteb/amazon_massive_intent` vem do MASSIVE, que só tem **pt-PT** ("Portuguese - Portugal (pt-PT)") | https://huggingface.co/datasets/AmazonScience/massive · https://huggingface.co/datasets/mteb/amazon_massive_intent |

Referência inglês na mesma corrida: CPU `en` 0,820 × 0,680; T4 MASSIVE-intent `en` 0,783 × 0,657. Em intent de
20 opções, **o melhor checkpoint em PT entrega ~57–63% da acurácia do melhor checkpoint em EN**: CPU
0,470/0,820 = 0,57; T4 0,493/0,783 = 0,63 (cálculo meu sobre os números acima).

Roteamento de PT: `laya/lang.py` tem lista de stopwords `pt` e trata os diacríticos (ã, ç, é…) como
sinal de "não inglês" (https://github.com/NandhaKishorM/laya/blob/main/laya/lang.py). Num teste do diretório
mrjev.com, português foi roteado para o **multilingual** (`is_english: False`). No mesmo teste, espanhol e
italiano **foram para o English** (issue #172, https://github.com/NandhaKishorM/laya/issues/172). Alemão
ASCII curto também vai para o English, com −20 pontos (issue #54, https://github.com/NandhaKishorM/laya/issues/54).
**[INFERÊNCIA]** PT curto sem acento pode sofrer o mesmo. A mitigação documentada é passar `lang="pt"` ou
`model="multilingual"` no `Router.predict`.

**Demais tabelas do BENCHMARKS.md:**
- *English vs resto*: MASSIVE intent EN 0,783/0,657, outros 0,306/0,451 · scenario EN 0,603/0,560, outros 0,281/0,439 · XNLI EN 0,860/0,843, outros 0,521/0,731.
- *Temas* (400 casos, 3 checkpoints): spam 0,993 · phishing 0,980–0,993 (ambos no treino) · guardrails 0,708–0,762 (held-out) · moderação **0,530** (held-out, F1 macro 0,400) · RAG 0,625–0,657 · triagem de suporte (10 filas) 0,502–0,522 · roteamento de modelo 0,639/**0,123**/0,659.
- *Datasets com número do Jev*: AG News 0,950/0,930/0,953 × 0,910 · DAIR 0,595/0,530/0,600 × 0,480 · **banking77 0,425/0,425/0,492 × 0,870**.
- *typed-decisions (400 casos / 2.000 decisões)*: `laya-typed-decisions` 0,766 (soft 0,471, Brier 0,061, ECE 0,213, MAE 0,242) · `laya` 0,361 · `laya-multilingual` 0,342 · Jev 0,727 (soft 0,580, ECE 0,144) · teto do professor 0,735 · classe majoritária 0,461 · acaso 0,318.
- *Velocidade T4*: 1/5/10/50 perguntas = 39,5/84,5/158,6/771,3 ms (English) e 32,8/40,1/72,3/337,4 ms (multilingual). p95 do multilingual com 50 perguntas = 693,7 ms (JSON).
- *Calibração*: ECE médio 0,466 → 0,081 (English) e 0,314 → 0,106 (multilingual) após reajuste.
- *Robustez à ordem das opções* (taxa de troca de resposta): MASSIVE-en 0,150/0,230 · emotion 0,040/0,090 · XNLI 0,000/0,015. O Jev foi medido em 0,13.

**Proveniência — números sem corrida commitada** (issues #134 e #170: https://github.com/NandhaKishorM/laya/issues/134, https://github.com/NandhaKishorM/laya/issues/170):
o `app_benchmark.json` citado no BENCHMARKS.md **nunca foi commitado**. Por isso não têm fonte verificável:
typed-decisions 0,766, AG News 0,953, DAIR 0,600, banking77 0,492 e a tabela de temas. A linha
typed-decisions do multilingual (0,342…) não bate com o JSON commitado (0,3515). O JSON do T4 dá AG News
0,9467 e DAIR 0,5733 para o English, contra 0,950/0,595 da coluna "routed" do README. O mantenedor não
respondeu até esta data (issues abertas).

### 1.3 Notebook de fine-tuning (`notebooks/laya_finetune_typed_decisions_2xT4_kaggle.ipynb`)

Fonte: https://github.com/NandhaKishorM/laya/blob/main/notebooks/laya_finetune_typed_decisions_2xT4_kaggle.ipynb

- **Dataset**: `LocalLLaMA/typed-decisions` (Apache-2.0), 1.200 casos de treino (6.000 decisões) e teste de 400 casos (2.000 decisões). Cada linha traz `state` (JSON), `questions` (JSON) e `gold` (JSON com `probabilities` e `label`). O gold é a **média de 3 amostras de um "teacher endpoint de ~4B"**, e o teto de auto-concordância do professor é 0,735 (card: https://huggingface.co/datasets/LocalLLaMA/typed-decisions). O alvo de treino é a **distribuição soft** normalizada, e cada item passa por `build_sequence` com `max_len` 1024 / `head_max_len` 256.
- **RLCD como implementado**: ruído gaussiano de média zero projetado sobre os logits, com **G=4** amostras e **σ de 0,4 a 0,1** ao longo das épocas. A recompensa é `proper_reward` = log score + **0,75**×esférico − 1,0×RPS (este só para `score`). A vantagem é normalizada pela média do grupo ("GRPO-style"). A perda é REINFORCE com log-prob gaussiano **+ 1,0 × cross-entropy soft**, ou seja, não é PG puro.
  - ⚠ **Divergência com o artigo dev.to**, que descreve "pure policy gradient (zero supervised cross-entropy loss)", G=8 e σ 1,0→0,3. O card HF diz log + esférico + RPS (sem pesos) e TD(λ=1) para multi-turno.
- **Hiperparâmetros**: 4 épocas · micro-batch 8 · grad-accum 4 (batch efetivo 64 sequências em 2 GPUs) · AdamW com LR encoder 2,5e-5 e head 1e-4 · weight decay 0,01 · cosine até 1e-6 · autocast **fp16** + GradScaler · clip 1,0 · gradient checkpointing do encoder · DDP/NCCL `torchrun --nproc_per_node=2`.
- **Hardware/tempo**: 2×T4 no Kaggle. ⚠ **Divergência**: a célula do notebook diz "~4 to 6 minutes total"; o README diz "roughly 4-5 hours for 4 epochs over ~30k questions" (o dataset tem 6.000 decisões). Nenhuma medição em GPU única de 8 GB **[NÃO ENCONTRADO]**.
- **Ajuste de temperaturas de calibração**: `fit_one_temp` minimiza NLL soft por LBFGS sobre `log T`, **um escalar por tipo de pergunta** (choice/score/noul), com clamp [0,1; 10]. Usa 400 itens tirados do **próprio conjunto de treino** (`all_items[::15][:400]`), não de um held-out. Grava `cfg["temperature"]`.
  - ⚠ O notebook **não limpa o `temperature_by_options` herdado**. Como a inferência dá precedência aos buckets, o ajuste novo é ignorado nesses buckets (PR #139 aberto, https://github.com/NandhaKishorM/laya/pull/139).
  - O runtime 0.3.5 faz clamp em **[0,5; 5,0]** (`laya/common.py`).
  - **Não há API pública de ajuste no pacote 0.3.5**. O PR #19 (aberto) adiciona `fit_temperatures`/`save_calibration`/`load(calibration=)`. O mantenedor pediu clamp [0,5; 5] e mínimo de ~2.000 amostras por bucket (https://github.com/NandhaKishorM/laya/pull/19). Um usuário que avalia o Laya **para carga PT-BR** comentou ali que a falta dessa API é "one of the main blockers".
- **Outros defeitos do caminho de treino**: `head_checkpointing=True` é ignorado pelo `DecisionModel` (issue #148, https://github.com/NandhaKishorM/laya/issues/148). O treino é CUDA-only; o PR #53 (aberto) torna o laço agnóstico de dispositivo/ROCm (https://github.com/NandhaKishorM/laya/pull/53). O notebook chegou a ficar inválido e foi corrigido (issue #24). **Dados e scripts do treino original não foram publicados** (issue #4, "pushing that soon", https://github.com/NandhaKishorM/laya/issues/4; HF discussion #4).
- Fine-tune comunitário com números: `cklxx/laya-browser` numa **RTX 4070 Ti SUPER**, com elemento top-1 de 0,10 zero-shot para 0,66 e operação de 0,54 para 0,88 (PR #143, https://github.com/NandhaKishorM/laya/pull/143).

### 1.4 API do pacote `laya` (0.3.5, lida no código-fonte)

Fontes: `laya/agent.py`, `laya/common.py`, `laya/router.py`, `laya/presets.py`, `laya/shortlist.py`,
`laya/__init__.py` em https://github.com/NandhaKishorM/laya/tree/main/laya

- `laya.load(model_id_or_path="convaiinnovations/laya", device=None, token=None, subfolder=None) -> Agent`.
- `Agent.predict(state, questions)` (alias `system_one`). `state` aceita str, dict ou lista de turnos; `questions` = `{qid: {"type": "choice"|"score"|"noul", "instructions": str, "criteria": dict|list}}`. Retorno: `{"model": "laya-rl-agent", "answers": {...}, "usage": {"input_tokens", "output_tokens": 0}}`.
  - `choice` → `choice`, `probabilities`, `confidence` (1−H/log k), `action.act_probability`.
  - `score` → `score` (nível esperado, base 0), `legend`, `probabilities`, `confidence`.
  - `noul` → `noul` (P(true)), `confidence` = max(p, 1−p). ⚠ São duas definições de `confidence` na mesma resposta; o PR #126 (aberto) mostra que só `max(p)` é a grandeza calibrada (https://github.com/NandhaKishorM/laya/pull/126).
- **Batch**: todas as perguntas de **um** estado vão num único forward. **Não existe `batch_size` nem batch multi-estado** no 0.3.5; os PRs #47 e #166 (abertos) adicionam `predict_batch`.
- **`head_max_len`/`max_len`**: lidos de `agent.cfg` a cada chamada, então `agent.cfg["head_max_len"]=512` funciona em runtime. Opções que estouram o orçamento geram `ValueError("options exceed head_max_len")`.
- **Device/precisão**: auto `cuda > mps > cpu`. O `amp_dtype` dos três configs é **`bf16`**, logo em CUDA com capability ≥8 (a RTX 4060 é sm_89) roda autocast **bf16**; com capability <8 força fp16; CPU/MPS usam fp32. Se o CUDA falhar no carregamento ou der OOM na inferência, cai para CPU com aviso impresso (issue #6 e PR #9).
- **Router**: `Router(models=None, device=None, token=None, max_loaded=1, default="english", auto_task_detection=False, standalone_repos=False, preload=False)`; `route(state, questions, model, task, lang)` com precedência model > task > workflow (opt-in) > lang > detecção; `predict(...)` devolve também `routing`; `preload()`, `attach()`, `unload()`, `loaded`. ⚠ O `max_loaded=1` padrão recarrega o checkpoint a cada troca de idioma: 7–10 s no README e ~21 s em CPU na issue #172.
- **Presets**: `router_questions()`, `guard_questions()`, `moderation_questions()`, `triage_questions()`, `email_questions(categories=None)` (`laya/presets.py`).
- **`predict_shortlist(agent, state, questions, embed_fn, k=20)`** + `embed_fn_from_agent(agent)` + `shortlist_choice` (entrou na 0.3.5 via PR #106 depois da issue #102).
- Exportados também: `detect_language`, `detect_script`, `is_english`, `ece_score`, `proper_reward`, `td_lambda_targets`, `clean_email_body`, `email_state`.
- **Ajuste de temperatura**: **ausente** no pacote (ver §1.3).

### 1.5 Requisitos

- `pyproject.toml` 0.3.5: Python ≥3.10; `torch>=2.0.0`, `transformers>=4.48.0`, `safetensors>=0.4.0`, `huggingface_hub>=0.20.0`, `numpy>=1.20.0` (https://github.com/NandhaKishorM/laya/blob/main/pyproject.toml). O README afirma que "huggingface_hub 1.x, transformers 5.x and torch 2.14 all require 3.10".
- ⚠ Com **transformers 4.x**, o multilingual usa em silêncio a base RoPE errada nas camadas sliding (PR #51, aberto: https://github.com/NandhaKishorM/laya/pull/51). **Use transformers 5.x.** O benchmark T4 usou torch 2.11.0+cu128 e transformers 5.17.0 (JSON). Se o TensorFlow estiver instalado, rode com `USE_TF=0` (card HF).
- CUDA: qualquer wheel PyTorch com suporte a sm_89 (Ada) serve. A issue #6 registra que o PyTorch estável "supports up to sm_90 / Ada".

### 1.6 Banking77: por que o Jev aparece como 0,870, 0,764 ou 0,798

| estudo | Jev | configuração | Laya no mesmo estudo | fonte |
|---|---|---|---|---|
| AbdelStark/jev-benchmarks (citado no README do Laya) | **0,870** | variante **BTZSC** com **72 hipóteses** descritivas em linguagem natural; **100 exemplos** balanceados por classe; 200 linhas sem candidato positivo excluídas; chamada da França | — | https://github.com/AbdelStark/jev-benchmarks · protocolo `docs/PROTOCOL.md` |
| dhruvmehra/**jevbench** | **0,764** (F1 macro 0,753) | `legacy-datasets/banking77` test, **77 rótulos**, n=500 com seed, descrições de rótulo iguais para todos, via **OpenRouter Decisions API** | **0,382** (ECE 0,511, 130 ms Mac) | https://github.com/dhruvmehra/jevbench · `docs/results/2026-09-22-n500-summary.md` |
| jourdanlabs/**ASSAY-001** | **0,7977** | teste oficial **completo n=3.080**, 77 intents, **descrições nulas** (rótulos crus), `jev-latest`, 18/09/2026, do Texas; ECE 0,0936 (sobreconfiante) | — | https://github.com/jourdanlabs/assay-001 · `REPORT.md` |
| nibzard/decision-model-benchmark | 0,763 | PolyAI banking77 77-way (S1) | — | https://github.com/nibzard/decision-model-benchmark |
| zhlei07/open-system-one (issue #102) | 0,778 (descrições enriquecidas) / 0,784 (rótulos crus) | 2.500 itens, via Cloudflare Workers AI | **0,543** (`head_max_len` 512) → **0,608** com shortlist@20 | https://github.com/zhlei07/open-system-one · https://github.com/NandhaKishorM/laya/issues/102 |
| cocodedk/jev-bench | 0,779 (F1 macro **0,764**) | 308 mensagens (4 por intent), rótulos crus | — | https://github.com/cocodedk/jev-bench |
| poorjev crossbench (medido pelo autor do poorjev) | 0,812 (n=154) | Banking77, harness próprio | **0,519** | https://github.com/rupeshpoojary9/poorjev |
| simonmesmith/jev-banking77-experiment | 0,924 | teste completo 3.080 + definições escritas + **24 exemplos de treino recuperados por BM25 em cada request** (few-shot) | — | https://github.com/simonmesmith/jev-banking77-experiment |

**Explicação (a partir das fontes):** a diferença não é ruído do Jev, é **protocolo**. (a) O 0,870 usa o
espaço de 72 hipóteses do BTZSC, com descrições em linguagem natural; os demais usam os 77 intents do
PolyAI. (b) O tipo de descrição de opção muda o número: crua, descritiva, curada ou few-shot. O 0,924 da
simonmesmith mostra o teto com exemplos recuperados. (c) Tamanho de amostra: 100 contra 3.080. (d) Com
rótulos crus/77 opções os estudos convergem em **0,76–0,80**. O README do Laya compara o seu 0,425 (77
rótulos) com o 0,870 (72 rótulos), o que o próprio README sinaliza ("72 vs 77 labels"). **No único
confronto com itens idênticos** (issue #102) ficou Jev 0,778 contra Laya 0,543.

### 1.7 Issues e discussões relevantes (calibração, cardinalidade, idiomas, CUDA, qualidade)

| # | tema | achado | URL |
|---|---|---|---|
| #6 / PR #9 | CUDA | GPU mais nova que o build do torch → queda **silenciosa** para CPU; hoje emite aviso | https://github.com/NandhaKishorM/laya/issues/6 |
| HF disc. #2 · PR #42 | calibração | `choice:11+`=0,1006 saturava a confiança (1,00 até em erro); 0.3.5 faz clamp [0,5; 5] | https://huggingface.co/convaiinnovations/laya/discussions/2 · https://github.com/NandhaKishorM/laya/pull/42 |
| #35 | idioma (romeno) | multilingual 50%, typed 55% em 13 opções; confiança errada alta (0,77–0,99); multilingual sem temperaturas | https://github.com/NandhaKishorM/laya/issues/35 |
| #124 | idioma (chinês) | multilingual 70%, typed 65%, English 60%; erros confiantes persistem | https://github.com/NandhaKishorM/laya/issues/124 |
| #54 / #172 | roteamento | alemão ASCII, espanhol e italiano vão para o English; **português vai para o multilingual** | https://github.com/NandhaKishorM/laya/issues/54 · https://github.com/NandhaKishorM/laya/issues/172 |
| #102 / PR #106 / PR #18 | cardinalidade | shortlist 54,3% → 60,8% no BANKING77; `auto_head_budget` (PR #18, aberto) | https://github.com/NandhaKishorM/laya/issues/102 |
| #131 | `score` | multilingual **nunca escolhe a 1ª opção** de `score` (0/290 em inglês): defeito dos pesos | https://github.com/NandhaKishorM/laya/issues/131 |
| #156 | `noul` | rótulos `true/false` (fixos em `render_options`) puxam o "negativo" com confiança 1,0; `choice` com `positive/negative` funciona | https://github.com/NandhaKishorM/laya/issues/156 |
| #126 | confiança | duas definições de `confidence` na mesma resposta | https://github.com/NandhaKishorM/laya/pull/126 |
| #99 | textos longos | multilingual perto do acaso em posts longos e ruidosos | https://github.com/NandhaKishorM/laya/issues/99 |
| #135 | benchmark externo | 68,8%/72,1%/63,3% em pares adversariais; ~50–62 ms/pergunta numa "edge-class GPU" | https://github.com/NandhaKishorM/laya/issues/135 |
| #171 · HF disc. #9 | seleção de catálogo | Laya 23–30/100 contra Jev 92/100 (via laya-mlx); sensibilidade à ordem 65–76/100 | https://github.com/NandhaKishorM/laya/issues/171 |
| HF disc. #6 | confronto idêntico | Laya × Jev 1.13 em 310 decisões: triagem 0,800 × 0,894, guardrails 0,883 × 0,950, moderação 0,833 × 0,989; chamada de 5 perguntas: Laya CPU 375–476 ms × Jev API 885–1.017 ms | https://huggingface.co/convaiinnovations/laya/discussions/6 |
| HF disc. #3 | multilingual | 40,3% em 305 decisões adversariais (constante sem ler texto = 53,1%); `noul` e `score` abaixo da constante | https://huggingface.co/convaiinnovations/laya/discussions/3 |
| HF disc. #7 | chinês/voz | Laya 10/20 × Jev 20/20; mais contexto "achata" as saídas | https://huggingface.co/convaiinnovations/laya/discussions/7 |

### 1.8 Artigo dev.to do autor

"I Built Non-Autoregressive Decision Models a Year Ago. Then a Frontier Lab Called It a 'Breakthrough'",
publicado em 18/09/2026 (https://dev.to/nandakishor_m_6cc0adfde9f/i-built-non-autoregressive-decision-models-a-year-ago-then-a-frontier-lab-called-it-a-18me).
Traz arquitetura (ModernBERT-large + head de 2 camadas + scorer por `[MASK]` + cabeça act/escalate),
RLCD (log + 0,5×esférico − RPS; G=8; σ 1,0→0,3; "zero supervised cross-entropy") e TD(λ=1). Afirma
dados "100% human-labeled, real-world public datasets". Compara com o Jev "~400 ms avg (70–500 ms)" e com
"67,8% across 4 production workflows" (≠ os 0,727 do README). ⚠ Os hiperparâmetros divergem do notebook
(§1.3), e os dados de treino citados não foram publicados (§1.3).

---

## 2. laya-mlx (github.com/mizorewww/laya-mlx) e a pergunta "MLX roda em Linux com CUDA?"

### 2.1 O projeto
- Criado em 19/09/2026, **5.200 estrelas, 366 forks, 7 issues abertas**, Apache-2.0, PyPI `laya-mlx` 0.2.0 (https://api.github.com/repos/mizorewww/laya-mlx, https://pypi.org/pypi/laya-mlx/json). O próprio README o define como "independent MLX port, not an official Convai Innovations release". Faz **só inferência e conversão**; o treino fica no upstream.
- **BENCHMARKS.md** (M3 Max com 40 núcleos de GPU e 128 GiB, macOS 27.2, MLX 0.32.2; https://github.com/mizorewww/laya-mlx/blob/main/BENCHMARKS.md):
  - 1 pergunta curta, P50/P95 (ms): English PyTorch MPS FP32 22,70/25,47 · MLX FP32 18,55/20,25 · MLX FP16 17,75/21,45. Multilingual MPS 13,60/14,57 · MLX FP16 10,91/19,48. No README: 13,42 ms (English) / 7,39 ms (multilingual) FP16.
  - 50 perguntas FP16: 143,3 q/s (English) e 402,2 q/s (multilingual). Contexto cheio, 1 pergunta: 49,84 ms (English, 512) e 43,50 ms (multilingual, 1024).
  - Memória FP16: pesos 803,6 MiB (English) / 614,0 MiB (multilingual); pico com 1 pergunta 943,6 / 687,6 MiB; pico com 10 perguntas de contexto cheio 1.833 / 1.509 MiB.
  - **Fidelidade**: argmax 63/63 nos 3 checkpoints × FP32/FP16 (378/378); erro máximo de probabilidade FP16 ≤ 0,0054; AG News (256 exemplos) com 256/256 de concordância com o upstream.
  - Pesquisa de desempenho: "does not support a further universal 10× speedup"; ganhos pareados de 1,03–1,08× (README, `docs/PERFORMANCE_RESEARCH.md`, `docs/ENGINEERING_10X_RESEARCH.md`).

### 2.2 MLX tem backend CUDA em Linux? **Sim.**
- Primeira release PyPI do backend CUDA na **v0.27.1 (25/07/2025)**: "Initial PyPi release of the CUDA back-end" (https://github.com/ml-explore/mlx/releases). A v0.30.4 (27/01/2026) traz "Better support for consumer GPUs (4090, 5090, RTX 6000, ...)". A versão atual é a **0.32.2 (25/08/2026)**.
- Instalação: `pip install mlx[cuda]` (README), `mlx[cuda12]` ou `mlx[cuda13]`, e `mlx[cpu]` para Linux sem GPU. Requisitos: **arquitetura NVIDIA ≥ SM 7.5, driver ≥ 550.54.14, CUDA toolkit ≥ 12.0, glibc ≥ 2.35**; CUDA 13 exige driver ≥ 580 (https://github.com/ml-explore/mlx/blob/main/docs/src/install.rst). No metadata da 0.32.2, o extra `cuda` resolve para `mlx-cuda-12==0.32.2; platform_system == "Linux"` (https://pypi.org/pypi/mlx/json). O pacote antigo `mlx-cuda` parou na 0.30.0.
- A RTX 4060 (sm_89) atende SM ≥ 7.5.

### 2.3 O laya-mlx suporta Linux? **Não.**
- `pyproject.toml`: `mlx>=0.32.2,<0.33; sys_platform == 'darwin' and platform_machine == 'arm64'`, e o único classifier de SO é `Operating System :: MacOS` (https://github.com/mizorewww/laya-mlx/blob/main/pyproject.toml). README: "Apple Silicon, Python 3.11+, macOS 14+". O CI roda em "macOS arm64 runner".
- **[INFERÊNCIA, não testada]** O runtime (`laya_mlx/*.py`) só escolhe dispositivo com `mx.gpu`/`mx.cpu` e não chama kernels Metal (os kernels Metal ficam em `experiments/`). Em tese, instalar `mlx[cuda12]` à mão poderia rodar em CUDA. **Nenhum relato de alguém fazendo isso [NÃO ENCONTRADO]**, e a fidelidade de 378/378 só foi validada em Metal.
- **Conclusão**: na RTX 4060 a rota viável é o **upstream em PyTorch + CUDA**. O ganho do laya-mlx foi medido contra PyTorch **MPS** no Mac, não contra CUDA, então não há motivo técnico para portar.

---

## 3. Servidores e clones compatíveis com `/v1/systemone`

Relatório completo, com fontes por projeto e ~40 repositórios:
`/tmp/claude-1000/-home-gabrielgadea-projects-touring/b824294f-30ac-4b20-acf1-dc8234e89a3f/scratchpad/jev/r2/clones/clones-report.md`.
Método: `gh api` autenticado, clone raso e grep de rotas/uso do `typesafe-sdk`, API HF e `curl` dos sites.
Todos os números são **do próprio autor**, salvo indicação. Estrelas e datas foram lidas em 22/09/2026.

**Contexto do contrato**: endpoint hospedado `https://api.typesafe.ai/v1/systemone` (Bearer, `model: jev-latest`).
O SDK oficial `typesafe-sdk` (MIT) aceita `base_url` e `TYPESAFE_BASE_URL`, mas **exige `api_key` não vazia**
(https://github.com/typesafe-ai/typesafe-sdk-python). O diretório mrjev.com rodou o SDK oficial contra vários
servidores: jeff 11/12, LocalJev 11/12, kev 9/12, Simple Jev 6/13. As falhas mais comuns foram nome de modelo
fixo (`jev-1.13.0`) e perguntas sem `instructions` (https://mrjev.com/jev-alternatives).

### 3.1 Os projetos pedidos: **nenhum roda Laya**

| projeto | modelo | licença código/pesos | `/v1/systemone`? | números (autor) | ★ / criado / push | CUDA/Linux |
|---|---|---|---|---|---|---|
| jev-local https://github.com/us/jev-local | logprob de LLM (Qwen3.5-9B; leve Qwen2.5-3B) | **sem licença** / Qwen | **sim**, drop-in com `typesafe-sdk==0.6.0` | set3 n=1316: 0,832; 0,25–0,46 s/pergunta | 3 / 18/09 / 18/09 | Docker; "needs a GPU box or 24GB+" |
| LitJev https://github.com/zhengxuyu/litjev | output head de Qwen (padrão Qwen3.8-27B em H100) | Apache-2.0 / pesos próprios | sim no schema; teste com o SDK [NÃO ENCONTRADO] | sem acurácia no README | 40 / 17/09 / 21/09 | CUDA (H100) |
| poorjev https://github.com/rupeshpoojary9/poorjev | NLI DeBERTa-v3-base zero-shot | MIT | **não** (Python + MCP) | crossbench: Banking77 Jev 0,812 / von 0,838 / **Laya 0,519** / poorjev 0,656 | 6 / 19/09 / 21/09 | CPU (cuda opcional) |
| NanoJev https://github.com/TianyuCodings/NanoJev | Qwen3-0.6B + cabeças (jogos) | MIT / pesos sem licença | **não** (`/api/evaluate`) | jogos (Maze, Snake, Doom) | 1.972 / 17/09 / 21/09 | CUDA |
| kev https://github.com/jaredpalmer/kev | LoRA + pointer head sobre Qwen3.5 0.8/4/9B | Apache-2.0 / Apache-2.0 | **sim** (testes com o SDK oficial; issue #33 de arredondamento) | Kev-9B 0,822/0,852 (Jev dev 0,857); Kev-4B em L4 bf16 118 ms | 3.616 / 17/09 / 22/09 (219 commits) | CUDA, ROCm, MLX |
| von https://github.com/wfzyx/von | ModernBERT-large 395M + head (mesma família do Laya EN) | Apache-2.0 / Apache-2.0 | declarado; **issue #11: 422 em toda request com `von-sdk 1.0.1`** | macro 72,0% (Jev 96,6%, Laya 58,3%) no jabr v2; números internos inconsistentes | 457 / 18/09 / 22/09 | CUDA/ROCm/MPS/CPU |
| minojev https://github.com/zeredy879/minojev | Qwen3-1.7B congelado + head de 0,8M | MIT / Apache-2.0 | **não** (`/score`) | 95,8% em 120 decisões; OOD 31,7% | 25 / 18/09 / 21/09 | não documentado |
| OpenJev → **SemIf** https://github.com/TheoLeeCJ/SemIf · https://openjev.com | logits de LLM congelado (Qwen3.5-4B BF16…) | MIT | **não no main** (CLI; HTTP no PR #27 aberto; o fork FastJev tem `/v1/systemone`) | RTX 3090: 21 critérios em 1,023 s; subset TypeSafe 0,845 × 0,883 do Jev | 3.716 / 16/09 / 21/09 | **CUDA/Linux** (4B BF16) |
| jevlike https://github.com/vinnylarouge/jevlike | scorer por opção (bytes ou encoder HF congelado) | MIT | **não** (biblioteca) | ~98% em menus sintéticos | 1.219 / 16/09 / 16/09 (3 commits) | CUDA/MPS/CPU |
| awesome-jev https://github.com/yibie/awesome-jev · mrjev.com | listas curadas | — | — | mrjev: "239 open-source projects… 226 installed and tested" | — | — |

### 3.2 Servidores `/v1/systemone` **sobre Laya**, os que interessam para a RTX 4060

| projeto | licença | compat. | números (autor) | ★ / criado | CUDA / Linux / Docker |
|---|---|---|---|---|---|
| **arbiter** https://github.com/0xBakeer/arbiter | MIT (pesos Apache-2.0) | sim, só troca `base_url` (+ `/metrics`, `/readyz`) | GB10 20,9 ms (1 pergunta), 152 ms (50), 287 q/s com 8 chamadores; **~5 GB de VRAM com os 3 checkpoints** | 20 / 20/09 | **wheels CUDA 13.0 x86_64/aarch64, Dockerfile** |
| PR #31 do Laya (FastAPI + Nix/NixOS CUDA) https://github.com/NandhaKishorM/laya/pull/31 | Apache-2.0 | sim; o mantenedor "I like this one" | **RTX 3090: 23 ms** EN com 1 pergunta; 2,6 ms/pergunta com 10; pico de ~705 q/s (EN) e ~1.270 q/s (multi) | aberto | CUDA (NixOS/systemd) |
| **Pidbid/laya-deploy** https://github.com/Pidbid/laya-deploy | sem licença | ponte `/v1/systemone` | **RTX 5050 8 GB, CUDA fp16, Ubuntu: 20–30 ms; 3 perguntas em 24 ms; partida a frio 371 ms** | 0 / 20/09 | **Linux + CUDA (torch 2.11 cu128, systemd)** |
| laya.cpp https://github.com/lkarlslund/laya.cpp | MIT | servidor com batching | RTX PRO 6000: 1,35–2,48× o throughput do Python (FP32) | 58 / 20/09 (217 commits) | CUDA e Vulkan (C++/ggml) |
| noahbclarkson/laya-server https://github.com/noahbclarkson/laya-server | MIT/Apache-2.0 | sim (alias `jev-*`) | RX 7600 ROCm: 33/76/138 ms para 1/5/10 perguntas | 0 / 22/09 | Docker CPU/CUDA/ROCm (**imagem CUDA não testada**, diz o autor) |
| cory-johannsen/laya-docker · nvkudva/laya-server · pambrose/laya-server · stiermid/laya-serve · exfly/laya-jev-compatible-server · decide (pydecide) · EdgeJev (ONNX INT8 CPU 15,6 ms/pergunta) · decision-infra | várias; algumas sem licença | sim (o SDK oficial passa sem mudança, segundo os autores) | poucos números | 0–6 ★ / 20–22/09 | compose GPU (cory-johannsen), Docker NVIDIA (decision-infra); os demais são CPU |
| ggmlc `laya serve` (§5) | sem licença declarada | sim (`/v1/systemone`, `/v1/decide`) | RTX 4050 Laptop 25 ms | 49 / 22/08 | **binário Linux CUDA sm89** |
| **Hospedados**: impossibl `https://api.impossibl.com/v1/systemone` (PR #48; IDs `convaiinnovations/laya` e `-multilingual` confirmados em `/v1/models` em 22/09) · zaitlabs (issue #32; ToS "might use data for training"; sem resposta ao `curl`) | comercial | "wire-compatible" | — | — | — |

O upstream **não tem servidor HTTP nem Docker no main**; os PRs #31 (HTTP + NixOS CUDA), #3 (FastAPI, dá 500
com `extra: forbid`) e #45 (Docker + CUDA, validado numa RTX 4070 Ti, mas roda o SDK do Laya e não um servidor
HTTP) estão abertos. O PR #1 (servidor drop-in, do autor do von) foi fechado sem merge. **Maturidade geral:
todos os projetos têm menos de 7 dias** (criados entre 16 e 22/09/2026), e vários não têm licença.

---

## 4. Latência independente do Jev

| estudo | origem da medição | p50 | p95 / p99 | observação | fonte |
|---|---|---|---|---|---|
| AbdelStark/jev-benchmarks | **França** | 236–256 ms (AG News/Emotion); 246,4 ms (Banking77/BTZSC) | p95 [NÃO ENCONTRADO no README] | Jev 1.13.0, 100 ex./condição | https://github.com/AbdelStark/jev-benchmarks · `results/reports/btzsc-pilot-v1.md` |
| nibzard/decision-model-benchmark | **[NÃO ENCONTRADO]** (README, SPEC e blog não dizem) | 264–276 ms (5 suítes) | p95 344–481 · p99 553–884 | p50 plano de 2 a 255 opções | https://github.com/nibzard/decision-model-benchmark · `results/v2/v2.md` |
| ASSAY-001 | **The Woodlands, TX** | 382,3 ms (B77) / 385,9 ms (CLINC) | p95 553,7/615,3 · p99 852,1/845,1 | "TypeSafe's '70–500 ms' is consistent with our median and not with our tails" | https://github.com/jourdanlabs/assay-001/blob/main/REPORT.md |
| dhruvmehra/jevbench | via OpenRouter (local não informado) | 376–389 ms | p95 648–715 ms | inclui o salto OpenRouter | https://github.com/dhruvmehra/jevbench |
| JevBench v1.2 (fstandhartinger) | **Alemanha (Hetzner)** | 0,65 s | p95 0,72 s | estados longos (~950 tokens/decisão); Jev #1 com 74,4 | https://github.com/fstandhartinger/jevbench/blob/main/RESULTS-v1.2.md |
| open-system-one | via Cloudflare Workers AI | 381 ms | — | | https://github.com/zhlei07/open-system-one |
| MohtashamMurshid/jev-speed-test | — | 347 ms (mediana válida) | — | 500 casos BANKING77 | https://github.com/MohtashamMurshid/jev-speed-test |
| cocodedk/jev-bench | — | 365,5 ms (OpenRouter) / 387,1 ms (classifier.dev) | — | conexão nova por tentativa | https://github.com/cocodedk/jev-bench |
| HF disc. #6 | — | chamada de 5 perguntas 885–1.017 ms | — | | https://huggingface.co/convaiinnovations/laya/discussions/6 |

**América do Sul / região dos servidores TypeSafe:**
- Medição publicada a partir da América do Sul: **[NÃO ENCONTRADO]**. Declaração oficial da TypeSafe sobre região: **[NÃO ENCONTRADO]**.
- **Medição própria (22/09/2026 14:18 BRT, desta máquina, Porto Alegre/RS, AS28573 Claro NXT)**, crua em `f2/rtt_typesafe.txt`:
  - `api.typesafe.ai` resolve para 100.20.85.248 e 44.227.31.201. O ipinfo mostra `ec2-100-20-85-248.us-west-2.compute.amazonaws.com`, Boardman/Oregon, AS16509 Amazon. `server: istio-envoy`.
  - n=12 conexões novas (GET / sem autenticação, retorno 404): **TCP connect 209–231 ms (~1 RTT)**, TLS completo 421–455 ms, primeiro byte 626–674 ms. ICMP bloqueado.
  - **[INFERÊNCIA]** Mesmo com keep-alive, cada chamada ao Jev daqui paga pelo menos ~1 RTT (≈210–230 ms) de rede, mais o tempo de servidor. Com conexão nova são ~0,65 s antes do 1º byte. Quem mediu da França obteve 236–256 ms fim a fim, então o tempo de servidor parece pequeno e a rede domina. É plausível esperar ≳ 300 ms p50 a partir do RS, mas **isso não foi medido com uma chamada real** (não há chave API nesta máquina).
  - (O site `typesafe.ai` é Framer, servido da borda `sa-east-1`. Isso é só o site de marketing, não a API.)

---

## 5. Laya em GPU de consumo NVIDIA

**Medição em RTX 4060: [NÃO ENCONTRADO].** GPUs mais próximas, com medição publicada:

| GPU | runtime | 1 pergunta | multi-pergunta | fonte |
|---|---|---|---|---|
| **RTX 4050 Laptop 6 GB (CC 8.9, a mesma geração/arquitetura da 4060 Laptop)**, Windows | `laya.Agent` Python CUDA, English | **57 ms** (melhor 52), 18 q/s | preset email (7 perguntas, 1 forward) **143 ms**, 49 q/s | https://github.com/monatis/ggmlc/blob/main/examples/laya/README.md |
| idem | `ggmlc` GGUF F16 + CUDA graph | **25 ms** (melhor 23,5), 40 q/s | 143 ms, 49 q/s | idem |
| idem | Python CPU | 273 ms | 2,26 s | idem |
| RTX 4070 Ti SUPER, torch 2.11/CUDA 13 | stock | **17,7 ms** English / **14,1 ms** multilingual | 3 perguntas 18,9/15,0 ms; 30 perguntas × 512–966 tokens 328/321 ms | PR #25 https://github.com/NandhaKishorM/laya/pull/25 |
| idem | TileLang fast path (PR aberto, opcional) | 4,6 / 2,8 ms | 232/188 ms; argmax 99,5–99,6% igual | idem |
| RTX 4070 12 GB, torch 2.6.0+cu124 | stock, 4 perguntas/estado | 2 estados em loop 34,8 ms (English) / 18,8 ms (multilingual) | pico ~220 q/s → ~360 q/s (English) e ~465 → ~860 q/s (multilingual) com `predict_batch` | PR #166 https://github.com/NandhaKishorM/laya/pull/166 |
| RTX 5060 Ti | multilingual | ~10 ms/decisão um a um | ~1 ms/decisão com batch (30 estados × 3 perguntas) | PR #47 https://github.com/NandhaKishorM/laya/pull/47 |
| **RTX 5050 8 GB**, Ubuntu, torch 2.11 cu128 | stock fp16 | **20–30 ms** (EN, em regime); partida a frio 371 ms | 3 perguntas 24 ms | https://github.com/Pidbid/laya-deploy (via §3) |
| RTX 3090 | stock (servidor do PR #31) | 23 ms (EN) | 2,6 ms/pergunta com 10; ~705 q/s (EN) e ~1.270 q/s (multi) | https://github.com/NandhaKishorM/laya/pull/31 |
| NVIDIA GB10 | arbiter (servidor, 3 checkpoints residentes) | 20,9 ms | 50 perguntas 152 ms; **~5 GB de VRAM** | https://github.com/0xBakeer/arbiter |
| Tesla T4 (referência oficial) | stock | 39,5 / 32,8 ms | 10 perguntas 158,6 / 72,3 ms | BENCHMARKS.md |

**VRAM:**
- Medido só em MLX/Metal (§2.1): FP16 com pico de 0,94 GB (English) e 0,69 GB (multilingual) para 1 pergunta, e 1,5–1,8 GB para 10 perguntas de contexto cheio. Em MPS FP32, o multilingual ficou com 1,29 GB residentes (HF disc. #3).
- **[INFERÊNCIA do código]** No upstream em CUDA, o `build_model` cria o módulo em FP32 e o `load_state_dict` copia os pesos FP16 do arquivo para esses parâmetros FP32; o autocast só troca a precisão do cálculo (bf16). Estimo **~1,7 GB residentes (English) / ~1,3 GB (multilingual)** mais ativações. Os três pré-carregados dariam ~4,6 GB (1,16B params × 4 B). Isso é coerente com o "~5 GB de VRAM com os 3 checkpoints" que o arbiter publica (https://github.com/0xBakeer/arbiter). Cabe em 8 GB, mas **o embedder do daemon Touring reserva até 2.048 MB nesta mesma RTX 4060** (`TOURING_EMBED_CUDA_MEM_MB`, CLAUDE.md do workspace, item 19). Prefira `Router(max_loaded=2)` ou pré-carregar só English + multilingual.
- fp16/bf16: autocast bf16 automático em sm ≥ 8; `.half()` manual não é documentado **[NÃO ENCONTRADO]**.

**Exportação:**
- **ONNX oficial: [NÃO ENCONTRADO]**. O mantenedor disse "We are looking into ONNX runtime export" (issue #37).
- ONNX comunitário: `navopw/laya-onnx` (Apache-2.0, serve `POST /v1/systemone`, **CPU-only**, 237–267 ms num Raspberry Pi 5; INT8 e FP16 misto rejeitados por deriva de probabilidade; https://github.com/navopw/laya-onnx) · `receptron/laya` (MIT, Node/TS via ONNX Runtime, 270 estrelas; https://github.com/receptron/laya) · `MstyAI/laya-onnx`, `m1rhan/laya-typed-decisions-ONNX` (web), `AXERA-TECH/laya.axera` (NPU).
- **GGUF**: `ggmlc` gera F16, Q8_0 e UD_Q4_K_M (HF `mys/laya-GGUF`, `mys/laya-multilingual-GGUF`, `mys/laya-typed-decisions-GGUF`), com **binário Linux x86_64 CUDA sm89** e `laya serve` compatível com o SDK TypeSafe (`/v1/systemone`, `/v1/decide`). O repo `monatis/ggmlc` tem 49 estrelas e **nenhuma licença declarada** na API do GitHub (issue #37, https://github.com/monatis/ggmlc/blob/main/examples/laya/README.md).
- **TensorRT: [NÃO ENCONTRADO].**

---

## 6. Caminho recomendado para testar na RTX 4060 (sem executar aqui)

1. `python -m venv` → `pip install torch` (wheel CUDA com sm_89) + `pip install "transformers>=5" laya==0.3.5`.
2. `Router(device="cuda", max_loaded=2)` e `preload(["english","multilingual"])`. Em PT, force `lang="pt"`.
3. Antes de confiar em `confidence`: rotular 2–3 mil decisões PT-BR do domínio, ajustar temperatura por (tipo, nº de opções) em held-out (lógica do PR #19) e usar `max(p)` como confiança.
4. Evitar `noul` com os rótulos fixos `true/false` (issue #156): usar `choice` binário `positive/negative`. Manter `choice` ≤ ~20 opções ou usar `predict_shortlist`.
5. Se a qualidade zero-shot em PT não bastar: fine-tune RLCD (notebook; exige adaptar DDP → 1 GPU e medir memória em 8 GB, **[NÃO ENCONTRADO]** relato).

