# J14 — julgamento VGP e daemon quente

## Description / Descrição

Evidências do julgamento do Touring 30.4.101 (`a6b9a550`) e da referência `8fd0efb0` no held-out selado 20261031b, mais a comparação de latência entre CLI fria e daemon quente. O resultado e os limites estão em `RELATORIO.md`.

## Install / Instalação

Não há pacote a instalar. Exige o Touring 30.4.101, o daemon global ativo, Python 3 e `pytest`. Os binários congelados ficam em `~/.claude/touring/vgp/binarios/`; o corpus permanece fora do repositório.

## Usage / Uso

```bash
bash rodar_j14.sh
python3 -I medir_daemon_quente.py --help
```

O runner confere o selo antes e depois. A consolidação do reparo documentado usa `consolidar_reparo_quente.py`. Nenhum conteúdo de corpus deve ser copiado para este diretório.

## Contributing / Contribuição

Preserve o corte, a semente, o adjudicador e os critérios pré-registrados. Novas medições exigem held-out novo e selado. Commits devem conter apenas metadados, traces, scripts e números, nunca código do corpus.

## Tests / Testes

```bash
python3 -I -m pytest -q test_medir_daemon_quente.py test_consolidar_reparo_quente.py
```

A fumaça E2E está em `smoke_hot.*`; o aceite quente consolidado está em `hot_reparado.*`.

## License / Licença

Sem licença adicional própria para estes artefatos; seguem as condições do repositório Touring.
