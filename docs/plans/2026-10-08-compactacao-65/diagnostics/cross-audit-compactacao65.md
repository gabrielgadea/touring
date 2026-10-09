---
type: Audit
title: Cross-audit da compactação 65 (S9 skills + adendo S7 regras e agentes)
scope: 9 SKILL.md · 9 regras · 8 agentes · scripts da bundle
date: 2026-10-08
method: 7 fases TACO-cross-audit · toda afirmação com comando executado
---

# Cross-audit — compactação 65

**Escopo**: o que a compactação 65 tocou, medido em disco. **Fases 1–4 e 6 concluídas.
Fase 5: F2 AUTORIZADO pelo Gabriel e APLICADO; F1 e F4 continuam no portão humano.**

Artefato de saída:
`~/projects/touring/docs/plans/2026-10-08-compactacao-65/diagnostics/cross-audit-compactacao65.md`

---

## 1. O veredicto em uma linha

A compactação **funciona** — e essa afirmação agora tem prova executada, não cálculo.
O juiz passa, o juiz reprova quando deve reprovar, e o conteúdo chega. O que ela **não**
tem é o instrumento que diria se o conteúdo chega *sempre* — e essa é a ressalva que
muda o número que se pode vender.

---

## 2. O que passa, com a prova

| # | Afirmação | Como foi provada | Resultado |
|---|---|---|---|
| P1 | O juiz `conservacao.py` não é cego | **Mutação**: injetei uma linha falsa em cada um dos 3 backups (`rules-backup/touring-per-project.md`, `agents-backup/touring-scouter.md`, `skills-backup/Touring.SKILL.md`) e rodei o juiz | `rc=1` nos **3 modos**, cada um nomeando o item e a linha exata. Restaurado depois |
| P2 | Nenhum conteúdo sumiu | `conservacao.py` nos 3 modos | skills **3.503/3.503** · regras **755/755** · agentes **3.701/3.701** — 100,00 % |
| P3 | As cópias de agente não divergem | comparação byte a byte, 5 pares | **5/5 idênticas** |
| P4 | Nenhum arquivo de destino é órfão | varredura de 5.486 `.md` por citadores | **7/7 citados**; zero órfãos |
| P5 | Todo ponteiro de stub resolve | extração de caminho **completo** (com `~` expandido), não fragmento remontado | **todos resolvem** |
| P6 | `paths:` funciona de verdade | evidência viva: o preâmbulo da sessão lista `Instructions no longer present: …/disk-hygiene.md` e `…/vgp-protocol.md` | as 2 regras com glob saíram do contexto; as outras 7 continuam |
| P7 | Os testes seguem verdes | 4 suítes | **39 passed, 0 failed** (`test_constituicao_viva` 11, `test_code_mode_sdk_section` 19, `test_skill_size_guard` 6, `test_docs_no_phantom_crates` 3) |
| P8 | O código do juiz está limpo | `ruff check` + complexidade | **0 erros**, `max_cc=9` (teto 10) |

---

## 3. Achados

### F1 — "Sob demanda" não tem mecanismo. É a ressalva que muda o número.

Varri **554 hooks** (Python e shell, em `~/.claude/hooks` e `.claude/hooks` do projeto)
procurando um que carregue um `references/*.md` concreto: **zero**. `settings.json` não
menciona `paths`.

O que isso significa, sem enfeite: os **30.169 chars** movidos para arquivos de destino
**não chegam ao contexto sozinhos**. Chegam quando o modelo **decidir** ler o ponteiro.
Antes da compactação, esse texto era gratuito; agora custa uma chamada de ferramenta.

O relatório anterior escreveu *"carrega só quando o tema pede"* — a frase descreve um
comportamento desejado, não um mecanismo existente. A diferença entre os dois é a
diferença entre uma medida e uma intenção.

**Não há também instrumento que meça se a leitura acontece.** Um destino que ninguém lê é
um arquivo morto com 100 % de conservação e valor zero.

**O que isso NÃO invalida**: o ganho é real para o texto que é efetivamente carregado, e a
conservação garante que nada se perdeu. O que falta é a segunda metade da frase — "e o que
não é carregado continua acessível".

### F2 — RESOLVIDO. Defeito real corrigido sob autorização do Gabriel

**Antes**: as cinco descrições `touring-*` tinham perdido a frase que diz *qual* agente e
cada uma, e terminavam em `..`. Os cinco passaram a ter **gatilhos sobrepostos** —
`wiring chains`, `functional chains` e `blast-cross-feature` em quatro ou cinco deles — e
nenhum diferenciador.

**A regra do corte**, acordada antes de aplicar: um gatilho presente em 4 ou 5 dos cinco
nao escolhe ninguem; dispara junto com todos e so ocupa orcamento. Cortar custa zero de
ativacao. O orcamento dos gatilhos partilhados paga a frase que discrimina.

Gatilhos cortados por estar em 4 ou mais dos cinco: `blast-cross-feature` (4/5),
`file-knowledge extended` (4/5), `functional chains` (5/5), `rust-semantic` (4/5),
`wiring chains` (4/5). Onde ainda assim estourou o teto, a regra e iterativa: dropa-se o de
menor discriminacao — o que mais aparece entre os cinco, desempate pelo mais longo.

| agente | antes | depois | cortes | frase de papel restaurada |
|---|---:|---:|---:|---|
| touring-architect | 434 | 399 | 5 | This agent is invoked in TACO Phase 2 after touring-scouter completes. |
| touring-auditor | 440 | 376 | 4 | Elite cross-audit agent. |
| touring-engineer | 441 | 380 | 5 | Elite implementation agent for TACO Phase 5. |
| touring-scouter | 425 | 438 | 4 | Deep codebase intelligence scout... **JSON-only output.** |
| touring-scriber | 422 | 343 | 5 | Elite documentation agent. |

O scouter levou **duas** frases porque a segunda e um diferenciador de *contrato*: e o
unico dos cinco cuja saida e JSON puro.

**Proveniencia**: cada fragmento e conferido com `in < backup` antes de gravar. Na primeira
tentativa o guard **recusou** o scouter — as duas frases que eu queria juntar nao sao
contiguas no original (entre elas esta a da VP-Scout) e o literal montado nao existia em
lugar nenhum. Abortou em vez de gravar uma frase reescrita: um literal montado e
indistinguivel de um literal inventado do ponto de vista da verificacao.

**Verificado depois de aplicar**: 5/5 dentro do teto de 450 · frase de papel presente ·
nome do agente preservado · zero pontos duplos · copias byte-identicas · conservacao
**3.701/3.701 (100,00 %)** · **39 testes verdes**.

**O que a auditoria ensina com isso**: a primeira tentativa de extrair a frase pegou a
*primeira* sentenca depois da lista de gatilhos — que, para `auditor` e `scriber`, e a nota
"Wave 4 (2026-04-18) adds...". Heuristica sobre prosa datada erra; tabela explicita com
proveniencia conferida, nao.

<details>
<summary>O achado como estava antes da correcao (registro preservado)</summary>

### F2 — Defeito real: 5 descrições `touring-*` perderam a frase de papel e terminam em `..`

Todas as cinco foram cortadas dentro da lista de gatilhos e a frase que diz *qual* agente é
cada uma não sobreviveu:

```
touring-scouter: Use this agent when the user asks to "scout codebase", … ,
or menciona "touring-scouter", … , "cargo_metadata"..          ← ponto duplo
```

Consequência medida: os cinco agentes passaram a ter **gatilhos sobrepostos** —
`wiring chains`, `functional chains`, `blast-cross-feature` aparecem em todos. São cinco
candidatos com o mesmo gatilho e **nenhum diferenciador**.

As três descrições de `analise` (`antt-vote-drafter`, `erickson-paradox-reformulator`,
`transformational-narrative-architect`) estão **íntegras e limpas** — a primeira frase é
autocontida e nenhuma citação entre aspas foi perdida. O defeito é só dos cinco `touring-*`.

Correção proposta: restaurar a frase de papel cortando 4–5 gatilhos menos distintos para
caber nos 450 chars. São 2–3 gatilhos a menos em troca de um diferenciador que volta.

</details>

### F3 — Globo morto: `**/target/**` no disk-hygiene

Casa **0** arquivos de trabalho, porque `target/` é cache gitignored e está fora do
universo por construção. Não é danoso — é peso que não dispara.

### F4 — O preço do `paths:`, medido em vez de suposto

| regra | arquivo onde carrega | deixa de estar no contexto |
|---|---|---|
| disk-hygiene | **12,4 %** do projeto touring | nos outros **87,6 %** |
| vgp-protocol | **2,6 %** do projeto analise | nos outros **97,4 %** |

Para `disk-hygiene` o combinado é razoável: higiene de `target/` só importa quando se toca
`Cargo.toml` ou `.rs`. Para **vgp-protocol, 2,6 % é baixo demais** — VGP trata de gerar
código que referencia structs existentes, o que acontece em `.py` tanto quanto em `.rs`.
A regra raramente estará carregada justamente onde ela é necessária.

### F5 — O instrumento da auditoria falhou 7 vezes antes de medir o que dizia medir

Este é o achado mais transferível do relatório, e é sobre **esta** auditoria:

| # | rodada | o que reportou | por que estava errado |
|---|---|---|---|
| 1 | T1 | 3 "OK" e 5 "não resolve" | extraí `references/x.md` e testei `~/.claude/references/…`; o caminho real tem `skills/Touring/` no meio — e `~/.claude/rules/<mesmo-nome>.md` existe: casei o **stub** como destino |
| 2 | T4 | 5 "órfão" | filtrei por `dest.parent not in c.parents`; o stub nunca é ancestral do destino — todos os citadores reais foram descartados antes da busca |
| 3–5 | T2 (3 rodadas) | "nunca carrega", "0 %" | (a) `glob[:-2]` deixou `*` literal no prefixo; (b) a varredura parava em 60.000 de 488.690 arquivos, sem rotular a fatia; (c) `(?:[^/]+/)*` já emitia a barra e o trecho seguinte emitia outra — `**/*.rs` não casava nem `src/main.rs` |
| 6 | T1 (2ª) | 1 "não existe" | o ponteiro é um link markdown com rótulo relativo e alvo absoluto; li o rótulo |
| 7 | F1 | leituras **6× infladas** | contei com `grep` sobre o JSONL: o padrão casa o `input` do tool_use, o eco do `toolUseResult` e o caminho repetido em mensagens `assistant` de 15–24 KB. **338 ocorrências para 1 leitura real** |

Cinco foram negativas bem-formadas; a sétima foi **positiva e inflada** — a mais difícil de
achar, porque um número grande parece achado, não defeito.

**O que as pegou**: sempre um **controle positivo** (um caminho que deve casar, casando) e
um **controle negativo** (um que não deve casar, não casando) antes de qualquer medição. O
`casa()` só entrou no relatório depois de **16/16 controles** — e uma rodada **recusou
medir** porque 3 controles falharam.

A sétima falha é a mais instrutiva: eu escrevi a regra *"menção em saída de texto não é
leitura"* dois parágrafos acima, **e depois mexi com grep que a viola** — e mandei o número
errado para a coordenação. Escrever a regra não a executa; só o controle a executa.

**O instrumento que sobreviveu** (`scripts/miner_referencias.py`) parseia o JSON, conta só
`tool_use`/`Read`, e **recusa-se a responder** (rc 2) se qualquer um dos três controles
falhar.

### F6 — Dívida da execução

- **45 scripts temporários** em `/tmp/skill_probe` (**166.353 bytes**), incluindo os
  backups de arquivos que eu mesmo quebrei durante o caminho
- `phases/` — diretório vazio do formato OKF
- `TASK_ID` e `create.out` — artefatos de execução
- `conservacao_quebrada.bak` — cópia do `conservacao.py` quebrado, fora da bundle

Nenhum é defeito do produto. Todos são dívida minha e ficam registrados para decisão.

---

## 4. O que NÃO é achado, e quase foi

Para que o próximo não repita a armadilha:

- **`daemon_main.rs:140` usa `--socket` como exemplo de argumento recusado** — e `--socket`
  é exatamente o que um daemon per-project poderia querer. **Verifiquei**: o daemon resolve
  o socket por `TOURING_DAEMON_SOCKET` no ambiente (`main.rs:1055`), nunca por argv, e
  `daemon_ctl.rs:776` deriva a raiz do próprio socket. Não é caminho de regressão.
- **`by_session` "não existe"** — existe, em `context_budget.rs:932`. Meu `grep` anterior
  cortou em `head -20` e eu li o corte como ausência.
- **`test_constituicao_viva` imprime `FAIL Required test coverage of 85%`** ao rodar sozinho:
  é o gate global de cobertura medindo uma suíte de um arquivo. **11 passed**. Não é
  regressão da compactação — e está declarado aqui em vez de silenciado.

---

## 5. Portão da fase 5 — F2 aplicado, F1 e F4 continuam abertos

**F2 — AUTORIZADO pelo Gabriel e APLICADO** (ver o registro no corpo da secao F2).
5/5 dentro do teto, frase de papel restaurada, `..` eliminado, copias identicas,
conservacao 3.701/3.701, 39 testes verdes.

**Dois canvas foram preenchidos** (o `decision-canvas` foi invocado no fim da auditoria):

**F1 — construir o instrumento?** Canvas completo, recomendação **d**: **minerar os
transcritos ja em disco**, com custo de runtime **zero**, em vez de criar um hook novo
(o sistema ja tem 17). **Construido e executado**:
`scripts/miner_referencias.py` — parseia o JSON e so conta `tool_use` com `name == "Read"`.

```
transcritos: 1.861   leituras de arquivo no acervo: 11.567   (~6 por sessao)

CONTROLES (sem os tres o resultado e descartavel; rc 2 se falham)
  positivo  context_budget.rs                     7   ok
  padrao    daemon_main.rs                        2   ok
  negativo  arquivo-que-nao-existe-9f3a2b.md      0   ok
  VEREDITO DO INSTRUMENTO: confiavel

destino                                 existe  leituras  sessoes
00-core-governance-restauracao.md            sim         0        0
auto-load-rule.md                            sim         0        0
disk-hygiene.md                              sim         0        0
git-safety-protocol.md                       sim         1        1
touring-4-pillars-provenance.md              sim         0        0
touring-cli-index-tiers-2-3.md               sim         0        0
touring-per-project.md                       sim         0        0
touring-process-hygiene.md                   sim         1        1
vgp-protocol.md                              sim         2        2
```

**A RETRATAÇÃO que estes numeros implicam** — ver F5. A primeira medicao, por `grep`,
reportou 10 / 6 / 2 leituras para os tres destinos pre-existentes e 42 para o controle.
Estavam **6x inflados**. No transcripto desta sessao: **338 ocorrencias do nome para 1
leitura real**. O `grep` casava tres coisas que nao sao leitura — o `input` do tool_use, o
eco do `toolUseResult` no mesmo registro, e o caminho repetido ao longo de mensagens
`assistant` de 15–24 KB.

**O que os numeros corretos dizem**: os 4 destinos criados hoje estao em 0 (esperado —
horas de vida), e os **tres pre-existentes tambem em 0–1**, nao em 2–10. Ate agora a
referencia sob demanda esta, na pratica, **quase nao sendo lida**. Confianca do canvas:
0,85 → o instrumento existe; a **leitura** do numero ainda exige semanas de janela.

**F4 — o `paths:` do vgp-protocol?** Canvas completo. A MEDICAO CORRETA inverte metade do
julgamento, porque o denominador estava errado (contagem de arquivos, nao de sessoes):

| regra | % de todos os arquivos | % de **arquivos de codigo** |
|---|---:|---:|
| disk-hygiene | 12,45 % | **68,9 %** |
| vgp-protocol | 2,64 % | **2,7 %** |

Os 12,4 % que eu reportei sobre disk-hygiene eram o numero errado para a pergunta; em
sessao de codigo no touring a regra carrega **69 %** das vezes. E o **vgp NAO e salvo pelo
denominador**: 2,7 % e real. Recomendacao: **manter** (o enforcement e o hook global
`pre-edit`, que `paths:` nao afeta), confianca **0,75**.

A opcao (c) — ampliar para as principais linguagens — foi **encaminhada a coordenacao**,
com o pedido de verificar primeiro se o hook cobre Python: se cobre, (c) e diluicao; se nao
cobre, e a correcao certa.

**F3** — remover o globo morto `**/target/**`. *Um minuto, aguardando teu toque.*

---

_Veredito por exit code, nao por leitura: `conservacao.py` rc 0 nos 3 modos · 39 testes
verdes · `ruff` 0 erros · F2 aplicado e verificado. F1 e F4 sao teus._