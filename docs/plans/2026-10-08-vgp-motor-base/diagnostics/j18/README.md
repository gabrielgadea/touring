# J18 — 30.4.108 em produção no held-out 20261110

## Description / Descrição

Julgamento do Touring 30.4.108 contra o controle 30.4.104 no held-out novo `20261110`, conforme `PREREGISTRO-J18-2026-11-10.md`. A v2 é intercalada; a V3 usa ABBA (`v3a` decide, `v3b` separa timeouts de máquina); o quente descarta o pré-aquecimento até rota daemon e repara pares standalone preservando os originais.

## Install / Instalação

Nada a instalar. Exige os binários congelados em `~/.claude/touring/vgp/binarios/{30.4.108,30.4.104}/`, daemon global idêntico à toolchain 30.4.108, Python 3 e `scripts/hw-slot`. Nenhum build, update ou restart de daemon durante o run.

## Usage / Uso

```bash
bash rodar_j18.sh
python3 -I consolidar_j18.py . --candidato 30_4_108 --controle 30_4_104 --saida aceite.json
```

O runner recusa reutilizar `rcs.txt`, confere o selo antes e depois e preserva todos os rc. O consolidador delega o critério a `consolidar_j15.avaliar`, acrescenta a regressão candidato×controle e reporta o efeito da correção sem usá-lo como gate.

## Contributing / Contribuição

O pré-registro é imutável depois da primeira medição. Nenhum conteúdo de corpus pode ser copiado para este diretório ou commitado: somente scripts, metadados, traces e números.

## Tests / Testes

```bash
python3 -I -m pytest test_consolidar_j18.py test_medir_quente_j18.py ../j15/test_consolidar_j15.py -q
```

Entrada ausente ou ilegível é INCONCLUSIVO; uma regressão única é NÃO PRONTO; `v3b` não altera o veredito.

## License / Licença

Sem licença adicional própria para estes artefatos; seguem as condições do repositório Touring.
