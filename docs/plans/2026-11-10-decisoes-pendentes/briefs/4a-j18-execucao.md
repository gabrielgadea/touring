# Brief 4a — execução do J18 (para a primeira sessão irmã que terminar seu brief atual)

Decisão do Gabriel (10/11/2026): julgar agora só o binário em produção. O pré-registro já está escrito e é a
especificação inteira: `docs/plans/2026-10-08-vgp-motor-base/PREREGISTRO-J18-2026-11-10.md`. Leia-o inteiro; nada
nele pode ser mudado depois da primeira medição.

## Passos
1. `diagnostics/holdout-20261110/`: copiar o padrão de `diagnostics/holdout-20261101c/` (README, `congelados.json`
   gravado ANTES, `rodar_construtor_20261110.sh` com semente 20261110 e os quatro `--excluir-corpus`, `selar_e_cruzar`).
   Construir, validar, selar, cruzar. Não julgar nada nesta etapa.
2. Congelar o candidato: `~/.claude/touring/vgp/binarios/30.4.108/touring` = cópia byte-idêntica da toolchain.
3. `diagnostics/j18/`: `rodar_j18.sh` derivado de `j17/rodar_j17.sh` com SÓ o que o pré-registro §4 muda (v2
   intercalada; V3 em ABBA com saídas `_v3a`/`_v3b`; aquecimento descartado até rota daemon, ≤ 5 tentativas) e
   `consolidar_j18.py` delegando a `consolidar_j15.avaliar` + as listas de regressão e de efeito do §5. Testes do
   consolidador no padrão de `test_consolidar_j17.py` (entrada ausente = INCONCLUSIVO; regressão única = NÃO PRONTO).
4. Rodar sob `hw-slot` (o runner já usa). Nada de `cargo build --release`, `update-touring`, daemon.
5. `diagnostics/j18/RELATORIO.md` no formato do J17.

## Arquivos exclusivos
`diagnostics/holdout-20261110/`, `diagnostics/j18/`, `~/.claude/touring/vgp/corpus_holdout_disjunto_20261110/`,
`~/.claude/touring/vgp/binarios/30.4.108/`. Nenhum conteúdo de corpus no repositório.

## Aceite (o juiz da touring-b4 confere)
- Selo antes e depois rc 0; `congelados.json` com timestamp anterior ao log da construção.
- `rcs.txt` completo até `FIM`; `aceite.json` com o veredito de `consolidar_j15.avaliar`.
- Testes do consolidador verdes (saída colada).
- Um relatório único via SendMessage para `touring-b4`.
