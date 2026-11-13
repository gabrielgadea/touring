# Pendências do aprofundamento do touring-code e do cross-audit de 12–13/11/2026

Task: task_1794549390311758325

Origem: `docs/plans/2026-11-12-touring-code-aprofundamento/` (DAG `task_1794458342745990025`) e
`docs/audits/cross-audit-2026-11-12.md`. Aqui ficam os achados que a frente mediu e **não** corrigiu, e as
decisões que são do Gabriel. Nada aqui bloqueia o juiz de convergência da frente de origem.

## O que entra

| Ticket | Tipo | Assunto |
|---|---|---|
| S-01 | decisão | o que a cobertura do `index` no `touring e2e` deve contar |
| S-02 | implementação | três smells da revisão de padrões que ficaram sem dono |
| S-03 | implementação | ramo de reentrada do cache de parsers sem chamador de produção |
| S-04 | implementação | classe de evidência na escrita do wiring (palpite por nome × referência exata) |
| D-05 | decisão | commit, deploy e ativação do C8 (lista no relatório do cross-audit) |

## Fora

Refazer qualquer candidato C1–C8 e instalar binário novo (deploy é decisão do ticket D-05).
