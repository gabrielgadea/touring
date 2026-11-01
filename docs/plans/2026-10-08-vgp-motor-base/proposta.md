---
type: Plan
title: VGP na base do Touring — um motor por linguagem, aplicado no executor
description: Plano aprovado pelo Gabriel em 08/10/2026 e ampliado em 09/10/2026 (Python, Rust, TS, TSX, HTML; versões de CDN e classes CSS). Motor VGP na base do Touring (vale em qualquer projeto), inventário de TODOS os erros das sessões e correção dos que ainda vivem, com /tmp tratado como trabalho de sessão.
plan_id: 2026-10-08-vgp-motor-base
timestamp: 2026-10-08T17:30:00-03:00
updated: 2026-10-09T17:30:00-03:00
tags: ["#kind:plan", "#status:active", "#domain:harness"]
---

# VGP na base do Touring — plano aprovado

| | |
|---|---|
| **Estado** | aprovado por Gabriel em 08/10/2026 («tudo aprovado»); **ampliado por ordem dele em 09/10/2026** para Python, Rust, TS, TSX e HTML, com versões de CDN e classes CSS (§10); em execução |
| **DAG** | `task_1791484992466538442` (27 subtarefas; criado do cwd `~/projects/analise`, cujo daemon o guarda) |
| **Retomar** | [`RETOMAR-AQUI-2026-10-09.md`](RETOMAR-AQUI-2026-10-09.md) (estado vivo, adendos datados) · [`RETOMADA.md`](RETOMADA.md) (inicial) |
| **Coordenação** | analise-89 (desde 09/10 tarde; antes analise-6b): planeja, julga cada entrega pelo próprio juiz e conduz os releases · analise-0e (Kimi): motor V1–V5 · analise-25 (MiniMax): V0c, destino das 606, negativos inline · subagentes sonnet: crate `touring-pyright` (`v3-camada2`), G1 sem replay (`g1-ajustes`), lexer de shell (`shell-inline`) |

## As ordens

1. Na analise-a9, 18:22 UTC: enviar a opção «c │ cobre o caso Python │ dilui: o registro de structs da regra é todo
   Rust (params.rs, graph.rs)» «para a sessão de coordenação investigar e propor uma ampliação estrutural para as
   principais linguagens, e que seja de fato eficiente e efetiva».
2. «o motor VPG precisa estar na base do Touring, para funcionar em qualquer projeto».
3. «tudo aprovado».
4. «a contagem deve considerar todos os erros, pois precisamos corrigir tudo».
5. «considere o que é escrito em /tmp, pois isso faz parte do trabalho que é realizado em todas as sessões».
6. «da mesma forma como incluímos o .temp/ faça uma verificação em .cache/» — o `<projeto>/.cache/` recebe o mesmo
   tratamento do `/tmp` (o `~/.cache` da home NÃO: são caches de ferramenta — uv, pip, hf — e ficam externos).
7. «prossiga até todo plano estar completo e perfeito e ao final me prove, na prática, que tudo funciona de forma
   completa e perfeita» (/goal, 09/10) — o fechamento é o juiz J por exit code **e** a prova prática
   [`scripts/prova_vgp_motor.py`](scripts/prova_vgp_motor.py) no binário instalado.
8. «investigue e encontre uma solução para» a dívida do `programa_inline` (`--lang` 1.133 · consumidor do `<<`
   `cat`/`tee` 665 + 414) — o lexer [`scripts/shell_inline.py`](scripts/shell_inline.py) (V5).
9. «encontre uma solução para o bloqueio arquitetural, mesmo que isso signifique refatorar o touring» — a crate
   `touring-pyright` sem dependência `touring-*` (V3).
10. «lembre-se de utilizar a delegação para agentes com outros modelos, não somente para as outras sessões» —
    implementação com spec fechada vai a subagente `sonnet`; a coordenação decide e julga.
11. «mas o VGP vai funcionar só para python? preciso que ele funcione para as principais linguagens (python, rust,
    ts, html), pelo menos» (§10).
12. «confira as versões de CDN e as classes CSS referenciadas; e também inclua tsx» (§10.1).

## 1. Quanto cada linguagem é editada (17/09–08/10/2026)

Denominador = chamadas Edit/Write/MultiEdit (quanto a verificação de cada linguagem é necessária), não contagem de
arquivos. Instrumento: [`diagnostics/medir_vgp.py`](diagnostics/medir_vgp.py).

| projeto | transcripts | edições | Python | Rust | TS/JS |
|---|---:|---:|---:|---:|---:|
| analise | 1.027 | 16.516 | 13.347 (80,8%) | 338 (2,0%) | 12 (0,1%) |
| touring | 527 | 2.939 | 1.010 (34,4%) | 1.157 (39,4%) | — |

Erros de API inventada (erro só em resultado de **Bash**; negação só em resultado de Edit/Write — um Read/grep que
ecoa um traceback antigo é menção, não evento):

| erro | analise atribuído · inline | touring atribuído · inline |
|---|---|---|
| Python atributo (`AttributeError … has no attribute`) | 27 · **40** | 6 · **13** |
| Python import de nome (`cannot import name`) | 2 · 4 | 0 · 0 |
| Python kwarg/assinatura | 1 · 3 | 0 · 0 |
| Python nome indefinido (`NameError`) | **6** · 6 | **11** · 0 |
| Rust campo/método/assinatura/import | 4 · — | 19 · — |

Duas leituras decidem o desenho:

1. **O gate na edição funciona, onde existe.** O analise tem o `python_pre_gate.py` (ruff F82, local ao projeto),
   que negou 396 edições com nome indefinido; ficaram 6 NameError atribuídos em 13.347 edições (0,045%). O touring
   não tem esse gate: 11 em 1.010 edições (1,09%) — **~24× mais por edição**. O lugar do motor é a base.
2. **Código inline erra mais que código editado** (atributo: 40 contra 27 no analise; 13 contra 6 no touring). Um VGP
   só em Edit/Write cobriria menos da metade do que é atribuível.

## 2. Inventário de TODOS os erros (V0a — feito)

Instrumento: [`scripts/inventario_erros.py`](scripts/inventario_erros.py) (selftest com 13 controles: um positivo
por formato, negativos para prosa e código externo, envelope do `touring run`). Retrato de 08/10 ~17:20 BRT em
[`diagnostics/V0a-inventario.txt`](diagnostics/V0a-inventario.txt); bruto, fora de repositório (as mensagens podem
carregar dado de processo): `~/.claude/touring/vgp/erros_inventario.jsonl`.

**Universo**: 1.555 transcripts (analise + touring, com subagentes) · 41.622 resultados de Bash.

**Erros de linguagem** (traceback Python, linha do pytest, `rustc error[E…]`, `tsc TS…`, inclusive os que o
`touring run` devolve dentro do JSON): **1.450 ocorrências em 862 distintos**.

| balde (predicado positivo) | analise | touring |
|---|---:|---:|
| atribuído — o arquivo do erro foi editado antes na mesma sessão | 267 | 166 |
| inline — o frame mais fundo é `<stdin>`/`<string>` (Bash, `touring run --code`) | 226 | 76 |
| projeto — arquivo do projeto ou de `/tmp` não editado naquela sessão | 84 | 23 |
| externo — só bibliotecas, stdlib, `~/.local/share/uv` | 17 | 3 |

Classes mais frequentes (distintos): AssertionError 146 · PermissionError 83 · AttributeError 81 · KeyError 68 ·
TypeError 65 · ModuleNotFoundError 60 · FileNotFoundError 51 · ValueError 38 · NameError 33 · SyntaxError 28 ·
rustc E0425 28 · rustc sem código 26.

**Falhas de Bash sem diagnóstico de linguagem: 3.090**, por predicado positivo e em ordem fixa:

| classe | n |
|---|---:|
| hook/guard negou (gates do Touring, guards de shell) | **2.484** |
| `touring run` falhou (sem traceback no envelope) | 220 |
| arquivo/caminho inexistente | 55 |
| permissão do Claude Code negou (`rm -rf`, `Force operation`, `requires approval`) | 53 |
| is_error com saída de sucesso (outro comando do pipeline falhou) | 34 |
| pytest falhou (só o sumário no recorte) | 22 |
| sintaxe do shell | 15 |
| git · timeout · grep/test sem casamento · sandbox · entrada inválida · traceback cortado · comando inexistente · CLI · cargo | 13 · 11 · 8 · 6 · 6 · 5 · 4 · 4 · 2 |
| **não classificado** (visível, nunca somado; 137 primeiras linhas distintas) | 148 |

### 2.1 As negações de gate (V0e — em curso)

Instrumento: [`scripts/v0e_negacoes.py`](scripts/v0e_negacoes.py); retrato em
[`diagnostics/V0e-negacoes.txt`](diagnostics/V0e-negacoes.txt). Uma negação é o que a **própria fonte anuncia no
início de linha** — prefixo de hook (`PreToolUse:… hook error`), etiqueta de gate (`[G<n> …]`) ou frase de permissão do
Claude Code; a 1ª versão aceitava «block»/«guard» soltos e contava log do CEG, asserção de pytest e erro do WebSearch
como negação. Em **todas** as ferramentas: **3.870 em 346 transcripts**; 43 (1,1%) sem gate identificado, visíveis.
«Rota seguida» = a chamada seguinte da mesma sessão faz o que a **própria mensagem do gate** ensina (lida em negações
reais, nunca suposta); «reincide» = o mesmo gate de novo em ≤ 5 min na mesma sessão.

| gate | n | transcripts | reincide | rota seguida | rota que a mensagem ensina |
|---|---:|---:|---:|---:|---|
| G1 rajada-de-inspeção | **1.776** | 303 | 38% | 44% | `touring run` com o programa acumulado (já montado na mensagem) |
| G7 re-inspeção (3ª leitura) | 457 | 118 | 20% | 49% | `touring run --file <r1_varredura_agregado.py>` |
| G3 edit sem Read recente | 428 | 48 | 39% | 51% | `Read <arquivo>` (nomeado) |
| python_pre_gate (F82…) | 402 | 46 | 43% | **85%** | o erro exato (`F821: Undefined name X`) |
| G11 orçamento de bypass | 172 | 79 | 15% | 39% | `touring run --file <script>` |
| OUTER · efeito (quebra-molas proposital) | 127 | 28 | 13% | 82% | refazer a mesma edição |
| write→run (script escrito-e-executado) | 122 | 75 | 10% | 34% | `touring run` |
| escrita cega inline (`sed -i`…) | 120 | 62 | 2% | 72% | Edit |
| guard de `rm` | 97 | 50 | 2% | 21% | `rm <arquivo>` / `rmdir` / o usuário roda |
| touring-quality-block-all | 63 | 29 | 16% | 48% | — |
| git · prosa (CJK) · retry cego · agent_model_gate | 26 · 12 · 4 · 2 | | | 38% · 100% · 0% · 50% | |
| **guards de segurança sem rota** (`dd`, pipe→shell, `--force`, Windows delete) | 19 | 15 | 5% | **0%** | **nenhuma** |

**Correção de 09/10/2026 — a régua creditava ao G1 o que era de outros gates.** O `gate_of` testava os padrões em
ordem com `re.I`, e o G1 (1º da lista) tinha a alternativa `RAJADA`: qualquer "rajada" casava — o
`[CODE MODE · rajada]` do S3 e o `[G10 exec-burst] … a rajada desenrolada`. Achado pela irmã analise-25 na V0e5
(controle positivo nomeado); consertado na régua por TDD (a etiqueta do gate tem precedência sobre o texto livre;
S3 com entrada própria; `--selftest` 6/6, RED 4/6 antes, 2 mutantes mortos). Re-rodada em 09/10 ~18:30Z (4.054
negações em 352 transcripts, até o instante): **S3 1.282 · exec-burst 405 · G1 182** — o «G1 1.776» acima era
essa soma. A maior alavanca era o **S3** (32% das negações), que desde a 30.4.95 avisa em vez de negar; o G1 de
verdade é 4,5%.

**Leituras.** (1) O **G1 é 46% das negações** e menos da metade segue a rota — a maior alavanca (⚠ corrigido acima:
eram S3 + exec-burst + G1). (2) A recusa que
nomeia o erro exato é seguida em 85%; as genéricas, entre 21% e 51%. (3) Quatro guards de segurança **não nomeiam
rota nenhuma** — violam a diretriz A5/D8 (a recusa ensina o caminho de volta). (4) O G7 ensina com um placeholder
(`<r1_varredura_agregado.py>`) quando o caminho é derivável — viola o invariante de densidade das injeções.
(5) Limite do instrumento: «rota seguida» olha só a chamada seguinte; rotas legítimas não listadas (p.ex. `: >` ou
`unlink` depois do guard de `rm`) contam como não seguidas — o número é piso, não estimativa.

**Consertos propostos (a aprovar — mexem em gates):**

| gate | conserto | por quê |
|---|---|---|
| G1 rajada | o hook **executa a rota** em vez de negar: reescreve o comando para o `touring run` do programa acumulado (`updatedInput`), como o code-mode-loop já faz com laços | 1.776 idas e voltas; a mensagem já monta o programa — o executor pode rodá-lo (D8: enforcement no executor) |
| G7 re-inspeção | nomear o esqueleto R1 por caminho real (sem placeholder) | invariante de densidade; 49% de rota seguida |
| guards sem rota | cada recusa nomeia a alternativa segura (`dd` → `cp`/`truncate`; pipe→shell → baixar, ler, depois rodar; `--force` → o comando sem `--force` ou o usuário roda) | 0% de rota seguida porque não há rota |
| guard de `rm` | aceitar `: >`/`unlink` como rota e dizê-lo na mensagem | 21% medidos sobre uma rota estreita demais |
| G3, G11, write→run | medir de novo depois do G1 (parte das reincidências nasce da mesma rajada) | evitar consertar sintoma |

## 3. O que ainda vive no código de hoje (V0b — feito)

Instrumento: [`scripts/vivos.py`](scripts/vivos.py) (selftest: um atributo inventado num arquivo real sai
`vivo-estático`; um atributo real, não). Cada erro Python que aponta arquivo — do projeto **ou de `/tmp`**, resolvido
contra o projeto da sessão que o escreveu — recebe um status por predicado positivo:

| status | n | predicado |
|---|---:|---|
| vivo-estático | **0** | pyright no arquivo de hoje acusa o mesmo nome na mesma família de regra (controle positivo ok) |
| teste vermelho hoje | **0** | o teste reroda e falha (47 passam; 1 não é coletado) |
| candidato | **71** | erro de execução cuja linha continua intacta → exige reprodução |
| sem evidência | **26** | o traceback não trouxe a linha de código |
| linha mudou | 39 | a linha do traceback não existe mais no arquivo |
| arquivo removido | 260 | dos quais **213 em `/tmp`** — o conteúdo continua nos `Write`/`Edit` dos transcripts e entra no corpus da V0d |

## 4. O que o Touring verifica hoje (subagente sonnet com arquivo:linha; 3 pontos reconferidos)

- **Nenhum hook compara os nomes que o código novo referencia (atributo, kwarg, parâmetro, import) com as
  definições, em linguagem nenhuma.** O único deny de código é o do `pre_edit_prevention.rs:415-440` (árvore
  tree-sitter que estava limpa e passa a ter erro de sintaxe).
- As camadas `SymbolResolution` e `ImportCheck` do speculate existem, mas **as 5 chamadas dos hooks passam
  `speculate_v2(…, None, None)`** e elas passam no vazio (`post_edit.rs:1162`, `pre_write.rs:260,780`,
  `pre_edit_prevention.rs:542`, `post_write.rs:711`).
- Python e TypeScript **têm parser** no Touring (`languages.rs:85,87`) e um extrator de membros
  (`extract_symbol_details`, `python_fields.scm`, `typescript_fields.scm`) que só o `touring ast detail` usa; **o índice
  não guarda pai, membros nem assinatura** (`touring-code/src/ast/store.rs:122`); `ast detail` com caminho relativo
  devolve 0 em silêncio (`index.rs:2182`).
- `VgpEngine` (`touring-generator`) é por nome + crate, sem consumidor nos crates de hook.
- **MultiEdit não passa por verificação do Touring** (o único matcher que o casa é o do gate de prosa).
- O registro de structs do `vgp-protocol.md` do analise descreve o **código interno do touring**, não os crates do
  analise (`kazuba-*`) que os `paths:` dele casam; `graph.rs` não existe mais com esse nome.

## 5. O motor — VGP na base, por linguagem, no executor

**Contrato único** (um trait em Rust, um adaptador por linguagem): entrada = (caminho, conteúdo em disco, conteúdo
proposto, projeto da sessão); saída = achados `(classe, nome, onde, sugestão)`; veredito = **delta proposto − disco**
(o que já estava errado nunca é culpa da edição — o desenho do `python_pre_gate.py`, que já se pagou). Fail-open
quando não consegue medir. Kill switch humano por projeto.

**Camada 1 — nativa, sempre presente, sem dependência** (tree-sitter + índice do Touring):
- `from <módulo do projeto> import <nome>` / `import { X } from './mod'` → o nome está definido no módulo?
- `<módulo do projeto>.<atributo>` → o atributo existe no módulo?
- `ClasseDoProjeto(...)` e chamada direta de função do projeto → os kwargs existem na assinatura (sem `**kwargs`)?
- `ClasseDoProjeto.membro` → o membro existe (campos, métodos, herança direta)?
- Rust: imports e caminhos `crate::…` (o compilador segue como juiz final, no pós-edição).
- Pré-requisito: o índice passa a guardar `parent`, membros e assinatura (o extrator já existe; falta gravar).
  **Revisto em 09/10/2026 pela coordenadora (ao executar o /goal do Gabriel)**: em vez de colunas novas na tabela
  `symbols`, o motor consulta a tabela de símbolos do MÓDULO extraída do arquivo em disco sob demanda, com cache por
  `(path, sha256)`. Razões medidas: o extrator (`touring-code/src/ast/symbol_detail.rs`) já devolve membros py/rs/ts
  em milissegundos; um índice persistido envelhece entre edições feitas fora do Claude Code, e o motor nunca pode
  julgar contra um módulo velho. Se o p95 ≤ 150 ms não for cumprido assim, a persistência volta à mesa.

**Camada 2 — o verificador da própria linguagem, quando o projeto o tiver** (descoberto no projeto, nunca instalado
pelo Touring): Python → pyright/basedpyright/ty/mypy do venv; TS → `tsc --noEmit`; Rust → `cargo check` (já roda no
pós-edição). **Sonda do pyright 1.1.408 no analise** ([`diagnostics/vgp_sonda/`](diagnostics/vgp_sonda/)): 5 de 5
formas inventadas (import inexistente, atributo de módulo, método de classe da stdlib, campo de classe do projeto,
kwarg em função sem `**kwargs`), 0 diagnóstico num módulo real de 866 linhas, **0,39 s por arquivo**. Falso positivo
conhecido: receptor união/`Unknown` (dict com valores heterogêneos) → aviso, não deny.

**O que nega** (alta precisão): nome importado inexistente · atributo inexistente de módulo · kwarg inexistente ·
membro inexistente de classe concreta do projeto. **O que só avisa**: receptor união/`Unknown`, herança dinâmica,
`__getattr__`. **A recusa ensina** (diretriz A5): nomeia os membros reais e o mais próximo
(«`No` não tem `campo_inventado`; tem `tipo, slug, corpo, facetas, procedencia, arestas`»).

**Revisão medida em 09/10/2026 (coordenadora, ao executar o /goal) — QUANDO negar é política, não verificador.**
Sonda da camada 2 ([`scripts/sonda_camada2_pyright.py`](scripts/sonda_camada2_pyright.py), pyright-langserver
persistente) no corpus V0d do analise, com o estado do projeto NO INSTANTE de cada edição
([`scripts/estado_no_instante_v0d.py`](scripts/estado_no_instante_v0d.py)) e o delta proposto − disco: **10 de 259**
edições sem erro de runtime depois têm achado REAL no instante, e as 10 são estados intermediários — 9 corrigidos numa
edição seguinte (consumidor escrito antes do provedor; 3 testes na fase RED de TDD) e 1 arquivo apagado; nenhuma
persiste no arquivo de hoje. Negar edição de arquivo do projeto por símbolo do projeto ou nome próprio ainda inexistente
dá 3,9% de falso deny com um verificador CORRETO — e bloquearia o RED que o próprio TDD exige. **Política P2**: código
que roda AGORA (inline e `/tmp`) → deny nas classes de alta precisão; arquivo do projeto → deny só quando o alvo é
EXTERNO (stdlib/terceiro/builtin, que nenhuma edição seguinte vai criar), aviso que ensina no resto. Medido: 1/259
(0,39%) de falso deny em arquivo; `/tmp` 7/7 e inline 16/28 de detecção de símbolo. Dois achados da mesma medição: (a)
**24 dos 65 positivos são FORMA do dado** (`'list' object has no attribute 'get'`), não símbolo inventado — o pyright pega
1 de 21 (o `json.load` devolve `Any`); saem do recall do juiz e ficam reportados à parte; (b) o pyright 1.1.408 só
publica (push) e o `didClose` emite uma limpeza (`version` nula) — cliente sem casamento por versão lê a limpeza como
resultado (inflou o falso deny de 10 para 27). Juiz revisado (v2): recall de DETECÇÃO de símbolo (deny ou warn),
falso deny por balde. Especificação da camada 2: [`V3-camada2-spec.md`](V3-camada2-spec.md).
**Política final medida (P4, 09/10 noite)** — com os 400 negativos de código que roda agora (300 inline + 100
`/tmp` escrito-e-executado, analise-25, todos compilando): dois escopos que só avisam, porque o verificador estático
não os modela — `forma` (membro de valor builtin: `list[int | set]` → `int.add`) e `dinamico` (atributo dunder como
`__wrapped__`; nome sob `from X import *`). Falso deny **0/259 arquivo · 0/100 `/tmp` · 1/300 inline** (o 1 é defeito de
corpus); detecção de símbolo `/tmp` 7/7, inline 16/27; p95 68 ms; custo das duas regras no recall de deny: zero.

**Código inline** (Bash `python3 - <<EOF`, `python3 -c`, `touring run --code`): o mesmo verificador sobre o programa
extraído no `pre-bash`/`touring run` — aviso primeiro, porque é onde mais erra (226 + 76 distintos).

**`/tmp` é trabalho de primeira classe** (ordem 5). Os scripts do scratchpad (`/tmp/claude-<uid>/<projeto>/<sessão>/…`)
passam pelo mesmo motor que um arquivo do projeto, e os nomes que importam (`scripts.*`) são resolvidos **contra o
projeto da sessão que os escreveu** (o segmento `-projects-<nome>` do caminho, ou `CLAUDE_PROJECT_DIR`), nunca contra a
pasta onde o arquivo está — como o `vivos.py::project_root` já faz.

**`<projeto>/.cache/` também** (ordem 6, verificação de 08/10/2026). Medido: 153 `Write`/`Edit` das sessões em
`analise/.cache/` desde 17/09, todos os arquivos ainda no disco (ao contrário do `/tmp`, nada se perde ali); de 1.463
erros parseados dos transcripts, 2 têm frame em `.cache` (ambos de 08/10, depois do inventário — o zero do inventário
era real, provado com controle positivo). A verificação achou um defeito que **não** era do `.cache` e sim de todo
diretório-ponto: o casamento frame×arquivo editado usava `lstrip("./")`, que remove um CONJUNTO de caracteres e comia
o ponto de `.cache/`, `.claude/`, `.touring/` — 4 erros de hooks em `.claude/` caíam em «projeto» em vez de
«atribuído» (867 = 867 linhas antes e depois; só o balde mudou). Corrigido no inventário (TDD + mutante) e no
`medir_vgp.py`, onde o casamento também não tinha fronteira de barra (`a.py` casava `…/ba.py`): re-medido, o
NameError atribuído ficou **6 (analise) × 11 (touring)** — o ≈ 24× desta proposta se mantém. O `replay.py` passou a
reconstruir e a cobrar arquivos sumidos de `<projeto>/.cache/` como os de `/tmp` (`session_work`, controle no selftest
com mutante morto), e o relatório da V0b mostra o `.cache` ao lado do `/tmp`.

**A regra textual encolhe**: o `vgp-protocol.md` do analise (6.326 chars) vira um ponteiro curto, agnóstico de
linguagem, ou sai — quem ensina é a recusa, no momento em que ela importa. Custo de contexto do motor: zero caracteres
quando nada está errado.

## 6. Fases, aceite, executor e estado

| fase | entrega | aceite (exit code) | executor | estado |
|---|---|---|---|---|
| D1, D2 | dedupe do `settings.json` + instalador | — | 6b | **failed** — premissa refutada (§8) |
| V0a | inventário de TODOS os erros | selftest; cada classe com predicado positivo; resto nomeado | 6b | **feito** |
| V0b | o que ainda vive no código de hoje | selftest; status por predicado positivo, nunca por eliminação | 6b | **feito** |
| V0c | triar e **corrigir** os 98 itens (71 candidatos · 26 sem evidência · 1 não coletado) | [`scripts/juiz_v0c.py`](scripts/juiz_v0c.py) exit 0: uma triagem válida e **específica do item** (arquivo/linha + classe/termo da mensagem; texto repetido em classes diferentes reprova) por id; todo `corrigido` com teste que o juiz reroda | a9 (MiniMax) | **feito** — juiz `juiz_v0c.py` CONVERGIU (DAG completed; a 1ª entrega, 14 de 40, foi devolvida) |
| V0d | corpus de replay: conteúdo de cada script no momento do erro (inclusive os 213 de `/tmp`, dos transcripts) + ≥ 300 edições bem-sucedidas sorteadas (semente fixa) | o corpus reproduz os erros fora da sessão (controle positivo) | 0e (Kimi) | **feito** 09/10 — `juiz_v0d.py` rc 0 «391 · 91 · 300 / PRONTO», aceito pela coordenadora após juízo independente |
| V0e | as **3.090 falhas sem diagnóstico de linguagem** + as negações em Edit/Write/Read: por classe, a causa e o conserto | cada classe com predicado positivo e destino (código, gate, mensagem que ensina a rota) | 6b | **em curso** — gates tratados: V0e1–V0e4 e V0e6 (release de 09/10 manhã), S3 que avisa + reescritas G1/G2/G8 que preservam a chamada (release agrupado de 09/10 tarde). **Destino das 606 fora de gate: FEITO** (a9/25 MiniMax, aceito pelo juízo da coordenadora: 15/15 classes dif 0, conservação exata 458+148, ≥3 evidências por classe com ids inteiros reencontradas 44/44 — (b) 39 · (c) 455 · (d) 112; [`diagnostics/V0e-fora-de-gate.md`](diagnostics/V0e-fora-de-gate.md)). **V0e5 em curso**: 1ª remedição devolvida (S3 e G10 lidos só no tool_result — o aviso vive nos registros `attachment`; controles positivos nomeados enviados); o G1 já separa (4,52 → 0,56 por 100 Bash, ICs disjuntos). **V0e1b** (G1 sem replay, regressão do V0e1 medida ao vivo): conserto aceito pela coordenadora — `cli_suggester.rs` sha `c79f12b0`, os já executados viajam como comentário e nunca rodam de novo, `additionalContext` avisa o modelo, `touring run` zera o lote; 9 testes, mutantes M-a…M-e mortos; fecha com a prova [4] no binário instalado |
| V1 | índice com `parent`/membros/assinatura (py, ts, rs) + caminho absoluto no `ast detail` | testes Rust; caminho relativo deixa de devolver 0 | 0e (Kimi) | **código pronto** (`cli_ast_detail`, [RELATORIO §5](RELATORIO-V1V2V5-2026-10-09.md)); fecha no deploy 30.4.96 |
| V2 | camada 1 em `pre-edit-prevention`, `pre-write` e **MultiEdit**, em sombra | recall de símbolo ≥ 0,70 e falso deny ≤ 1% **por balde** (arquivo · inline); p95 ≤ 150 ms — [`scripts/juiz_motor_vgp.py`](scripts/juiz_motor_vgp.py) | 0e (Kimi) | **em curso** — motor em `touring-code/src/vgp/`, CLI `touring vgp check`, sombra nos 3 hooks. Juiz v2 no binário congelado sha `c8b503c4`: recall arquivo 8/13 · inline 12/27 · falso deny 43/400 · 42/300 · p95 100,6 ms → **NÃO PRONTO**. Causas medidas por predicado positivo em cada snap ([`diagnostics/v2-falsos/`](diagnostics/v2-falsos/)): **R1** `from M import X as Y` liga Y ao módulo M (74 atributo + 4 import) · **R2** o vinculador de escopo ignora `with … as`, `except … as`, atribuição encadeada, `*args` e walrus (74 de `nome`, todos ligados no próprio arquivo) · **R3** `from __future__ import annotations` (9). Com a Kimi, TDD + mutante por causa |
| V3 | camada 2 (pyright persistente) em sombra | recall maior que a V2 sem piorar o falso deny; p95 ≤ 1 s | 89 (spec + juiz) · subagente `v3-camada2` (crate) · 0e (wiring) | **em curso — só falta o falso deny da camada 1**. Crate `touring-pyright` sem dependência `touring-*` (resolve o ciclo de pacote) + trava `habilitar_neste_processo()`: só o daemon e a CLI `--camadas 1,2` sobem pyright, nunca o hook standalone efêmero. V3 no sha `c8b503c4`: recall arquivo **11/13**, inline **19/27** (ambos ≥ 0,70), 10 positivos recuperados e 0 perdidos, falsos deny idênticos aos da V2 (os mesmos 85 ids), p95 **877,6 ms**, nenhum `pyright-langserver` órfão |
| V4 | deny ligado nas classes de alta precisão | 5 sessões reais com deny correto; zero falso deny reportado — [`scripts/juiz_v4_sessoes.py`](scripts/juiz_v4_sessoes.py) (fixtures prontas) | 0e | pendente — depende do deploy 30.4.96 e da ativação `[vgp] modo = "ativo"` no projeto |
| V5 | inline (pre-bash + `touring run`) e `/tmp`, em aviso | recall medido no inline e no `/tmp` do corpus V0d | 0e (motor) · 89 + subagente `shell-inline` (extrator) | **em curso** — lexer [`scripts/shell_inline.py`](scripts/shell_inline.py) aceito (sha `b6acd415`; selftest 107/107; 7/7 mutantes da coordenadora e 36/36 do subagente mortos): o programa só sai por predicado positivo sobre o comando simples que o consome. Religação de `corpus_v0d`/`corpus_v5_inline_neg` ao lexer medida em staging, sem tocar no corpus vivo |
| V6 | regra textual encolhida; docs e skill Touring em co-evolução | juiz de drift verde | 89 | ponteiro escrito ([`V6-ponteiro-vgp-protocol.md`](V6-ponteiro-vgp-protocol.md)); aplica no deploy, com a 8ª reconciliação do drift |
| VL0 | contrato multi-linguagem: despacho por linguagem + pool LSP genérico | Python sem regressão, por id | a definir | pendente (§10) |
| VRS | Rust: camada 1 nativa + rust-analyzer persistente | recall ≥ 0,70 nos 84 erros rustc reais; falso deny ≤ 1% | a definir | pendente (§10) |
| VTS | TS/JS: camada 1 nativa + `tsc`/`deno` do projeto | corpus semeado e rotulado; mesmos limiares | a definir | pendente (§10) |
| VTSX | TSX: componentes, props, `className` | corpus semeado nos 542 `.tsx` | a definir | pendente (§10.1) |
| VHTML | HTML: refs locais, ids, scripts inline | 87 edições reais + corpus semeado | a definir | pendente (§10) |
| VCDN | versões de CDN | 404 confirmado nega; timeout nunca nega | a definir | pendente (§10.1) |
| VCSS | classes CSS referenciadas | só aviso; abstenção declarada | a definir | pendente (§10.1) |
| J | juiz final ([`scripts/juiz_plano_vgp.py`](scripts/juiz_plano_vgp.py)) + prova prática ([`scripts/prova_vgp_motor.py`](scripts/prova_vgp_motor.py)) + memória + reward + relatório | exit 0; depende de V0c, V0e5, V6, VRS, VTS, VTSX, VHTML, VCDN, VCSS | 89 | pendente |

## 7. Riscos e sequência

- **Colisão (atual)**: a Kimi corrige R1–R3 em `touring-code/src/vgp/`; a VL0 refatora os mesmos arquivos → a VL0
  começa depois que V2/V3 convergirem. Arquivos de escrita exclusivos por executor: crates com a Kimi;
  `shell_inline.py` e `corpus_*.py` com a coordenação. (A colisão original, com a S8, acabou com o deploy dela.)
- **Binário medido**: todo número de juiz vem de uma cópia congelada do binário, com o sha256 ao lado do veredito
  (lição de 09/10, §8).
- **Latência**: camada 2 do Python com p95 de 877,6 ms (orçamento ≤ 1 s), por isso é sombra antes de deny. Rust:
  indexar o workspace do touring no rust-analyzer pode custar minutos e gigabytes — sonda de tempo e RSS antes de
  ligar a VRS, nunca concorrendo por CPU com um juiz em curso.
- **Rede (VCDN)**: um HEAD do jsdelivr pendurou 15 s → cache, orçamento de 300 ms no hook, preenchimento em
  background no daemon; offline ou timeout = «não conferido», nunca deny.
- **Precisão é o produto**: um gate que nega errado é contornado (lição do `judge_attest`). O deny só liga na V4.
- **Retrato móvel**: os transcripts crescem enquanto as sessões trabalham; cada número aqui leva a data e o
  instrumento, e reproduzi-lo é rodar o script do bundle.
- **Sequência**: release 30.4.96 (Python: V1–V3 + G1 sem replay + `touring-pyright`) → V4, V6 e a prova prática
  [1]–[4] no binário instalado → VL0 → VRS → VTS + VTSX → VHTML + VCDN → VCSS → J.

## 8. Erros meus que o processo pegou (registrados no grafo de memória)

- **Duplicação no `settings.json` — retratada.** Eu disse que `pre-read`/`pre-edit`/`pre-edit-prevention`/`pre-write`
  estavam registrados 17× por um instalador não idempotente, e o Gabriel aprovou o conserto. Falso: as 17 entradas
  têm campos `if` diferentes (`Edit(*.rs)`, `Edit(*.py)`…), um filtro por extensão. Pegou o invariante do
  [`scripts/dedupe_settings.py`](scripts/dedupe_settings.py): 148 entradas = 148 identidades, 0 a remover. Nada foi
  alterado.
- **11 «testes vermelhos» que não eram.** A 1ª reexecução tratou todo código de saída ≠ 0 do pytest como falha:
  10 eram rc 4 (o id que montei não era coletado — teste dentro de classe) e 1 era rc 1 de cobertura mínima. Hoje:
  `-k <função>`, plugin de cobertura desligado, e rc 4/5 = «não coletado».
- **Zero falso por sandbox.** Um laço `for` no Bash foi desviado para o `touring run` (sem acesso a `~/.claude`) mesmo
  com `TOURING_CODE_MODE=native` e deu «0 transcripts»; o controle positivo (1.027 no analise) o denunciou.
- **Razão 29× → 24×.** A 1ª medição contava erro em qualquer resultado de ferramenta; refeita só em resultado de Bash.
- **Prova ao vivo mal endereçada (09/10).** A parte [2] da prova herdou `TOURING_PROJECT_ROOT=touring` do shell: o
  daemon do analise recusou e o hook devolveu `{}` com o erro só no stderr — 5 «registro ausente» que eram endereço.
  A parte [4] sondava o G1 no `pre-bash`, mas os gates de code mode vivem no `cli-suggest`. Hoje: ambiente de uma
  sessão real, erro no stderr vira falha, e o índice do disparo se acha, não se supõe.
- **Binário mais novo que a fonte, sem a fonte (09/10).** O V3 deu «a camada 2 não acrescenta» em 762/762 chamadas
  com um binário 3 min mais novo que `cli/vgp.rs` — o build tinha começado antes da edição. Um `cargo build -v`
  seguinte deu «Fresh» e eu «refutei» a hipótese certa: o Fresh era de um build novo, feito por outra sessão no meio.
  Hoje: juiz em cópia congelada com sha (grafo: `licao:binario-mais-novo-que-a-fonte-nao-prova-que-a-contem-congele-o-medido`).
- **Sandbox (09/10).** Afirmei duas vezes o que o Landlock do `touring run` lê; a regra é medir com `open`/`listdir`
  reais — `os.access` devolve True para o que é negado e o glob devolve 0 em silêncio.
- **Zero por chave errada (09/10).** Ao conferir as subtarefas novas do DAG filtrei `subtask_id == "J"`, mas o id
  gravado é `task_…::J`: zero linhas; o controle (imprimir os ids) mostrou o escopo.

## 9. Artefatos

| arquivo | o que é |
|---|---|
| [`scripts/inventario_erros.py`](scripts/inventario_erros.py) | V0a — inventário de todos os erros (`--selftest`) |
| [`scripts/vivos.py`](scripts/vivos.py) | V0b — o que ainda vive (`--rerodar-testes`) |
| [`scripts/juiz_v0c.py`](scripts/juiz_v0c.py) | juiz da V0c |
| [`scripts/dedupe_settings.py`](scripts/dedupe_settings.py) | prova de que não havia duplicação (invariante de identidades) |
| [`scripts/registrar_dag.py`](scripts/registrar_dag.py) | registro das subtarefas no DAG |
| [`diagnostics/medir_vgp.py`](diagnostics/medir_vgp.py) · [`diagnostics/vgp_sonda/`](diagnostics/vgp_sonda/) | medição por linguagem · sonda do pyright |
| [`diagnostics/V0a-inventario.txt`](diagnostics/V0a-inventario.txt) · [`diagnostics/V0b-vivos.txt`](diagnostics/V0b-vivos.txt) | agregados (sem mensagens de erro) |
| `~/.claude/touring/vgp/` | bruto: `erros_inventario.jsonl`, `vivos.jsonl`, `v0c_itens.jsonl`, `v0c_triagem.jsonl` (fora de repositório) |
| [`scripts/juiz_motor_vgp.py`](scripts/juiz_motor_vgp.py) | juiz V2/V3 do motor (`--touring <cópia congelada>`; V3: `--arg-motor=--camadas --arg-motor=1,2`) |
| [`scripts/juiz_plano_vgp.py`](scripts/juiz_plano_vgp.py) · [`scripts/prova_vgp_motor.py`](scripts/prova_vgp_motor.py) | juiz J do plano · prova prática [1]–[4] (`--selftest`) |
| [`scripts/shell_inline.py`](scripts/shell_inline.py) | lexer de shell: o programa Python que um comando executa (`--selftest`) |
| [`scripts/corpus_v0d.py`](scripts/corpus_v0d.py) · [`scripts/corpus_v5_inline_neg.py`](scripts/corpus_v5_inline_neg.py) · [`scripts/juiz_v0d.py`](scripts/juiz_v0d.py) | corpus de replay V0d · negativos inline e `/tmp` da V5 · juiz da V0d |
| [`scripts/juiz_v4_sessoes.py`](scripts/juiz_v4_sessoes.py) | juiz V4 (5 sessões reais, temporal) |
| [`V3-camada2-spec.md`](V3-camada2-spec.md) · [`V6-ponteiro-vgp-protocol.md`](V6-ponteiro-vgp-protocol.md) · [`RELATORIO-V1V2V5-2026-10-09.md`](RELATORIO-V1V2V5-2026-10-09.md) | spec da camada 2 · ponteiro da V6 · relatório do motor (Kimi) |
| [`diagnostics/v2-falsos/`](diagnostics/v2-falsos/) | juiz v2 e V3 no sha `c8b503c4` (JSON) + classificadores dos falsos deny por construto |
| [`diagnostics/multilinguagem/`](diagnostics/multilinguagem/) | demanda por linguagem, sonda dos verificadores (rust-analyzer, deno, tsc), CDN e classes CSS |
| crates do touring | `touring-code/src/vgp/` (motor) · `touring-pyright/` (camada 2) · `touring-server/src/cli/vgp.rs` (CLI) · `touring-hook-handlers/src/hooks/vgp_l2.rs` (camada 2 nos hooks) · `touring-cli/src/cli_suggester.rs` (G1 sem replay) |

## 10. Ampliação multi-linguagem (ordem de Gabriel, 09/10/2026 ~20:05Z)

> «mas o VGP vai funcionar só para python? preciso que ele funcione para as principais linguagens (python, rust, ts,
> html), pelo menos»

**Estado medido no código**: o motor de V2/V3 é só Python — `vgp/mod.rs:100` (`input.lang == "python"`), parse com
`Lang::Python` fixo em `checks.rs`/`modules.rs`/`scope.rs`, camada 2 só para `lang == "python"` (`cli/vgp.rs:32`). O
contrato da §5 já previa um adaptador por linguagem; as fases V2–V5 foram medidas só em Python porque §1 dava 80% das
edições do analise. HTML não estava no plano. O `touring-code` já tem as gramáticas tree-sitter de Rust, TSX, JS, HTML e
CSS (`ast/languages.rs:85-91`).

**Demanda por linguagem** (`diagnostics/multilinguagem/medir_linguagens_vgp.py`, nativo, transcripts com subagentes, tool_use por id;
erros só em resultado de Bash, um por (chamada, código)):

| | analise (1.179 transcripts) | touring (566) |
|---|---:|---:|
| edições Python | 16.027 (80,2%) | 1.179 (30,5%) |
| edições Rust | 618 (3,1%) | **1.557 (40,3%)** |
| edições HTML | 64 (0,3%; 10 arquivos) | 23 (0,6%; 1 arquivo em /tmp) |
| edições TS | **0** | **0** |
| edições JS | 22 | 0 |
| erros rustc de nome (E0425/E0432/E0433/E0599/E0063/E0061…) | 30 | 54 |
| erros TS de nome (TS2304/TS2339/TS2305…) | 0 | 0 |

**Sonda dos verificadores (camada 2), 09/10** (`diagnostics/multilinguagem/sonda_linguagens/`): `rust-analyzer diagnostics` (instalado
via rustup) pegou 5 de 6 nomes inventados — E0432 import, E0560 campo no literal, E0559 campo, E0599 método, E0425 nome
— em 2,3 s a frio; perdeu `util::inventada()` (função inexistente num módulo; o `cargo check` pega). `deno check` 5 de 5
(TS2724 com «did you mean», TS2353, TS2339, TS2304, TS2551) em 0,43 s; o `tsc` 5.8.3 do `analise/frontend` os mesmos
em 0,13 s (fora do tsconfig do projeto deu ruído de ambiente). Nenhum validador HTML instalado (html-validate, vnu, tidy).

**Fases novas** (DAG `task_1791484992466538442`, J passa a depender delas):

| fase | o quê | aceite | dep |
|---|---|---|---|
| VL0 | contrato multi-linguagem: `vgp::check` despacha por linguagem com o mesmo `CheckOutput`, delta proposto − disco e política P4; pool LSP genérico extraído do `touring-pyright` | Python sem regressão (juiz v2/V3 idênticos por id) | V3 |
| VRS | Rust: camada 1 nativa (`use`/`crate::`/`super::` resolvem; campos de literal de struct do projeto) + camada 2 rust-analyzer persistente no daemon (só diagnósticos nativos) | recall ≥ 0,70 nos 84 erros reais; falso deny ≤ 1% nos negativos das 2.175 edições `.rs`; p95 medido (risco: indexação do workspace touring — sonda de RSS e tempo antes de ligar) | VL0 |
| VTS | TS/JS: camada 1 nativa (`import {X} from './m'` exportado; módulo relativo existe) + camada 2 do PROJETO (`node_modules/.bin/tsc` via tsserver persistente; `deno check` em projeto deno) | corpus SEMEADO e rotulado como tal (0 edições TS reais na janela) sobre arquivos reais de `frontend/`, `konverter/portal`, `video-studio`; mesmos limiares | VL0 |
| VHTML | HTML: camada 1 nativa — `src`/`href` locais existem, `#id`/`for`/`aria-*` resolvem no documento, id duplicado, `<script>` inline e handlers pelo adaptador JS (`getElementById`/`querySelector` com id do documento) | 87 edições reais + corpus semeado; mesmos limiares | VTS |

**Sequência recomendada**: o release 30.4.96 sai com Python (V2–V6), porque cada linguagem é ligada pelo próprio juiz e
nenhuma espera a outra; Rust primeiro (maior demanda medida fora do analise e 84 erros reais), depois TS/JS, depois HTML
(que reaproveita o adaptador JS no `<script>`).

### 10.1 CDN, classes CSS e TSX (ordem de Gabriel, 09/10/2026 ~20:10Z)

> «confira as versões de CDN e as classes CSS referenciadas; e também inclua tsx»

**Sonda** (`diagnostics/multilinguagem/sonda_cdn_css_tsx.py`, `cdn_vivos.py`, nativo):
- CDN responde e distingue: URL real 200 em 81–197 ms (jsdelivr, unpkg, cdnjs); versão inventada (`react@18.3.99`, `d3@7.9.99`) **404** nos três. **Risco**: HEAD de ARQUIVO inventado numa versão real do jsdelivr **pendurou 15 s** → a existência do arquivo vem da API de listagem (jsdelivr data 2,5 s, cdnjs 0,6 s), em cache, nunca de HEAD síncrono.
- Disco do analise: **7.666 .html**, 702 URLs de CDN (+150 Google Fonts). As **42 distintas com versão exata (575 refs) dão 200 hoje** (controle positivo: a inventada dá 404). **9 distintas sem pin (127 refs)**: 5 `cdn.jsdelivr.net/gh/…` sem tag (templates do folium, plugins Leaflet, CSS do VS Code — conteúdo mutável), `unpkg.com/piii` e `piii-filters` sem versão, `ua-parser-js@0`, `swiper@10`.
- Classes: 6.123 arquivos com `class=`; **87% das classes usadas (149.024/171.818) não estão no `<style>` do próprio arquivo** — vêm de folhas linkadas ou de CDN. Resolver TODAS as folhas que o documento carrega é pré-requisito; aproximar pelo arquivo seria falso positivo em massa.
- TSX: **542 `.tsx` no analise**, 21 no konverter, 0 editados na janela → corpus semeado sobre arquivos reais.

| fase | o quê | dep |
|---|---|---|
| VTSX | componente JSX `<Comp>` importado/definido; props existem no tipo `Props` do componente do projeto (o análogo do kwarg); `className` alimenta a VCSS; gramática TSX só para `.tsx` e TYPESCRIPT para `.ts` (hoje `Lang::TypeScript` usa `LANGUAGE_TSX` para tudo, `languages.rs:87`); camada 2 = `tsc` do projeto com o tsconfig | VTS |
| VCDN | versão existe (404 confirmado = deny) · arquivo existe na versão (API de listagem) · pin exato (range/latest/`gh` sem tag = aviso com a versão exata resolvida) · `integrity` SRI conferido quando presente · cache: exata+200 imutável, 404 TTL 24 h, timeout = unknown (nunca deny) · orçamento de 300 ms no hook; miss preenche em background no daemon | VHTML |
| VCSS | classes usadas (`class=`, `className` literal, `classList`, `querySelector('.x')`) × definidas em todas as folhas carregadas (`<style>`, `<link>` local, `@import`, CSS de CDN pelo cache da VCDN) · abstenção declarada sob classes geradas (Tailwind/UnoCSS) e `className` dinâmico · **só aviso** com «você quis dizer» (classe-gancho de JS é legítima) · seletor de JS/CSS que não casa nenhum elemento = aviso | VHTML, VTSX, VCDN |

**Alcance dos hooks**: HTML gerado por script (folium, relatórios) não passa por Edit/Write — para ele, o mesmo motor é
exposto como `touring vgp check --lang html` para os gates dos geradores (p.ex. `scripts/design_elite/gate_artefato.py`).
