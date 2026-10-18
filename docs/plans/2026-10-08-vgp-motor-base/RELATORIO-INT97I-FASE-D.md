# INT97 integrado: pipeline, fase D, reconciliação e J13 (18/10/2026)

Release candidato **30.4.97** = INT97 + P0B + FRAG + P0B-d + roteamento + as correções da análise ECC integradas no
int97 (hk, qa, mem, C8/C7, validador do heredoc). Referência fixa de latência: `8fd0efb0`.

## 1. Pipeline (`diagnostics/int97-integrado/resultados-2026-10-18/`)

| Etapa | Resultado |
|---|---|
| Gates derivados dos pacotes tocados (20 pacotes) | `GATES_OK`, 25 passos com rc 0 |
| Release reproduzível | `REPRODUZIVEL`; binário novo `5c7b3d80663ffcab` (`~/.claude/touring/vgp/binarios/5c7b3d80/touring`) |
| Juízes v2/V3, corpora ev e bak | rc 0; réguas R1/R2 contra INT97 e P0B gravadas |
| Pareado | pulado (`PAREADO=0`): o portão de latência é a fase D |

## 2. Fase D, o portão de latência (pré-registro `PREREGISTRO-FASE-D-2026-10-18.md`, sha `def79d90bf510b17`)

Medidor `conta_custo.py` (instruções retiradas em espaço de usuário, PMUs `cpu_core` e `cpu_atom`; CPU da árvore por
`wait4`; parede), ordem ABBABAAB por id, bootstrap de 2.000 reamostras com semente 20261018. Saída:
`diagnostics/int97-integrado/fase-d-2026-10-18/`.

| Par | A/A | Régua | A/B | R_típico (IC95) | R_p95 (IC95) | Absoluto: p95 CPU de B |
|---|---|---|---|---|---|---|
| **v2 ev** (portão) | PROTOCOLO_OK | instruções | **SEM_REGRESSAO** | 0,9990 [0,9989; 0,9991] | 0,9994 [0,9979; 0,9994] | 107,24 ms ≤ 150 |
| **v2 bak** (portão) | PROTOCOLO_OK | instruções | **SEM_REGRESSAO** | 0,9990 [0,9989; 0,9991] | 0,9979 [0,9979; 0,9993] | 98,34 ms ≤ 150 |
| V3 ev (relatado) | PROTOCOLO_INADEQUADO | — | não lido | — | — | — |
| V3 bak (relatado) | PROTOCOLO_INADEQUADO | — | não lido | — | — | — |

- Nenhuma medida ausente: 5.904 chamadas (ev) e 6.096 (bak) nas três réguas.
- O novo executa **0,1% menos instruções** que a referência no caso típico. Parede p95 A 85,0 / B 81,5 ms (ev) e
  70,3 / 68,9 ms (bak), só relatada.
- V3: o A/A reprovou nas instruções por pouco (R_p95 [0,9899; 1,0089] no ev, [0,9904; 1,0334] no bak) e na CPU
  (IC largo). Era o risco declarado no pré-registro (JIT e GC do Node dentro do pyright); o V3 não é portão. A parede
  mediana por chamada no V3 é de ~607 ms, com p95 de 940–993 ms, perto do prazo de 1.000 ms da camada 2.

## 3. Reconciliação por id contra o INT97 (`reconcilia_int97i.py`, 11 testes, 3/3 mutantes)

Toda divergência R1 (veredito + achados) contra o INT97 precisa de causa: P0B (aceite), FRAG (base × frag), previsão
escrita antes (P0B-d: `d94f8090d150` warn → deny, relativa ao P0B) ou timeout da camada 2 com perda só de achados.

| Par | Divergências | Causas | rc |
|---|---|---|---|
| v2 ev | 5 | P0B 1 · FRAG 4 | 0 |
| v2 bak | 5 | P0B 1 · FRAG 4 | 0 |
| V3 ev (rodada limpa do ABBA) | 8 | P0B 2 · FRAG 4 · previsto P0B-d 1 · timeout na BASE 1 | 0 |
| V3 bak (rodada do pipeline) | 9 | FRAG 5 · P0B 2 · previsto P0B-d 1 · timeout 1 | 0 |

A previsão do P0B-d se cumpriu nos dois corpora. Mesmo binário contra ele mesmo: 0 divergências (B×B e A×A).

### Achado: a rodada V3 do pipeline mediu a máquina

- Na rodada V3 do pipeline, a camada 2 estourou o prazo em 521/738 ids (ev) e 278/762 (bak), contra 46 e 50 do INT97.
- O ABBA (`timeouts_v3_abba.sh`, mesmo corpus ev, uma reserva só) deu ref 50, novo 34, novo 38, ref 37: os timeouts
  seguem a máquina, não o binário.
- Causa provável: comandos pesados do orquestrador (prontidão nova, que roda `touring e2e`/`kpi`/`doctor`, e o
  dry-run dos journals) rodaram fora do `hw-slot` durante a janela do juiz. A vaga rust não bloqueia a classe heavy
  nem comando fora do slot.
- Efeito: o timeout tira os achados da camada 2, inclusive deny, e o veredito cai para `allow`, o que **favorece**
  falsamente o critério de falso deny. Conserto no DAG (`JUIZ-TIMEOUT`, prioridade alta): o juiz V3 declara
  INCONCLUSIVO acima do patamar de timeouts do A/A, e o `hw-slot` ganha uma classe de medição exclusiva.

## 4. J13: held-out selado 20261013f

`diagnostics/j13/rodar_j13.sh`. Critério pré-registrado (Gabriel, 10/10): (A) contagem bruta, CP95 unilateral ≤ 1% por
balde, `--exigir-limite-superior`. Selo `2f9282aa59a0566f` INTACTO antes e depois. Corpus híbrido: 600 negativos de
arquivo + 553 inline do held-out, mais os positivos do vivo só como controle de liveness (1.217 chamadas por juiz).

| Binário · camadas | Falso deny arquivo | Falso deny inline | Recall (arquivo · inline) | Avisos em negativos | p95 | Veredito |
|---|---|---|---|---|---|---|
| **5c7b3d80 (integrado) · v2** | **0/600** · CP95 0,50% | **0/553** · 0,54% | 0,846 · 0,778 | 53 | 80,3 ms | **PRONTO** |
| 8fd0efb0 (ref) · v2 | 6/600 · 1,96% | 1/553 · 0,85% | 0,846 · 0,778 | 47 | 67,3 ms | não pronto |
| 5c7b3d80 · V3 | 2/600 · 1,05% | 1/553 · 0,85% | 0,923 · 0,852 | 56 | 1.027,5 ms | não pronto |
| 8fd0efb0 · V3 | 9/600 · 2,60% | 2/553 · 1,13% | 0,923 · 0,852 | 49 | 1.028,3 ms | não pronto |

- O held-out discrimina: a referência reprova nas duas camadas. O integrado tira 6 falsos de arquivo e 1 inline no v2,
  e 7 de arquivo e 1 inline no V3.
- Timeouts da camada 2 nas rodadas V3: 86/1.217 (integrado) e 76/1.217 (ref), patamar normal: medida não contaminada.
- **Os 3 falsos do V3 integrado** (diagnóstico; não muda o veredito), todos da camada 2 com «is not a known attribute of
  module» em código que rodou sem erro:
  - `ddc0a56fb49f` e `056b54167b0b` (arquivo): `urllib.error`, carregado como efeito colateral de `import urllib.request`;
    o submódulo existe (`urllib/error.py`), logo não é inexistência definitiva. É a classe do P0B (atributo implícito
    de módulo), vazando pela camada 2.
  - `94a5472efe4c` (inline): `PIL.Image.LANCZOS`, presente em tempo de execução e ausente do stub (atributo dinâmico).
- O p95 do V3 (1.027 ms) passa o teto de 1 s também na referência: é o custo da camada 2 nesta máquina, não do binário
  (fase D, §2).

## 5. R97: release 30.4.97 FEITO (18/10, ordem do Gabriel «Lançar o R97 agora», sem push)

- Propagação: `scripts/propagate-release.sh 30.4.97` deu rc 0 na 6ª tentativa (`diagnostics/r97/propagate.log`).
  - Gate 1/6: 260 suítes `ok`, 0 `FAILED`, 15.950 testes.
  - Prova 5.5: 41/41 asserções.
  - `analise` e `konverter` ficaram em lock 30.4.97, com `touring 30.4.97`.
  - Daemon global: `target/release/touring-daemon`, que já não aparece como «(deleted)».
- As 5 tentativas reprovadas, cada uma com o conserto e o log:
  1. E0425 `triad_hook::take` no teste do `pre_write.rs:1492`, API renomeada no merge; conserto em `c70fd6c3`.
  2. Drift do `docs/reference/modules.md`; conserto em `833c468d`.
  3. rc 75: sem vaga no hw-slot para o `touring-hooks-shared`, porque um lote paralelo ocupava a vaga rust.
  4. `version_matches_crate_version`: o `target/release/touring` ainda era o 30.4.96, pois o rito exige o build do
     release ANTES do gate 1.
  5. Espelho `client/` fora de sincronia: o `judge.attest.json` foi re-atestado às 10:59, com hash igual ao do
     `loop_converged.py` vivo; `--apply` resolveu.
  - Durante a 5ª, a cota do `/tmp` (tmpfs com usrquota) encheu; o Gabriel limpou 32 pastas mortas do `pytest-of`.
- Binário congelado `~/.touring/toolchains/30.4.97/bin/touring` = `target/release/touring` (sha `9f4e3e89a84c57f5`).
- Vereditos por id contra o `5c7b3d80`, juiz v2, corpus ev `corpus_v0d` (`diagnostics/r97/vereditos-ev/`): 738
  comparados, R1 = 0 divergências de veredito, R2 = 0 de findings inteiros. Veredito do juiz: PRONTO. Falso deny no
  arquivo 0/400 (CP95 0,75%) e no inline 0/274; p95 67,9 ms. O binário publicado decide igual ao binário julgado no
  J13.
