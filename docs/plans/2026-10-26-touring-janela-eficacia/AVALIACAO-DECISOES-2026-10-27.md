---
type: Decision-Analysis
title: "Avaliação das 5 decisões HITL do PLANO.md — 27/10/2026"
description: "T-W0, T-A5, T-B3, T-A3 e T-C1: fatos medidos, opções, riscos, reversibilidade e recomendação."
plan_id: 2026-10-26-touring-janela-eficacia
okf_version: 1
timestamp: 2026-10-27
---

# Avaliação das 5 decisões (27/10/2026)

> **Status (27/10)**: as 5 recomendações foram **APROVADAS por Gabriel** e incorporadas ao `PLANO.md` v2 §3 e ao DAG (T-C1, T-W0, T-A5, T-A3 como tickets de decisão fechados; B3 encerrado).

> Base: `REVISAO-2026-10-27.md` (premissas re-medidas) + medições deste turno
> (working tree, F2.5, grafo do `Cargo.lock`) + protocolo de hooks no Context7
> (`anthropics/claude-code`, `mods/types/claude-code.d.ts`).
> Grau: FACT = medido agora; INF = inferência com confiança.

## Resumo

| ticket | recomendação | por quê, em uma linha |
|---|---|---|
| **T-C1** | **árvore toda, como triagem**, e já | o gate é binário: corrigir só 2 pacotes deixa 29 CVEs e o F2_5 segue em 0 |
| **T-W0** | **um deploy só**, depois de commit + A1/A2/C5 entrarem no release | deployar agora = 2 reinícios do daemon e WIP alheio no binário |
| **T-A5** | **nenhuma classe nova agora** | as classes eram artefato de medição; o G6 já nega o retry idêntico |
| **T-B3** | **fechar: fora do alcance**; a intenção vai para o B1 | o PreCompact só pode bloquear; o Touring não evicta nada |
| **T-A3** | **quase-experimento grátis primeiro**, piloto pago só com sinal | atribuição custa crédito; medir na emissão (D1) primeiro |

Ordem: **T-C1 → (A1, A2, C5, C0 implementados) → T-W0 → D1 → T-A3 → T-A5**.
T-B3 sai da lista.

---

## T-W0 — quando deployar

**Fatos (FACT)**
- Binários release de **18/10** (`target/release/touring{,-daemon}`).
- Working tree com **61 arquivos sujos**, nada commitado (11 em `client/skills`,
  11 em `client/hooks`, os demais em `crates/*` e `docs/`). O build leva **o
  que estiver no disco**, inclusive edições de outras sessões (lição
  `artefato-do-working-tree-carrega-wip`).
- `propagate-release.sh` tem `--dry-run`, `--stop-before-propagate` e
  `--rollback`; o gate 1/6 constrói o release, roda clippy e os testes dos crates
  tocados, e espera a vaga rust até 2 h.
- O deploy **já muda comportamento** mesmo com o default `full`: o dedup (F4), o
  `directives_only` (F3) e o alvo do `similar-fail` (F1) ficam ativos
  independentemente do interruptor.
- Máquina a 84 °C agora (`hw-slot --status`).

**Opções**
1. Agora, como está: um reinício, e A1/A2/C5 exigem um segundo; o WIP alheio
   entra no binário.
2. **Release R1 = wave S-\* + A1 + A2 + C5 (+ C1 e C4, se prontos)**: um
   reinício só, binário identificável por commit.
3. Adiar: as correções F1/F3/F4 seguem inertes, e o corpus continua medindo o
   binário velho.

**Reversibilidade**: alta (`--rollback`). O custo é o reinício para as 6+
sessões.
**Recomendação**: opção 2. Sequência: commitar só os arquivos desta wave numa
branch → implementar A1/A2/C5 (tamanho S–M) → `propagate-release.sh <v>
--dry-run` → avisar as sessões irmãs → executar fora de pico → `--rollback`
pronto.

---

## T-A5 — quais classes viram enforcement

**Fatos (FACT, re-medição de 27/10)**: dos `cat`, 75,5% são escrita por heredoc;
o retry real de Edit é **6,5%** (não 61,5%); 100% dos `cd` são prefixo de
comando composto; 0 chamadas de `ctx_execute` nas 200 sessões recentes; o
bloqueio de `sleep` é do harness.

**Dois limites estruturais**
- **`updatedInput` não recupera bytes que o modelo já emitiu.** O `cd …&&` e o
  `export` repetidos são texto da **chamada**: quando o hook vê, o custo já foi
  pago. Reescrever muda o que executa, não a janela. (FACT pelo protocolo:
  `updatedInput` "replaces the tool's arguments … before the tool runs".)
- **Gates são globais**: valem para toda sessão que passa pelo shim ou pelo
  daemon, inclusive as irmãs (R6 do plano).

**Opções**: (a) a lista do plano, que quebraria escritas por `cat` e bloquearia
edições legítimas; (b) só o retry idêntico, que **já é o G6**
(`redundant-exact-call`); (c) nenhuma até re-medir.
**Recomendação**: (c) + manter o G6. Reabrir depois do C0 (instrumento
versionado) com um único critério de entrada: a classe só vira enforcement se a
decisão **mudar o resultado ou evitar uma execução desperdiçada**, medido no
D1. Redução de bytes de resultado é assunto do B1 (`updatedToolOutput`), não de
enforcement.

---

## T-B3 — evicção de tool results na compactação

**Fatos (FACT, Context7)**: o `PreCompact` **não** tem `hookSpecificOutput`; só
`block`/`preventContinuation`/`stopReason` (não está em `ClassicResultFields`).
A compactação é do harness. O único texto que o Touring influencia são as
"Compact Instructions" do CLAUDE.md, que são prosa (formato seguido em 7 de 129
compactações, medido em 08/10).

**Opções**: (a) bloquear a compactação, o que é pior; (b) prosa no CLAUDE.md,
que já existe e tem adesão medida baixa; (c) **evicção no nascimento**: o B1
substitui o resultado por resumo + ponteiro em disco (`updatedToolOutput`), e o
que entra pequeno sai pequeno da compactação, com o ponteiro sobrevivendo no
resumo.
**Recomendação**: fechar o T-B3 como "fora do alcance do Touring" e registrar a
intenção como critério de aceite do B1 (todo resultado comprimido carrega o
caminho do cru).

---

## T-A3 — o desenho do harness de desfecho

**O que está em jogo**: atribuição, isto é, se a injeção melhora o resultado.
Custa crédito (R5 HIGH).

**Restrições (FACT)**
- O braço precisa ser trocável por processo: `TOURING_HOOK_INJECTION` é lido no
  processo do hook (o env do hook é herdado do processo do CC). Isso só vale
  para a rota nativa **depois do A1 deployado**; antes, só a rota do shim.
- Lições aplicáveis do próprio acervo: ABBA pareado por id (o p95 de rodada mede
  a máquina), pré-registro com hora, held-out selado, oráculo por tarefa.
- M2: aderência é modelo × apresentação. Uma sessão irmã (Kimi/MiniMax) é
  barata, mas mede **outro** modelo.

**Opções**
| opção | custo | validade | risco |
|---|---|---|---|
| (a) `claude plugin eval` | crédito | boa, **se** carregar os hooks do usuário (não verificado; um sandbox que isola o `~/.claude` não testa o Touring) | INF 0,5 de servir |
| (b) Caliper | crédito | pareado (segundo o notebook) | detalhes não verificados nesta sessão |
| (c) próprio: `claude -p` headless via runner `touring adw`, braços full × no-advisory, ABBA | crédito, controlável | alta | o mais caro de construir |
| (d) **quase-experimento**: dias/sessões alternados por calendário pré-registrado, trio medido na emissão (D1) | **zero** de crédito extra | média (mix de tarefas confunde) | exige D1 + A1 deployados |

**Recomendação**: (d) primeiro. Se der sinal ou ambiguidade, (c) como piloto
pequeno (≈10 tarefas × 2 braços × ABBA, orçamento aprovado por você antes). (a)
só depois de verificar que carrega os hooks do usuário.

---

## T-C1 — o escopo do trabalho com CVEs

**Fatos (FACT)**
- `touring-quality check --gate F2.5 --target .` → **34 CVEs** + 19
  informativas; listados: `ctutils@0.4.2` (RUSTSEC-2026-0385), `quinn-proto@0.11.15`
  (RUSTSEC-2026-0338..0341), **+29 não listados** na evidência.
- Caminhos no `Cargo.lock` (por nome): `quinn-proto ← quinn ← reqwest ←
  touring-foundation/-server/-offensive`; `ctutils ← digest ← sha2 ←
  touring-dispatch/-hook-runtime/-hooks-core`.
- `cargo tree -i quinn-proto --target all` → "nothing to print";
  `cargo tree -i ctutils` → "did not match any packages".

**Inferência (INF 0,7)**: os dois pacotes estão **travados no lock, mas fora do
grafo compilado** (o `quinn` do `reqwest` fica atrás de `cfg(reqwest_unstable)`,
e o lock trava dependências de todo cfg). O F2.5 mede o **lockfile**, não o
binário. Confirmar nativamente antes de agir.

**Opções**: (a) só ctutils/quinn-proto: o F2_5 continua 0 com as outras 29, e o
`quality_gold` segue bloqueado, ou seja, não cumpre o objetivo; (b) **árvore
toda, como triagem**: listar as 34 com (alcançável? há versão corrigida
semver-compatível?), aplicar um `cargo update -p …` só de lock para as
corrigíveis, rodar a suíte completa e documentar as restantes com o motivo;
(c) árvore toda com upgrades maiores, de risco alto e sem necessidade
demonstrada.
**Recomendação**: (b). A triagem é mecânica (código/haiku). A mudança é só de
lock, então é reversível. O `Cargo.lock` já está modificado no working tree:
conferir de quem é antes. Instalar `cargo-audit` no CI. O T-C1 é independente e
pode correr já, entrando no release R1.

---

## T-B3b — reaberta e decidida (27/10/2026)

O T-B3 foi fechado sob a premissa "o `PreCompact` só pode bloquear", que vale
para hooks de comando. Os mods do CC (`session.compact`) alcançam a evicção.
**Decisão de Gabriel: opção (2)**, um mod `session.compact` determinístico do
Touring, **condicionado ao D1**. Descartados: (1) `fast-jev-compaction` (o
classificador hospedado recebe o transcript; poda de 89–96% no default; o ganho
anunciado vem de uma sessão, sem checagem de desfecho) e (3) híbrido, que fica
para depois se (2) não bastar. Implementação: S-16 (B3m) no `PLANO.md`.
