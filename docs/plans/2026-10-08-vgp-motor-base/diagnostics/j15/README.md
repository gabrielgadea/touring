# J15 — 30.4.102 no held-out 20261101

## Description / Descrição

Julgamento do Touring 30.4.102 (`404106cc…`) e do controle `8fd0efb0` no held-out selado `6c228591f1f404f5`. Os runs frios preservam recall, falsos denies, comparativo V3×v2 e timeouts; o critério de latência é o p95 do daemon quente, conforme `../../PREREGISTRO-V3-QUENTE-2026-10-31.md`.

## Install / Instalação

Não há pacote a instalar. Exige os binários congelados em `~/.claude/touring/vgp/binarios/`, o daemon global já ativo, Python 3 e `hw-slot`. O runner não instala, atualiza nem reinicia o daemon.

## Usage / Uso

```bash
bash rodar_j15.sh
```

O runner recusa reutilizar o diretório de saída, confere o selo antes e depois e preserva os rc individuais em `rcs.txt`. Um rc frio devido somente ao p95 não decide o aceite; as demais pendências e a inconclusividade continuam válidas.

## Contributing / Contribuição

Não alterar corte, semente, adjudicador ou critérios depois da medição. Nenhum conteúdo de corpus pode ser copiado para este diretório ou commitado: somente scripts, metadados, traces e números.

## Tests / Testes

A consolidação do aceite deve ter testes com pendências frias de p95 isoladas e combinadas com falhas reais, além de sinal quente ausente e timeout acima do patamar.

## License / Licença

Sem licença adicional própria para estes artefatos; seguem as condições do repositório Touring.
