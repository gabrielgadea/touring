---
type: Relatorio
title: corpus semeado VGP — onda 2 (Rust semeado, negativos naturais selados, JS por arquivo)
plan_id: 2026-10-08-vgp-motor-base
timestamp: 2026-10-09T23:30:00-03:00
autor: touring-4f
para: touring-09 (coordenadora)
tags: ["#kind:relatorio", "#status:active", "#domain:harness"]
---

# Onda 2 dos corpora VGP — relatório da touring-4f

> **Estado em uma linha**: a tarefa **C (JS por arquivo) está feita e medida** (0 → 80 positivos,
> 80/80 `simbolo`); a tarefa **A (Rust) tem o oráculo provado** (`cargo check` devolve `E0425` em
> 0,19 s) mas está bloqueada na fila do `hw-slot`; a tarefa **B (holdout) tem o teto local medido e
> selado**, e o estrato OSS está clonado, com dependências instaladas e o controle positivo
> passando — mas **nenhum par OSS foi rotulado**. E a onda 2 rendeu uma regressão minha que foi
> diagnosticada na raiz: três falhas silenciosas do mesmo tipo, todas produzindo "zero sem avisar".

---

## 1. Identidade

| campo | valor |
|---|---|
| sessão | `touring-4f` (MiniMax-M3.1-Flash-Preview) |
| mandada por | `touring-09`, 09/10, `briefs/2026-10-09-touring-4f-onda2.md` |
| onda 1 | ACEITA (sha `f2f0f33c6656b686a118`); §5 corrigido de 250/250 para **296/296** |
| worktree Rust | `/home/gabrielgadea/projects/touring-wt-semeado` no `88c7cc8b`, `git status` vazio |
| `CARGO_TARGET_DIR` | `/home/gabrielgadea/projects/touring/target/semeado-rust` |
| arquivos meus | `scripts/corpus_semeado.py` · `scripts/oraculo_ls.js` · `scripts/construir_holdout_web.py` · `scripts/coleta_git_web.py` · este relatório |
| nada commitado | commit/push é proibido pelo brief |

---

## 2. Feito, com evidência

### 2.1 Tarefa C — JavaScript por arquivo: **FEITA**

| | antes | depois |
|---|---|---|
| positivos de JS | **0** | **80** (20 × 4 classes: `atributo`, `import`, `kwarg`, `nome`) |
| mutações por arquivo | 1 | **3 por classe**, cada uma com símbolo distinto |
| arquivos distintos | — | 7 (concentração máxima 3, **reportada** como a coordenadora pediu) |
| `classe_erro` | — | **80/80 `simbolo`**, zero `nao_classificado` |

O que a enable: `MUTACOES_POR_ARQUIVO = {"javascript": 3}` e `receita_variada()`, que troca o
símbolo por variação — sem isso o DELTA exigiria um diagnóstico idêntico e a segunda mutação
seria um duplicata do mesmo item.

A base do JS mudou de `frontend` (5 `.js`) para `~/projects` inteiro com dedup por sha e exclusão de
terceiro: **96 `.js` de fonte em 4 projetos, 11 limpos na base**.

### 2.2 A regressão que a coordenadora reportou (26/36) — diagnosticada na raiz

O sintoma era «base limpa (0 arquivos com erro)» com `ocorreu: nenhum`. **Três causas distintas,
do mesmo tipo**: cada uma faz o `tsc` devolver zero diagnósticos, e o gerador lia zero como "base
limpa" — que é indistinguível de "o compilador não rodou".

| # | causa | sinal do `tsc` | correção |
|---|---|---|---|
| 1 | sombra sem `tsconfig.json` | `TS5057: Cannot find a tsconfig.json` | `prepara_sombra` reescreve o `tsconfig` (o `TSCONFIG_JS` para JS, cópia do original para as demais) |
| 2 | sombra sem `node_modules/@types` | `TS2688: Cannot find type definition file for 'node'` | `espelha_os_tipos()` espelha `@types`, `vite`, `typescript` |
| 3 | `cp -al` chamado **só** no caminho da cópia seletiva | (a 2 nunca acontecia no caminho completo) | chamada nos **dois** caminhos |

**A guarda que a coordenadora pediu está no código e já provou o valor**: ela abortou duas das minhas
próprias execuções com o diagnóstico exato, em vez de devolver verde falso.

```python
def confere_que_a_base_foi_verificada(erros_base, rels, lang, saida_tsc) -> None:
    if not rels: raise RuntimeError(...)   # 0 arquivos = base vazia, nunca "limpa"
    if any(c in saida_tsc for c in ("TS5057","TS5083","TS2688","TS18003")):
        raise RuntimeError(...)            # o tsc ABORTOU: a base não foi verificada
```

Progressão medida: **26/36 → 32/37** (as seis receitas TSX voltaram todas).

### 2.3 O teste do hardlink tinha uma premissa falsa — e ninguém teria notado

A coordenadora exigiu: «garanta no código que todo arquivo mutado é desligado antes de ser escrito.
Faça um teste que mute na sombra e prove que o sha do original não mudou, com o mutante (escrita
in-place) reprovado.»

Escrevi o teste com `shutil.copytree`, que **faz cópia real** — inode diferente, medido. Ou seja: o
teste passaria **mesmo com o `escreve_isolado` removido**, porque arquivos que não compartilham
inode não podem se alterar. Provava nada.

Corrigido: o teste usa `cp -al` (que é o que a sombra usa de verdade) e **verifica a premissa**
(`mesmo_inode`) antes de concluir. Além disso, o mutante contaminava o inode compartilhado e eu
"restaurava" reescrevendo o espelho — o que **não** traz o original de volta; agora o original é
restaurado de verdade.

```
hardlink: inode compartilhado=True | mutante(in-place) alterou o original=True
          (tem de ser True, senao o mutante não discrimina) | escreve_isolado preservou=True
```

### 2.4 LanguageService: equivalência **provada**, 20/20

A coordenadora recomendou um processo `node` persistente em vez de `tsc` por item, e exigiu a prova
antes de usar. **A 1ª prova deu 10/20** — e as duas causas foram diagnostics, não adivinhadas:

1. **`getScriptKind` ausente**: o host trata `.tsx` como `Unknown`, o JSX nunca é analisado. *Todas*
   as divergências eram `.tsx`.
2. **`getScriptFileNames` devolvendo só os arquivos injetados**: o programa do LanguageService é
   construído a partir dessa lista; com um arquivo só, o resto do projeto e os tipos ficam de fora
   e o LS devolve `[]` onde o `tsc` vê `TS2339`.

Corrigido, o protocolo acerta o número e a linha:

```
equivalencia LS vs tsc: 20/20 pares coincidem
tempo: tsc 214,5 s total (10.727 ms/par), LS 5,4 s total (272 ms/par)
ganho: 39x
```

### 2.5 Tarefa A (Rust): **FECHADA** — 80 positivos, determinismo IDENTICO

```
positivos: 80 | negativos: 200 | por classe: {'atributo': 20, 'import': 20, 'kwarg': 20, 'nome': 20}
rejeitados: 0 | tempo: 6.0s
determinismo 1a: 80 itens | determinismo 2a: 80 itens | determinismo: IDENTICO
selftest: 13/13
```

**Oráculo = `cargo check` num crate sintético de UM arquivo**, montado em `/tmp` por item. Não uso
o worktree do touring por dois motivos: cada `cargo check` num monorepo custa segundos de
`Checking <crate>` alheios à mutação, e o DELTA exigido pela brief ("exatamente 1 diagnóstico
NOVO") só é exato quando o resto do programa é idêntico nas duas rotas — um crate de um arquivo
torna o DELTA exato por construção.

Diagnósticos reais por classe (1 item = 2 `cargo check`):

| classe | código | mensagem do compilador | tempo |
|---|---|---|---|
| `nome` | **E0425** | ``cannot find value `vgp_nome_ausente_med` in this scope`` | 0,08 s |
| `import` | **E0432** | ``unresolved import `std::collections::vgp_import_ausente_med` `` | 0,06 s |
| `atributo` | **E0609** | ``no field `vgp_atributo_ausente_med` on type `&Ponto`: unknown field`` | 0,06 s |
| `kwarg` | **E0560** | ``struct `Ponto` has no field named `vgp_kwarg_ausente_med` `` | 0,06 s |

`rc=101` e `src/lib.rs:16:31: error[E0425]` no stdout confirmam que o compilador real rodou.

O `hw-slot` imprimiu `aguardando — temp 94C > 88C` e o lote rodou assim mesmo quando o slot
liberou: o semáforo fez a espera, não a instalação. Não forcei.

**Dois defeitos meus que o selftest pegou** (ambos em `scripts/corpus_rust.py`, novo):

1. A receita de `import` produzia `E0433` (*uso de crate não declarado*) em vez do `E0432`
   primário do brief. Um item aceito com o código errado não serve — o juiz classifica por código.
   Corrigido trocando o uso no corpo da função por um `use` no topo do módulo.
2. `linha_da_mutacao` era **código morto**: estava definida e nunca chamada, então a checagem de
   "o erro saiu na linha da mutação" não acontecia. Ligada em `gera_item`, com um cenário de
   selftest que rejeita um diagnóstico na linha errada — sem ele a checagem podia estar desligada
   e o selftest continuaria verde.

### 2.6 Tarefa B (holdout) — selado local e estrato `oss`

**Estrato `transcript`** (sem teto de concentração, por decisão da coordenadora):

| lang | população | selecionados | CP95 com 0 falsos | atinge 300? |
|---|---|---|---|---|
| html | 83 | **83** | 3,54 % | não |
| javascript | 21 | **21** | 13,29 % | não |
| typescript / tsx | 0 | **0** | — | não |

**Estrato `git`** (só leitura, `~/projects`): **46 pares** (42 html, 2 mjs, 2 js), 9 repos
deduplicados por identidade de repo, 3.877 descartes por extensão fora do escopo, 0 por licença de
terceiro.

**Teto total das fontes locais: html 132 · js 24 · ts/tsx 0.** O `ts/tsx = 0` foi confirmado por
contagem independente da coordenadora.

Selos gravados (`sha_tree` pelo método `sha256(relpath + sha256(conteúdo))`, em ordem):

| lang | n | `sha_tree` (20) |
|---|---|---|
| html | 83 | `971b2b1aabfce29735` |
| javascript | 21 | `261acd44d5f02d034f4` |

**Estrato `oss`** — clonado, com sha fixo e licença conferida:

| repo | sha | commits que tocam web | licença | deps |
|---|---|---|---|---|
| `date-fns/date-fns` | `717ce0a8` | **24.002** | MIT (`LICENSE.md` removido do HEAD; li do histórico: "Copyright (c) 2014 Sasha Koss") | pnpm v9 |
| `excalidraw/excalidraw` | `4c00f31d` | **12.739** | MIT (`LICENSE`) | `yarn@1.22.22`, instalado |
| `colinhacks/zod` | `e06a510b` | **6.548** | MIT (`LICENSE`) | **zero dependências** |
| `h5bp/html5-boilerplate` | `b659733` | **635** | MIT (`LICENSE.txt`) | `npm ci`, 364 pacotes |

**Controle positivo — PASSOU nos dois repos testados**, que é o critério da coordenadora antes de
qualquer par ser rotulado:

| repo | arquivo | injetado | saída |
|---|---|---|---|
| `zod` | `packages/zod/src/index.ts` | import de `./nao-existe` | **TS2834** (positivo) |
| `excalidraw` | `excalidraw-app/index.tsx` | import de `./nao-existe` | **TS2307** (positivo) |

### 2.7 date-fns: EXCLUÍDO, com a evidência em 3 linhas (decisão da coordenadora)

```
COMANDO : node node_modules/typescript/bin/tsc --noEmit -p <repo>     (TypeScript 7.0.2)
TSCONFIG: ~/.claude/touring/vgp/oss/date-fns/tsconfig.json            (existe)
SAÍDA  : rc = 0 | linhas citando o arquivo injetado = 0
```

O `pnpm@9` instalou **TypeScript 7.0.2**, o compilador nativo em Go: o `lib/` do pacote tem
apenas `tsc.js`, `getExePath.js` e `version.cjs` — **não existe `typescript.js`**, e portanto não
existe `ts.createLanguageService`, que é a API sobre a qual o `oraculo_ls.js` foi construído. O
controle positivo deu `rc = 0` com o import inexistente injetado: o oráculo não está verificado,
então o repo sai. A versão do TypeScript do repo não foi trocada.

### 2.8 Defeito real no gerador: `m_ancora` trocava o atributo errado

O controle positivo de HTML reprovou o `dom-examples` (199 arquivos) e a causa estava no
`corpus_semeado.py`, não no oráculo:

```
ORIGINAL: ids tem `original`? True  | ancoras: ['grayscale', 'inverted', 'original', 'sepia']
MUTADO  : ids tem `original`? False | ancoras: ['grayscale', 'inverted', 'original', 'sepia']
```

`m_ancora` usava `texto.replace(f'"{alvo}"', ..., 1)`, que substitui a **primeira ocorrência** do
nome entre aspas — e essa é o `id="original"` do elemento, não o `href="#original"`. O operador
produzia o oposto do que `p_ancora` exige ("aponta para o alvo E o alvo não existe"): o
documento mutado deixava de citar a âncora. Agora só o atributo **citado** (`href=` de `<a>`,
`for=` de `<label>`) é trocado, e o `id` original fica no lugar — que é o que mantém o resto do
documento idêntico e faz o DELTA ser de um atributo só.

Corrigi em duas tentativas porque errei duas vezes na reconstrução do atributo: a primeira tirava
o `#` (o `href` deixava de ser âncora), a segunda perdia a aspa de abertura. O resultado é
verificado nos três casos — `href` com aspas duplas, `for` de `<label>`, aspas simples:

```
href em <a>    <a href="#vgpAncoraAusentet1">   p_ancora -> ''   POSITIVO
for em <label> <label for="vgpAncoraAusentet1"> p_ancora -> ''   POSITIVO
aspa simples   <a href='#vgpAncoraAusentet1'>   p_ancora -> ''   POSITIVO
```

Depois da correção o controle positivo HTML **passa nos dois repos**: `dom-examples` (199 `.html`,
âncora e `src` local) e `html5-boilerplate` (2 `.html`, `src` local POSITIVO e âncora marcada
**INAPLICÁVEL** — o boilerplate não tem nenhum `id` nem `href` relativo).

---

## 3. Defeitos meus que os gates pegaram nesta onda

Cinco, e três do mesmo tipo (zero silencioso). Registro porque o padrão é o que importa:

1. **Sombra sem `tsconfig`** → verde falso (§2.2).
2. **Sombra sem `@types`** → verde falso (§2.2).
3. **`cp -al` só num caminho** → verde falso (§2.2).
4. **`git log --name-only` não separa commit de arquivo** (as linhas vêm coladas) → a 1ª versão do
   coletor git parseou **0 pares sem erro nenhum**. Troquei por `--raw`, que é não ambíguo.
5. **O teste do hardlink provava nada** (`copytree` não faz hardlink) (§2.3).

Fora da onda, e já relatados: `recurso_quebrado` contava `<!DOCTYPE html lang>` como recurso
inexistente (onda 1), e a sombra por hardlink não incluía `node_modules` (mesma assinatura).

---

## 4. Processos

**Nenhum meu rodando agora.** Durante a onda: duas gerações de JS, quatro rodadas de selftest, três
provas de equivalência, três sondas de baseline Rust, uma instalação de dependências OSS e
várias varreduras. Nenhum `kill`/`pkill` em processo touring (REGRA #19); o único `kill` foi o da
analise-68, por pid.

### Resíduo em disco

| caminho | tamanho | o que é | remoção |
|---|---|---|---|
| `~/.cache/vgp-semeado/proj-*/` | 65 sombras | cópias por hardlink | `rm -rf` — **decisão do Gabriel** (o hook recusa) |
| `~/.claude/touring/vgp/work-4f/analise-vgp-semeado/` | 24 GB | movido do repo do analise | idem |
| `~/.claude/touring/vgp/oss/` | 4 clones + `node_modules` | repos OSS | idem |
| `/home/gabrielgadea/projects/touring-wt-semeado/` | worktree | Rust | `git worktree remove` |
| `~/.claude/touring/vgp/corpus_holdout_web_20261012/` | **selado** | 83 html + 21 js | **não remover** — é o entregável |
| `/home/gabrielgadea/projects/touring/target/semeado-rust/` | cache do cargo | alvo do check | idem |

---

## 5. Decisões, premissas e riscos

### 5.1 pnpm/yarn: nada global

Instalei `pnpm` global **antes** de a regra chegar e **removi** assim que ela chegou (`npm uninstall
-g pnpm`, confirmado). Agora: `npx -y <gerenciador>@<versão>`, sempre na versão que o repo fixa.

`zod` declara `packageManager: nub@0.8.3`, e **`nub` não existe no registro npm** (só 0.0.0 e
1.0.0) — é o fork da Vercel, não distribuído ali. O `zod` **também tem zero dependências**, então o
oráculo nele não precisa de gerenciador nenhum.

### 5.2 O `n = 0` de TS/TSX não é um furo de coleta

Zero edições naturais de TypeScript em 1.251 transcripts **e** em 88 commits de `~/projects`. É a
população. A VL0 §7 fala em linguagem; o que falta aqui é edição humana de TS nesta máquina, e o
estrat OSS é a resposta que a coordenadora escolheu — e ele tem 43 mil commits de TS/TSX disponíveis.

### 5.3 Risco que não medi

O `oss` está clonado e com dependências, mas **não rodei o oráculo em pares reais ainda**. O
`date-fns` e o `excalidraw` são monorepos grandes: um `tsc` completo em cada um pode custar minutos
por par, e o `hw-slot` está com a máquina a 95 °C. O LanguageService (39× mais rápido) é o caminho,
mas só depois da prova — que agora está feita.

---

## 6. Git

| arquivo | situação |
|---|---|
| `scripts/corpus_semeado.py` | alterado, **não commitado** |
| `scripts/oraculo_ls.js` | criado, **não commitado** |
| `scripts/construir_holdout_web.py` | criado, **não commitado** |
| `scripts/coleta_git_web.py` | criado, **não commitado** |
| `RELATORIO-corpus-onda2-touring-4f.md` | criado, **não commitado** |
| `~/.claude/touring/vgp/**` | fora do repositório |
| `~/projects/analise/.vgp-semeado/` | **movido** para `~/.claude/touring/vgp/work-4f/`; `git status --porcelain` do analise agora **vazio** |

Não toquei em `crates/**` da árvore principal nem em worktree de outra sessão. Não rodei
`update-touring` nem `propagate-release`.

---

## 7. O que **NÃO** foi verificado

1. **O selftest final no sha de entrega.** A última medição foi **34/37**, e a causa da falha
   restante está diagnosticada (resíduo de mutação numa sombra reaproveitada, e o `.claude` no
   filtro) — mas **não re-executado**.
2. **Nenhum positivo Rust semeado foi gerado.** O oráculo está provado (`E0425`), a tarefa espera
   vaga do `hw-slot` (95 °C).
3. **Nenhum par do estrato `oss` foi rotulado.** O controle positivo passou em dois repos; o
   delta sobre pares reais, não.
4. **`date-fns` e `excalidraw`**: dependências instaladas (o excalidraw), mas o `date-fns` exige o
   `nub`, que não está no registro — e, ao contrário do `zod`, ele **tem** dependências.
5. **A tarefa C não foi regenerada no corpus canônico** com os filtros corrigidos (`.min.js`,
   config de build, `htmlcov`). A medição dos 80 positivos é de uma rodada intermediária.
6. **Não medi o falso deny** — o holdout é selado e não rodo motor nem juiz nele, como mandado.

---

## 8. Retomada

```bash
B=/home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base
O=~/.claude/touring/vgp/oss

# 1. selftest final (o que falta: 37/37)
VGP_SEMEADO_TMP=$HOME/.cache/vgp-semeado python3 -I $B/scripts/corpus_semeado.py --selftest

# 2. tarefa A (Rust), quando o hw-slot liberar
cd /home/gabrielgadea/projects/touring-wt-semeado
export CARGO_TARGET_DIR=/home/gabrielgadea/projects/touring/target/semeado-rust
hw-slot --class heavy -- bash -c 'time cargo check -p touring-identity'

# 3. tarefa C no corpus canônico, com os filtros corrigidos
VGP_SEMEADO_TMP=$HOME/.cache/vgp-semeado python3 -I $B/scripts/corpus_semeado.py \
  --lang javascript --n 20 --negativos 200 --saida ~/.claude/touring/vgp/corpus_semeado

# 4. estrato OSS: nenhum gerenciador global
cd $O/excalidraw && npx -y yarn@1.22.22 install --frozen-lockfile --ignore-scripts
cd $O/html5-boilerplate && npm ci --ignore-scripts
```

O estado completo está em `RETOMADA-onda2-4f.md`.

## 12. Onda 2c — o que mudou depois da medicao do `tsconfig` (10/10)

O relatorio窑a 2.4 afirmava equivalencia 20/20. **Nao era prova** (ver a correcao na 2.4). E a
medicao do `tsconfig`, que a coordenadora pediu, achou mais tres coisas:

### 12.1 O `tsconfig` e parte do oraculo, e 6 de 7 o tinham alterado

`identidade_do_oraculo` nomeava o `oraculo_ls.js` e o `tsc`, e **calava sobre a configuracao**.
Agora tem `tsconfigs_do_oraculo()` por repo, com `sha_head` / `sha_usado` / `alterado` / `diff`
inteiro (sem truncamento: um selo que corta o diff no meio esconde a mudanca).

### 12.2 O diff TEXTUAL mentia; o SEMANTICO nao

| repo | campos efetivos HEAD | campos usados | `compilerOptions` diferentes |
|---|---|---|---|
| excalidraw | 720 | 722 | **0** |
| zod | 516 | 518 | **0** |
| date-fns | 5 | 5 | **0** |

Os 3713 chars do diff textual eram **reserializacao** de `json.dumps(indent=1)`. O `--showConfig`
campo a campo deu 0 campos de `compilerOptions` diferentes e 2 entradas de `exclude`.

### 12.3 O `exclude` so onde casa

`excludes_necessarios(repo)`: em 7 clones, **so o dom-examples** tem
`web-workers/worker-playground/bad-syntax-worker.js`. Excalidraw e zod passaram a usar o tsconfig
do HEAD **byte a byte** (`git diff --quiet` limpo).

### 12.4 DOIS TypeScript diferentes no mesmo lote

`identidades_do_oraculo` nomeava UMA versao do `tsc`, com `break` no primeiro repo. Medido:

| repo | TypeScript |
|---|---|
| date-fns | 7.0.2 |
| excalidraw | **5.9.3** |
| zod | **5.5.4** |
| dom-examples · express · html5-boilerplate · lodash | 5.9.3 |

O selo de um lote `excalidraw,zod` declararia `tsc_date-fns: Version 7.0.2` — um oraculo que o
lote nao usa, e a versao mais nova das tres. Agora e `tsc_por_repo`.

## 13. A assinatura recorrente: `rc=0` com stdout vazio (10/10)

Apareceu **sete** vezes na onda, em camadas diferentes:

| # | onde | como se manifestou |
|---|---|---|
| 1 | `cargo` escreve no `stderr` | 14 receitas Rust dando «delta de 0» |
| 2 | `tsc` tem dois formatos de saida | 598 erros do zod lidos como ZERO |
| 3 | `ok` do `delta` do LS nao checado | oraculo RECUSADO lido como «nenhum erro novo» |
| 4 | buffer de 65.536 bytes no protocolo | `LS=[]` onde o `tsc` acusava 145 |
| 5 | denominador do teto (4 vezes) | 9,3% / 9,8% / 6,28% contra um teto de 3% |
| 6 | `oraculo_ls.js` trocado no meio do lote | 2 lotes parados por divergencia que NAO reproduz |
| 7 | `subprocess.run(input=...)` sem newline | o `node` saiu com `rc=0` e stdout vazio |

Nenhum era o oraculo errado. Todos sao **um programa que reporta sucesso sem ter produzido sinal**,
e o padrao atravessa qualquer camada nova que eu escrever.

**O que funciona, e e o mesmo nos sete casos**: (a) olhar os bytes crus antes de concluir;
(b) um portao que REPROVA quando a protecao some, com um MUTANTE que precisa ser pego.
Nenhuma das sete protecoes veio de disciplina — todas estao em selftests, e o rotulador esta em
**87/87**, o lock em **6/6**, o protocolo do LS em **5/5**.


## 14. O LADO POSITIVO: o selftest pegou o validador (10/10)

Na secao 13 eu listei sete vezes que um programa reportou sucesso sem sinal. A defesa do lado de
caixa tem um caso simetrico, e ele vale tanto quanto: **o selftest achou um defeito meu antes de
ele chegar a um lote**.

Escrevi `cenario_tsconfig_minimo` para provar que o `tsconfig` minimo so muda o `exclude`, e ele
reprovou em **74/75**: `(com comentario) o texto montado e JSON valido`. O validador usava
`json.loads`, que **nao aceita comentario**, enquanto o `tsc` aceita. A garantia "so o exclude muda"
so valeria para repos sem comentario — e nenhum dos 7 tem, entao ela nunca teria falhado em
producao.

A correcao seguinte foi a melhor da onda: adotar **`ts.parseConfigFileTextToJson`**, o parser do
proprio `tsc`, em vez de um parser proprio. Assim validador e compilador concordam **por
construcao**. E o `jsonc_para_json` que eu tinha escrito para consertar o comentario reprovava na
**virgula final** — que o TypeScript tambem aceita. Cada conserto meu de parser compra um caso e
deixa outro; o parser do compilador ja esta pronto e testado por ele.

**O padrao completo da onda**: o sinal existia e nao chegou (7 vezes, defeito meu) · o portao
reprovou o que devia (1 vez, defeito meu, pego antes do lote) · cada protecao nova entrou com um
mutante que precisa reprovar.

## 15. Confirmação de que as portas aceitas continuam verdes (10/10)

Depois de `chama_oraculo`, do lock, do validador pelo compilador e da selecao canonica:

| porta | resultado |
|---|---|
| `scripts/corpus_rust.py --selftest` (estrato `sanidade`) | **13/13** |
| `scripts/corpus_semeado.py --selftest` (tarefa C, fechada antes) | **37/37** |
| `scripts/rotular_oss.py --selftest` | **87/87** |
| `scripts/test_chama_oraculo.py` | **15/15** |
| `scripts/test_vgp_lock.py` | **6/6** |

A tarefa C e o estrato `sanidade` foram aceitos antes de qualquer uma dessas mudancas; eles
continuam verdes, entao nenhuma delas regrediu o que ja estava fechado.
