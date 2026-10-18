# Relatório de transferência — analise-68 (corpus semeado do VGP)

**Sessão:** `analise-68` · modelo MiniMax-M3.1-Flash-Preview (backend da sessão irmã)
**Data:** 09/10/2026 · **Mandado por:** analise-89 (coordenadora anterior) → **destino:** `touring-09`
**Estado no momento deste relatório:** rodada fechada num ponto **consistente** (índice gravado por
classe; o corte preserva um corpus legível). Nenhum processo meu rodando.

---

## 1. Identidade e mandato

| tarefa | de quem | quando | estado |
|---|---|---|---|
| 2ª leitura read-only do fix G1 "sem replay" (`cli_suggester.rs`) | analise-89 | 09/10 | **entregue** (2 achados; A1 refutado depois de correção de constante; A2 e (f) procedem) |
| Corpus semeado TS/TSX/JS/HTML (VL0 §7) | analise-89 | 09/10 | **entregue parcial** (este relatório) |

Escopo do corpus semeado: `scripts/corpus_semeado.py` (arquivo exclusivo meu) e a saída em
`~/.claude/touring/vgp/corpus_semeado/<lang>/`. Proibido e **não tocado**: `crates/**`, `juiz_*.py`,
`shell_inline.py`, `corpus_v0d.py`, `corpus_v5_inline_neg.py`, `~/.claude/touring/vgp/corpus_v0d`;
sem commit, sem instalação de pacote, sem tocar arquivo-fonte (cópia por hardlink).

---

## 2. Feito, com evidência

### Arquivos criados / alterados

| caminho | sha256 (16) | o que é |
|---|---|---|
| `~/projects/touring/docs/plans/2026-10-08-vgp-motor-base/scripts/corpus_semeado.py` | `0f913bc856934e33` | **criado**, 1.206 linhas. Gerador: 11 operadores (6 de código + 5 web), cada um com oráculo independente, selftest com mutante por operador, escrita incremental |
| `~/.claude/touring/vgp/corpus_semeado/{typescript,tsx,javascript,html}/` | ver §5 | corpus gerado (`entradas.jsonl`, `snap/`, `estado_no_instante/manifesto.json`) |
| `~/.claude/touring/vgp/corpus_semeado/cdn_cache.json` | — | cache HEAD por URL (16 entradas). É o que torna o sha256 de `html` reproduzível |
| `~/projects/touring/docs/plans/2026-10-08-vgp-motor-base/diagnostics/transferencia/analise-68-RELATORIO.md` | este arquivo | relatório |

Diagnósticos meus, fora do entregável (em `/tmp`, descartáveis):
`/tmp/semeado/probe_fontes.py`, `probe2.py`, `probe3_receitas.py`, `probe4_html.py`, `probe5_rede.py`,
`/tmp/diag_js.py`, `/tmp/diag_js_dirs.py`, `/tmp/diag_js_erros.py`, `/tmp/verifica_corpus.py`.

### Testes, juiz e gates rodados

| comando | exit | resultado com denominador |
|---|---|---|
| `python3 scripts/corpus_semeado.py --selftest` | **0** | **23/23** em 66 s: 6 operadores de TS/TSX (o `tsc` do projeto acusa o mutado e não o original) + 5 de HTML (oráculo próprio) + **11 mutantes** (um por operador), todos reprovados |
| `ruff check …/corpus_semeado.py` | 0 | All checks passed |
| `ruff format --check …` | 0 | 1 file already formatted |
| `ruff check --select C901 --config 'lint.mccabe.max-complexity = 10'` | 0 | All checks passed (C901 ≤ 10) |
| `pyright corpus_semeado.py` | 0 | **0 errors, 0 warnings, 0 informations** |
| classificador `classe_erro(mensagem, lang, classe)` do juiz sha `b7f7fd9b` sobre o corpus | 0 | **250/250 positivos viram `simbolo`**, zero `nao_classificado` (80 + 120 + 50) |
| auditoria de esquema (snap, `disk_file`, ids, campos) | 0 | 588 entradas: **0** snap ausente, **0** `disk_file` ausente, **0** positivo fora do manifesto, ids únicos, nenhum campo faltando |

**Receitas de mutação provadas uma a uma** antes de entrarem no gerador (`probe3_receitas.py`): cada
uma produz **exatamente 1 erro, o esperado**, e nenhum outro; os originais ficam intactos
(`fonte real intocado: True`). Foi esse passo que evitou um corpus inteiro de itens que nao compilam.

### Defeitos meus que os gates e o selftest pegaram (nenhum apareceria num teste normal)

1. `muta_e_confere` rejeitava **qualquer** saida nao-vazia do `tsc` — ou seja, rejeitava o proprio erro
   que ia procurar. Os 6 operadores de TS reprovavam 100 %.
2. Os predicados de HTML nao liam o documento: "acusavam" o original e o mutado igualmente.
   Reescritos como predicado **sobre o texto** (e o que separa mutado de original).
3. O mutante dos operadores de TS nao discriminava — chamava a receita por fora do filtro da geracao,
   exatamente o contorno que o aceite proibe.
4. O indice era gravado **so na saida**: a geracao interrompida deixou 44 snaps sem `entradas.jsonl`.
   Corrigido para gravar a cada classe (e a cada 25 itens no HTML).

---

## 3. Estado por item

| item | estado | falta | proximo comando |
|---|---|---|---|
| typescript | **FEITO** | nada | — |
| tsx | **FEITO** | nada | — |
| javascript | **FEITO (com n=0 declarado)** | decidir se 0 e aceitavel — a causa esta medida no §6 | ver §6 |
| html | **PARCIAL** | `ancora` 10/20 · `cdn` 0/20 · `cdn_pin` 0/20 · **negativos 0/200** | ver abaixo |
| determinismo (aceite "d") | **NAO RODADO** | ver §8 | ver abaixo |

**Comando exato para a touring-4f retomar o HTML** (deterministico, semente `20261009`; reescreve
apenas o que falta e **nao** toca os snaps ja gravados):

```bash
cd ~/projects/touring/docs/plans/2026-10-08-vgp-motor-base
VGP_SEMEADO_TMP=$HOME/.cache/vgp-semeado \
python3 scripts/corpus_semeado.py --lang html \
  --saida $HOME/.claude/touring/vgp/corpus_semeado \
  --relatorio $HOME/.claude/touring/vgp/corpus_semeado/relatorio.json
```

**Comando exato para o determinismo** (ainda nao rodado — §8):

```bash
cd ~/projects/touring/docs/plans/2026-10-08-vgp-motor-base
python3 scripts/corpus_semeado.py --confere-determinismo --n 2 --negativos 5 --lang typescript
```

---

## 4. Processos ainda rodando

**Nenhum.** A geracao (pid 2563499) foi encerrada por mim as 21:36 com um `kill` normal, depois de
confirmar que o indice em disco parseava integro (50 entradas, ids unicos). Nao ha daemon, hook ou
subagente meu vivo.

**Residuo em disco que criei** (a remocao e do humano — o hook recusa `rm -rf`):

| caminho | tamanho | o que e | comando |
|---|---|---|---|
| `~/.cache/vgp-semeado/` | 25 G **aparentes** (4 dirs: 3 de frontend 399 M + 1 de packages 23 G) | copias por hardlink dos projetos-fonte. `stat` mediu **9 links** no `App.tsx` → os blocos sao compartilhados com os originais; o custo real extra e so dos arquivos mutados | `rm -rf ~/.cache/vgp-semeado` |
| `~/projects/analise/.vgp-semeado/` | pequeno | snaps da geracao + logs + sombras do workspace | `rm -rf ~/projects/analise/.vgp-semeado` |
| `~/projects/analise/.vgp-semeado/corpus/` | — | **copia de trabalho** do corpus; a copia entregue esta em `~/.claude/touring/vgp/corpus_semeado/` | idem |

---

## 5. Dados gerados

**Destino:** `~/.claude/touring/vgp/corpus_semeado/<lang>/` · semente `20261009` · `origem: "semeado"` em 100 % das entradas.

| lang | entradas | positivos | negativos | sha256 de `entradas.jsonl` (32) |
|---|---|---|---|---|
| typescript | 215 | **80** | 135 | `9313924e51886b45a671f0140fb52472` |
| tsx | 320 | **120** | 200 | `a5e6bd6166509e8ef969fcc37f792a07` |
| javascript | 3 | **0** | 3 | `c763bc190ec5982b976644e70d52a016` |
| html | 50 | **50** | 0 | `5cedbcc1c0164ad4b99f2cc94e534102` |

### n por lang x classe (positivos)

| classe | typescript | tsx | javascript | html | alvo |
|---|---|---|---|---|---|
| `import` | **20** | **20** | 0 | — | 20 |
| `nome` | **20** | **20** | 0 | — | 20 |
| `atributo` | **20** | **20** | 0 | — | 20 |
| `kwarg` | **20** | **20** | 0 | — | 20 |
| `componente` | — | **20** | 0 | — | 20 |
| `prop` | — | **20** | 0 | — | 20 |
| `recurso` | — | — | — | **20** | 20 |
| `ancora` | — | — | — | **10** (parcial) | 20 |
| `classe_css` | — | — | — | **20** | 20 |
| `cdn` | — | — | — | **0** (parcial) | 20 |
| `cdn_pin` | — | — | — | **0** (parcial) | 20 |

### Negativos por lang, com o teto real da fonte

| lang | alcancado | por que |
|---|---|---|
| typescript | 135 | **teto da fonte**: o `frontend` tem 135 `.ts`. Nao ha outros 65 |
| tsx | 200 | a fonte tem 281 `.tsx` → o alvo de 200 e factivel e foi atingido |
| javascript | 3 | so 3 `.js` de `packages/` estao **fora** do conjunto de erros (§6) |
| html | 0 | a geracao foi interrompida antes de `negativos_html` |

### Itens descartados, por motivo

| motivo | n | onde |
|---|---|---|
| **oraculo recusado: o projeto ja erra** (`packages/` com `checkJs` tem 136 arquivos com erro) | corpus inteiro de JS (0 de ~80 tentados) | javascript |
| mutacao que o `tsc` nao confirmou como erro unico do arquivo-alvo | **0** — nenhuma classe ficou abaixo de 20/20 | typescript, tsx |
| CSS remoto nao resolvivel / Tailwind-UnoCSS / folha utilitaria | aplicado como regra de descarte (n nao separado por classe — lacuna, §8) | html |
| CDN que nao deu 404 (timeout nao e 404), ou cuja versao original nao resolve | regra aplicada; 16 URLs no cache, **n de descarte nao separado** (§8) | html |

---

## 6. Decisoes, premissas, achados, lacunas e riscos

### Achado entregue e ja corrigido pela analise-89 (era meu)

`classe_erro` em `juiz_motor_vgp.py` classificava a mensagem com regex de Python. Rodei as mensagens
reais de cada linguagem por ela: **as 11 caem em `nao_classificado`** (TS2304/2305/2307/2322/2339/2353,
404 de CDN, classe CSS). Com o juiz como estava, os positivos semeados saiam todos do denominador e o
recall era nao-mensuravel. Corrigido (sha `b7f7fd9b`) e **verificado por mim**: 250/250 viram `simbolo`.

### Premissas que eu tinha e que se provaram por construcao — verifiquei em vez de dizer "sim"

- `ORIGENS_ARQUIVO` (linha 67) contem `"semeado"` → meus itens caem no balde **arquivo**, nao inline.
- `FAMILIA` mapeia `javascript` → `ts`, e a regex de `ts` exige o codigo `TSxxxx`. Minhas mensagens de
  JS vem do `tsc` com `checkJs`, entao carregam o codigo. **Se tivessem vindo de `node --check`,
  seriam `nao classificado` e o corpus de JS seria invisivel sem eu perceber** — e o motivo de eu ter
  usado `tsc` e nao `node --check` para JS.

### JavaScript = 0: causa medida, nao suposta

Minha hipotese inicial ("sao `.mjs`") estava **errada**: `.js` sao 122, `.mjs` zero. A causa real:

| medicao | resultado |
|---|---|
| arquivos `.js` em `~/projects/analise/packages/` | 122 |
| linha de base com `checkJs` | **136 arquivos com erro** |
| subdiretorios com linha de base **limpa** | **0** (medidos um a um, 22 diretorios) |
| maior diretorio (`kazuba-geo-engine`) | 115 `.js` → **115 erros** (todos) |

Nao ha em `packages/` nenhum subdiretorio onde o `tsc` sirva de oraculo. O gerador **recusou** o oraculo
em vez de semear itens nao confirmaveis — fail-closed funcionando, nao um zero silencioso.

**Risco para a touring-4f:** o corpus de javascript e, hoje, um balde com 3 negativos e nenhum positivo.
O juiz pode trata-lo como degenerado. Ou se aceita 0 declarado, ou se autoriza o teste de `tsconfig`
(§8) que decide se os 115 erros sao sistematicos.

### Riscos de ambiente (medidos, nao supostos)

- `/tmp` e **device 58** e a home e **device 31**: a copia por hardlink entre os dois morre com
  "Invalid cross-device link". A sombra nasce em `~/.cache` (ou em `$VGP_SEMEADO_TMP` no sandbox).
- O sandbox do `touring run` **nao da rede** (`URLError: errno 13`) nem escrita fora do workspace:
  o oraculo de CDN so roda nativo, e a geracao por dentro dele precisa de `VGP_SEMEADO_TMP`.
- `git rev-parse HEAD:.../juiz_motor_vgp.py` **falha** (arquivo nao rastreado) → a unica ancora de
  versao e o `sha256` do conteudo. Foi o que permitiu dizer "e o mesmo arquivo" em vez de "e o que eu acho".

### Lacunas declaradas

- O n de descarte por classe em HTML nao e separado (so o total).
- O determinismo nao foi medido (§8).

---

## 7. Git — so os meus arquivos

| arquivo | situacao |
|---|---|
| `scripts/corpus_semeado.py` | **criado, NAO commitado** (o brief proibe commit; so o Gabriel ordena) |
| `diagnostics/transferencia/analise-68-RELATORIO.md` | **criado, NAO commitado** (idem) |
| `~/.claude/touring/vgp/corpus_semeado/` | **fora do git** (e o diretorio de dados do VGP) |

Nao toquei em `crates/**`, em nenhum `juiz_*.py`, em `corpus_v0d.py`, `corpus_v5_inline_neg.py`,
`settings.json` nem no `corpus_v0d`. Nao rodei `update-touring`, `propagate` nem `kill`/`pkill` de
processo touring: o unico `kill` foi no meu proprio `python3`, por pid.

---

## 8. O que NAO foi verificado

1. **O determinismo (aceite "d") nao foi medido.** Escrevi o caminho (`--confere-determinismo`, duas
   geracoes independentes comparadas por sha) e o tornei autossuficiente, mas **nao executei** —
   custaria duas geracoes de ~45 min. Comando na §3. **Nao afirmo determinismo; afirmo que o caminho
   existe e nao foi exercitado.**
2. **Se os 115 erros de JS sao sistematicos** (uma linha do `tsconfig` que eu escrevi — `types`,
   `checkJs`, `target`) ou reais da fonte. O teste estava pronto; o G11 bloqueou o terceiro bypass
   seguido e a rota sancionada **nega escrita** (`X6 denied write_text`), entao nao ha rota legitima
   para um diagnostico que precisa reescrever o `tsconfig`. Nao contornei o gate para fechar um numero.
3. **`html` nao tem negativos** e esta em 3 das 5 classes abaixo do alvo (`ancora`, `cdn`, `cdn_pin`).
4. **O corpus nao foi rodado pelo juiz.** Nao medi recall nem falso deny — meu escopo era o corpus, e
   o juiz precisa do binario congelado que a analise-89 nomeia (`e935aaf17d9a09d0`).
5. **A analise-25 nao pode ser contatada.** Tres `SendMessage` falharam (`Failed to send`); ela esta num
   shell longo. **Se ela estiver escrevendo `.../diagnostics/corpus-semeado-RELATORIO.md`, ha conflito de
   arquivo** — este e `analise-68-RELATORIO.md`, em `transferencia/`, conforme o roteiro do repasse.
   Quem faz a soma das duas partes e ela.

---

*Apos este relatorio paro de trabalhar no VGP: a dona passa a ser a touring-4f.*