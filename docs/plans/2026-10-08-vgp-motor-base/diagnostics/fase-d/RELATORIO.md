# Fase D do portão de latência do VGP — implementação da medição (18/10/2026)

Implementa, sem alterar nenhuma regra, o [PREREGISTRO-FASE-D-2026-10-18.md](../../PREREGISTRO-FASE-D-2026-10-18.md)
(sha `def79d90bf510b17`, conferido antes de começar), que emenda o
[PREREGISTRO-P95-PAREADO-2026-10-17.md](../../PREREGISTRO-P95-PAREADO-2026-10-17.md). Scripts em
`diagnostics/int97-integrado/scripts/` (`S/`). Este relatório foi pedido pela coordenação como parte da entrega.

## O que foi construído

| Arquivo | O que faz |
|---|---|
| `S/conta_custo.py` (novo) | Lançador `conta_custo.py --out <json> -- <cmd…>`: fork, o filho espera um pipe, o pai abre um contador por PMU híbrida (`cpu_core` tipo 4, `cpu_atom` tipo 10, lidos do sysfs) com `perf_event_open` no PID do filho (`disabled`, `enable_on_exec`, `inherit`, `exclude_kernel`, `exclude_hv`, `PERF_COUNT_HW_INSTRUCTIONS \| tipo<<32`), libera, `wait4`, grava `{instr, instr_core, instr_atom, cpu_ms, wall_ms, rc, erro}` (+ `pmus`, `t_enabled_ms`, `t_running_ms` para auditoria). stdout/stderr/rc do filho passam intactos. Extra não pedido: `PR_SET_PDEATHSIG` no filho — o `subprocess.run(timeout)` do juiz mata só o lançador, e sem isso o motor sobreviveria a ele. |
| `S/tempos_pareados.py` | Cada chamada do motor passa pelo lançador (prefixo trocado no `touring` que o juiz recebe); guarda `instr_A/B` e `cpu_A/B` por id ao lado de `A/B/verdict_*`. Ordem ABBABAAB, juiz e saída anteriores inalterados. `--sem-custo` (1º argumento) roda o C′ sem o lançador. |
| `S/analisa_pareado.py` | `veredito_d` + CLI com `--fase d` (padrão) e `--fase c` (comportamento antigo, funções do C′ intactas). Régua escolhida no A/A; o A/B exige `--aa <veredito_aa.json>`; semente 20261018; absoluto de CPU. |
| `S/pareado_corpus.sh` | 5º parâmetro opcional `d` (padrão) ou `c`; env opcional `JUIZ_EXTRA` (ex.: argumentos do V3). Uso atual de 4 parâmetros inalterado. |
| `S/test_conta_custo.py`, `S/test_analisa_pareado_d.py`, `S/test_tempos_pareados.py` (novos) | 18 + 45 + 5 testes. |
| `S/mutantes_fase_d.py` (novo) | Controle + 47 mutantes (cada regra nova trocada pela implementação errada mais barata). |

## Testes: vermelho, verde, mutantes

- **Vermelho antes do verde.** Lançador: `ERROR test_conta_custo.py - FileNotFoundError` (arquivo ausente), depois
  `15 passed`. Análise D: **37 failed** em 2,24 s (`veredito_d` inexistente), depois 41 passed / 4 failed, e os 4 eram
  defeitos DO TESTE (ruído sintético de CPU de 2% largo demais para a margem testada; a espia da semente capturava o
  `Random` da própria fixture; uma asserção de R_p95 incompatível com o bootstrap) — corrigidos sem tocar na regra.
  `tempos_pareados` (comportamento novo) foi escrito antes do teste: aí a prova de vermelho são os mutantes P01–P04.
- **Verde final: 76 passed** (8 do C′ **sem alteração** — `test_analisa_pareado.py` mantém o sha `93c78e495505b480` —
  + 45 + 18 + 5), 5 repetições do arquivo do lançador sem instabilidade, `ruff check` limpo na pasta, `shellcheck` limpo.
- **Mutantes: 47/47 mortos** (`mutantes.txt`/`.json`), controle verde primeiro (`76 passed`). A 1ª rodada deu **45/47**
  e achou dois furos reais na suíte, fechados:
  - A18 (A/B ignora o R_típico) sobrevivia: meu teste «só o típico piora» deixava o p95 cair na zona dos ids lentos em
    parte das reamostragens, e o R_p95 também reprovava. Agora a cauda é 25% dos ids e o teste afirma R_p95 ≤ 1,10 com
    R_típico > 1,10.
  - L05 (sem `enable_on_exec`, conta o python entre fork e exec) sobrevivia: o python do filho antes do exec era pequeno
    demais para a tolerância. Novo teste: `PATH` com 3.000 entradas mortas antes do binário real (`execvp` itera no
    python) — milhões de instruções antes do exec que não podem entrar na contagem.
- **Os 11 mutantes de limite/semente sem o teste que fixa as constantes** (`--sem-constantes`): 10/11 mortos por
  comportamento; só o de 200 reamostragens (A10) sobrevivia, porque o número de reamostragens não aparece no veredito. Um
  teste com espia agora conta 3 × 2.000 chamadas de `choices` e mata A10 por comportamento (reexecutado: killed).
- **Calibração absoluta do contador** (teste `test_calibracao_com_laco_de_contagem_exata`): um laço `dec; jnz` × 100 M
  retira exatamente 200 M instruções; o lançador leu 200.249.146 (181 M em P-core + 19 M em E-core, o processo migrou) e,
  noutra execução que caiu inteira em E-core (`instr_core = 0`), 200.249.148 — as 249 k são o startup+exit do binário
  estático. A contagem não depende do tipo de núcleo.

## Fumaça: A/A real, binário `8fd0efb0` nos dois lados

`hw-slot --wait 3600 -- bash pareado_corpus.sh <ref> <ref> <corpus_40> <pasta> d`, cwd `~/projects/analise`,
`TOURING_CODE_MODE=native`, juiz v2 (`--v5-estrito`). Corpus reduzido: 40 ids sorteados (semente 1) de
`corpus_v0d` — 20 de `entradas.jsonl` (positivos reproduzíveis e negativos) e 20 de `entradas_inline_neg.jsonl`, só
Python, só snaps que compilam —, montado por symlinks em diretório próprio (`monta_corpus40.py`; o corpus original não foi
tocado). 40 ids × 8 chamadas por rodada, ~24 s por rodada. Resultado em `fumaca-aa-2026-10-18/`.

| Medida | Rodada | R_típico (IC95) | R_p95 (IC95) |
|---|---|---|---|
| **Instruções** | A/A | 1,0000 [0,9997; 1,0005] | 1,0000 [0,9998; 1,0001] |
| | 2ª (mesma ref × ref) | 0,9999 [0,9992; 1,0000] | 1,0000 [0,9998; 1,0002] |
| **CPU** | A/A | 0,9910 [0,9496; 1,0300] | 1,2010 [0,8495; 1,2010] |
| | 2ª | 1,0049 [0,9621; 1,0269] | 1,0170 [0,8961; 1,2074] |
| Parede (só relatada) | A/A | 0,9465 [0,8860; 1,0245] | 1,2524 [0,8483; 1,2524] |
| | 2ª | 1,0126 [0,9639; 1,1088] | 1,0484 [0,7909; 1,2175] |

**Régua que o A/A escolheria: instruções** (`PROTOCOLO_OK`, `regua=instr`; os IC têm ±0,05%, ~20× mais estreitos que a janela de ±1%). A CPU, no
mesmo A/A, teria reprovado a janela [0,95; 1,05] (R_típico inferior 0,9496; R_p95 [0,85; 1,20]) — com n = 40 o IC do p95
é largo; nos 738 ids do corpus ele estreita, mas a diferença de ordem de grandeza para as instruções fica.

Dispersão intra-id (8 chamadas): mediana de (máx−mín)/mediana — **instruções 0,32–0,39%**, CPU 21%, parede 34–37%; no p95
dos ids, instruções 1,8–3,3%, CPU 60–65%, parede 82–114%. Nenhum id com `verdict` instável entre as 8 chamadas.

**Absoluto (v2, relatado na 2ª rodada):** p95 da CPU por chamada de B = **139,78 ms** (320 chamadas; p95 das medianas por
id 131,93 ms) contra o teto de 150 ms: passa, com 7% de folga nesta amostra de 40 ids. A CPU por chamada mediana é 23–29 ms,
contra 3,5–4 ms do `elapsed_ms` do motor: a CPU da árvore inclui spawn, exec, carga do binário e tempo de kernel, e **não é a
mesma grandeza** que o `elapsed_ms` do portão antigo.

## Decisões onde o pré-registro era omisso (para revisão da coordenação)

1. **Absoluto = p95 de todas as chamadas de B reunidas** (texto: «por chamada»), não das medianas por id; o valor por id
   também sai no JSON. Reprovar o absoluto dá `REGRESSAO` (exit 1) com `portoes.absoluto=false`; no V3 vira só relato
   (`portoes.absoluto=null`). O juiz é deduzido de `--camadas` com a camada 2 nos argumentos do pareado (`--juiz` força).
2. **Ausência:** instruções e CPU contam por chamada (> 1% → INCONCLUSIVO); a parede segue a regra do C′ por id (> 1% dos
   ids). Um id com furo em qualquer das três medidas sai das três réguas (todas comparam os mesmos ids), listado no JSON.
   Campo ausente do arquivo (um pareado antigo lido pela fase D) conta como 4 chamadas ausentes por lado → INCONCLUSIVO.
3. **A/B exige `--aa`:** lê a régua do veredito do A/A, confere `fase=D` e que a referência (`sha_A`) é a mesma, e **não lê
   o A/B** se o A/A não foi `PROTOCOLO_OK` (sai com 2 ou 3). Sem `--aa`, exit 4. `exit 4` = uso errado em ambas as fases:
   o `argparse` saía com 2, que colide com `PROTOCOLO_INADEQUADO`.
4. **Leitura 0 depois de um exec bem-sucedido** → `instr: null` + erro (um processo real nunca retira 0 instruções).
   Falha de exec (127/126) → `null` + o erro do filho. Uma PMU híbrida que não abre anula a soma inteira.
5. O bootstrap da fase D usa `rng.choices` sobre índices (mesmos estimadores, 2.000 reamostras, semente 20261018); o do C′
   continua sendo o `bootstrap` original com `rng.choice`, usado só em `--fase c`. Os IC da D não são reprodutíveis com o
   código do C′ — outra semente de todo modo.

## Limites

- **A CPU depende do tipo de núcleo e da frequência.** O mesmo laço de Python (2.070 M instruções) levou 84 ms de CPU num
  P-core e ~147 ms rodando quase todo em E-core (máquina a 76–88 °C). As instruções são idênticas; a CPU não. Por isso a
  CPU é régua de reserva, e o absoluto de 150 ms herda essa variação.
- **As instruções excluem o kernel por construção** (`exclude_kernel`): uma regressão em syscalls, I/O ou falhas de página
  é invisível para a régua de instruções (a CPU e a parede a veem). O pré-registro já diz que instruções protegem contra
  regressão relativa de código de usuário, não medem milissegundos.
- **Camada 2 (V3) não foi exercitada.** O JIT e o GC do Node dentro do `pyright-langserver` tornam as instruções menos
  determinísticas (previsto no pré-registro); o neto está provado com um neto Python, não com o pyright real. O A/A do V3
  é quem mede isso.
- **CPython não é determinístico em instruções** nos testes de carga: o mesmo laço no escopo do módulo retira 2.070 M,
  2.146 M, 2.236 M ou 2.321 M conforme a semente de hash por processo (`STORE_NAME` sonda o dict; com `PYTHONHASHSEED=0`,
  8 execuções idênticas). Os testes fixam a semente; o motor Rust mediu 0,3–0,4% de dispersão mediana, mas o corpus
  completo é quem diz a do p95.
- **Requisitos do ambiente:** x86_64, `perf_event_paranoid ≤ 2` (aqui 2), PMUs híbridas no sysfs. Sem isso a medida é
  `null` e o veredito é INCONCLUSIVO por desenho, nunca 0. `t_enabled_ms`/`t_running_ms` são gravados mas não escalonados
  (em híbrida, `time_running` por PMU é legitimamente menor que `time_enabled`); multiplexação não é gate.
- **Amostra da fumaça:** 40 ids, só Python, camada 1. Não substitui o A/A dos corpora vivo e bak.
- **Os três scripts editados deixam de ter o sha congelado no C′** (`tempos_pareados.py` `355e9b18…`, `analisa_pareado.py`
  `5f519db4…`, `pareado_corpus.sh` `d8868671…`); as funções do C′ não foram tocadas e o teste do C′ passa sem mudança.
  Novos sha256 (16 primeiros): `tempos_pareados.py` `a4e4eae533357cc3`, `analisa_pareado.py` `cad9601c55f77fa8`,
  `pareado_corpus.sh` `56647b74d978a09d`, `conta_custo.py` `43653928b609de0f` (o mesmo da fumaça),
  `test_conta_custo.py` `0473804cf079e66f`, `test_analisa_pareado_d.py` `5f03f1855aeb28d3`,
  `test_tempos_pareados.py` `6f58da7fe45db283`, `mutantes_fase_d.py` `940b361dba9aae97`. Os vereditos da fumaça foram
  recalculados com a análise final e saíram byte a byte iguais.

## Como rodar

```bash
S=~/projects/touring/docs/plans/2026-10-08-vgp-motor-base/diagnostics/int97-integrado/scripts
hw-slot --class rust --wait 7200 -- bash $S/pareado_corpus.sh <ref 8fd0efb0> <novo> <corpus> <pasta>      # fase D
bash $S/pareado_corpus.sh <ref> <novo> <corpus> <pasta> c                                                    # C′ como antes
JUIZ_EXTRA='--arg-motor=--camadas --arg-motor=1,2 --p95-ms 1000' bash $S/pareado_corpus.sh …                 # V3
hw-slot -- python3 $S/mutantes_fase_d.py --jobs 2 --json mutantes.json                                       # 47 mutantes
```
