# J17 — relatório (08/11/2026)

Veredito do critério (`consolidar_j17.py` → `consolidar_j15.avaliar`, sem alteração): **INCONCLUSIVO** (rc 3).
Selo do held-out `20261101c` íntegro antes e depois (`selo_antes`/`selo_depois` rc 0).

| Fase | 30.4.104 (candidato) | 30.4.103 (controle) |
|---|---|---|
| v2 frio (camada 1) | PRONTO · 0/600 + 0/371 falsos deny · recall 11/13 + 21/27 · p95 65,6 ms | PRONTO · idem · p95 61,0 ms |
| V3 frio (camadas 1+2) | NÃO PRONTO · 1/600 falso deny · recall inline 23/27 · timeouts c2 170/1035 (16,4%) · p95 1051 ms (informativo) | NÃO PRONTO · **o mesmo** 1/600 falso deny · timeouts c2 80/1035 (7,7%) · p95 1025 ms |
| Quente (daemon vivo 30.4.104) | instrumento inválido · p95 202 ms (informativo) · timeouts 12 | — |

## Por que INCONCLUSIVO

`hot.json` tem `valido: false` porque `rotas = {daemon: 2069, standalone: 1}`: a **primeira** das 2.070 chamadas
(o aquecimento da 1ª entrada, `hot.trace.jsonl` linha 0, 3.294 ms, `exit_reason: emit-allow`) foi atendida pelo
caminho standalone do hook, não pelo daemon. O critério exige 2× o denominador pela rota daemon; não foi alterado
depois da medição. Mesmo com o quente válido o veredito não seria PRONTO, pela pendência da V3 abaixo.

## A pendência da V3 é um defeito pré-existente do motor, não regressão da 30.4.104

Caso `b81d9fa3c3e5` (negativo, `analise/backend/tests/unit/services/test_ausencia_dita_uma_vez.py`, ts 25/09):
`from services import ingestion_pipeline as ing` → camada 2 nega `"ingestion_pipeline" is unknown import symbol`.
O controle 30.4.103 nega o mesmo caso, logo o defeito já existia (o J16 não chegou a rodar a V3).

O módulo existe no instante e hoje (`backend/services/ingestion_pipeline.py`, sem remoção no Git). Hipótese
(confiança ~0,75, não provada por execução isolada): `backend/tests/unit/services/` é um **pacote regular**
(`__init__.py` desde 06/09) e `backend/services/` é um **namespace package** (sem `__init__.py`). Em runtime o pytest
põe `backend/` no `sys.path` (primeiro diretório sem `__init__.py` acima de `tests`) e `services` resolve para
`backend/services`; a camada 2 resolve `services` pelo diretório do próprio script e encontra o pacote regular
`tests/unit/services`, que não tem `ingestion_pipeline`.

## Timeouts da camada 2: 80 (controle) × 170 (candidato) — ambiente, não binário

Pergunta da revisão de 08/11/2026: o «não regressão da 30.4.104» só cobria o falso deny. Medido:
`git diff 340d3114 6d343c0b -- crates/touring-pyright crates/touring-code/src/vgp` é **vazio** (nenhuma linha
da camada 2 mudou entre os builds 30.4.103 e 30.4.104); os timeouts se distribuem pelos quatro quartis da ordem
de chamada (104: 34/36/56/44; 103: 17/13/25/25), sem efeito de aquecimento; 69 dos 80 do controle também
estouraram no candidato; um terceiro run, noutro horário (anexo `pos-correcao-l2/`), deu 143. Os três ficam
abaixo do patamar de 20%. Leitura: a variação é da máquina (o orçamento de 1.000 ms da camada 2 contra um
pyright que disputa CPU a 93–95 °C com outras sessões), não do código — mesma família de
`p95-de-rodada-mede-a-maquina`. Consequência para o J18: rodar candidato e controle intercalados por id, não
em blocos sequenciais, para que a carga da máquina não seja atribuída a um dos dois.

## Próximos passos

1. **Feito em `90d491ec` (08/11/2026).** Reproduzido isolado (`touring vgp check`, deny determinístico). A causa
   exata era a classificação de escopo, não a resolução do pyright: `modulo_do_projeto` só procurava o módulo na
   raiz e no diretório do arquivo, e só como `.py`/`__init__.py`, então `services` (namespace `backend/services`)
   virava **externo**, e "unknown import symbol" de módulo externo é deny. Agora os ancestrais do arquivo até a
   raiz contam e PEP 420 é reconhecido; módulo do projeto → a camada 2 abstém (warn) e a camada 1 decide. Controles
   `json.loadz`/`os.pathx` seguem deny. V3 informativa no mesmo híbrido: falso deny 1/600 → 0/600, recall igual
   (`pos-correcao-l2/`).
2. Corrigir o instrumento quente para que a chamada de aquecimento que cai no standalone seja descartada e repetida
   (ou o daemon seja aquecido antes da 1ª entrada) — mudança de instrumento exige novo pré-registro, nunca reaplicar
   neste run.
3. Novo julgamento (J18) depois de 1 e 2, em held-out novo ou neste com o pré-registro revisado.
