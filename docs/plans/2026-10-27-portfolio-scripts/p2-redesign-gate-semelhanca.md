# P2-redesign — Gate de recriação por SEMELHANça (substitui o hash exato 3×)

> **Data**: 2026-10-27 | **Ordem**: Gabriel — "o harvest por recorrência disparar apenas com a 3ª execução idêntica é totalmente falho; a probabilidade de execução idêntica é mínima; na segunda tentativa já deve bloquear; é preciso um critério por semelhança"
> **Status**: design calibrado; implementação aguarda o release R1 (30.4.98) da touring-44 (working tree congelado para Rust até o sinal verde — coordenação mútua).

## 1. O que muda

| Antes (P2 v1, commit b19d3353) | Depois (P2 v2) |
|---|---|
| Identidade = hash EXATO do corpo | Identidade = **SimHash-64** de shingles de 3 tokens (corpo normalizado: sem comentários/blank) |
| Dispara na 3ª execução idêntica, como **hint** | Dispara na **2ª execução semelhante INLINE**, como **bloqueio** |
| Sem distinção de origem | `--file` (script persistido) **nunca bloqueia** — re-rodar o persistido é o padrão desejado; o alvo é o corpo inline reenviado |
| Sem bypass | Bypass explícito `--allow-repeat` (+ jornalizado como `repeat_allowed`) |

## 2. Calibração medida (calibrate_simhash.py sobre os 14 campeões reais)

| Classe de par | Distância Hamming |
|---|---|
| Positivos (mutações do mesmo programa: renomear vars, sem comentários, metade invertida) | máx **8**, p95 6 |
| Negativos (campeões diferentes entre si, 91 pares) | mín **20**, p5 24 |
| **τ recomendado** | **10** (margem: pior positivo 8 < 10 < 20 melhor negativo) |

**Limite honesto do critério** [INFERENCE 0.8]: SimHash pega **cópia editada** (a recriação que parte do programa anterior). A reescrita **do zero** com o mesmo propósito pode cair fora do τ — essa camada é a do hook P0 (propósito via MANIFEST/tantivy), não a deste gate. As duas camadas são complementares: P0 por propósito (antes de escrever o arquivo), P2-v2 por semelhança de corpo (antes de executar o inline).

## 3. Implementação (quando o tree destravar)

1. `ctx_execute_tools.rs`: `code_simhash(code) -> u64` (FNV-1a 64 dos shingles — `hash()` do protótipo Python é randomizado por processo, não serve); journal v4 ganha `"code_simhash": "<hex>"`; `count_prior_similar(journal_tail, hash, τ=10) -> Vec<(run_id, dist, harvest)>` (varredura byte a byte como `count_prior_same_hash`, parse só das linhas candidatas).
2. `run.rs`: após `resolve_code`, se origem = `--code` (inline) e `prior_similar` não vazio → **deny** (exit 2) com mensagem ensinando: o run_id anterior + slug harvestado se houver + as 3 rotas (`--file`, `--harvest <slug>`, `--allow-repeat`). Tenta bloqueada é jornalizada (`repeat_blocked: true`) para o KPI medir.
3. `recurrence_hint` (v1) morre — substituído pelo gate; testes trocados: `simhash64` estável/sensível; `count_prior_similar` casa ≤10 e rejeita ≥20; gate bloqueia na 2ª inline semelhante, não bloqueia `--file`, bypass `--allow-repeat` passa e jornaliza.
4. Rollout: o gate nasce em **sombra** (log + advisory) se `TOURING_REPEAT_GATE=shadow`, default... — **decisão do Gabriel**: nascer bloqueando direto (a ordem diz "deve bloquear") ou 1 semana de sombra primeiro (padrão prose gate)? Perguntar.

## 4. Achado colateral

`code_hash` (exato) segue no journal — útil para dedup de observability e para o `prior_same_program`. O que morre é só o gatilho por identidade exata na 3ª run.

## 5. Prova ao vivo + classe legítima de repetição inline (observado 27/10, R1 em curso)

O nudge v1 disparou **ao vivo nesta sessão** sobre o loop de espera do R1 (`prior_same_program: 2` → harvest_hint na 3ª janela idêntica) — o mecanismo funciona. Mas o mesmo evento mostra uma **classe legítima de repetição inline**: loops de polling/espera ("rode a cada 30s até X") DEVEM ser idênticos a cada run. No v2, essa classe cai no gate. Mitigações: (a) `--allow-repeat` como rota nomeada; (b) avaliar exceção estrutural para corpos cujo propósito é esperar (heurística frágil — preferir (a)); (c) medir em sombra quantos bloqueios seriam dessa classe antes de apertar. Registrado para a implementação.
