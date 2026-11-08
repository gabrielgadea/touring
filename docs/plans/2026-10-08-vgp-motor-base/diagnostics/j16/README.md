# J16 — 30.4.103 no held-out 20261101c

## Description / Descrição

Julgamento do Touring 30.4.103 (`d8fcbbb5…`, candidato) e do controle 30.4.102 (`404106cc…`) no held-out selado `20261101c` (sha_tree `b5a819f38c57d227`), cujo estado no instante congela os módulos locais pelo Git vigente no `ts` (`8d166db3`). Os runs frios preservam recall, falsos denies, comparativo V3×v2 e timeouts; o critério de latência é o p95 do daemon quente (`../../PREREGISTRO-V3-QUENTE-2026-10-31.md`). O critério é o do J15: `consolidar_j16.py` delega a `consolidar_j15.avaliar`.

## Install / Instalação

Nada a instalar. Exige os binários congelados em `~/.claude/touring/vgp/binarios/{30.4.103,30.4.102}/`, o daemon global ativo (o runner recusa se o daemon vivo não for o do 30.4.103), Python 3 e `scripts/hw-slot`. O runner não instala, atualiza nem reinicia o daemon.

## Usage / Uso

```bash
bash rodar_j16.sh        # J16_SCRATCH=<dir> opcional: onde mora o híbrido (default: $TMPDIR/vgp-j16)
python3 -I consolidar_j16.py . --candidato 30_4_103 --saida aceite.json
```

O runner recusa reutilizar o diretório de saída, confere o selo antes e depois e preserva os rc individuais em `rcs.txt`. Um rc frio devido somente ao p95 não decide o aceite.

## Contributing / Contribuição

Não alterar corte, semente, adjudicador ou critérios depois da medição. Nenhum conteúdo de corpus pode ser copiado para este diretório ou commitado: somente scripts, metadados, traces e números.

## Tests / Testes

`python3 -I -m pytest test_consolidar_j16.py ../j15/test_consolidar_j15.py -q`: o aceite do J16 não reescreve o critério e trata entrada ausente ou ilegível como INCONCLUSIVO.

## License / Licença

Sem licença adicional própria para estes artefatos; seguem as condições do repositório Touring.
