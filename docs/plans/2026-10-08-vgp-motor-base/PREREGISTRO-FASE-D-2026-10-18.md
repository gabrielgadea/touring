# Pré-registro — fase D do portão de latência do VGP (emenda ao C′, 18/10/2026)

Escrito **antes** de qualquer implementação ou resultado da fase D. Emenda o
[PREREGISTRO-P95-PAREADO-2026-10-17.md](PREREGISTRO-P95-PAREADO-2026-10-17.md) (sha `3e40aa8e9efddf5e`), que manda para
a fase D quando o A/A do C′ reprova.

## Por que a fase D

- O A/A do C′ no corpus `ev` reprovou: R_p95 com IC95 [0,7142; 1,0807], fora de [0,95; 1,05]. O R_típico ficou estreito,
  [0,9915; 1,0049], com 738 ids e o mesmo binário (`8fd0efb0`) nos dois lados
  (`diagnostics/int97-integrado/resultados-2026-10-17b/pareado_ev/veredito_aa.json`). O tempo de parede da cauda mede a
  máquina, que é compartilhada por 6 ou mais sessões e roda a 78–96 °C.
- Forma aprovada pelo Gabriel em 18/10 («recomendações aprovadas», canvas da sessão): **forma B**, com instruções
  retiradas como portão, CPU relatada e parede relatada.

## O que se mede, por chamada e para os dois binários

A medida é feita **de fora**: o motor e a referência fixa `8fd0efb0` não mudam. Um lançador (`conta_custo.py`) faz fork
do motor e abre contadores sobre o pid do filho antes do `exec` (`enable_on_exec`, `inherit`), espera com `wait4` e grava:

1. **Instruções retiradas em espaço de usuário**: soma das PMUs híbridas `cpu_core` (tipo 4) e `cpu_atom` (tipo 10), com
   `PERF_TYPE_HARDWARE` e `config = PERF_COUNT_HW_INSTRUCTIONS | (tipo << 32)`, `exclude_kernel`, `exclude_hv`,
   `inherit`. Conta o motor e os descendentes: o `pyright-langserver` é filho do motor e é encerrado e esperado no drop
   (`touring-pyright/src/client.rs:594-610`).
2. **CPU da árvore** (`ru_utime + ru_stime` do `wait4`, que inclui os descendentes esperados), em ms.
3. **Parede** (relatada, como no C′).

Ordem ABBABAAB por id, A/A antes do A/B na mesma reserva rust, referência FIXA `8fd0efb0`, bootstrap de 2.000
reamostras por id com semente **20261018**. Tudo o mais segue o C′.

## Regras de decisão

| Situação | Regra | Saída |
|---|---|---|
| Medida ausente | contador indisponível, ou leitura falha em > 1% das chamadas | INCONCLUSIVO, exit 3 (ausência nunca vira zero) |
| A/A nas instruções | IC95 de R_típico **e** de R_p95 ⊂ [0,99; 1,01] | régua = instruções |
| A/A nas instruções reprova | IC95 de R_típico **e** de R_p95 da CPU ⊂ [0,95; 1,05] | régua = CPU |
| As duas reprovam | — | PROTOCOLO_INADEQUADO, exit 2; o A/B não é lido |
| A/B (portão) | limite superior do IC95 de R_típico **e** de R_p95 na régua escolhida ≤ 1,10 | SEM_REGRESSAO; senão REGRESSAO, exit 1 |
| Absoluto | p95 da CPU por chamada do binário B ≤ 150 ms no juiz v2 (camada 1) | portão; no V3 (camada 2) só relatado, que tem teto próprio de 1 s |

A régua é escolhida pelo A/A **antes** de olhar o A/B. Não se troca a régua depois do resultado do A/B.

## O que este pré-registro não cobre

- Instruções não são milissegundos: protegem contra regressão relativa. O absoluto fica na CPU.
- O JIT e o GC do Node, dentro do pyright, tornam as instruções da camada 2 menos determinísticas que as da camada 1.
  O A/A mede isso; não há previsão numérica.
- A latência sentida pelo usuário em uso real continua sendo a parede, e segue relatada.
