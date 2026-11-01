# Instrumentos da medição (reprodutíveis)

Cada número do documento sai de um destes. Rode via code mode — é a forma que o
próprio documento recomenda:

```bash
touring run --lang python --file evidence/diag_tools.py --timeout-ms 300000 | jq -r .stdout
touring run --lang python --file evidence/diag_gate.py  --timeout-ms 300000 | jq -r .stdout
touring run --lang python --file evidence/diag_mem.py   --timeout-ms 300000 | jq -r .stdout
touring run --lang python --file evidence/medir.py --args '["<transcript.jsonl>"]' | jq -r .stdout
```

| script | responde |
| --- | --- |
| `diag_tools.py` | frequência por tool, bigramas, rajadas, redundância, antipadrões estruturais |
| `diag_gate.py` | taxonomia dos comandos Bash, distribuição do tamanho da rajada, conformidade P9 |
| `diag_mem.py` | classificação do corpus de memórias por modo de falha e prevenibilidade |
| `medir.py` | adoção de code mode e exit-code-através-de-pipe numa sessão |

**Limites declarados**: `diag_mem.py` é lexical e conservador (~0.7 de confiança;
21% do corpus não casa nenhum padrão e fica de fora). Os contadores de tool call
são leitura direta do transcript — fato (1.0). Nenhum deles escreve.

## Rodada 2 (24/08 noite)

| script | responde |
| --- | --- |
| `diag_motifs.py` | trigramas/quadrigramas de workflow + re-inspeção do mesmo alvo |
| `diag_erros.py` | erros reais (`tool_result is_error`) classificados + comportamento pós-erro |
| `diag_gate_sim.py` | simulação G1–G6 contra as 55 sessões + proxy de precisão do G1 |
