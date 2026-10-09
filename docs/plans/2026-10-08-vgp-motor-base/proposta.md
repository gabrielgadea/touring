---
type: Plan
title: VGP na base do Touring — um motor por linguagem, aplicado no executor
description: Plano aprovado pelo Gabriel em 08/10/2026. Motor VGP na base do Touring (vale em qualquer projeto), inventário de TODOS os erros das sessões e correção dos que ainda vivem, com /tmp tratado como trabalho de sessão.
plan_id: 2026-10-08-vgp-motor-base
timestamp: 2026-10-08T17:30:00-03:00
tags: ["#kind:plan", "#status:active", "#domain:harness"]
---

# VGP na base do Touring — plano aprovado

| | |
|---|---|
| **Estado** | aprovado por Gabriel em 08/10/2026 («tudo aprovado»); em execução |
| **DAG** | `task_1791484992466538442` (14 subtarefas; criado do cwd `~/projects/analise`, cujo daemon o guarda) |
| **Retomar** | [`RETOMADA.md`](RETOMADA.md) |
| **Coordenação** | analise-6b (esta sessão): planeja e julga · analise-a9 (MiniMax): V0c · analise-a0 (Kimi): V1–V6, depois da S8 |

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

**Leituras.** (1) O **G1 é 46% das negações** e menos da metade segue a rota — a maior alavanca. (2) A recusa que
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
| V0c | triar e **corrigir** os 98 itens (71 candidatos · 26 sem evidência · 1 não coletado) | [`scripts/juiz_v0c.py`](scripts/juiz_v0c.py) exit 0: uma triagem válida e **específica do item** (arquivo/linha + classe/termo da mensagem; texto repetido em classes diferentes reprova) por id; todo `corrigido` com teste que o juiz reroda | a9 (MiniMax) | **em curso** — 1ª entrega: 40 triagens, só 14 passam no juiz endurecido (25 das 26 «ambiental» eram texto-padrão colado); devolvida |
| V0d | corpus de replay: conteúdo de cada script no momento do erro (inclusive os 213 de `/tmp`, dos transcripts) + ≥ 300 edições bem-sucedidas sorteadas (semente fixa) | o corpus reproduz os erros fora da sessão (controle positivo) | 6b | próxima |
| V0e | as **3.090 falhas sem diagnóstico de linguagem** + as negações em Edit/Write/Read: por classe, a causa e o conserto | cada classe com predicado positivo e destino (código, gate, mensagem que ensina a rota) | 6b | **em curso** (§2.1: 3.870 negações medidas, 43 sem gate; consertos propostos a aprovar) |
| V1 | índice com `parent`/membros/assinatura (py, ts, rs) + caminho absoluto no `ast detail` | testes Rust; caminho relativo deixa de devolver 0 | a0 (Kimi) | depois do deploy da S8 |
| V2 | camada 1 em `pre-edit-prevention`, `pre-write` e **MultiEdit**, em sombra | recall ≥ 70% no corpus V0d; falso deny ≤ 1% nas 300; p95 ≤ 150 ms | a0 | — |
| V3 | camada 2 (adaptador pyright/ty/mypy, tsc) em sombra | recall maior que a V2 sem piorar o falso deny; p95 ≤ 1 s | a0 | — |
| V4 | deny ligado nas classes de alta precisão | 5 sessões reais com deny correto; zero falso deny reportado | a0 | — |
| V5 | inline (pre-bash + `touring run`) e `/tmp`, em aviso | recall medido no inline e no `/tmp` do corpus V0d | a0 | — |
| V6 | regra textual encolhida; docs e skill Touring em co-evolução | juiz de drift verde | a0 | — |
| J | juiz final do plano + memória + reward + relatório | exit 0; depende de V0c, V0e, V6 | 6b | — |

## 7. Riscos e sequência

- **Colisão**: a S8 (Kimi, dedupe do conselho) mexe agora nos emissores do `touring-hook`. O motor mora nos mesmos
  crates → V1–V6 **começam depois do deploy da S8**. A V0c não toca `crates/**`.
- **Latência**: a camada 2 custa ~0,4 s por edição Python; por isso é sombra antes de deny, com orçamento próprio.
- **Precisão é o produto**: um gate que nega errado é contornado (lição do `judge_attest`). O deny só liga na V4.
- **Retrato móvel**: os transcripts crescem enquanto as sessões trabalham; cada número aqui leva a data e o
  instrumento, e reproduzi-lo é rodar o script do bundle.

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
