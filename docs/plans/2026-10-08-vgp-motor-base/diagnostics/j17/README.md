# J17 — 30.4.104 no held-out 20261101c

## Description / Descrição

Julgamento do Touring 30.4.104 (`2acce203…`, candidato) e do controle 30.4.103 (`d8fcbbb5…`) no held-out selado `20261101c` (sha_tree `b5a819f38c57d227`), com os módulos locais congelados pelo Git vigente no `ts` (`8d166db3`). É o J16 com candidato e controle trocados: o J16 rodou a fase fria (30.4.103 e 30.4.102 PRONTO, 0 falsos deny em 600 arquivos + 371 inline), mas a fase quente deixou de ser atribuível quando o release 30.4.104 substituiu o daemon da 30.4.103 em 08/11/2026 — a checagem de identidade do runner recusaria. Decisão do Gabriel (08/11/2026): abrir o J17 em vez de rebaixar o daemon global. Corte, semente, juiz e critério são os do J15/J16: `consolidar_j17.py` delega a `consolidar_j15.avaliar`; a latência é o p95 do daemon quente (`../../PREREGISTRO-V3-QUENTE-2026-10-31.md`).

## Install / Instalação

Nada a instalar. Exige os binários congelados em `~/.claude/touring/vgp/binarios/{30.4.104,30.4.103}/`, o daemon global ativo e idêntico ao `touring-daemon` da toolchain 30.4.104 (o runner recusa caso contrário), Python 3 e `scripts/hw-slot`. O runner não instala, atualiza nem reinicia o daemon. Nenhum `cargo build --release` no workspace durante o run: trocaria o binário sob o caminho que a checagem de identidade lê.

## Usage / Uso

```bash
bash rodar_j17.sh        # J17_SCRATCH=<dir> opcional: onde mora o híbrido (default: $TMPDIR/vgp-j17)
python3 -I consolidar_j17.py . --candidato 30_4_104 --saida aceite.json
```

O runner recusa reutilizar o diretório de saída, confere o selo antes e depois e preserva os rc individuais em `rcs.txt`. Um rc frio devido somente ao p95 não decide o aceite.

## Contributing / Contribuição

Não alterar corte, semente, adjudicador ou critérios depois da medição. Nenhum conteúdo de corpus pode ser copiado para este diretório ou commitado: somente scripts, metadados, traces e números.

## Tests / Testes

`python3 -I -m pytest test_consolidar_j17.py ../j15/test_consolidar_j15.py -q`: o aceite do J17 não reescreve o critério e trata entrada ausente ou ilegível como INCONCLUSIVO.

## License / Licença

Sem licença adicional própria para estes artefatos; seguem as condições do repositório Touring.
