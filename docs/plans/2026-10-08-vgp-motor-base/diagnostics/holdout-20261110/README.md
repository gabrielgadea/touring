# Held-out 20261110 — J18, dependências locais no instante

## Description / Descrição

Corpus novo para o J18, semente `20261110`, construído com o reconstrutor de `8d166db3`: módulos locais são congelados pelo Git vigente no `ts`, incluindo `src/`, `python/` e `packages/*/{src,python}`. Entradas não reconstruíveis ficam fora do denominador. Os quatro corpus imediatamente anteriores entram na exclusão; o corpus fica fora do repositório.

## Install / Instalação

Não instala pacotes nem altera daemon. Usa o ambiente Touring existente, Git e `hw-slot`.

## Usage / Uso

```bash
bash rodar_construtor_20261110.sh
bash selar_e_cruzar_20261110.sh
```

O primeiro comando constrói sem julgar. O segundo exige 600 arquivos e inline, valida, sela e cruza contra todos os corpus anteriores, também sem julgar.

## Contributing / Contribuição

Não alterar semente, corte, resolvedor ou critérios depois da construção. Não copiar conteúdo do corpus para este diretório.

## Tests / Testes

- `scripts/test_estado_no_instante_v0d.py`: 46 testes, incluindo os quatro casos Git sintéticos.
- `diagnostics/instrumentos-2026-10-09/test_construir_disjunto_ho13.py`: suíte do seletor.
- `mutantes_estado_no_instante.py`: 23/23 mutantes mortos.
- `scripts/confere_selo.py --selftest` e o cruzamento com controle positivo.

## License / Licença

Sem licença adicional própria para estes metadados; seguem as condições do repositório Touring.
