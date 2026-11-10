# J18 — relatório (10/11/2026)

Veredito: **INCONCLUSIVO por corpus**, conforme `PREREGISTRO-J18-2026-11-10.md` §3. Nenhuma medição de binário foi executada. O universo construído fechou **503/600 arquivos + 229 inline**; o pré-registro proíbe julgar um corpus menor.

## Construção e Adendo 1

A primeira construção abortou em `replay.py:48` (`JSONDecodeError`), antes de qualquer corpus ou medição. O **Adendo 1** (`ee897dce`) corrigiu `replay._records` para registrar e pular linhas malformadas, sem alterar corte, semente, braços, juiz ou critério. Os artefatos anteriores foram preservados como `congelados.json.antes-adendo-1`, `construir_20261110.log.antes-adendo-1` e `construtor_rc.txt.antes-adendo-1`.

O novo `congelados.json`, com 14 hashes incluindo o novo `replay.py`, foi gravado em **2026-11-10T22:00:13.195076+00:00**, antes do novo log da construção (19:00:26–20:34:22, horário local). O construtor terminou com rc 0 e registrou exatamente as 3 linhas previstas:

- `analise/5ff8eadd…jsonl:22504` — `Expecting ',' delimiter`, char 1676;
- `analise/ad6c086f…jsonl:41191` — `Expecting ',' delimiter`, char 57417;
- `touring/0fbe5042…jsonl:11847` — `Invalid \uXXXX escape`, char 3379.

Elas aparecem duas vezes no log porque o replay é usado em duas fases do construtor; não houve outro abort.

## Validação do corpus

`validacao_pre_selo.json`:

```json
{
  "arquivo": 503,
  "inline": 229,
  "sem_ts": 0,
  "depois_do_corte": 0,
  "inconclusivo_reconstrucao_no_corpus": 0,
  "corte": "2026-10-09T18:14:31.894Z",
  "ts_ausente_no_universo_arquivo": 2940,
  "overlays": 1667
}
```

A validação do denominador terminou com rc 1 (`503 != 600`). O cruzamento de disjunção, que não julga binário, terminou com rc 0: `OK_zero_em_todos_os_cruzamentos=true` e `controle_positivo_enxerga=true`.

## O que não foi executado, deliberadamente

- `selo.json`: não criado, pois o denominador falhou antes do selo;
- congelamento do 30.4.108: etapa posterior ao selo, não alcançada;
- `rodar_j18.sh`: não executado, pois mediria um corpus proibido pelo §3;
- `aceite.json`: não criado. `consolidar_j15.avaliar` exige V2/V3/hot inexistentes; produzir um veredito por outra via violaria a fonte única do critério.

`rcs.txt` registra essas etapas e termina em `FIM`. Nenhum daemon, release ou toolchain foi alterado.

## Testes dos instrumentos preparados

O runner ABBA, o consolidador e o instrumento quente foram preparados, mas não usados para medir. Teste executado:

```bash
scripts/hw-slot -- python3 -I -m pytest \
  diagnostics/j18/test_consolidar_j18.py \
  diagnostics/j18/test_medir_quente_j18.py \
  diagnostics/j15/test_consolidar_j15.py -q
```

Saída:

```text
........................                                                 [100%]
24 passed in 0.06s
```

A cobertura inclui entrada ausente/ilegível = INCONCLUSIVO, regressão única = NÃO PRONTO, `v3b` fora do veredito, pré-aquecimento com teto de 5 tentativas e reparo de par standalone com o original fora do denominador.

## Próximo passo

Abrir um held-out novo com pré-registro próprio e universo capaz de fechar 600 arquivos + inline. Não se deve reutilizar o 20261110 como evidência de prontidão nem preencher o déficit com entradas dos corpus excluídos.
