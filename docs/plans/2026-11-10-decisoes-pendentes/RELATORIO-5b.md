# RELATORIO-5b — D-8: camada html montada e selada

> Tarefa 5b (item 5 de `DECISOES.md`, continuação). Executor: `touring-c0` (MiniMax).
> Brief da coordenadora: `briefs/5-minimax-html.md` + a mensagem de 10/11.
> Relatório único desta tarefa. A lista de clones está em `LISTA-HTML-OSS.md` (commit `1fe1e0fb`).

## 1. Estado

**Verificado nesta sessão** (saídas dos comandos, não estimativas):

- **Camada montada e selada**: `~/.claude/touring/vgp/camada_html_oss_20261110/`
  (12.763 arquivos · 95.059.549 bytes · 116 MB em disco).
- **`situacao: "provada"`** — o veredito do próprio montador, pelo critério inteiro
  (n ≥ 299 negativos **e** dentro do teto medido).
- **n = 6.381 entradas** = **5.944 negativos** + **437 positivos** (`cdn_pin`).
  Meta do brief (≥ 3.700): **1,61×**.
- **Concentração dentro do teto**: máximo de **91** entradas num caminho e **122** num
  commit, contra o teto de **191** (3% de 6.381) → `dentro_do_teto: true`,
  `n_infratores: 0`, `podadas_por_teto: 0`.
- **Disjunção**: **828** entradas descartadas por id contra **11.277** ids já usados
  nos 40 corpora excluídos.
- **`descartadas_nao_utf8`: 3** — os 3 commits cujo html não decodifica em UTF-8
  foram **descartados e contados**, não abortaram a camada (fix `fcf4db48`, da
  coordenadora).
- **Selo conferido**: `confere_selo.py` → `selo INTACTO` (**rc 0**),
  `sha_tree a665624d48b91494`.
- **Nada de corpus no repositório** — a camada vive inteira em `~/.claude/touring/vgp/`
  (`git status` do workspace sem nenhum arquivo de corpus).
- **46 repos** minerados (9 de `oss/` + 37 de `oss-html/`), `--limite-commits 250`,
  **40 corpora** passados como `--excluir-corpus`.
- Disco livre ao final: **105 GB**.

**Em construção:** nada.

**Esperado (não medido aqui):** que o juiz, sobre esta camada, produza um veredito
de **falso deny** — esta tarefa monta e sela o insumo, **não julga**.

## 2. Capacidade

- **O que**: o estrato html do VGP tem, pela primeira vez, uma camada **provada**:
  5.944 negativos e 437 positivos, com concentração de 91/122 contra um teto de 191.
- **Para quem**: para o adjudicador html (`oraculo_instancia_git.py` + `cdn_pin.py`),
  que até 01/11 vivia com 642 negativos e concentração violada (29 contra teto de 19)
  — a razão declarada do «não provada».
- **Como é usado**: cada entrada carrega `snap` (o html em C) e `snap_antes` (o html
  em C^), o `project_root` (o clone), o `path`, o `commit`, o `rotulo` e o
  `nome_esperado`; o juiz lê a camada como corpus selado, e o selo é conferido por
  `confere_selo.py` (que recalcula o `sha_tree`).
- **Pra que serve**: para medir **falso deny** do motor html sobre html real — a
  classe que, até agora, nenhuma prova conseguia sustentar.

## 3. Significado para o projeto

- O D-8 era o **único** bloqueio restante da prova html, e ele era de **população**:
  `RELATORIO-V8-SEM2.md:373` — «a camada precisaria de **n ≥ 3 700**, e o acervo
  inteiro (7 clones) dá **1 704**». Com 5.944 negativos, o critério inteiro do veredito
  (`situacao_de()`: 299+ negativos **e** teto medido) passa com folga, e a
  concentração — que era natural do html (11,2% numa chave) — cai para 91 contra 191.
- `RETOMADA.md:212` fixou o alvo: «falta população html cerca de 4× maior (n ≥ 3.700
  contra 1.704 no acervo)». O acervo agora é **46 clones** e a camada tem **5.944**
  negativos: o alvo foi ultrapassado em 1,61×, com **zero podas** (nada foi
  descartado por concentração).
- Consome: o passo de julgamento (o juiz sobre esta camada). Depende de: nada novo —
  a camada está selada em disco, com o selo conferido.
- Registro de Coordinate: a integração que a Tarefa 5 apontava como pendente
  (o montador ler `oss` **e** `oss-html`) foi feita pela coordenadora em `f07a5d3a`,
  e o defeito do oráculo que ela corrigiu em `fcf4db48` foi **descoberto por medição
  desta sessão** (ver §6).

## 4. Depois da conclusão

**Passa a poder**
- Julgar o adjudicador html sobre uma camada com denominador suficiente — a prova que
  o D-8 existia para desbloquear.
- Rodar a camada com `--limite-commits 250` e `--teto-concentracao 0.03` reproduzindo
  os mesmos números (o selo congela o `sha_tree` e os denominadores).
- Minerarpopulation nova do mesmo jeito: clonar → medir `commits .html` → montar com
  `--excluir-corpus` → selar → conferir.

**Continua sem poder**
- **O veredito do juiz**: esta camada é insumo, não prova. O «provada» aqui é o
  veredito do **montador** sobre o **selo** (n e teto), não o do motor html sobre
  falsos denies.
- A camada **não** foi julgada, e nada aqui diz que o motor html está bom.
- `html5lib-python` entrou com 12 entradas (não 250): 3 commits do acervo são
  ilegíveis em UTF-8 e foram descartados pelo oráculo.

**Ainda falta**
1. **Julgar** a camada (o passo seguinte da coordenadora).
2. **Descartar o que sobrou** (guard proíbe `rm -rf`; é comando do Gabriel):
   - `~/.claude/touring/vgp/camada_html_oss_20261110.parcial-exclusao` (8,7 MB) —
     rodada abortada quando excludei html5lib antes do fix do oráculo;
   - `~/.claude/touring/vgp/oss-html/_stage/` (20 GB) — candidatos medidos e fora da
     lista; já levado ao Gabriel pela coordenadora.
   ```bash
   ! rm -r ~/.claude/touring/vgp/camada_html_oss_20261110.parcial-exclusao
   ! rm -r ~/.claude/touring/vgp/oss-html/_stage
   ```
3. **Commitar** este relatório (`RELATORIO-5b.md` está untracked; a última instrução
   sobre commit foi da coordenadora, que commita os docs depois de aceitar).

## 5. Como o código produz isso

| peça | papel |
|---|---|
| `oss-html/_tools/monta_camada.sh` | monta o comando: as 46 raizes de clone, os 40 `--excluir-corpus`, `TMPDIR` dentro de `~/.claude/touring/vgp/` (o `tmp_write_guard` nega `/tmp`, `~/.cache` e scratch) e o `--print-repos` testável |
| `oss-html/_tools/preflight_fast.py` | **pré-requisito**: reproduz a seleção do montador e lê os blobs como bytes para achar html ilegível **antes** de a camada abortar |
| `oss-html/_tools/licenca.py` | classifica a licença **lida** no LICENSE do repo (a lista da Tarefa 5 tinha a licença digitada à mão — 37 linhas que podem mentir) |
| `oss-html/_tools/conta_html_efetivo.py` | conta o html efetivo: descarta minificado (linha > 10.000 chars) e vendored |
| `oss-html/_tools/test_oss_html_tools.py` | **41 testes** (pytest): classificação de licença, filtro de minificado/vendored, o aceite do brief travado por teste, e o clone medido sem rede via `file://` |

**O comando efetivamente executado** (sob `hw-slot --class heavy`):

```
python3 -I docs/plans/2026-10-08-vgp-motor-base/scripts/monta_camada_html_oss.py \
  --saida ~/.claude/touring/vgp/camada_html_oss_20261110 \
  --selo ~/.claude/touring/vgp/camada_html_oss_20261110.selo.json \
  --json  ~/.claude/touring/vgp/camada_html_oss_20261110.denominadores.json \
  --limite-commits 250 --repos <46 nomes> \
  --excluir-corpus <40 caminhos>
```

## 6. Notas de honestidade

- **`descartadas_nao_utf8` = 3, não 1.** A expectativa da coordenadora era 1; o
  preflight desta sessão (que varre o lado **C** dos blobs) achou 1 arquivo
  ilegível — `html5lib-python:benchmarks/data/wpt/weighted/big5_chars_extra.html`,
  fixture Big5 do WPT. Os outros 2 são versões **C^** (o lado «antes») dos mesmos
  arquivos: o montador lê os dois lados, e o meu pré-flight v1 só varreu o lado C.
  O número do selo (3) é o do montador, e é o que vale.
- **A camada anterior não entra como population**: os 9 clones antigos entraram com
  pouco ou nada (dom-examples 0, date-fns 0, zod 0 — todos os ids já usados foram
  descartados pela disjunção; learning-area 224 e wpt 250 trouxeram commits novos).
  Os 5.944 negativos vêm essencialmente dos **37 clones novos**.
- **A rodada com exclusão foi abortada por mim** (não por falha): quando o pré-flight
  provou que o oráculo abortaria em html não-UTF-8, excluí `html5lib-python` da
  mineração em vez de editar o script de outro — e o parcial ficou em disco
  (8,7 MB, listado em §4). Com o fix `fcf4db48` da coordenadora, a rodada final rodou
  com os **46** repos.
- **O selo declara `podadas_por_teto: 0`**: nenhuma entrada foi descartada por
  concentração — a population cresceu sem que a concentração natural do html
  voltasse a violar o teto.
