# Held-out 20261101 — construção e selo, sem julgamento

## Description / Descrição

Corpus novo e independente após o J14. Inclui o parcial 20261031 e o corpus 20261031b nas exclusões, além de todos os held-outs anteriores. O J15 só será executado após o aviso do release 30.4.102.

## Install / Instalação

Exige Python 3, os instrumentos do bundle VGP e `scripts/hw-slot`. O corpus permanece em `~/.claude/touring/vgp/`, fora do git.

## Usage / Uso

```bash
bash rodar_construtor_20261101.sh
bash selar_e_cruzar_20261101.sh
```

O corte e o adjudicador são os mesmos dos corpora anteriores; a semente é 20261101. Não execute o J15 neste diretório antes do aviso da coordenação.

## Contributing / Contribuição

Preserve a regra de parada por balde, o fecho transitivo e todos os `--excluir-corpus`. Commits apenas de metadados, scripts, selo e números. Nunca copie conteúdo de corpus ou transcripts.

## Tests / Testes

A validação pré-selo exige `arquivo ≥ 300`, `inline ≥ 300`, zero `ts=""` e zero entradas posteriores ao corte. O selo é conferido antes de qualquer futuro julgamento; as sobreposições exigem zero com controle positivo.

## License / Licença

Sem licença adicional própria; seguem as condições do repositório Touring.
