# RETOMADA — V-8 SEM2 (continuação: touring-d1 → esta sessão)

> Estado em disco para a coordenadora. O item é o **V-8 (SEM2)** da §1 das pendências
> consolidadas. O relatório do item é `docs/plans/2026-10-08-vgp-motor-base/RELATORIO-V8-SEM2.md`
> (a §5 é o que esta sessão acrescenta).

## RESUMO FINAL (01/11/2026) — o item V-8 SEM2 está FECHADO

**O critério foi cumprido numa classe: `typescript` saiu de «não provada».**

| classe | n | negativos | situação |
|---|---:|---:|---|
| **typescript** | 604 | **394** | **PROVADA** (>= 299, dentro do teto, selo conferido) |
| `tsx` | 488 | 152 | NÃO PROVADA — falta ~480 entradas (taxa medida de 31 %) |
| `javascript` | 24 | 23 | NÃO PROVADA — falta população |
| `html` | 3 | 3 | NÃO PROVADA — **sem adjudicador próprio** (D-8) |

- **Selo** (externo, fora do corpus): `sha_tree` **`e298fe3b6fe48d58`**, 2 150 arquivos,
  40 449 756 bytes — `confere_selo.py` → **INTACTO** (rc=0).
- **Disjunção**: **DISJUNTA** — 0 ids em comum com os 8 901 ids do histórico de corporas.
- **Teto de 3 %**: 0 infratores em todas as classes (medido, não declarado).
- **Relatório**: §5.6 do `RELATORIO-V8-SEM2.md` escrito com o n por classe, a concentração
  medida e o hash do selo. **Commit**: `ebda1d37`.
- **O que falta** (não depende do método): alvo total ~1 700 para levar `tsx` e `javascript`
  aos 299 negativos. O `html` depende do **D-8** (adjudicator próprio), cujo plano está em
  `docs/plans/2026-10-08-vgp-motor-base/PLANO-ADJUDICADOR-HTML.md` — **antes** de qualquer
  código, como o Gabriel pediu.
- **Planos de V-6, V-7, V-9 e V-12**: `docs/plans/2026-10-08-vgp-motor-base/PLANOS-V6-V7-V9-V12.md`.

## 0. O que a sessão ANTERIOR (touring-65) deixou — não repetir

1. **O selo EXTERNO do held-out web** (`scripts/selar_holdout_web.py`):
   `~/.claude/touring/vgp/corpus_holdout_web_20261012.selo.json` — **fora** do corpus, no formato
   do `confere_selo.py` (`sha_tree`/`arquivos`/`bytes`), com os denominadores e a concentração
   **medida**. Conferido: **INTACTO** (rc=0, `sha_tree 42106741066f40f9`). Os denominadores por
   classe estão em `corpus_holdout_web_20261012.denominadores.json`.
2. **As classes do estrato `transcript` como NÃO PROVADA** (html n=83, javascript n=21;
   `n_para_cp95_1pct`=299), com o motivo escrito e sem inflar o número. O alvo de 300 é
   **inalcançável** nessa população (medido: a população inteira do estrato é 83+21/23).
3. **A concentração OSS medida por camada** — e o **diagnóstico que a camada antiga estava acima
   do teto**: confirmado nesta sessão por medição própria (`selar_camada_oss.py`): **tsx 1
   infrator** (chave com 7 contra o teto medido 6) e **typescript 1 infrator**. A camada antiga
   declara `teto_por_chave_no_fim` = 7, mas `teto_de(213, 0.03)` = 6 — declarar e medir são
   coisas diferentes, e só a medição restringe.
4. **A camada OSS antiga** (`corpus_holdout_web_20261012/oss/`): tsx n=213 (169 negativos),
   typescript n=214 (169 negativos), javascript n=3 (3 negativos). **DISJUNTA** do
   `corpus_semeado` (446 ids, verificado).

## 1. O que foi FEITO nesta sessão

### 1.1 A CAUSA RAIZ do run anterior vazio (encontrada e corrigida)

O run do V-8 anterior (que voltou vazio) **não era lento: era um travamento silencioso**, e a
causa está no leitor de frames do adjudicador:

- o `oraculo_ls.js` declara o comprimento do corpo como `#<n>` com **`Buffer.byteLength`**
  (**bytes**) e escreve o corpo em UTF-8;
- o rotulador (`scripts/rotular_oss.py`, classe `LS`) abria o pipe com `text=True` e lia
  `stdout.read(n)` — que pede **n CARACTERES**;
- num corpo com caractere multibyte (acentos), os caracteres são **menos** que os bytes
  declarados: o pipe esvaziava e o `read` bloqueava **para sempre** — node VIVO e ocioso
  (`ep_poll`), Python em `anon_pipe_read`, CPU ~0, zero erro, zero saída.

**Corrigido**: pipe **binário** (`text=False`), `read(n)` em **BYTES**, e o `\n` que o oraculo
escreve depois do corpo é consumido explicitamente (o `\n` sobrado virava o «cabeçalho» do frame
seguinte). E `chama_oraculo.ler_framing` passou a comparar `len(corpo.encode("utf-8"))` contra `n`
(o `len(corpo)` em caracteres acusava «truncado» num frame inteiro com acento).

**Prova**: `scripts/test_rotular_oss_framing.py` — **6/6**, e o **mutante em modo texto TRAVA**
(sem travar, o teste passaria sem provar nada). `rotular_oss.py --selftest` **92/92**,
`test_chama_oraculo.py` **15/15**, suíte do bundle **259 passed / 0 failed**.

### 1.2 A DISJUNCAO entre corpora (não existia)

MEDIDO: o rotulador **não tinha** nenhuma disjunção entre corporas — `estado_no_disco` lê SÓ o
diretório de saída e o `grava_entrada` **reescreve** o id em vez de recusar o par. Ou seja, a
camada nova repetiria a antiga par a par.

**Corrigido** (opt-in, o comportamento padrão fica intacto): flag `--excluir-corpus` (repetível),
que tira da **população** os pares que um corpus anterior já rotulou, **antes** da seleção — então
o par excluído não consome oracle nem entra no teto (o teto mede o que foi ROTULADO). O `id` da
comparação é o mesmo do `grava_entrada` (`ident_do_par`), e a política fica **no selo**.

**Prova**: `scripts/test_rotular_oss_exclusao.py` — **11/11**, incluindo «o par EXCLUIDO não
chega ao adjudicador» (o único jeito de provar a disjuncao) e «sem a flag, NENHUM par é
descartado» (controle negativo).

### 1.3 O selo externo da camada OSS

Novo `scripts/selar_camada_oss.py`: selo no formato que o `confere_selo.py` aceita
(`sha_tree`/`arquivos`/`bytes`, **fora** do corpus), com os denominadores por classe, a
**concentração medida** e a **disjunção medida**. O `n` que decide a prova é o dos
**NEGATIVOS** (o falso deny só existe sobre um negativo) — o `n` total quase dobra a força da
prova sem acrescentar um único falso deny possível.

### 1.4 O launcher reproduzível

`scripts/rodar_rotulacao_oss_20261101.sh` (o comando exato do lote, com o log **ao lado** da
saída, e sem tocar no daemon global).

### 1.5 A DIVERGÊNCIA que matava o lote — e o adjudicador que NÃO é o congelado

O primeiro lote desta sessão (11:26→11:56, BRT) **parou com 218 entradas**: a **divergência**
entre o adjudicador LS e o `tsc` de referência. No `excalidraw/packages/excalidraw/actions/
manager.tsx`, o LS acusa `TS2307` de módulos (`../utils`, `../element/types`) que o `tsc`
resolve — a causa provável é o **programa do LanguageService ser cacheado**: um arquivo criado
pelo commit não está na lista do programa, então o módulo não resolve no LS e resolve no `tsc`.

**O desfecho antigo era PARAR O LOTE INTEIRO.** E o efeito medido foi **nenhum corpus**: um
corpus que não existe não prova nada. Agora a divergência **EXCLUI o par** (que é o item
suspeito) e o lote continua — fail-closed no nível do par, que é onde a divergência vale. O que
não pode é o par entrar. A divergência fica **registrada** no relatório do repo e no selo.

**E um achado que muda a comparação entre as camadas**: o adjudicador vivo
(`~/.claude/touring/vgp/oss/oraculo_ls.js`) é **byte a byte igual** à cópia versionada do bundle
(sha `80976ec9…`, 10 580 bytes, mtime **09/10 23:39**), e a camada **antiga** foi feita com
`8e8dd5f1…` (7 884 bytes). **O adjudicador foi trocado DEPOIS da camada antiga** (o fix do
framing de 64 KB, medido em 10/10) — **as duas camadas não foram feitas pelo mesmo oráculo**, e
por isso os números delas não são comparáveis sem declarar isso.

### 1.6 A COLETA: 3 spawns por par → 1 processo

A coleta era `git rev-parse` (o pai) + 2 `git show` (antes e depois) **por par**: 17 670 pares em
mc=600 são ~50 000 spawns, e 1,5 h dos 2,4 h de coleta eram de repos cujas linguagens já
estavam cheias (o `por_lang >= alvo` só é conferido **depois** da coleta). Agora o pai e a **data**
de cada commit vêm no **mesmo log** (`%P`, `%ct`) e o conteúdo de todos os pares vem em **um**
`git cat-file --batch` — o mesmo blob, byte a byte (o teste compara com o `git show`).

**E um defeito que o número expôs**: o `datas` **nunca era preenchido**, então o `data_commit`
era sempre `0` e a «ordem canônica» era **alfabética por caminho** — o docstring prometia a data
do commit na chave de ordem. Agora a ordem é a que foi pedida: pela data, depois pelo caminho,
com o `id` desempata.

### 1.7 A camada parcial tinha DOIS rotuladores no mesmo diretório

Sinal: a camada tinha os **primeiros 12 índices repetindo caminho** (o teto não se aplicava) e do
índice 12 em diante **não repetindo** (o teto se aplicava) — dois comportamentos na MESMA
camada. Causa: dois `rotular_oss.py` escrevendo no mesmo `--saida` (um relançado sobre o outro
sem matar o anterior), **cada um com os seus contadores**. O teto é contado por processo e o
arquivo é compartilhado: o que um processo contou, o outro não viu.

Duas consequências operacionais, ambas agora no launcher e no procedimento:

1. o **runner** (`bash …/rodar_rotulacao_oss_20261101.sh`) tem de ser morto **junto** com o
   rotulador — o wrapper continua vivo e relança;
2. o `mv` do diretório de saída tem de acontecer **depois** de o processo antigo morrer, senão
   o processo velho continua escrevendo no caminho (que agora aponta para o diretório novo).

A camada foi descartada (guardada como `oss_rotulado_20261101-parcial3a-2processos`) e refeita
com **um** processo, verificado por `ps` antes de continuar.

## 2. ENCERRADO — o lote terminou e a camada foi selada

O lote de re-rotulação (js/tsx/ts) **terminou** (1 119 entradas, 572 negativos) e a camada foi
**selada e conferida**. O §5.6 do `RELATORIO-V8-SEM2.md` **está escrito** com os números finais.

```
bash scripts/selar_e_conferir_camada_oss_20261101.sh
```

- saída: `~/.claude/touring/vgp/oss_rotulado_20261101/`
- log: `~/.claude/touring/vgp/oss_rotulado_20261101.log` (+ `.log.err`), runner em
  `oss_rotulado_20261101.runner.log`;
- selo **fora** do corpus: `~/.claude/touring/vgp/oss_rotulado_20261101.selo.json`
  (`sha_tree` **`e298fe3b6fe48d58`**, 2 150 arquivos, 40 449 756 bytes);
- camadas anteriores guardadas para o registro: `-parcial1a-divergencia`, `-parcial2a-teto`,
  `-parcial3a-2processos`, `-parcial4a-alvo450`.

### 2.1 O resultado final (medido, não estimado)

| classe | n | negativos | taxa | situação |
|---|---:|---:|---:|---|
| **typescript** | 604 | **394** | 65 % | **PROVADA** (>= 299, dentro do teto, selo conferido) |
| tsx | 488 | 152 | 31 % | NÃO PROVADA — faltam ~480 entradas (taxa baixa) |
| javascript | 24 | 23 | 96 % | NÃO PROVADA — falta população (os repos de js vêm no fim da ordem) |
| html | 3 | 3 | — | **NÃO PROVADA por falta de adjudicador** (D-8) |

**Disjunção: DISJUNTA** — 0 ids em comum com os 8 901 ids do histórico de corporas.
**Teto**: 0 infratores em todas as classes (o teto é `max(1, int(n × 0,03))` entradas por chave).
`confere_selo.py` → **INTACTO** (rc=0) nos dois comandos.

### 2.2 O que ficou para continuar (e o que NÃO depende do método)

Para levar `tsx` e `javascript` aos 299 negativos o alvo total precisa subir de 1 200 para
**~1 700** (o `tsx` sozinho precisaria de ~965 entradas, pela taxa medida de 31 %). Não é
problema de método: é **população e tempo de adjudicador**. O comando de fechamento acima
continua válido — a camada é selável a qualquer momento, e o selo é sobre o que está em disco.

## 3. O PLANO do adjudicador html (D-8, decidido pelo Gabriel em 01/11) — **antes do código**

**`docs/plans/2026-10-08-vgp-motor-base/PLANO-ADJUDICADOR-HTML.md`**

Em uma linha: o html fica com as classes que dependem **só do documento** (`ancora`, `cdn_pin`);
`recurso` e `classe_css` exigem a árvore do **commit**, e com o clone em HEAD o rótulo viria do
drift da árvore, não da edição — desvio declarado até a segunda camada. A independência do
adjudicador é de **implementação** (parser diferente, tokenização pela regra do juiz, controles
sintéticos com mutante, e o Nu `vnu` — que o `java` desta máquina permite — como **controle
cruzado**, nunca como fonte de rótulo, porque o motor do VGP não reporta erro de spec e o recall
sairia 0 por construção). A população medida é curta (518 pares html em mc=600 ⇒ ~260 negativos
abaixo de 299), então **mais repositórios públicos com html são necessários, escolhidos por
medição**.

## 4. Os planos de V-6, V-7, V-9 e V-12 (pedidos antes de começar)

**`docs/plans/2026-10-08-vgp-motor-base/PLANOS-V6-V7-V9-V12.md`**

- **V-6 (VRS)**: o recall `nome` é 5/20 = 0,25 contra o piso de 0,70; a causa dominante é
  `use super::*` não resolver (204 dos 344 ids naturais). O plano é resolver isso (RED→GREEN +
  medida por id), gerar os estratos que **não existem** (o `real`, e sobretudo **negativos Rust no
  balde de deny** — hoje o «0/307 falsos deny» é vazio quanto ao balde onde o motor nega), e
  consertar o contrato corpus/julgador (`contexto` como string quebra o juiz).
- **V-7 (VHTML)**: o patch de CLI non-UTF-8 (§2.10) não está no worktree (1 238 HTML Latin-1
  voltariam como `lang: python`); o teto de 1 % não está provado (n = 200 ⇒ 1,49 %); «HTML nunca
  nega» foi provado em `check`, não por sessão real.
- **V-9 (HORUST)**: julgar **uma vez** os 184 ids primários + 340 negativos, selar o estrato `cfg`
  (ou registrar a exclusão) no formato do validador atual, com o pré-registro congelado antes.
- **V-12**: o aceite TS/TSX/JS depende do n que o **V-8** entrega; `.d.ts`/`Alvo::Externo` é desvio
  declarado; o juiz web precisa de controles reais (CDN no código do repositório, CSS-in-JS com
  pacote real); a VL0 camada 2 (precisão, concorrência, daemon, custo do rust-analyzer) sob
  `hw-slot`.

**Três decisões são da coordenadora** (no fim do arquivo dos planos): JS clássico aviso ou
abstenção; as 4 causas de ruído de `warn` do Rust; e (agora resolvido) o adjudicador html.

## 5. DOIS ACHADOS que a próxima sessão precisa saber

1. **A camada OSS atual não é consumível pelo juiz como está.** O juiz **já suporta** o contrato
   do `--disk-file` pela entrada (`juiz_motor_vgp.py:180-182`: `elif entrada.get("snap_antes")`),
   mas a camada do rotulador tem **três campos a menos** do que o juiz lê:
   - **`snap`** (o conteúdo depois) — o juiz faz `corpus / entrada["snap"]`; a camada tem
     `conteudo` (`conteudo/<id>.txt`), outro nome;
   - **`project_root`** — o juiz passa `--project-root <root>` e não tem como obtê-lo;
   - **nos positivos**: `rotulo` é `"positivo_natural"` e o juiz filtra `e["rotulo"] == "positivo"`
     (e exige `reproduz == "true"`); e **não há `nome_esperado`** — sem ele não há recall.

   O **falso deny** (o denominador do V-8) **não é afetado**: o critério do V-8 é sobre
   NEGATIVOS, e o rótulo `"negativo"` está certo. Mas o **recall** de TS/TSX/JS (V-12) depende
   de corrigir o formato. O modelo do formato certo é
   `~/.claude/touring/vgp/corpus_semeado/html/entradas.jsonl`.
2. **`contexto` tem de ser LISTA.** O juiz faz `for c in entrada.get("contexto") or []`; a string
   `"[]"` quebra (o mesmo defeito que o `RELATORIO-VRS.md` §6.1 registra para o corpus Rust).

## 6. Regras que continuam valendo

- **AB34 armado até 08/11 00:10**: nenhum `update-touring`, `propagate-release.sh`,
  `touring daemon-ctl restart|stop`, nenhum daemon spawnado à mão.
- **`cargo build --release` no `target/` do workspace principal**: PROIBIDO (o daemon global
  executa esse binário). `cargo test/check/clippy` em debug no principal são livres; `hw-slot`
  para job pesado.
- **Nenhum conteúdo de corpus no repositório.** Commit com `git commit -- <paths>`; nunca
  `reset`, nunca `--amend`.
- O disco está em ~88 %: cache de worktree que não se usa pode ser limpo com `cargo clean`
  dirigido.

## 7. Security review de 01/11 — respondida com MEDIÇÃO (achado do item I-4/I-5, não desta sessão)

O review automatizado apontou «Injection policy bypass / untrusted marker spoofing» em
`crates/touring-foundation/src/injection_family.rs` e no espelho `client/hooks/injection_policy.py`:
o marcador de texto (`[SUGESTÃO DE SKILL`, `[PORTFOLIO ·` → família `plugin-hook`) seria usado
como prova de confiança.

**Medido** (`allows_payload` do espelho Python, em `no-advisory`):

| payload | `full` | `no-advisory` |
|---|---|---|
| com o marcador (spoofado) | permite | **CORTA** |
| de fato (`pre-edit`) | permite | permite |
| desconhecido | permite | permite |

**O marcador de texto só CORTA; ele não concede permissão nenhuma.** E `allows("plugin-hook")`
é decidido pelo **nome literal** da família (o hook perguntando por si), não por uma família
derivada do texto de um chunk. **A correção sugerida pelo review abriria fail-open**: hoje um
payload desconhecido passa e um com o marcador é cortado; tirando o match por texto, o payload
com o marcador passaria como «desconhecido».

**O resíduo real (menor, e de integridade de MEDIDA, não de privilégio)**: a **atribuição** da
família — a régua do KPI e o motivo atribuído a um DENY — pode vir do TEXTO quando o emissor é
desconhecido, então conteúdo não confiável que reproduza o marcador polui a **medição** da
família `plugin-hook` (que era o item I-5). O `attributed_family` já prefere o emissor quando
ele tem `Claim::Owns` ✓; o resíduo é só o caminho do emissor desconhecido. **Isso é do item
I-4/I-5 (outra sessão), não do V-8** — fica aqui registrado para não se "corrigir" ao contrário.
