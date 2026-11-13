# O que a cobertura do `index` no `touring e2e` deve contar

Task: task_1794549390311758325
Subtask: S-01

## Medido em 13/11/2026 (geração 49 do índice, árvore da frente)

O componente `index` do `touring e2e -j` deu 0,882. A cobertura é `file_count / disk_files`, e o denominador vem
de um caminhador próprio (`crates/touring-cli/src/cli_e2e.rs`: `count_code_files`, `collect_files_recursive`,
`is_code_extension`), que não consulta a regra de admissão do índice. Dos 12.406 arquivos que ele conta:

| Parcela | Arquivos | O índice deveria tê-los? |
|---|---|---|
| indexados | 8.582 | sim |
| elegíveis sem símbolo | 2.580 (1.399 `.json`) | admitidos, mas sem linha de símbolo |
| ignorados pelo git | 724 | não: o índice respeita o `.gitignore` |
| `client/` | 520 | não: espelho gerado, fora por configuração |

Ou seja, 1.244 arquivos entram no denominador e nunca poderão entrar no numerador.

## A decisão

1. Denominador pela regra de admissão do índice (`IndexPolicy`), a mesma fonte do numerador. O número passa a medir
   "do que o índice aceita, quanto ele tem".
2. Manter o caminhador próprio e aceitar o teto abaixo de 1,0.

A linha de base do `CLAUDE.md` do projeto (regra 3: `index` 0,982 em 04/10/2026) foi medida com outra composição
de árvore; qualquer mudança de definição pede remedir a mediana de cinco execuções.

## Comments
