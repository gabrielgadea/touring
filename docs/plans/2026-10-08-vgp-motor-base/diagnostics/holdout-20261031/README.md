# Held-out 20261031b — construção, selo e disjunção

## Description / Descrição

Instrumentos e evidências da tentativa abortada 20261031 e do held-out corrigido 20261031b. Inclui a correção da proveniência de timestamps, o viés medido, o selo `e8d8016f45bff2c5` e as sobreposições zero nas quatro chaves.

## Install / Instalação

Não há pacote a instalar. Exige Python 3, `pytest`, os scripts do bundle VGP e `scripts/hw-slot`. O corpus e seus snapshots permanecem em `~/.claude/touring/vgp/`, fora do git.

## Usage / Uso

```bash
bash rodar_construtor_20261031b.sh
bash selar_e_cruzar_20261031b.sh
```

O parcial 20261031 é histórico e não deve ser selado. A construção válida usa destino `corpus_holdout_disjunto_20261031b`, semente 20261031 e o corte de 2026-10-09.

## Contributing / Contribuição

Preserve o adjudicador congelado e a regra de parada por balde. Correções de instrumento exigem RED→GREEN e mutante morto por asserção. Não copie conteúdo de transcripts ou de corpus para o repositório público.

## Tests / Testes

```bash
python3 -I -m pytest -q test_ts_inline.py test_ts_arquivo.py test_mutantes_ts_ausente.py test_rastrear_ts_casos.py
python3 -I -B mutantes_ts_ausente.py
```

Os logs `tdd_*`, `mutante_*` e `regressao_*` registram as evidências. `RELATORIO-CONSERTO-TS.md` documenta a causa; `RELATORIO-HELDOUT-20261031B.md` documenta selo e disjunção.

## License / Licença

Sem licença adicional própria para estes artefatos; seguem as condições do repositório Touring.
