---
type: PreRegistration
title: "Estado no instante — fecho de dependências locais pelo Git"
description: "Emenda ao critério de reconstrução dos held-outs VGP, anterior ao próximo corpus; não reinterpreta J14/J15."
plan_id: 2026-10-08-vgp-motor-base
subtask: task_1793544410603554012
okf_version: 1
timestamp: 2026-11-01
---

# EMENDA — DEPENDÊNCIAS LOCAIS NO INSTANTE (01/11/2026)

> Escrita antes da construção do próximo held-out. Os documentos anteriores permanecem
> inalterados. Nenhum veredito já emitido é recalculado por esta emenda.

## 1. Regra de reconstrução

Para cada entrada, o construtor deve congelar **todo módulo Python local resolvido dentro do
repositório do caso e alcançável pelos imports**, em qualquer profundidade, pela versão do Git
vigente no `ts` da entrada, seguida das edições da sessão até esse instante quando existirem.
A resolução deve cobrir também layouts `src/`, `python/` e monorepos `packages/*/{src,python}`,
não apenas a raiz e o diretório do script.

Se uma dependência local não for reconstruível — repositório sem Git, arquivo fora do histórico,
`ts` ausente/ambíguo ou resolução ambígua — a entrada será **INCONCLUSIVO-RECONSTRUÇÃO**:
contada e relatada, fora do denominador do juiz, nunca substituída silenciosamente pela árvore atual.

Pacotes de terceiros (`site-packages`, extensões nativas e seus stubs) ficam fora desta regra.
Essa é uma limitação declarada: o congelamento cobre módulos locais versionados, não todo o
ambiente de execução histórico.

## 2. TDD mínimo

Antes da correção, testes sintéticos devem falhar e depois passar para:

1. dependência que perdeu um símbolo depois do `ts`: o overlay contém a versão antiga, com o símbolo;
2. dependência que ganhou um símbolo depois do `ts`: o overlay contém a versão antiga, sem o símbolo;
3. dependência não reconstruível: a entrada fica INCONCLUSIVO-RECONSTRUÇÃO, sem overlay atual;
4. controle sem dependência local: comportamento e overlay permanecem vazios, como hoje.

## 3. Diagnóstico retroativo 20261101 — só medição

População: as 600 entradas de arquivo + 400 inline do held-out `20261101`, sem excluir o gasto
anterior e sem criar novo veredito de release.

Medidas, sempre com denominadores explícitos:

- dependências locais identificadas e quantas diferem entre o `ts` e a árvore atual;
- entradas com ao menos uma dependência local alterada;
- entradas INCONCLUSIVO-RECONSTRUÇÃO e seus motivos;
- entre as entradas reconstruíveis com dependência alterada, quantas apresentam mudança de
  **veredito efetivo do motor** ao comparar a avaliação com a dependência atual contra a avaliação
  com os overlays históricos (`deny` × não-`deny`, com `unknown` reportado separadamente).

“Mudança de veredito efetivo” é uma medida de sensibilidade da reconstrução, **não** uma
reclassificação automática de verdade do rótulo nem um novo aceite. O resultado dimensiona o viés
potencial de J14/J15 e não reescreve seus vereditos.

## 4. Próximo held-out

Somente depois dos testes e do diagnóstico, construir e selar um held-out novo com o construtor
corrigido. Ele ficará **sem julgamento até o J16**, após o release previsto para 04/11/2026.
Nenhum conteúdo do corpus será copiado ao repositório: somente código, testes, manifestos, hashes
e agregados numéricos.
