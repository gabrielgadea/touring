# Commit, deploy e ativação da frente de aprofundamento do touring-code

Task: task_1794549390311758325
Subtask: D-05

Decisão do Gabriel. A lista completa e atual vive em `docs/audits/cross-audit-2026-11-12.md`, seção "Decisões
abertas com Gabriel"; este ticket existe para a decisão aparecer na fronteira do DAG.

## O que está pronto

A árvore de trabalho da branch `feat/2026-09-16-shell-quality-wiring-python` carrega C1–C8 e F1–F16, sem commit
e sem instalação. Gates do workspace medidos pelo lead; relatório e evidência nos caminhos do relatório acima.
Snapshot de segurança em branch `safety/` (ver `RETOMADA.md` da pasta do plano de origem).

## O que depende da decisão

1. **Forma do commit.** Os hunks dos candidatos se entrelaçam em arquivos compartilhados
   (`ast/graph/mod.rs`, `knowledge_wiring.rs`, `handlers/index.rs`, `GLOSSARY.md`). Opções: um commit por
   candidato onde separa e um commit conjunto para o resto; ou poucos commits temáticos (parse, índice e
   arestas, sinais, correções do cross-audit).
2. **Deploy.** Bump de versão + `scripts/propagate-release.sh <versão>` + `touring index rebuild`. Só depois
   disso o juiz de convergência da frente fecha: a cláusula `orphans_base` lê o índice escrito pelo binário
   instalado, e os quatro órfãos falsos que ela acusa só somem com o detector novo (F14, F15).
3. **C8 no primeiro deploy.** Ligado por padrão, como construído e já com a regra de aresta do F13, ou
   `TOURING_FILE_EDGES_DISABLED=1` até observar o custo no daemon.

## Answer

## Comments
