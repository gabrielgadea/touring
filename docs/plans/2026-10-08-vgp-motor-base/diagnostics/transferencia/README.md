# Relatórios de transferência — sessões do analise → touring-09 (09/10/2026)

Ordem do Gabriel: «solicite um relatório completo do estado atual do trabalho para as demais sessões do analise, para
que elas enviem tudo para a sessão coordenadora do touring». Cada sessão manda o relatório por mensagem à
**touring-09** e grava o mesmo texto aqui. Roteiro comum (8 seções; seção vazia = «nada», nunca omitida): identidade
e mandato · feito com evidência (caminho, sha, comando, exit code, denominador) · estado por item com o próximo
comando · processos em curso · dados gerados · decisões/achados/riscos · git dos próprios arquivos · o não verificado.

| sessão | modelo | relatório | como foi pedido |
|---|---|---|---|
| analise-89 | Anthropic (coordenadora anterior) | [../../HANDOFF-touring-2026-10-09.md](../../HANDOFF-touring-2026-10-09.md) | é o próprio HANDOFF |
| analise-0e | Kimi K3 1M | `analise-0e-RELATORIO.md` | mensagem direta (~21:45Z) |
| analise-68 | MiniMax M3.1-Flash-Preview | `analise-68-RELATORIO.md` | mensagem direta (~21:45Z) |
| analise-25 | MiniMax M3.1-Flash-Preview | `analise-25-RELATORIO.md` | **por repasse da analise-68** — mensagens diretas da analise-89 falharam («Failed to send», sessão em comando de shell longo) |
| subagente `shell-inline` (da analise-89) | sonnet | `../holdout/RELATORIO.md` | held-outs temporal e disjunto |

Ausência de um arquivo desta tabela = relatório não entregue (nunca «nada a relatar»).

**Situação às 21:40Z (18:40 BRT):**
- analise-68: **entregue** (mensagem + arquivo). Na MENSAGEM, a linha das premissas do §6 saiu corrompida
  («ORIGENS_ARQUIZA_ARQUIVO…»); o ARQUIVO está correto (`ORIGENS_ARQUIVO`, linha 67 do juiz, contém `"semeado"`).
  Resumo dela: typescript e tsx completos e auditados (80 e 120 positivos, 20 por classe, 100 % `simbolo` no juiz
  b7f7fd9b); html parcial (50 positivos, 3 classes abaixo do alvo, sem negativos); javascript 0 (nenhum subdiretório
  com linha de base limpa sob `checkJs` — o gerador recusou o oráculo em vez de semear sem confirmação);
  determinismo NÃO medido (declarado, com o comando).
- analise-25: **SUSPENSA (Ctrl+Z), não presa em shell** — diagnóstico por `/proc` às 21:40Z: `claude` PID 30301 em
  estado `T (stopped)` no terminal integrado do Zed (`pts/1`, bash 15371 → zed-editor 14331), com pyright e MCPs
  também parados; os 2 «shells» dela são zumbis (comandos já terminados, não recolhidos). A analise-68 foi aberta no
  MESMO terminal às 17:54 BRT (primeiro plano, `Sl+`) e continuou o `corpus_semeado.py` (gravado às 17:57) — por isso
  as mensagens falham e a sessão não aparece. O estado em DISCO do trabalho dela está coberto pelo relatório da 68; só
  o que estava apenas na conversa da 25 depende de reativá-la: no terminal da 68, depois de encerrá-la, `jobs` → `fg`.
- analise-0e: **entregue** (mensagem + arquivo, seções 1–8, com as 3 correções do RELATORIO-V1V2V5); nada aberto do lado dela.
- held-out temporal (subagente): e935 **0 falsos** em 77 arquivo + 189 inline, sem sobreposição com o ajuste;
  controle: o binário antigo c8b503c4 dá **15 falsos** no mesmo corpus (o holdout discrimina). Limite superior 95%
  ~3,9 % (arquivo) — não prova ≤ 1 %; o DISJUNTO (600 + 600) deu 5 e 2 falsos no e935 — limite superior 1,74% e 1,05%, «≤ 1%» NÃO provado; 3 são defeito real do motor. Relatório: `../holdout/RELATORIO.md` (entregue 21:55Z). O juiz recusa corpus sem
  positivos («corpus degenerado») — o subagente julgou um híbrido (negativos do holdout + 65 positivos do vivo, só
  como controle de detecção).

