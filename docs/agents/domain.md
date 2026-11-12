# Documentos de domínio

Como as skills de engenharia devem consumir a documentação de domínio deste repo ao
explorar o código.

## Layout: contexto único

```
/
├── GLOSSARY.md      ← glossário do domínio (em português; o código usa o equivalente
│                      em inglês indicado em cada termo)
└── docs/adr/        ← decisões de arquitetura, numeradas (0001-…, 0002-…)
```

Não há `GLOSSARY-MAP.md`: o workspace Cargo tem muitos crates, mas um só contexto de
domínio. `site/docs/adr/` é a saída gerada do site (ignorada pelo git) e os `adr/`
dentro de `docs/plans/<bundle>/` são decisões de escopo de um plano; nenhum dos dois é
a fonte das decisões do sistema. A fonte é `docs/adr/`.

## Antes de explorar, leia

- **`GLOSSARY.md`** na raiz.
- **`docs/adr/`**: os ADRs que tocam a área em que você vai trabalhar
  (`docs/adr/README.md` explica o formato, MADR).

Se algum desses arquivos não existir, **siga em silêncio**. Não acuse a ausência nem
sugira criá-los de antemão. A skill `/domain-modeling` (alcançada por `/grill-with-docs`
e `/improve-codebase-architecture`) os cria sob demanda, quando um termo ou decisão é
de fato resolvido.

## Use o vocabulário do glossário

Quando a sua saída nomear um conceito de domínio (título de ticket, proposta de
refactor, hipótese, nome de teste), use o termo como definido em `GLOSSARY.md`. Não
derive para sinônimos que o glossário evita.

Se o conceito de que você precisa ainda não está no glossário, isso é um sinal: ou você
está inventando linguagem que o projeto não usa (reconsidere), ou há uma lacuna real
(anote para `/domain-modeling`).

## Acuse conflito com ADR

Se a sua saída contradiz um ADR existente, diga isso explicitamente em vez de
sobrescrever em silêncio:

> _Contradiz o ADR-0005 (claim atômico de subtask), mas vale reabrir porque…_
