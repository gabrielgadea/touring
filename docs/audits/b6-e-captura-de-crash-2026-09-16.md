# B6 e a captura do crash — o que foi feito, medido e descoberto

> 16/09/2026 · ordem de Gabriel: *"faça os dois agora"* · sessão touring-18, com medições
> cruzadas da analise-08 · **nada propagado sem nova ordem**

Duas frentes numa rodada: fechar o ponto cego do wiring Python (B6) e instalar a camada 1
da estratégia do crash (captura). O que segue é o registro do que **foi medido**, incluindo
quatro defeitos que não estavam no plano e apareceram no caminho — três deles em
instrumentos meus.

---

## 1. B6 — `from pacote import modulo [as alias]`

### O que faltava

`python_qualified_uses` (`crates/touring-code/src/ast/graph/python_uses.rs`) casava um
único tipo de nó: `import_statement`. Todo uso alcançado por `from pacote import modulo` —
a forma dominante em Python de projeto — não produzia aresta, e o símbolo consumido lia
como órfão.

### O tamanho, por dois instrumentos independentes

| | escopo do juiz (analise-08, AST + disco, por ocorrência) | projeto inteiro (touring-18, AST + disco, por símbolo) |
|---|---|---|
| ocorrências da família | 77 de 77 | 649 |
| símbolos alcançados | 53 de 53 | 126, em 49 arquivos |
| `import a.b` pontilhado | **0** | **0** |

A primeira varredura minha, por regex e basename, deu 203 símbolos e 6 pontilhados — um
teto 61% acima, com os 6 sendo homônimos colidindo. A hipótese de "um segundo defeito na
resolução módulo→arquivo" morreu por medição, não por argumento.

### A decisão de implementação que a medição impôs

A chave do mapa `nome_local → módulo` é **`asname or name`**, não só o alias:

| | só alias | ambos | **só sem alias (o que um braço só-alias perde)** |
|---|---|---|---|
| escopo do juiz (53) | 33 | 7 | **13 — 24,5%** |
| projeto inteiro (126) | 59 | 24 | **43 — 34,1%** |

Um braço que lesse apenas `as <alias>` fecharia 40 dos 53 e pareceria um conserto,
entregando três quartos de um.

### Um defeito que só existe porque as duas formas passaram a conviver

Com dois tipos de import alimentando o mesmo mapa, duas ligações podem competir pelo mesmo
nome local (`import u` e `from b import u`). O walk é uma **pilha**, não ordem de fonte:
o vencedor dependia da travessia, e a mesma entrada podia render arestas diferentes entre
execuções. As ligações passaram a carregar o byte de origem, e a última textual vence — que
é também a semântica do Python. Dois testes fixam isso.

### Testes

12 no arquivo (3 antes, 9 novos), `cargo clippy -p touring-code --all-targets -D warnings`
limpo, 754 testes do crate verdes. Três são **controles negativos**, adotados da correção
que a analise-08 fez no `hook_discovery` no mesmo dia:

```python
from pacote import io_utils as io
...
io_utils.PROCESSADO      # Python NÃO liga este nome → a aresta não pode existir
```

Sem esse controle, um verde no caso positivo prova apenas que *alguma coisa* produziu a
aresta, não que foi este braço.

---

## 2. Camada 1 da estratégia do crash — captura

### O que impedia o diagnóstico

1. **`strip = true`** em `[profile.release]`: os dois cores reais trouxeram **um** frame.
2. O wrapper `touring-quality-score` repassava stderr e **não guardava nada** — um crash
   sob CI ou sob o juiz era indistinguível de nota baixa (`tier=None` nos dois casos).

### O que passou a existir

- **Journal por execução** (`~/.claude/touring/logs/quality-runs.jsonl`): alvo, argumentos,
  duração, exit, cauda do stderr, artefato. Limitado a 4000 linhas — um log sem limite é um
  incidente de disco esperando a semana cheia.
- **Artefato por crash**: exit ≥ 128 copia stderr, nomeia o sinal e extrai backtrace do core
  que o systemd já coletou.
- **Perfil release**: `strip = "none"`, `debug = "line-tables-only"`,
  `split-debuginfo = "unpacked"`. O binário foi de 33,9 MB para 58,4 MB — o peso exato dos
  símbolos que faltavam.

### O critério de aceite, cumprido

`scripts/crash_capture_accept.sh` mata um score **real** e verifica o resultado:

```
✅ o wrapper reporta o sinal (134)     ✅ o artefato traz o stderr do motor
✅ a execução deixou linha no journal  ✅ ele nomeia o sinal
✅ um artefato de crash foi criado     ✅ a pilha tem frames NOMEADOS (1163)
```

```
#0  scan () at crates/touring-analysis/src/quality/code_regions.rs:398
#1  non_executable_regions () at crates/touring-analysis/src/quality/code_regions.rs:154
#2  analyze_modernization () at crates/touring-analysis/src/quality/modernization.rs:325
```

### Camada 2 — o harness, versionado

`scripts/crash_matrix.sh`: matriz `(binário, alvo, RAYON_NUM_THREADS, N)` que reporta
**taxa com intervalo de confiança**, nunca um caso isolado. Um zero em 20 execuções é
compatível com uma taxa real de até ~16%, e a tabela diz isso em vez de imprimir "0%".
O motor é chamado **direto**, nunca pelo wrapper com cache: uma matriz de cache hits
reporta uma tabela limpa que não mediu nada.

---

## 3. Quatro defeitos que não estavam no plano

### 3.1 `kill -SEGV` não mata um processo Rust

`/proc/<pid>/status` do motor: `SigCgt: 0000000100000440` — os sinais **7 e 11 estão
capturados**. O runtime instala um handler de SIGSEGV/SIGBUS para diagnosticar estouro de
pilha, e um sinal entregue por `kill(2)` não traz endereço de guard page. Medido três vezes:
o motor **terminou o score e saiu com 0**, com 18 KB de JSON. Quem provasse o coletor assim
concluiria que o binário é imune a segfault.

O aceite usa **SIGABRT**, que não é capturado. Nota lateral com valor de hipótese: com
`panic = "abort"`, o abort do Rust emite `ud2` → **SIGILL** — um dos dois sinais vistos em
produção é compatível com um panic, sem exigir corrupção de memória.

### 3.2 O gate de limpeza que se autobloqueava

`safe-clean.sh` recusou limpar três vezes com "cargo/rustc ativo" **sem build algum**. O
predicado era `pgrep -f "cargo (build|…)"`, que casa a linha de comando — inclusive a do
shell que o invoca. Medidos lado a lado, com um build real rodando:

| predicado | vê |
|---|---|
| `pgrep -x` (compara `comm`) | 2× `rustc` + 1× `cargo`, os builds reais |
| `pgrep -f` (compara a linha) | um `bash` inocente e **só 1 dos 3 reais** |

Errava nos dois sentidos. Corrigido para `pgrep -x`; a recusa agora nomeia os pids.

### 3.3 Disco a 100%

`target/debug` com **615 GB** derrubou o primeiro build (`No space left on device` ao
linkar `libort_sys`), com 64 KB livres — a um passo de corromper índice e bancos. A limpeza
liberou **345 GB**. O gate defeituoso de 3.2 é parte de por que isso acumulou.

### 3.4 Três instrumentos meus falharam em silêncio

Na mesma rodada, medindo o B6: `wiring orphans -j` elide arrays por padrão (li zero órfãos
com 27.300 arquivos varridos), a chave é `module_file` e não `file`, e casar por basename
inflou a contagem em 61%. Nenhum era defeito do produto; os três eram meus, e cada um
produziu um número plausível.

---

## 4. O que NÃO foi feito

- **Nada propagado.** O B6 muda o motor; o benefício medido vive no projeto analise, e
  chegar lá exige release nova e `propagate-release.sh`. Sem ordem, não vai.
- **A baseline `orphans_base` da analise não foi tocada.** Se a contagem nova exigir
  regravação, o dado vai para quem opera aquele escopo e a decisão é de Gabriel.
- **Camadas 2 (execução), 3 (ASan) e 4 (prevenção) da estratégia do crash**: a 2 tem o
  harness pronto e não rodado; 3 e 4 seguem no papel.
- **A causa do SIGSEGV continua não provada.** O que mudou é que o próximo crash nasce com
  pilha nomeada em vez de um frame.
