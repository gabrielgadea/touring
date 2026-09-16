# Canvas D: shell no touring-quality (investigação, 15/09/2026)

Pedido de Gabriel: *"Aprovado. E investigue profundamente o Canvas D"*. A decisão D3 aprovada
dizia: shell passa a ser lido como shell em todas as dimensões, dimensões sem semântica de shell
respondem N/A, F2.1/F2.4/F2.6 seguem lendo shell, e o composite é medido antes e depois.

Artefatos: `target/audit-2026-09-14/r2/canvas-d/` (sondas, `before-*.json`, `cf-*.json`,
`counterfactual.py`, `p0_shell_census.{py,json}`).

## Veredito

A premissa do D3 cobre metade do problema. Um script shell é lido como Rust quando é **alvo de
arquivo**, mas **nunca entra na nota de diretório**, que é a do juiz, do `e2e` e dos projetos. Nos
três projetos, 186 scripts rastreados (9.824, 19.122 e 123 linhas) nunca passaram pelos gates P0 numa
nota de diretório. O D3 como aprovado corrige a leitura por arquivo e não move nenhuma nota de
diretório. O que move a nota é pôr shell no corpus, e isso só é seguro depois de consertar a
agregação, que hoje transforma N/A de arquivo em 1,0.

## Fatos medidos

**1. O coletor não aceita shell.** `enumerate_source_and_vendored_files` filtra por `SOURCE_EXTS`
(`crates/touring-quality/src/verifications/mod.rs:92-95`), que não tem `sh`, `bash` nem `zsh`. Arquivo
sem extensão cai fora. `SOURCE_EXTS` não mudou na R1 nem na R2 (`git show 91d58984`).
Prova com um diretório que só tem `install.sh` e um script sem extensão:

| Comando | Resultado |
|---|---|
| `touring-quality score <dir>` | F1.1, F2.1 e F2.4: "0 files in scope (vacuously satisfied)"; composite 0,914 **Platinum** |
| `touring-quality check --gate F2.1 --target <dir>` | 1,000 **Diamond**, rc=0 |
| `touring-quality check --gate F2.4 --target <dir>` | 1,000 **Diamond**, rc=0 |

**2. Contrafactual: o shell pesa zero hoje.** Duas cópias por hardlink de cada projeto (arquivos
rastreados e não ignorados), idênticas a não ser pelos scripts shell, pontuadas pelo mesmo motor:

| Projeto | Scripts | Com shell | Sem shell | Original (controle) |
|---|---|---|---|---|
| touring | 87 | 0,9382244 | 0,9382238 | 0,9382344 |
| konverter | 441 (inclui não rastreados) | 0,7687839 | 0,7687888 | 0,7687880 |

A cópia "com shell" reproduz a nota original, então o método vale. A diferença é ruído de 10⁻⁶.

**3. Um script avulso é pontuado com regras de Rust.** Sondas: `install.sh` real, o mesmo arquivo sem
extensão e um `trivial.sh` de 2 linhas, com controles `trivial.rs` e `trivial.py`.

- 39 dimensões imprimem `(rust)` na evidência. Cerca de 28 devolvem 1,0 porque os detectores de Rust
  não acham nada em shell: é aprovação vazia, não medição (`density_score` dá 1,0 sem achado,
  `quality/score_utils.rs:44`).
- O arquivo sem extensão tem nota idêntica ao `.sh` em todas as dimensões fora da segurança.
- O comentário `#` não é comentário para o léxico de Rust, e `dir/*` abre um comentário de bloco que
  esconde o resto do arquivo até o próximo `*/`. O léxico `SHELL` já existe
  (`quality/code_regions.rs:133`) e nunca é escolhido.
- **Alvo que não é fonte vira artefato.** Um `.sh` fora de `SOURCE_EXTS` é lido na íntegra como
  README, CHANGELOG, CI, runbook, manifesto ou doc de arquitetura (`mod.rs:702-708`). Resultado:
  `trivial.sh` 0,864 contra `trivial.rs` 0,958 e `trivial.py` 0,975. F3.11, F3.13, F4.7 e F4.11
  saem com 0,01 ("missing [Unreleased]", "no cargo clippy in workflow", "no runbook").
- F4.10 e F4.12 cobram `metrics::counter!` e `figment`/`dotenvy` de um script. F1.6 dá 0,85 fixo
  a qualquer arquivo sem `Result`.
- O motor já sabe parte do que falta: `lang_for_cc` tem a gramática tree-sitter Bash
  (`quality/complexity.rs:148`) e nunca a recebe. `count_lines` reconhece `#` para `bash`/`sh`, não
  para `shell` (`quality/complexity.rs:205`).

**4. A agregação apaga o N/A de arquivo.** Dimensões agregadas por arquivo guardam só o valor
(`scope_report.rs:267`, `FileScore = (&Path, f32, usize)` em `aggregate.rs:193`), e `aggregate`
reconstrói o status a partir dele (`DimScore::from_value`, `aggregate.rs:223`). A lista vazia é
Pass por definição: `aggregate.rs:203` devolve 1,0 com "0 files in scope (vacuously satisfied)". Um `[N/A]` de arquivo vira 1,0 Pass dentro de
`WeightedLoc`/`WorstOf`. Hoje isso está latente, porque nenhuma dimensão por arquivo devolve N/A. Com
shell no corpus e N/A por dimensão, cada script inflaria a nota.

**5. O que os gates P0 diriam dos scripts** (cada script como alvo de arquivo, F2.1/F2.4/F2.6):

| Projeto | Scripts | F2.1 | F2.4 | F2.6 |
|---|---|---|---|---|
| touring | 87 | 0 | 2 Fail, 2 Warn | 0 |
| konverter | 2 | 0 | 1 Warn | 0 |
| analise | 97 | 6 Fail | 7 Warn | 2 Warn |

Os Fail conferidos são falsos positivos:

- touring, F2.4: `client/omarchy/hooks/post-update.d/45-touring-ci-fire:38` é
  `TOKEN="$(cat "${TOKEN_FILE}")"`, com o arquivo sob `$HOME/.config`.
- touring, F2.4: `docs/plans/2026-08-11-memory-hashtag-library/validate_f2_e2e.sh:8` é um caminho
  `docs/plans/...` lido como literal de alta entropia.
- Nenhum valor foi impresso na conferência, só a forma: nome da variável, tamanho e classes de
  caractere.
- analise, F2.1: 5 dos 6 são o idioma `cd "$(dirname "$0")/../.."` lido como path traversal. O
  sexto é um `import ... "../../"` de JavaScript dentro de heredoc.
- UNVERIFIED: os Warn de F2.4 e os 2 de F2.6 do analise.

**6. shellcheck.** A versão 0.11.0 está instalada, mas nenhum crate a usa. O CI roda
`shellcheck -S warning` só em `client/omarchy` (`.github/workflows/ci.yml:238-240`). Achados sobre
os scripts rastreados:

| Projeto | error | warning | info | style |
|---|---|---|---|---|
| touring | 0 | 22 | 46 | 2 |
| konverter | 0 | 2 | 0 | 0 |
| analise | 116 (113 são SC1017, CR de fim de linha) | 72 | 103 | 24 |

SC1017 é defeito real: o bash lê o `\r` como parte do comando. Os 113 estão num único arquivo,
`.claude/hooks/test_quality_gate_hooks.sh`, com 140 linhas CRLF (conferido pela analise-08 com
`shellcheck -f gcc` arquivo a arquivo). No analise, 2 dos `.sh` rastreados
(`start_rag_api.sh`, `start_rag_system.sh`) foram apagados na árvore sem commit; o censo e o
shellcheck só leram arquivos presentes no disco.

## Correção ao registro da R2

O R2-20 lista "scripts shell lidos como Rust" entre os 7 arquivos que reprovavam o F2.1 no juiz. Com o
`SOURCE_EXTS` de então, a nota de diretório não lia esses scripts: eles só reprovavam como alvo de
arquivo, na bisseção. A correção W2 (`lang_for_source` no F2.1) vale para o gate por arquivo e não
mexeu na nota do juiz.

## Achados fora do Canvas D

Antes de qualquer mudança, com o motor 30.4.51 da fonte:

- **konverter** está Unranked (0,7688): F2.1 fail-closed em 30 de 1.484 arquivos, F2.4 em 11, e
  F2.5 com 13 CVEs.
- **analise** está Unranked (0,7820): F2.1 em 192 de 8.339 arquivos, F2.4 em 47, e F1.3 truncado
  (corpus de 154 MB).
- **touring** está Platinum (0,9382).

Nenhum desses achados foi triado ainda.

## D3 revisado (proposta para decisão)

| Etapa | O quê | Muda nota de diretório? |
|---|---|---|
| D3.0 | A agregação carrega o N/A de arquivo (peso zero). "0 files in scope" deixa de ser Pass e vira N/A `unmeasured`. | Só em escopo sem fonte |
| D3.1 | `lang_for_source` nas 40 dimensões; N/A onde não há semântica de shell; F1.1/F1.2/F1.4/F1.5 medem shell (Bash tree-sitter, `count_lines`); alvo não-fonte nunca é lido como artefato. | Não |
| D3.2 | Precisão de F2.1/F2.4 em shell: `cd "$(dirname "$0")/.."`, `VAR="$(cat "$FILE")"`, literal de caminho. Cada caso com o verdadeiro positivo ao lado no teste. | Não |
| D3.3 | Shell no corpus (`sh`/`bash`/`zsh` e shebang), com medição antes e depois nos três projetos. | Sim |
| D3.4 | shellcheck no F4.1 para shell. Sem o binário, a dimensão é UNVERIFIED, nunca Pass. | Sim |

Riscos:

- D3.0 muda a nota de escopos só com documentos, o que pode alterar a cláusula
  `measured_whole_scope` de juízes de outras sessões.
- D3.3 e D3.4 mudam a nota do analise, onde a analise-08 roda juiz.
- D3.4 derruba o F4.1 do analise enquanto os 113 SC1017 existirem.

## Decisão e implementação (15/09/2026)

Gabriel aprovou D3.0–D3.4 em ordem, com cada correção de precisão acompanhada do verdadeiro positivo
no mesmo teste. DAG `task_1789510502472468790`. A analise-08 foi avisada antes da mudança de corpus e a
touring-61 liberou o workspace.

| Etapa | Contrato entregue | Onde |
|---|---|---|
| D3.0 | Lista vazia é N/A (`EMPTY_SCOPE_EVIDENCE`); o roll-up deixa de fora o arquivo N/A e, se nenhum se aplica, a dimensão é N/A; erro do motor segue 0,0 | `aggregate.rs`, `scope_report.rs::roll_up` |
| D3.1 | `lang_of` (extensão, depois shebang) substitui `lang_from_ext` nos 43 arquivos; **um gate no despacho** (`not_applicable_to_shell`) responde N/A para script shell fora de `SHELL_DIMS` (F1.1, F1.2, F1.3, F1.5, F2.1, F2.4, F2.6, F4.1), o que também impede ler script como README/CI/runbook; o motor de complexidade conta funções shell e comentário `#`; `is_shell_language` é o predicado único | `verifications/mod.rs`, `complexity.rs`, `code_regions.rs` |
| D3.2 | F2.1 em shell: subida ancorada em `dirname "$0"`/`BASH_SOURCE` (direta ou por variável atribuída dela no mesmo script) não é traversal. F2.4: RHS entre aspas que é expansão de shell (`$(`, `${`, crase, `$NOME` maiúsculo) não é literal, mas `"$2b$…"` e `"$vault1"` são; caminho relativo de diretório com segmento hifenizado não é token | `security.rs`, `f2_4_secrets.rs` |
| D3.3 | `SOURCE_EXTS` ganha `sh`/`bash`/`zsh`; sem extensão entra pelo shebang de shell (`is_scored_source`); **só arquivo regular** entra no corpus | `verifications/mod.rs` |
| D3.4 | F4.1 de script shell vem do ShellCheck (`--format=json1 --norc`, error 1,0 · warning 0,5 · info 0,1 · style 0,05 na curva `density_score`); binário ausente, exit fora de 0/1 ou saída sem JSON é UNVERIFIED 0,5, nunca Pass | `f4_1_idioms.rs` |

**Achado durante a implementação (D3.3).** Com shell no corpus, o analise caiu de 0,782 para 0,673 com
F2.2, F2.3 e F4.3 em Fail. A causa: `start_rag_api.sh` e `start_rag_system.sh` são symlinks rastreados
cujo alvo foi apagado (a leitura inicial era "arquivo apagado"). O coletor aceitava o nome pela extensão, a
leitura falhava e o motor dava 0,0 fail-closed. O defeito é anterior ao D3 (um `.py` quebrado faria o
mesmo); o conserto é `p.is_file()` no coletor, com teste de symlink quebrado e vivo.

**Achado na verificação (D3.4).** Com arquivo ilegível, o ShellCheck imprime `{"comments":[]}` e sai 2;
lido só pelo JSON seria Pass. O teste exercita esse caminho.

**Prova.**

- `cargo clippy -p touring-quality -p touring-analysis --all-targets --all-features -- -D warnings`
  passou limpo.
- Suítes completas passaram (lib, integração e doc): touring-analysis e touring-quality. A lib do
  touring-quality tem 436 testes.
- `cargo check -p touring-quality --no-default-features` passou.
- A mutação matou **23 de 23** mutantes (`target/audit-2026-09-14/r2/canvas-d/mut_d3.py`, arquivos
  restaurados).
- Três lacunas apareceram só ao escrever os mutantes, e cada uma virou asserção:
  - `function nome {` sem parênteses, que `() {` já cobria;
  - um token minúsculo com barras e sem hífen;
  - o exit 2 do ShellCheck.
- O ajuste de N/A no caminho por crate saiu: nenhuma dimensão devolve N/A para um diretório de crate,
  então era código inalcançável.

**Juiz (15/09, unit `touring-d3-judge`, `MemoryHigh=40G`, `CARGO_BUILD_JOBS=8`, depois de
`index rebuild`): CONVERGED, exit 0.** `judge_intact` ✅ · `dag_done` ✅ 6/6 · `quality_gold` ✅ Platinum
0,9382514 · `no_p0_fail` ✅ · `measured_whole_scope` ✅ · `orphans_base` ✅ 1.519 contra 1.579 ·
`cargo_green` ✅ check + test + clippy · `cross_audit` ➖. A cláusula de qualidade usa o
`touring-quality` implantado, anterior ao D3; a nota com o motor do D3 é a medição (d) acima (0,938397
Platinum). DAG `task_1789510502472468790` finalizado.

### Medição, mesmo motor em diretório separado

O `touring-quality` do PATH usa `target/release/touring-quality`, o binário que serve todas as sessões e o
juiz; as medições usaram `target/canvas-d-engine` via `TOURING_QUALITY_BIN`, sem tocar no compartilhado.
Controle de paridade: o estado (b) reproduziu a base (a) com **0 diferenças** nas 50 dimensões do
konverter e do analise.

| Projeto | (a) antes | (b) D3.0–D3.2 | (c) + corpus | (d) + symlink + ShellCheck |
|---|---|---|---|---|
| touring | 0,938234 Platinum | 0,938256 | 0,938394 | **0,938397 Platinum** |
| konverter | 0,768788 Unranked | 0,768788 | 0,769278 | **0,769244 Unranked** |
| analise | 0,782039 Unranked | 0,782039 | 0,673182 (symlinks) | **0,782120 Unranked** |

Os bloqueadores de (d) são os mesmos de (a) nos três projetos. No touring, 110 scripts entram no F1.1 e
ficam fora das dimensões sem leitura de shell; o F2.4 do diretório passa a ler `cidata_build.sh` (Warn
0,5, palavra sem valor). No analise, o F2.1 lê 3 arquivos a mais em Fail do que antes; 2 eram os
symlinks, corrigidos.

Reverificação dos falsos positivos do censo com o motor (d):

| Arquivo | Antes | Depois |
|---|---|---|
| touring `validate_f2_e2e.sh` (F2.4) | Fail 0,0 | Pass 1,0 |
| touring `45-touring-ci-fire` (F2.4) | Fail 0,0 | Warn 0,5 (palavra `TOKEN` sem valor, pondera sem bloquear) |
| analise, 5 validadores `cd dirname/..` (F2.1) | Fail 0,2 | Pass 1,0 |
| analise `frontend/create_remaining_tests.sh` (F2.1) | Fail 0,2 | **Fail 0,2**: `import "../../"` de JavaScript num heredoc, classe fora do escopo aprovado |
| touring `scripts/install.sh` (F4.1) | — | Pass: ShellCheck 0 achados |
| analise `test_quality_gate_hooks.sh` (F4.1) | — | Fail 0,01: 113 SC1017 |

### Pendências que este trabalho deixa

- **Import relativo de JS/TS lido como traversal** (`import … from "../../x"`). Hipótese, não medida: parte
  dos 192 arquivos em Fail no F2.1 do analise pode ser dessa classe.
- **Scripts Python sem extensão** (shebang `python3`) seguem fora do corpus; `lang_of` já os reconhece.
- **Symlinks quebrados no analise**: decisão do dono do repositório.
