---
fase: 3.4 — A/B fatorial do compactador de resultado (C) e do mod de evicção (E) [AB34]
pre_registrado_em: 2026-10-31 (BRT) — a hora vale pelo commit deste arquivo
status: PRÉ-REGISTRADO. O mecanismo está pronto e testado; NADA foi armado (nenhum timer, nenhuma flag, nenhum restart, nenhum mod instalado). Quem arma é o orquestrador, pelo §8.
---

# AB34 — pré-registro do fatorial 2×2 compactador × evicção

Este documento fixa **antes de qualquer dado** a pergunta, o desenho, o calendário, o
mecanismo, os desfechos, a análise e a regra de decisão. Mudar qualquer item depois do dia 0
invalida o experimento; o que mudar vira um experimento novo com pré-registro novo.

Decisões do Gabriel já tomadas (`2026-10-29-pendencias-consolidadas.md`, 31/10): **A/B completo
agora**, **antes** do A3 (o 3.11 entra depois); **release só na virada do dia** durante o A/B.

## 1. Pergunta

1. **C** — `TOURING_RESULT_COMPACTOR=on` (`result_compactor.rs`: troca a saída de um `Bash` longo
   e em forma de log por cabeça, falhas, cauda e um ponteiro para o bruto no disco) reduz o
   contexto que entra por turno humano em pelo menos 10%, sem piorar o canal de ferramentas?
2. **E** — `TOURING_COMPACT_EVICT=on` (mod `tools/mods/touring-compact-evict`: na fronteira da
   compactação, troca resultados já superados por um stub que aponta o bruto) faz o mesmo?

Cada fator vira **default-on** só se passar a regra do §6; senão fica desligado. O padrão atual
dos dois é desligado.

## 2. Desenho

**Fatorial 2×2 por DIA** (00:00–24:00 America/Sao_Paulo): fator C ligado/desligado × fator E
ligado/desligado. Quatro células, cada uma exatamente **2 dias**:

| Célula | C (compactador) | E (evicção) |
|---|---|---|
| `--` | desligado | desligado |
| `C-` | ligado | desligado |
| `-E` | desligado | ligado |
| `CE` | ligado | ligado |

**8 dias em 2 blocos completos de 4** (cada bloco tem as quatro células uma vez). A ordem de
cada bloco é uma permutação sorteada por semente, **escrita no arquivo**: semente **20261105**,
embaralhamento de Fisher-Yates sobre `random.Random(seed).random()` (o fluxo do Mersenne
Twister é estável entre versões do Python; `random.shuffle` não promete isso). O validador do
calendário **refaz o sorteio a partir da semente e recusa** um `blocks` que não o reproduza, e
recusa uma semente que ponha a mesma célula na mesma posição em dois blocos (equilibra tendência
linear entre blocos). A semente 20261101, a primeira tentada, sorteou dois blocos idênticos e
foi descartada pela regra acima **antes de qualquer dado**; 20261105 é a primeira a partir dela
que passa.

| Dia | 0 | 1 | 2 | 3 | | 4 | 5 | 6 | 7 |
|---|---|---|---|---|---|---|---|---|---|
| Bloco | 0 | 0 | 0 | 0 | | 1 | 1 | 1 | 1 |
| Célula | `C-` | `CE` | `-E` | `--` | | `-E` | `C-` | `--` | `CE` |

`day0` fica `null` em `ab34-calendario.json` e **entra num commit próprio antes do dia 0**; esse
commit é a hora do calendário. O efeito principal de cada fator compara 4 dias com 4.

**Escopo.** Só entram transcritos de `projects/touring` (`transcript_dirs` do calendário): é o
projeto cujas sessões usam o daemon **global** (pin `dev`), o único que o mecanismo reinicia
(`daemon_socket`). Os daemons por projeto (`analise`, `konverter`) têm binário e lock próprios e
**não** são tocados; sessões deles ficam fora.

**Liberações (decisão do Gabriel):** um `update-touring`/release só vale **entre blocos**, ou
seja, na virada do dia 3 para o dia 4. Um bloco que viu **dois binários** é descartado inteiro
(§4). Depois de qualquer release, rodar `ab34_arm.py arm --yes` na hora (§8, R2).

## 3. Mecanismo de troca

Duas flags vivem em **processos diferentes**, e é a diferença que dá o trabalho.

### 3.1 Fator C — o ambiente do DAEMON (achado, com arquivo:linha)

* A flag é lida com `std::env::var` **dentro do daemon**, a cada resultado:
  `crates/touring-hook-handlers/src/hooks/result_compactor.rs:113-114` (`Config::from_env`),
  chamada por `attach` (`:156-158`), que o `post_bash.rs:61` invoca. Ligar/desligar exige o
  daemon com **outro ambiente**, ou seja, `touring daemon-ctl restart` (`touring daemon-ctl --help`).
* O ambiente de um daemon é **o do processo que o spawnou**: `spawn_with_launcher`
  (`crates/touring-foundation/src/daemon_spawn.rs:185-214`) faz `Command::new(...)` sem
  `env_clear`; só `scrub_per_command_relaxations` (`:82-88`) tira `TOURING_CODE_MODE` e
  `TOURING_CODE_ONLY`. Os sítios de spawn acrescentam o mínimo: `daemon_ctl.rs:798-821`
  (`TOURING_DAEMON_SOCKET`; e, num daemon por projeto, `CLAUDE_PROJECT_DIR`/`TOURING_PROJECT_ROOT`/cwd)
  e a autoinicialização por hook, `crates/touring-hooks/src/main.rs:1266-1272`.
* O `systemd-run --user --scope` do launcher (decisão 2-A) **repassa** o ambiente do chamador:
  provado aqui com uma variável de sonda (`ABC_PROBE=zzz systemd-run --user --scope env` a
  devolveu). Não há unit do daemon (`~/.config/systemd/user` só tem `routine-touring-scout`) nem
  arquivo de configuração que a flag possa ler.
* **Medido no daemon vivo (pid 461574, 31/10):** `/proc/<pid>/environ` traz `CLAUDE_CODE_SESSION_ID`,
  `CLAUDECODE`, `TOURING_PILLAR_INDUCTION_ARMED` e as chaves de API — é o ambiente **de uma
  sessão Claude Code** (quem o autoiniciou por último), não um ambiente declarado; a flag está
  ausente e o cwd é `~/projects/touring`.
* **Conclusão: o ambiente do daemon NÃO é determinístico por construção** — vale o do último
  spawner (uma sessão, um `update-touring`, um `daemon-ctl restart` de qualquer shell). O mecanismo
  não confia no restart: **lê `/proc/<pid>/environ` de volta** e repete (§3.3).

### 3.2 Fator E — o ambiente da SESSÃO

O mod lê `await $.env.get('TOURING_COMPACT_EVICT')` (`register.ts:80`), `on`/`1`/`true`/`yes`
liga. O braço é `env.TOURING_COMPACT_EVICT = "on"` no `~/.claude/settings.json` (chave ausente =
desligado), com backup e escrita atômica, exatamente como o A3 fez com a injeção. Vale por
**sessão**, fixado quando ela abre (convenção do A3). **Não documentado:** a doc oficial de
`$.env` só diz "get and set environment variables"; que seja o ambiente do processo do Claude
Code com o `env` do settings aplicado na abertura é **hipótese**, provada pelo pré-voo P3 (§8).

### 3.3 `ab34_arm.py arm` (idempotente, com `--yes` grava; sem ele é dry-run)

1. Lê o calendário (falha alto se malformado), calcula o dia pelo relógio local e a célula.
2. **Observa:** `env.TOURING_COMPACT_EVICT` do settings; do daemon global, via `daemon-ctl status -j
   --socket`, o pid e a flag em `/proc/<pid>/environ` (REGRA #19: `comm` tem de ser `touring-daemon`).
3. **E:** se diverge, backup do settings e escrita atômica.
4. **C:** se diverge, `touring daemon-ctl restart --socket <global>` **sob o ambiente e o cwd do
   daemon antigo com UMA variável mudada** (`restart_env`: nada além da flag muda, nem as chaves
   que o daemon já carregava), depois **relê** `/proc`; se a flag não colou (um hook de outra
   sessão autoiniciou um daemon sem a flag e ganhou o lock do singleton), repete, no máximo 3 vezes.
5. **Registra** uma linha em `~/.claude/touring/ab34/arm.jsonl`: célula, o que viu, o que fez, o
   que verificou (`ok`), a identidade dos binários (`touring --version` e sha do `touring`; exe e
   sha do daemon em execução; sha do shim de hook) e o snapshot cumulativo de
   `code_mode_adherence` do `touring kpi -j` (insumo do desfecho de dano, §5).
6. Exit 0 verificado · **3** armado mas NÃO verificado · **4** o calendário diz que não se aplica
   agora (antes do dia 0, depois do fim; as units tratam como sucesso) · **2** entrada inválida.

A unit `ab34-arm` roda **de hora em hora** (`OnCalendar=*-*-* *:00:30`, `Persistent=true`: o timer
do systemd recupera o disparo perdido quando a máquina dorme à noite; o cron não recuperava, §5 do
A3). A 1ª execução do dia arma; as demais são **batimentos** (idempotentes, sem ação) e a
evidência de que nada derivou. Um batimento que **precisa agir** é registro de **deriva**.

`reset` desliga as duas flags pelo mesmo caminho, com backup e restart verificado, e **recusa
(exit 4) enquanto o calendário tiver dia em curso**. `arm --force-cell XY --yes` é o pré-voo: só
vale com o calendário inativo, usa o mesmo caminho e registra `event: preflight`.

### 3.4 O que NÃO é determinístico (declarado antes)

* O restart perde a corrida para um autostart de hook; o laço de leitura/repetição reduz isso,
  não o elimina. O `ok: false` fica no log e o dia é excluído (§4).
* Um spawn de daemon por **outro agente** (um `update-touring` de outra sessão, um crash seguido
  de autostart) reverte C **entre dois batimentos** (até ~1 h). O batimento seguinte repara e
  registra deriva; o dia sai (§4). Mitigação operacional: nenhum release fora da virada de bloco.
* A flag E vale por sessão aberta; uma sessão aberta **antes** do `arm` do dia carrega o
  ambiente da véspera (§4, `aberta_antes_do_arm`).
* Daemon/CLI de projetos pinados em outra versão não entram (escopo do §2).

## 4. Unidade, atribuição e exclusões

**Unidade de análise: o dia** (8 observações). Cada dia agrega as sessões **abertas** naquele dia
(a data local do 1º registro do transcrito) que o experimento consegue atribuir com segurança.

**Sessões descartadas** (contadas e relatadas, nunca em silêncio):

| Motivo | Regra |
|---|---|
| `cruza_a_virada` | o último registro cai em outro dia local que o primeiro (as duas flags mudam na virada) |
| `aberta_antes_do_arm` | o 1º registro é anterior à primeira execução verificada do `arm` do dia (a sessão carregou o ambiente da véspera) |
| `fora_da_sequencia` | o 1º registro cai antes do dia 0 ou depois do dia 7 |

**Dias excluídos** (listados com o motivo; um dia pode ter vários):

| Motivo | Regra |
|---|---|
| `sem_registro` | nenhum registro do `arm` naquele dia (sinal ausente não é zero) |
| `nao_verificado` | algum registro do dia com `ok: false` |
| `deriva` | algum registro do dia **depois do primeiro** com ação (a flag derivou e foi reparada) |
| `binario_misto` | o **bloco** do dia viu mais de um binário (`touring_sha`+`daemon_sha`) em qualquer registro de qualquer dia seu: **todos os dias do bloco saem** |
| `binario_ausente` | algum registro do bloco sem sha de binário |
| `poucos_turnos` | menos de 20 turnos humanos nas sessões atribuídas do dia — o dia é excluído **e contado** |
| `contaminado_c` | dia com C desligado em que surgiu arquivo novo em `~/.claude/touring/compacted/` (o daemon é global: o arquivo prova que C esteve ligado) |
| `em_curso` | o dia ainda não terminou no `--as-of` |

Um release **entre** blocos é permitido: cada bloco precisa de um único binário, e o modelo (§6)
leva o bloco como efeito fixo, o que absorve a diferença entre blocos. A versão do Claude Code
de cada sessão (`version` do transcrito) entra no relatório por dia; uma troca de versão do CC é
ressalva, não exclusão.

**Verificação de manipulação (relatada, não decide):** arquivos novos em `compacted/` (dias C
ligados) e em `evicted/` (dias E ligados). Dia com tratamento ligado e **zero** arquivos fica
no relatório como "tratamento não disparou" — análise por intenção de tratar, o dia permanece.
Arquivo em `evicted/` num dia E desligado é só aviso (sessão antiga de outro dia pode tê-lo escrito;
as sessões do dia já foram protegidas por `aberta_antes_do_arm` e `cruza_a_virada`).

## 5. Desfechos

**Primário — bytes de contexto por turno humano, por dia:**
`(bytes injetados por hooks + bytes dos tool_result) / turnos humanos`, somados sobre as sessões
atribuídas do dia.

**Danos (não-inferioridade), por dia, em pontos percentuais:**

| Métrica | Definição | Fator |
|---|---|---|
| `erro_pp` | resultados de ferramenta com erro (sem os negados por hook nem os vazios) ÷ chamadas | C e E |
| `retry_pp` | chamadas que repetem a assinatura de uma das 8 anteriores ÷ chamadas (`retry` do `measure_tool_trio.py`) | C e E |
| `wasted_pp` | pares de retry do `code_mode_adherence` ÷ execuções do `touring run`, por **delta** entre os snapshots cumulativos do `arm` do dia D e do dia D+1 (o último dia usa o `reset`) | C e E |
| `raw_c_pp` | chamadas `Read`/`Bash`/`Grep`/`Glob` cujo input cita `/.claude/touring/compacted/` (o ponteiro do bruto do compactador) ÷ chamadas | C |
| `raw_e_pp` | idem para `/.claude/touring/evicted/` (o bruto da evicção) ÷ chamadas | E |

**Instrumento: reaproveitado, não reescrito.** Chamadas, erros, retries e `result_bytes` vêm de
`scripts/measure_tool_trio.py` (`measure_session`, o mesmo do A3); os blocos injetados, de
`scripts/r3_a1_kpi.py::hook_injections` (só `hook_success`, como o KPI); o predicado de turno
humano, de `client/skills/loop-engineering/scripts/hooks/loop_turn_record.py::is_human_record`
(a fonte única que o guard D8 trava contra o Rust). Divergência declarada do KPI: registros
`isMeta` e de subagente (`isSidechain`) não contam como turno.

**Por que não `touring kpi -j` direto.** O que o `context_budget` entrega, por escopo:

| Escopo | Campos |
|---|---|
| **por sessão** (`by_session[]`) | `session_id`, `user_turns`, `injected_bytes`, `injected_bytes_per_turn`, `tool_calls`, `must_emitted`, `must_followed`, `duplicate_blocks`, `cross_session_duplicate_bytes`, `injection_suppressed_bytes` |
| **por projeto** (agregado) | `tool_output.by_tool[].bytes`, `window_bytes.{tool_results,assistant,user,injection}`, `tpcd_proxy_bytes_per_tool_call` |
| **cumulativo global** | `code_mode_adherence.{runs_total,runs_ok,wasted_attempts_retry_pairs}` |

Os bytes de **tool_result não são por sessão** no KPI, e o KPI lê só os **3 transcritos mais
recentes** (`crates/touring-cli/src/cli/context_budget.rs:40`, aplicado em `:1108`) e as linhas
`by_session` não têm data. Por isso o primário e os danos por transcrito são recomputados **por
dia** a partir dos transcritos; do KPI só entra o `code_mode_adherence`, por snapshot.

**Secundário (relatado, fora da regra):** compactações por dia e `postTokens` médio dos registros
`compact_boundary` (`compactMetadata.postTokens`). Ver §7: é a única medida que o transcrito dá
sobre o efeito de E.

## 6. Análise e regra de decisão

1. **Dias retidos** = os que passam pelas exclusões do §4.
2. **Modelo:** regressão de mínimos quadrados de `log(bytes por turno)` sobre `C`, `E` e **efeito
   fixo de bloco** (sem interação: C age no `PostToolUse` do Bash e E na fronteira da compactação,
   canais distintos). O efeito de cada fator é a variação relativa `exp(β)−1`; o IC de 95% usa
   a t de Student com `n − p` graus de liberdade (tabela fixa no código; 4 gl com os 8 dias,
   t = 2,776). Os danos usam o mesmo desenho em pontos percentuais, sem log.
3. **Interação C×E:** relatada como exploratória (o ajuste com o termo extra), **fora da regra**.
4. **Regra por fator** (cada fator é decidido sozinho, com a mesma base de dias):
   - **`adotar`** só se **todas** valerem: o efeito estimado no primário é **≤ −10%**; o IC de
     95% **exclui o zero** (limite superior < 0); e **nenhuma métrica de dano** tem o limite
     superior do IC acima de **+2 pp**. Métrica de dano sem estimativa **bloqueia** a adoção
     (sinal ausente não é zero);
   - **`nao-adotar`** se algum dano estiver estabelecido (limite **inferior** do IC > +2 pp), ou
     se o IC do primário **excluir** uma redução de 10% (limite inferior > −10%);
   - **`inconclusivo`** em qualquer outro caso, inclusive com **menos de 2 graus de liberdade**
     (`MIN_DF`). Inconclusivo mantém o padrão **desligado**.
5. Sem correção de multiplicidade: `adotar` é uma conjunção (todas as condições), que já é a
   forma conservadora; a disjunção de `nao-adotar` só recusa, nunca adota.
6. O relatório (`ab34_arm.py analyze --json`) traz a tabela dia a dia, as exclusões com motivo, as
   sessões descartadas, os efeitos com IC, os danos, a interação, as ressalvas e o veredito.

## 7. O que este experimento não responde

* **Provavelmente sai inconclusivo, e isso é um resultado possível, não uma falha.** Com 8 dias e
  4 gl, o erro-padrão de um efeito principal é `σ/√2` (σ = desvio residual de dia para dia do
  log dos bytes por turno) e o IC é ±2,776·σ/√2. Para o IC de uma redução real de 10% excluir o
  zero, σ tem de ser ≤ ~5%; com 80% de poder o efeito detectável é ~2,6σ (σ = 5% → 13%;
  σ = 10% → 26%). Dia a dia a mistura de tarefas varia mais que 5%; o mix confunde (a validade é
  média). **Cada exclusão tira graus de liberdade**: com um bloco inteiro fora restam 4 dias e
  1 gl, e o veredito é `inconclusivo` por construção.
* **E atua sobre a entrada do sumarizador, não sobre o transcrito.** O mod troca resultados
  superados por stubs no que vai ser resumido (`session.compact`: `next({ ...e, messages })`,
  `register.ts:5-7`); o transcrito, já escrito, deve guardar os `tool_result` originais (inferência
  pelo contrato; não verificada). O primário (bytes que entram por turno) **não deve mover** com E, e o resultado esperado é `nao-adotar` ou `inconclusivo` para E **sem que isso diga
  que o mod é inútil**: ele pode reduzir o custo da chamada de compactação e mudar a qualidade do
  resumo, que nem o primário nem o transcrito medem (só `postTokens`, secundário). Um teste de E
  que valha exige o tamanho da entrada da compactação, que o transcrito não traz.
* **C só aparece no transcrito se ele guardar a saída substituída.** Em 400 transcritos recentes há
  **zero** rodapés do compactador (ele nunca esteve armado numa sessão real); se o Claude Code
  grava o `stdout` original, o primário não vê C. O pré-voo P1 decide, **antes** do dia 0.
* O tamanho do **resumo** pós-compactação e o efeito de `--resume` (limite conhecido do README do
  mod: não se sabe se os stubs sobrevivem) não são medidos.
* Sessões que atravessam a virada ou abrem antes do `arm` saem; sessões longas (as que mais
  compactam) ficam sub-representadas — igual nas quatro células, mas o resultado vale para sessões
  curtas e médias.
* Efeito sobre a **qualidade da entrega** (testes verdes por tarefa) fica fora; só há o canal.
* Daemons por projeto e outras máquinas ficam fora (§2).

## 8. RUNBOOK (para o orquestrador executar; nada foi executado)

Toda a rotina usa `docs/plans/2026-10-31-ab34-compactador-eviccao/` como diretório de trabalho
(`cd ~/projects/touring`). `python3 -m pytest -q -p no:cacheprovider` no bundle precisa estar verde
antes de armar.

### R0 — Pré-voo (gates de go/no-go; falhou um, **não arme**)

**P2 — instalar e validar o mod (congelado por cópia).** O CC não tem pasta `~/.claude/mods`;
um mod é um plugin e a instalação persistente passa por um marketplace. Marketplace local:

```bash
mkdir -p ~/.claude/ab34-marketplace/.claude-plugin
cp docs/plans/2026-10-31-ab34-compactador-eviccao/mod-install/marketplace.json ~/.claude/ab34-marketplace/.claude-plugin/marketplace.json
cp -r tools/mods/touring-compact-evict ~/.claude/ab34-marketplace/touring-compact-evict
claude plugin validate ~/.claude/ab34-marketplace
claude plugin validate ~/.claude/ab34-marketplace/touring-compact-evict
claude plugin test ~/.claude/ab34-marketplace/touring-compact-evict
claude plugin marketplace add ~/.claude/ab34-marketplace
claude plugin install touring-compact-evict@ab34-local
claude plugin list
```

Cópia, não link: o código do mod fica **congelado** durante o experimento. Identidade da cópia
(`sha256`, primeiros 16 hex, de `tools/mods/touring-compact-evict` no commit `394fc635`):
`hooks/register.ts` `df8beebd6fb0e41c` · `hooks/evict.ts` `bd8c0060c8a06389` · `hooks/hooks.json`
`c835c92d8e1159d2` · `.claude-plugin/plugin.json` `1a39009232c4fae6`; conferir com
`sha256sum` na cópia. O README do mod diz testado no CC **2.1.311** e a API de mods se move entre
releases; o CC instalado hoje é **2.1.316**: `validate` e `test` precisam passar nele. Com o mod
instalado e a flag ausente ele é **inerte** (`return next(e)`), nos dois braços.

**P3 — a flag do settings chega ao mod, e o mod age.**
`python3 ab34_arm.py arm --force-cell -E --yes` (calendário com `day0: null`: vale como pré-voo e
registra `event: preflight`). Abra uma **sessão nova** em `projects/touring`, leia um arquivo grande,
edite-o, leia de novo, rode `/compact` e confira `ls -la ~/.claude/touring/evicted/`: tem de haver
arquivo novo. Zero arquivos = o `env` do settings não chega ao `$.env.get`: **pare** (E precisaria de
outro canal).

**P1 — o transcrito guarda a saída compactada?**
`python3 ab34_arm.py arm --force-cell C- --yes`, sessão nova, e rode um Bash que **não** termine em
impressor/seletor e passe de 2.400 bytes, por exemplo
`python3 -c "print('\n'.join(f'compiling crate {i} ... ok' for i in range(300)))"` (saída 0, sem
vocabulário de falha; `echo`, `printf`, `cat`, `touring` etc. são "payload" e nunca são compactados,
e uma saída diferente de 0 também não: `result_compactor.rs:246-260,270-274`). Confira que há
arquivo novo em `~/.claude/touring/compacted/` **e** que o `tool_result` do transcrito (em
`~/.claude/projects/-home-gabrielgadea-projects-touring/<sessão>.jsonl`) termina no rodapé
`[touring result-compactor] stdout was …`. Arquivo no disco **sem** rodapé no transcrito = o
transcrito guarda o original: o primário não enxerga C, **pare** e redesenhe o desfecho.

**P4 — dry-run e estado.** `python3 ab34_arm.py status` e `python3 ab34_arm.py arm` (sem `--yes`)
mostram o diff sem gravar.

**P5 — volta ao zero.** `python3 ab34_arm.py reset --yes`; confira `status` (as duas flags
desligadas; `/proc/<pid>/environ` do daemon sem `TOURING_RESULT_COMPACTOR`).

### R1 — Armar

1. Escolha o `day0` (a data local cujas 00:00 abrem o dia 0; pode ser amanhã), edite
   `ab34-calendario.json` e **faça o commit só desse arquivo**: é a hora do calendário.
2. Instale e ligue as units (`systemd/` do bundle; antes do dia 0 o `arm` sai com exit 4, sem efeito):

```bash
cp docs/plans/2026-10-31-ab34-compactador-eviccao/systemd/ab34-{arm,reset}.{service,timer} ~/.config/systemd/user/
systemctl --user daemon-reload
systemctl --user enable --now ab34-arm.timer ab34-reset.timer
systemctl --user list-timers 'ab34-*'
```

### R2 — Durante

* Diário: `python3 ab34_arm.py status` · `tail -3 ~/.claude/touring/ab34/arm.jsonl` (campo `ok`) ·
  `python3 ab34_arm.py analyze` (o progresso; dias `em_curso` aparecem).
* **Release só entre os dias 3 e 4.** Faça-o na virada e rode `python3 ab34_arm.py arm --yes` logo
  depois (o `update-touring` reinicia o daemon com o ambiente do chamador, sem a flag; o `arm` o
  corrige). Release no meio de um bloco descarta o bloco inteiro (§4): evite.
* Não spawne daemon à mão nem rode `daemon-ctl restart` fora do `arm`: reverte C.
* Registro em `~/.claude/touring/ab34/arm.log` (stdout das units) e `arm.jsonl` (os dados).

### R3 — Encerrar

1. Depois das 24:00 do dia 7: `python3 ab34_arm.py analyze --json > docs/plans/2026-10-31-ab34-compactador-eviccao/AB34-resultado.json`
   e o texto (`analyze` sem `--json`) para o relatório.
2. O `ab34-reset.timer` (00:10) faz o reset sozinho depois do último dia; confira `status`.
3. Desligue as units e desinstale o mod:

```bash
systemctl --user disable --now ab34-arm.timer ab34-reset.timer
rm ~/.config/systemd/user/ab34-{arm,reset}.{service,timer} && systemctl --user daemon-reload
claude plugin uninstall touring-compact-evict@ab34-local
claude plugin marketplace remove ab34-local
rm -r ~/.claude/ab34-marketplace
```

4. Aplicar o veredito (tornar C ou E default) é um passo à parte, com decisão do Gabriel.

### R4 — Abortar

`systemctl --user disable --now ab34-arm.timer ab34-reset.timer`; ponha `day0: null` no calendário
(com um campo `encerrado` explicando, como o A3 fez) e rode `python3 ab34_arm.py reset --yes`.
Nenhuma conclusão sai de um experimento abortado.

## 9. Evidência da construção (31/10/2026)

* `python3 -m pytest -q -p no:cacheprovider`: **150 passam** — calendário (balanço 2 dias por
  célula em 40 sementes, semente reproduzível e embaralhamento fixado por valor, calendário
  malformado recusado em 11 formas), `arm` dia a dia contra uma tabela escrita à mão (oráculo
  que não chama o código), idempotência, dry-run, corrida de autostart, `reset` recusado em
  dias 0/3/4/7, pré-voo, o `Host` real com um `touring` falso e um filho cujo `comm` é
  `touring-daemon` (lê `/proc`, ambiente com uma variável trocada), exclusões por binário/deriva,
  OLS contra numpy, a regra de decisão nos limiares exatos, a análise ponta a ponta com
  transcritos sintéticos e as invariantes do bundle (units, números do documento, marketplace).
* **14 mutantes feitos à mão, 14 mortos**, cada um por um teste que nomeia a causa: troca de C/E
  em `want_flags`; célula trocada no calendário; `reset` sem a recusa; mudança de binário
  ignorada; restart a cada `arm` (não idempotente); IC que pode tocar o zero (`hi <= 0`); dia de
  exatamente 20 turnos excluído (`<=`); quantil t errado; flag do daemon nunca relida; binário
  desconhecido contado como conhecido; sessão que cruza a virada mantida; validador que deixa
  de refazer a semente; limiar de dano 2 → 3 pp; backup do settings omitido. Método: o harness
  roda com `python3 -B` — o `.pyc` guarda mtime em segundos e o tamanho, e dois mutantes de mesmo
  tamanho no mesmo segundo reaproveitavam o bytecode do anterior (o 1º lote deu um falso "morto").
* `ruff check` e `ruff format --check` limpos nos dois `.py`.
* **Fumaça só de leitura no daemon vivo:** `status`, `arm --force-cell CE` sem `--yes` e
  `Host.daemon_state/identity/kpi_snapshot` leram o daemon global (pid 461574, `comm` verificado,
  flag ausente), a identidade (`touring 30.4.101`) e o snapshot cumulativo do KPI; nada gravado.
* **Não provado sem armar** (os pré-voos do §8): o canal do compactador no transcrito (P1), que o
  `env` do settings chegue ao `$.env.get` do mod e que o mod aja (P3), `claude plugin validate/test`
  no CC 2.1.316 (P2), e o restart real do daemon sob o ambiente herdado.
