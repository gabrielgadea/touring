# RELATORIO-5 — D-8: população html do VGP (lista + clone)

> Tarefa 5 do canvas `DECISOES.md` (item 5: «D-8 HTML — A: aprovar a lista e clonar»).
> Executor: `touring-c0` (MiniMax). Brief: `briefs/5-minimax-html.md`.
> Relatório único desta tarefa. A lista completa está em `LISTA-HTML-OSS.md`.

## 1. Estado

**Verificado nesta sessão** (comandos e saídas medidos):

- **37 repos** clonados em `~/.claude/touring/vgp/oss-html/`, cada um com **sha fixo** e
  **licença lida no arquivo LICENSE do próprio repo** (37/37).
- **8.628 arquivos `.html` escritos à mão** novos (de 8.684 brutos; 51 minificados e
  5 vendored/não-lidos excluídos pelo filtro do brief).
- **5.614 de rendimento de mineração** novo — `Σ min(250, commits que tocam .html até o
  corte)` — que é a mesma unidade do acervo atual (1.704).
- **n = 1.704 + 5.614 = 7.318**, contra a meta de **≥ 3.700** (1,98×).
- Maior repo: **three.js, 19,8%** dos html (≤ 25% exigido); maior rendimento: **250 =
  4,5%**. Os dois tetos passam com folga.
- Disco do conjunto: **4,0 GiB** (`du -sh`, teto de 5 GB); disco livre ao final:
  **106 GB** (piso de 100 GB).
- 5 repos foram **medidos e excluídos** com motivo verificável (tabela na
  `LISTA-HTML-OSS.md` §4), entre eles `validator/validator` (5.798 html) por uma
  **impossibilidade aritmética** sob o teto de 25%, e `webpack/webpack` (2.946 html)
  que ficaria em **24,996%** — o teto exato, sem margem.

**Em construção:** nada. A tarefa está fechada.

**Esperado (a etapa seguinte é da sessão que integra):** que o montador, com
`--limite-commits 250`, extraia ≈ 7.318 entradas da camada antes da poda de
concentração. **Não foi medido** — `monta_camada_html_oss.py` não é arquivo meu e a
integração é o passo seguinte (brief, «Arquivos exclusivos seus»).

## 2. Capacidade

- **O que**: a camada html do acervo do VGP passa a ter **população 4,3× maior em
  arquivos** e **3,3× maior em rendimento de mineração**, vinda de 37 repositórios
  públicos com HTML escrito à mão, todos com licença permissiva e sha fixo.
- **Para quem**: para o adjudicador html (`oraculo_instancia_git.py` + `cdn_pin.py`),
  que hoje só consegue extrair 1.704 negativos do acervo inteiro — e que precisa de
  **n ≥ 3.700** para a concentração natural do html caber no teto de 3% por chave.
- **Como é usado**: o `monta_camada_html_oss.py` minerar os commits que tocam `.html`
  nos clones (cada commit = 1 entrada, com o `snap` em C e o `snap_antes` em C^) e a
  camada vira o corpus que o juiz consome.
- **Pra que serve**: para **provar** — pela primeira vez com um denominador
  suficiente — que o motor html do VGP **não produz falso deny** sobre html real, em
  vez de declarar «não provada» por falta de população.

## 3. Significado para o projeto

- O bloqueio era **população, não método**: `RELATORIO-V8-SEM2.md:373` diz que «o
  acervo inteiro (7 clones) dá **1 704**» e que a camada «precisaria de **n ≥ 3 700**»
  para caber no teto de 3%. Com 7.318 de entradas possíveis, o critério inteiro da
  prova fica ao alcance — e a concentração (o outro motivo do «não provada» em
  `RELATORIO-V8-SEM2.md:366`) se dilui por 37 repos de caminhos distintos.
- `RETOMADA.md:212` (2026-11-04) registrou o D-8 como «falta população html cerca de
  4× maior (n ≥ 3.700 contra 1.704 no acervo), clonando repositórios públicos» — esta
  tarefa entrega exatamente essa população, e o item 5 de `DECISOES.md` («Aprovar a
  lista e clonar») é a aprovação que a aguardava.
- Consome: o passo de integração da camada ao montador (próxima sessão). Depende de:
  nada novo — os clones já estão em disco, com histórico git completo o bastante para
  o montador ler C e C^ **sem rede**.

## 4. Depois da conclusão

**Passa a poder**
- Montar a camada html com ~7.318 entradas (antes da poda) e julgar o adjudicador html
  com denominador suficiente para o critério de 299 negativos + teto de 3%.
- Levar o VGP html de «não provada (falta população)» a uma prova com n real.
- Reusar a mesma receita (clone sem filtro + medição de `commits .html`) para qualquer
  estrato que precise de população.

**Continua sem poder**
- O montador **não enxerga ainda** os clones novos: `monta_camada_html_oss.py:52-53`
  fixou `OSS = BASE / "oss"`, e os clones estão em `BASE / "oss-html"` — a integração
  é o passo seguinte e **não** foi feita aqui (o script não é meu).
- A prova do juiz (n ≥ 299 negativos **e** teto medido) continua por conta própria: o
  que esta tarefa entrega é o **insumo**, não o veredito.
- `validator/validator` (5.798 html) e `webpack/webpack` (2.946 html) continuam fora
  do acervo clonado: o primeiro por impossibilidade aritmética sob o teto de 25%, o
  segundo por ficar no teto exato.

**Ainda falta**
1. **Integrar `oss-html/` ao montador** (sessão seguinte): iterar os dois diretórios
   (`oss/` e `oss-html/`) ou passar `--repos` com a lista completa.
2. **Rodar com `--limite-commits 250`**: com 100, o acervo antigo dá 540
   (`RELATORIO-V8-SEM2.md:349`) e os novos repos somam ≈ 2.946 → **3.486, abaixo de
   3.700**; com 250, são 1.704 + 5.614 = **7.318**. O limite é o que decide, não os
   arquivos.
3. **Conferir o selo** (`confere_selo.py`) e medir a concentração por chave — o
   motivo do «não provada» anterior.
4. **Descartar os 20 GB de candidatos medidos** (o guard de escrita proíbe `rm -rf`; é
   comando do Gabriel):
   ```bash
   ! rm -r ~/.claude/touring/vgp/oss-html/_stage
   ```

## 5. Como o código produz isso

Quatro ferramentas em `~/.claude/touring/vgp/oss-html/_tools/` (fora do workspace,
para não tocar em arquivo de outro):

| ferramenta | papel |
|---|---|
| `clona_e_mede.sh` | clona (`--depth 1500..3000`, **sem** `--filter`) e mede sha, html no HEAD, commits `.html` até o corte, bytes e o nome do LICENSE |
| `conta_html_efetivo.py` | conta o html **efetivo**: aplica os filtros do brief (minificado = linha > 10.000 chars; vendored = caminho) e devolve o total por repo |
| `verifica_selecao.sh` | move os escolhidos de `_stage/` para o destino, mede a contagem em disco, o maior `.html`, o `du` e **lê o LICENSE** de cada repo |
| `tabela_final.py` | consolida tudo na tabela da lista, com os totais e os shares |

**A decisão de método que valeu**: o `--filter=blob:none` foi descartado porque o
filtro server-side do GitHub virou gargalo (≈ 40 KB/s medido: 3 clones em paralelo
cresceram 1 MB em 25 s cada). Um pack normal com profundidade limitada baixa o clone
para **4–14 s** e ainda traz os blobs que o montador precisa — sem rede na mineração.

## 6. Notas de honestidade

- Nenhum número da lista é estimado: html, commits, sha, licença e disco saíram de
  script, e o script está no diretório `_tools/` ao lado dos clones.
- 6 repos foram **descartados por medição**, não por opinião — e os dois maiores
  candidatos (`validator`, `webpack`) estão fora **por causa** do teto de diversidade
  do brief, o que reduz o total disponível. Se o Gabriel quiser relaxar o teto,
  `validator/validator` está pronto para entrar: clone feito, sha
  `0f0f1f9eb6949e9044cfbfa4c45e55a6cd1a15d3`, 5.798 html, 172 MB, MPL-2.0 — ele
  sozinha levaria o acervo para **14.426** html, e o teto de 25% passaria a exigir
  Σ ≥ 23.192 (o acervo permissivo inteiro medido dá 21.022, então **ele só entraria
  junto com mais repos ainda**).
- A limpeza dos 20 GB de `_stage/` ficou pendente porque o guard de escrita bloqueia
  `rm -rf` e o sandbox bloqueia unlink de `.git` — o comando está na seção 4.
