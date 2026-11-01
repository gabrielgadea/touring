# O motor de qualidade morre, e o juiz chama isso de nota baixa (16/09/2026)

Achado colhido durante a verificação do B5, **fora do escopo do B5** e ainda **sem causa provada**.
Fica registrado com a evidência inteira porque a próxima pessoa que vir `quality_gold` vermelho vai ler
uma mensagem que aponta para o lugar errado.

## O que aconteceu

O juiz do B5 reprovou `quality_gold` com `tier=None composite=None` em duas passadas. A mensagem que
ele imprime nesse caso é:

```
❌ quality_gold   tier=None composite=None
→ next: raise touring-quality to >= Gold (0.80)
```

A nota não estava baixa. **O motor morreu.** Um shim observador colocado à frente do `touring-quality`
no PATH — que só encaminha para o binário real e guarda o que o juiz descarta — registrou:

```
--- 2026-09-16T01:26:37-03:00 args=score <workspace> --format json rc=139 secs=20 stdout_bytes=0
    stderr: timeout: the monitored command dumped core
    stderr: touring-quality-score: line 89: 2383957 Segmentation fault  nice -n 19 timeout 600 …
```

`rc=139` é 128+11: SIGSEGV. O core confirma (`coredumpctl`):

| Campo | Valor |
|---|---|
| Sinal | 11 (SEGV), `si_code: SEGV_MAPERR` |
| Thread | 2383986 — **não** a principal (2383958) |
| Stack | `#0 0x0000000000000004` — um único frame, salto para endereço inválido |
| cgroup | `b5-judge4.service` (a unit do juiz) |

E não foi o único: às 01:00:57 o mesmo binário caiu com **SIGILL**, também dentro de uma unit do juiz.
Dois sinais diferentes, mesmo binário, meia hora de intervalo.

## O que já foi medido (e o que cada medição descarta)

| Medição | Resultado | O que descarta |
|---|---|---|
| Censo por alvo: 5× workspace, 8× diretório só de shell, 8× diretório de controle sem shell | **0/21 crashes** | O alvo não é o gatilho. Em particular **não é regressão do Canvas D**: o D3 pôs shell no corpus, e o `scripts/` (só shell) mediu 8 vezes sem cair |
| Score em laço enquanto `cargo check --workspace --all-targets` satura a máquina | **0/3 crashes** | Carga de cargo concorrente não reproduz (amostra pequena — não prova ausência) |
| Cronologia da 4ª passada: quality 01:26:17→35, `cargo test --workspace` só às 01:26:38 | crash **sem** cargo concorrente | Derruba "morreu porque o cargo estava junto" |
| Reprodução manual da chamada exata do juiz, logo após a falha | `EXIT=0`, Platinum 0,9384, 20.888 bytes | O binário não está corrompido em disco; a falha é intermitente |
| 5 execuções isoladas do teste `snapshot_returns_nonzero_in_test_process` | 5/5 verde | O outro intermitente da mesma noite também não reproduz isolado |

Placar: **2 crashes em 5 passadas do juiz, 0 em 24 execuções fora dele.**

## O que isto significa para quem lê um juiz vermelho

1. **`tier=None` não é nota baixa — é ausência de medição.** A cláusula é fail-closed, e isso está
   certo; o remédio sugerido é que mente. Antes de "melhorar a nota", rode o score à mão: se voltar
   Platinum, o que caiu foi o motor.
2. **`secs=0` no trace quer dizer cache.** A 3ª passada mostrou `quality_gold` verde com
   `rc=0 secs=0`: o wrapper respondeu do cache, sem medir nada. Um verde de cache não prova que o
   motor está de pé — foi o que escondeu o defeito por uma rodada.
3. **O juiz descarta o stderr da chamada que ele julga.** Sem o shim observador, a única informação
   que sobra é o nome da cláusula. Todo o diagnóstico acima existe porque o stderr foi preservado por
   fora; o juiz não pode ser alterado sem atestação humana (`judge_attest.py --attest`), então isto
   fica como proposta, não como correção aplicada.

## Hipóteses ainda abertas (nenhuma provada)

- **Corrida de dados no motor**: a thread que morre é worker, e o frame é `0x4` — assinatura de
  ponteiro de função corrompido. Combina com paralelismo (rayon) no score de diretório.
- **FFI de tree-sitter**: o histórico da casa tem SIGSEGV em scanner vendorizado
  (`third_party/PATCHES.md`, `ctype` sobre codepoint no md e no bash). O censo por alvo não incrimina
  o shell, mas não isenta o parser: um crash dependente de ordem/concorrência não aparece por alvo.
- **Memória do hardware**: SIGILL e SIGSEGV alternando no mesmo binário, sem reprodução em 24
  tentativas, é assinatura clássica também de RAM instável. Barato de testar e ainda não testado.

## O que fazer a seguir (proposta, para decisão de Gabriel)

1. Rodar o motor sob ASan/valgrind no corpus inteiro, como foi feito com o `tree-sitter-md`
   (`PATCHES.md`): é o instrumento que já pegou esta classe nesta casa.
2. Forçar `RAYON_NUM_THREADS=1` num laço longo de scores: se o crash some com uma thread só, a causa
   é corrida, e o candidato passa a ser o paralelismo do score de diretório.
3. Guardar o stderr do motor no próprio wrapper (não no juiz): hoje `touring-quality-score` repassa o
   stderr, mas nada o persiste — um crash em CI desaparece. Um arquivo por execução em
   `~/.claude/touring/logs` deixa o próximo crash já diagnosticado.
4. Fazer a cláusula do juiz distinguir "não medido" de "medido abaixo do piso" na MENSAGEM (o veredito
   já é o mesmo, e deve continuar sendo). Muda arquivo do juiz → exige atestação.
5. Um `memtest86+` numa janela ociosa, para descartar hardware antes de caçar UB.
