# Pré-registro — critério de latência (p95) do VGP, protocolo C′

**Registrado**: 17/10/2026 13:35 BRT, sessão efe418c2. **Aprovado por**: Gabriel («recomendações aprovadas», canvas de
decisão da mesma sessão). **Momento**: ANTES de qualquer juiz do binário integrado (INT97 + P0B + FRAG + roteamento +
P0B-d) — `diagnostics/int97-integrado/` não tinha nenhuma pasta de resultados neste instante. Mudar uma regra abaixo
depois de ver um resultado do integrado invalida este pré-registro.

## Por que o critério antigo não mede o binário (dados, `diagnostics/p0b/resultados-2026-10-17/tempos/`)

- O MESMO binário, em rodadas de ~1 min: p95 do v2 no bak 72,0 / 166,2 / 167,0 ms (INT97) e 66,8 / 133,4 / 168,8 ms
  (P0B). A distribuição inteira muda de escala, não só a cauda: mediana 3,0–7,8 ms, p90 37,6–119 ms, ids > 150 ms 17–54.
- Não é temperatura (INT97 158,8 ms a 60 °C × 107,3 ms a 59 °C): é disputa de CPU entre sessões.
- A razão P0B/INT97 entre rodadas vizinhas foi 0,44 · 0,96 · 2,16 — comparar rodadas mede a máquina.
- Mediana de 3 rodadas por id (a proposta B, testada): p95 151,2 [IC95 133,6–177,2] no INT97 e 123,1 [100,1–149,6] no P0B
  — o IC ainda atravessa os 150 ms e ainda confunde máquina com binário.

## Protocolo

1. Juiz `scripts/juiz_motor_vgp.py` v2 (camada 1: `--v5-estrito --corpus <c>`), cwd `~/projects/analise`,
   `TOURING_CODE_MODE=native` — o mesmo ambiente dos juízes por id.
2. `main()` do juiz roda UMA vez; `rodar_motor` é embrulhado (`tempos_pareados.py`): para cada id, o motor de A
   (referência) e de B (candidato) é chamado na ordem balanceada **ABBABAAB** (4 + 4; posição média 4,5 dos dois lados)
   sobre a entrada idêntica.
3. **A/A primeiro** (A = B = referência) e **A/B depois**, na MESMA reserva rust do hw-slot (`pareado_corpus.sh`), nos
   dois corpora (vivo `corpus_v0d` e bak `corpus_v0d.bak-2026-10-09`).
4. **Referência FIXA**: `8fd0efb0` (INT97 final). Nunca a release anterior — 10% por release acumularia.
5. V3 (camada 2, teto 1 s) fica FORA: o pyright domina e o p95 já excede 1 s na referência; segue relatado como antes.

## Estatística (`analisa_pareado.py`)

- Por id: m_A, m_B = mediana das 4 chamadas de cada lado.
- R_típico = mediana_i(m_B / m_A) (ids com m_A > 0); R_p95 = p95_i(m_B) / p95_i(m_A), com o p95 do juiz
  (posto mais próximo, `sorted[ceil(0,95·n) − 1]`).
- IC95 por bootstrap sobre ids: 2000 reamostras, semente 20261017.

## Regras de decisão

| Situação | Regra | Saída |
|---|---|---|
| ids sem tempo em qualquer lado | > 1% → inconclusivo (ausência nunca vira zero) | INCONCLUSIVO, exit 3 |
| A/A | IC95 de R_típico **e** de R_p95 ⊂ [0,95; 1,05] | PROTOCOLO_OK; senão PROTOCOLO_INADEQUADO (exit 2) → o A/B não é lido e o caminho é a fase D |
| A/B (portão) | limite superior do IC95 de R_típico **e** de R_p95 ≤ 1,10 | SEM_REGRESSAO; senão REGRESSAO (exit 1) |
| 150 ms absoluto | relatado (p95_i(m_B) em ms + fator da máquina = mediana_i(m_A)), **não é portão** | — |

O absoluto vira portão na **fase D**: o motor passa a reportar tempo de CPU por chamada (`CLOCK_THREAD_CPUTIME_ID`),
imune à disputa por construção — próxima release. A latência em uso real (logs dos hooks) é KPI de observação, não
portão de release.

## O que falsifica o protocolo

- A/A fora de [0,95; 1,05] num dos corpora → o pareamento não cancela o ruído desta máquina → fase D antes de qualquer
  veredito de latência.
- Verdicts diferentes entre as 4 chamadas do MESMO binário no mesmo id (gravados em `verdict_A`/`verdict_B`) → motor
  não determinístico: investigar antes de ler tempo.

## Artefatos congelados (sha256, 16 primeiros)

| Arquivo (`diagnostics/int97-integrado/scripts/`) | sha256 |
|---|---|
| `tempos_pareados.py` | `355e9b180745de00` |
| `analisa_pareado.py` | `5f519db406530261` |
| `pareado_corpus.sh` | `d88686714a0e5882` |
| `test_analisa_pareado.py` (8 testes verdes: A/A ok, A/A com desvio, A/B 0,8/1,0/1,3, cauda sozinha, ausência) | `93c78e495505b480` |

Fumaça de ponta a ponta contra um juiz e um motor falsos: 3 ids × 8 chamadas, A/A = PROTOCOLO_OK. O encaixe no juiz
real foi conferido por leitura (`rodar_motor(touring: list[str], …)`, `touring = [a.touring]`) — a 1ª versão passava
string onde o juiz espera lista; corrigida antes deste registro.
