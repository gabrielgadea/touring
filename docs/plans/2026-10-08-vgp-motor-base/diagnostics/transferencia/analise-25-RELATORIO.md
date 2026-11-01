---
type: Handoff
title: "analise-25 → touring — corpus semeado do VGP (TS/TSX/JS/HTML)"
description: Relatório de transferência da sessão analise-25. O que entregou, o que mediu, o que ficou por medir e o comando exato para retomar. Nada commitado.
plan_id: 2026-10-08-vgp-motor-base
timestamp: 2026-10-09T21:50:00-03:00
tags: ["#kind:handoff", "#status:active", "#domain:harness"]
---

# analise-25 — corpus semeado do VGP · relatório de transferência

> Enviado à **touring-09** (coordenadora). A corpora do VGP passa a ser das sessões de
> `~/projects/touring`. Esta sessão **não** edita mais `scripts/corpus_semeado.py` nem
> `~/.claude/touring/vgp/corpus_semeado/`.
>
> Confiança de cada afirmação marcada: **FATO** = medido nesta sessão · **INFERÊNCIA** = conclusão com
> evidência · **ESPECULAÇÃO** = hipótese, com o que a confirmaria.

---

## 1. Identidade e mandato

| | |
|---|---|
| sessão | `analise-25` (MiniMax M3.1-Flash-Preview), projeto `~/projects/analise` |
| mandato | corpus semeados para julgar o VGP em TS, TSX, JS, HTML, CDN e classes CSS (`VL0-multilinguagem-spec.md` §4 classes · §7 corpus) |
| arquivos exclusivos | `scripts/corpus_semeado.py` + saída em `~/.claude/touring/vgp/corpus_semeado/<lang>/` |
| proibido (cumpri) | `crates/**`, `juiz_*.py`, `shell_inline.py`, `corpus_v0d.py`, `corpus_v5_inline_neg.py`, `~/.claude/touring/vgp/corpus_v0d`, commit, instalação de pacote, tocar nos fontes |

## 2. Feito, com evidência

### 2.1 O gerador — `scripts/corpus_semeado.py` (1.214 linhas, um arquivo)

**FATO.** Um positivo por mutação, em arquivo real, cada um confirmado por um **oráculo independente
do motor** que acusa o mutado **e não** o original. Item sem oráculo conclusivo sai do corpus.

- **TS/TSX/JS** — oráculo é o `tsc` do **projeto**, rodado numa **cópia por hardlink** (`cp -al`) de
  `~/projects/analise/frontend` (281 `.tsx`, 135 `.ts`; `tsconfig` próprio). A mutação é anexada ao
  arquivo, o `tsc` roda, e o arquivo é **restaurado** — inclusive se o `tsc` explodir (`finally`).
  Sobrescrever é sempre `unlink` + `write`, **nunca** escrita no inode: é essa a garantia de que o
  fonte fica intacto.
- **HTML** — cinco operadores, cada um com o seu oráculo: `recurso` (o arquivo não existe no disco),
  `ancora` (o id não existe no documento parseado), `cdn` (HEAD → 404), `cdn_pin` (versão não exata
  que resolve), `classe_css` (classe que nenhuma folha carregada define).

**As seis receitas de TS/TSX foram provadas uma a uma antes de entrarem no script** — cada uma
produz **1 erro, o esperado, e nenhum outro**:

| operador | linha anexada | o `tsc` respondeu |
|---|---|---|
| `import` | `import { vgpAusente… } from './__vgpModAusente…';` | `TS2307 Cannot find module` |
| `nome` | `vgpAusente…;` | `TS2304 Cannot find name` |
| `atributo` | `new Date().vgpPropAusente…` | `TS2339 Property … does not exist on type 'Date'` |
| `kwarg` | `{ vgpCampo…: 1, vgpPropAusente…: 2 }` | `TS2353 Object literal may only specify known properties` |
| `componente` | `<VgpCompAusente… />` | `TS2304 Cannot find name` |
| `prop` | `<div vgpPropAusente…={1} />` | `TS2322 … not assignable to type 'DetailedHTMLProps…'` |

### 2.2 O selftest — **23/23, exit 0**, 66 s

**FATO.** `python3 scripts/corpus_semeado.py --selftest`

- **1 controle positivo**: a linha de base do projeto tem **0 erros** (sem ele, "o oráculo não achou
  nada" seria indistinguível de "o oráculo está cego").
- **6 operadores de TS/TSX**: o `tsc` acusa o mutado **e não** o original.
- **5 operadores de HTML**: o oráculo acusa o mutado e não o original.
- **11 mutantes**, um por operador: cada mutante que **desliga** um operador tem de **reprovar** o
  selftest. Todos reprovados. O mutante passa pelo **mesmo filtro da geração** — chamando a receita
  por fora seria exatamente o contorno que o aceite proíbe (foi o defeito real que o selftest pegou).

### 2.3 Portões estáticos — aceite (b) inteiro

**FATO.** `ruff check` → *All checks passed* · `ruff format --check` → *already formatted* ·
`ruff check --select C901 --config 'lint.mccabe.max-complexity = 10'` → *All checks passed* ·
`pyright corpus_semeado.py` → **0 errors, 0 warnings, 0 informations**.

## 3. Estado por item, com o próximo comando

| item | estado | próximo comando |
|---|---|---|
| receitas TS/TSX | ✅ provadas no `tsc` do projeto | — |
| oráculo de CDN | ✅ medido fora do sandbox: arquivo inexistente na versão certa → 404 · versão inexistente → 404 · patch inexistente → 404 · range `@18` → 200 | — |
| corpus `typescript` | ✅ **215** (80 pos + 135 neg) | — |
| corpus `tsx` | ✅ **320** (120 pos + 200 neg) | — |
| corpus `javascript` | ⚠️ **3** (0 pos + 3 neg) — **oráculo recusado** | ver §6.1 |
| corpus `html` — `recurso`/`ancora`/`classe_css` | ⚠️ **50** (20 / 10 / 20), **0 negativos** | ver §6.2 |
| corpus `html` — `cdn` / `cdn_pin` | ❌ **0** | ver §6.3 |
| **determinismo (aceite d)** | ❌ **não verificado** — a rodada foi interrompida | §5.2 |

## 4. Processos rodando

**FATO.** **Nenhum.** Os dois jobs em fundo foram parados por mim (`TaskStop`): a geração de código
(já encerrada por SIGTERM) e a verificação de determinismo. Nenhum `tsc`, `node` ou `touring run`
meu continua vivo. Nenhum processo foi morto por nome (REGRA #19 respeitada).

## 5. Dados gerados

### 5.1 Corpus no caminho canônico — `~/.claude/touring/vgp/corpus_semeado/`

**FATO.** Contagem e sha256 medidos por mim, independentemente, no caminho canônico:

| lang | entradas | positivos por classe | negativos | sha256 de `entradas.jsonl` |
|---|---|---|---|---|
| `typescript` | **215** | `import` 20 · `nome` 20 · `atributo` 20 · `kwarg` 20 | 135 | `9313924e51886b45a671f0140fb52472f677524499efe977fd87972eda5b899e` |
| `tsx` | **320** | `import` 20 · `nome` 20 · `atributo` 20 · `kwarg` 20 · `componente` 20 · `prop` 20 | 200 | `a5e6bd6166509e8ef969fcc37f792a07ecfcd097aa49f716541030046ebb3230` |
| `javascript` | **3** | — (nenhuma) | 3 | `c763bc190ec5982b976644e70d52a016e4f84d9f38014d884b041512b1987d12` |
| `html` | **50** | `classe_css` 20 · `recurso` 20 · `ancora` 10 | **0** | `5cedbcc1c0164ad4b99f2cc94e534102809733fc7746772075f9034035395e1a` |

**FATO.** A **minha** geração foi interrompida por timeout no meio do `typescript`
(`[FALHOU] typescript rc=143` = SIGTERM, e a rota sancionada saiu com 255). O corpus canônico
completo é o da **analise-68**; minhas contagens independentes batem com as dela exatamente
(215 / 320 / 3). Isto é **corroboração**, não entrega — o crédito é dela.

### 5.2 O que falta e o comando exato

```bash
cd ~/projects/touring/docs/plans/2026-10-08-vgp-motor-base

# (1) determinismo — aceite (d). ~4 min com n reduzido; o que se prova é o caminho, não o volume.
python3 scripts/corpus_semeado.py --confere-determinismo --lang typescript --n 2 --negativos 5

# (2) HTML completo: negativos + cdn + cdn_pin. PRECISA DE REDE -> roda NATIVO,
#     nunca dentro de `touring run` (lá a rede é negada: medido, URLError errno 13).
python3 scripts/corpus_semeado.py --lang html --n 20 --negativos 200

# (3) regerar tudo do zero (demora: ~45 min por linguagem de código, 1 `tsc` por mutação)
python3 scripts/corpus_semeado.py --lang todas
```

> A retomada é **determinística**: semente fixa `20261009`, toda lista ordenada, `id = sha12` do
> conteúdo lógico, e o cache de CDN em `corpus_semeado/cdn_cache.json` (sem ele, um 404 que mudasse
> de resposta mudaria o sha).

## 6. Decisões, premissas, achados, lacunas e riscos

### 6.1 `javascript`: 0 positivos — o script **recusou** o oráculo, e está certo
**FATO.** `packages/` tem 122 `.js` e **136 arquivos com erro** na linha de base com `checkJs`;
nenhum subdiretório fica limpo. Um projeto que já erra não serve de oráculo — o erro pré-existente
contaminaria o recall. O script devolve `projeto_ja_erra` em vez de semear itens não confirmáveis.
**INFERÊNCIA (0.8):** para ter JS medido, ou um subconjunto com linha de base limpa (não existe hoje),
ou um oráculo **diferencial** (mutado menos original), que eu não implementei. Evidência que fecharia:
rodar `checkJs` por diretório e listar os que fecham em 0.

### 6.2 `html` com **0 negativos** — lacuna real, causa **não diagnosticada**
**FATO.** O filtro de negativos exige, para cada documento: folha resolvível, nenhum recurso local
quebrado, nenhuma âncora órfã. O resultado foi 0. **Não medi a causa** — duas sondas minhas estouraram
o orçamento de 60 s de CPU do sandbox na varredura dos 6.495 documentos. Para a `touring-4f`: sem
negativos, **o falso deny de HTML não é mensurável**. Próximo passo: amostrar 200 documentos e contar
o descarte por filtro (é uma linha usando `folha_resolvivel` / `recurso_quebrado` / `ancora_quebrada`).

### 6.3 `cdn` e `cdn_pin`: 0 — o sandbox não tem rede
**FATO.** A rede é negada dentro de `touring run` (`URLError: [Errno 13] Permission denied`, medido em
três URLs). Meu oráculo trata isso como **`indeterminado`, nunca como 404** — o item sai (fail-closed).
O cache e a corrida são nativos: **rode `--lang html` fora do sandbox**.

### 6.4 Achado já resolvido por vocês — e que vale registrar
**FATO.** Com o juiz anterior (`classe_erro` em `juiz_motor_vgp.py:74`, regex de Python), **todas as 11
mensagens não-Python caíam em `nao_classificado`** — `TS2304`, `TS2305`, `TS2307`, `TS2339`, `TS2353`,
404 de CDN, classe CSS. Consequência: os positivos semeados saíam todos do denominador e o recall
ficava não-mensurável. **A coordenadora já corrigiu** (sha `b7f7fd9b`, classificação por linguagem,
`origem: "semeado"` no balde arquivo) e a analise-68 verificou 80/80 e 120/120 como `simbolo`. Registrado
aqui porque o corpus **depende** desse conserto para ter denominador.

### 6.5 Armadilhas do ambiente que mudaram o método
**FATO.** Três, todas medidas:
1. **`/tmp` é outro dispositivo** (`/tmp` = dev 58, `$HOME` = dev 31). `cp -al` cruzado morre com
   *Invalid cross-device link*. A sombra tem de nascer no mesmo dispositivo da fonte.
2. **O sandbox contém o filesystem**: escrita fora do workspace é negada, e `/tmp` também.
   Daí `VGP_SEMEADO_TMP` e `--saida` apontarem para o workspace.
3. **Orçamento de CPU de 60 s por `touring run`**, independente de `--timeout-ms`: varredura dos 6.495
   HTML estoura.

### 6.6 Três defeitos meus que os hooks e o selftest pegaram
**FATO.** Nenhum apareceria num teste normal, e todos teriam produzido corpus falso:
1. `muta_e_confere` rejeitava **qualquer** saída não-vazia do `tsc` — ou seja, rejeitava o próprio erro
   que ia procurar. Os 6 operadores de TS "reprovavam" 100 %.
2. Os predicados de HTML não liam o documento: "acusavam" o original e o mutado igualmente.
   Reescritos como predicado **sobre o texto** (o que separa o mutado do original).
3. O mutante dos operadores de TS não discriminava — chamava a receita **por fora** do filtro da
   geração.

## 7. Git — só dos meus arquivos

**FATO.** **Nada commitado, nada em push, nenhum comando destrutivo.** Nenhum `git commit`, `git add`,
`stash`, `reset` ou `checkout` foi executado nesta sessão. O único arquivo de código que escrevi é
`scripts/corpus_semeado.py`, hoje **não commitado** na árvore de `~/projects/touring`.

## 8. O que **NÃO** foi verificado — leia antes de confiar

1. **Determinismo (aceite d): NÃO verificado.** As duas gerações paralelas foram interrompidas. Não
   afirmo determinismo; afirmo apenas o comando da §5.2.
2. **O motor VGP nunca foi rodado contra este corpus.** Não medi recall, falso deny nem p95 — o juiz é
   daspiritual da organização e não estava no meu mandato.
3. **`javascript` não é mensurável** hoje (0 positivos) — o adaptador JS não tem corpus.
4. **`cdn` e `cdn_pin` não têm corpus** (0 entradas).
5. **Falso deny de HTML não é mensurável** (0 negativos) — causa não diagnosticada (§6.2).
6. **A integridade do esquema** (todo `snap` presente, todo id reencontrável) eu **não** re-verifiquei
   no corpus canônico; a analise-68 afirma ter verificado para `typescript` e `tsx`.
7. **O `tsconfig` do frontend** não tem `noUnusedLocals` — por isso as receitas com `export const`
   funcionam. Se alguém ligar essa flag, as receitas de `nome`/`atributo`/`kwarg`/`prop` passam a gerar
   erro a mais e o oráculo as recusa (comportamento correto, volume diferente).

## 9. Resíduo deixado no `projects/analise` (remoção é do humano)

**FATO.** Nada disto é do VGP e nada disto é meu para apagar — o hook bloqueia `rm -rf` por design:

| caminho | o que é | remover |
|---|---|---|
| `~/projects/analise/.vgp-sombra-frontend/` | cópia por hardlink do frontend (300 MB, criada por mim durante a prova das receitas) | `rm -rf ~/projects/analise/.vgp-sombra-frontend` |
| `~/projects/analise/.vgp-semeado/` | sondas, cópia de trabalho do corpus, sombras e os 2 drivers `.sh` | `rm -rf ~/projects/analise/.vgp-semeado` |
| `~/.cache/vgp-semeado/` | sombras de projeto criadas pelo gerador | `rm -rf ~/.cache/vgp-semeado` |

> ⚠ O `.vgp-sombra-frontend` e as sombras em `~/.cache` são **hardlinks**: removê-los **não** apaga os
> arquivos-fonte, que continuam com seus outros links. Ainda assim, é dele a decisão de rodar o `rm`.

---

---

# ADDENDUM — o que só a analise-25 sabe

> Acrescentado depois do handoff, a pedido da analise-89: as seções acima tratam do
> `corpus_semeado.py`. Aqui vai **todo o meu trabalho no plano VGP** —-inclusive o que nunca foi
> para o disco — e um conflito de sha que precisa ser resolvido antes de a touring-4f retomar.

## A. Meu trabalho no VGP além do corpus semeado (7 tarefas)

**FATO.** Todas decai **analise-89**, 09/10/2026, cada uma com o aceite escrito **antes** do trabalho.

| tarefa | artefato (sha256 16) | estado |
|---|---|---|
| **C1 por família** — onde cai a queda de 898 → 592 chars/call | `~/projects/touring/docs/plans/2026-10-08-compactacao-65/diagnostics/c1-por-familia-2026-10-09/` | entregue — **atenção: outro bundle e outro repositório**, ver §D |
| **V0e5** — quebrar o balde "rajada G1" por origem do texto | `diagnostics/v0e5-2026-09/v0e5_por_origem.py` `d4e68e2b6279a298` + `.json` `0fa06bd491fc` | entregue |
| **V0e-resto** — destino das 606 falhas Bash fora de gate | `diagnostics/v0e-fora-de-gate/{coleta,causas,probe_606}.py` `0ae05891` / `7aecadb4` / `1811324c` + `causas.json` | entregue |
| **V0e5-remediação** — re-medir por exposição, Wilson 95 % | `diagnostics/v0e5-remedicao/remedicao.py` `8129598f2af59442` | entregue, **aguardando janela maior** |
| **Juiz da V4** — escrito *antes* do dado real | `scripts/juiz_v4_sessoes.py` `8331314c82e02c3e` + `scripts/fixtures_v4/gerar_fixtures_v4.py` `3a816177b89fa700` | entregue, 24 checks verde |
| **V5 corpus inline negatives** | `scripts/corpus_v5_inline_neg.py` `f676b1f439ebd87c` + `scripts/verificar_inline_neg.py` `ee70a3c29c445435` | entregue, 400 entradas |
| **G1 "sem replay", 2ª leitura read-only** | nenhum (só relatório) | entregue |

## B. O que ficou **só na conversa** — nunca foi para o disco

**FATO.** 1. **A1 foi refutado pela própria analise-25, por erro meu de constante.** O limiar do G1 é
`G1_DENY_AT = 3` (`crates/touring-cli/src/cli_suggester.rs:3934`). Escrevi **4** no meu script de
reprodução — carreguei o número de memória em vez de ler o fonte. Toda a tabela do achado A1 estava
deslocada, e o "o limiar fica inerte dentro de uma rajada" **não existia**: com invalidate o gate
dispara na 3ª de cada lote, que é o contrato do G10. Instrumente corrigido e re-executado. A lição
ficou na memória da sessão (`licao-instrumento-de-reproducao-nao-herda-constante-de-memoria.md`).

**FATO.** 2. **C1: a linha de base é irrecuperável e o juiz C1 depende do instante.** Nenhuma N de 8 a 25
transcritos reproduz as 8.441 chamadas do baseline, e o mtime se move porque os transcritos continuam
sendo escritos. E o mesmo juiz passa na janela fechada (536,8) e reprova na aberta (549,3): **é
dependente do tempo**, então um número dele sem o instante congelado não reproduz.

**FATO.** 3. **O estrato `touring_run` da V5 era majoritariamente não-Python — 1.133 casos
excluídos.** O "1/300 falso deny" era a ponta de um estrato contaminado; foi por isso que ainline ficou
significativo pela primeira vez.

**FATO.** 4. **Débito registrado e não pago** (para quem pegar o `corpus_v0d.py`):
`programa_inline` **não olha `--lang`** (1.133 casos) **nem quem consome o `<<`** (`cat` 665 · `tee` 414).
Os dois ramos devolvem texto que não é o Python que rodou, e **nenhum dos dois é detectável por
"compila"**.

**FATO.** 5. **Regra que a rodada do corpus reforçou:** instrumento de reprodução não herda constante de
memória — `grep` no fonte antes de rodar, senão a tabela sai pronta e errada sem nenhum sinal.

**FATO.** 6. **Cinco vezes**, nesta sessão, caracteres CJK escaparam na minha prosa e o hook
`prose-integrity` recusou cada uma. Duas delas dentro de deliverable escrito.

## C. ⚠ Conflito de sha a resolver antes de retomar

**FATO.** O `scripts/corpus_semeado.py` **em disco agora é a versão da analise-25**:
`fb11047de57218c2`, 1.464 linhas, com `VGP_SEMEADO_TMP` e `tmp_gravavel`.

**FATO.** O relatório da **analise-68** declara para esse mesmo caminho sha `0f913bc856934e33`,
1.206 linhas. **Os dois não podem estar certos, e o sha que está no disco é o meu.** Consequência
prática: **antes de a touring-4f continuar, ela precisa decidir qual versão é a boa** — a minha
(tem `VGP_SEMEADO_TMP`, `tmp_gravavel`, o mutante passando pelo filtro da geração e o
`--confere-determinismo` autossuficiente) ou a da 68. Eu **não** sobrescrevi o arquivo da 68: o
`Write` meu-created porque o caminho **não existia** quando escrevi (a 68 escreveu depois, e o que
está hoje é o meu).

## D. ~~O entregável do C1 sumiu do disco~~ — **RETIRADO: era falso**

**FATO.** O C1 **nunca sumiu**. Ele vive em **outro bundle e outro repositório**:
`~/projects/touring/docs/plans/2026-10-08-compactacao-65/diagnostics/c1-por-familia-2026-10-09/`
(10 arquivos, sha16: `RELATORIO.md` `74abc0efd887a6b5` · `c1_por_familia.py` `1852104698313e98` ·
`c1_por_familia.json` `4f8b229213610b03` · `varrer.py` `6bd4d838e26cc363` · `reconciliar.py`
`f5c097aebf1bbcee` · `reconciliacao.json` `edc2ce98029ba880` · `nao_classificado.json` `22f79a5ce161e6ca` ·
`abrir_nao_classificado.py` `a369459cbd75933d` · `recon.py` `00f5ccb9fbf73733` · `cache_bruto.json`
`e12fc0c9c8e82fc4`). Confirmado com a coordenadora touring-09.

**POR QUE EU ERREI — e é a parte que importa registrar.** Meu `find` estava limitado ao bundle do VGP
(`~/projects/touring/docs/plans/2026-10-08-vgp-motor-base/`), e é lá que eu declarei "não existe, nem
em todo o plano". O **controle positivo que eu apresentei era verdadeiro e não provava o que eu
afirmei**: ele provava que o `find` funcionava, não que a ausência fosse global. Eu tinha um predador
teoricamente correto e o usei como se ele cobrisse o **escopo**, que é outra coisa.

**A regra, agora escrita:** antes de declarar uma ausência, o `find` tem de varrer **todo o espaço onde
o artefato poderia estar** — e, num plano com vários bundles e dois repositórios, "todo o espaço" é
`~/projects/touring` **e** `~/projects/analise`. Controle positivo valida a **consulta**; só a amplitude
da busca valida a **ausência**.

---

**Fim do relatório.** Não continuo no VGP: a partir daqui a dona é a `touring-4f`, sob a `touring-09`.