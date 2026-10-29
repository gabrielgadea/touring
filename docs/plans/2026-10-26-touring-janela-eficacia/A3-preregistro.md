---
fase: S-12 (A3) — desfecho por quase-experimento (d) [T-A3]
pre_registrado_em: 2026-10-27 (BRT) — a hora vale pelo commit deste arquivo
status: ENCERRADO no dia 0 (29/10/2026 17:26) por decisão do Gabriel, sem análise de eficácia; ver §"Encerramento" no fim
---

# A3 — pré-registro do quase-experimento `full` × `no-advisory`

Este documento fixa **antes de qualquer dado** a pergunta, as unidades, o calendário, o
desfecho, a análise e a regra de decisão. Mudar qualquer item depois do dia 0 invalida o
experimento; o que mudar vira um experimento novo com pré-registro novo.

## 1. Pergunta

Retirar os avisos de hook (nível `no-advisory` da política de injeção, S-2/S-4) piora o
desfecho das sessões em relação ao nível `full`? A hipótese de trabalho é que **não piora**
(não-inferioridade) e reduz o contexto injetado. Se não piorar, `no-advisory` vira o
default; se piorar, fica `full`.

## 2. Braços e mecanismo

| Braço | Nível | Como se aplica |
|---|---|---|
| A (controle) | `full` | `TOURING_HOOK_INJECTION` ausente do `env` do `~/.claude/settings.json` |
| B (tratamento) | `no-advisory` | `env.TOURING_HOOK_INJECTION = "no-advisory"` |

O nível é lido do ambiente do processo que EMITE, ou seja, do Claude Code
(`injection_policy::current`, `crates/touring-foundation/src/injection_policy.rs:443`):
o braço vale por sessão, fixado quando ela abre.

**Vazamentos conhecidos (declarados antes, não descobertos depois):**
1. Na rota do daemon, avisos colados na `permissionDecisionReason` de um deny só são
   filtrados pelo ambiente do daemon (PLANO §11, ponto 1). No braço B parte dos avisos
   ainda chega. O efeito medido é, por isso, um **limite inferior** do efeito real.
2. Os cortes dos hooks Python não entram no `injection_journal` (PLANO §11, ponto 2): a
   dose medida (bytes cortados) subconta.

## 3. Unidade, atribuição e calendário

- **Unidade de análise:** a sessão, com o braço em vigor no instante do **primeiro**
  registro do transcript. Uma sessão que atravessa a troca é do braço em que abriu.
- **Troca diária às 04:00 BRT** (madrugada, sem sessão ativa). O dia vai das 04:00 às
  03:59:59 do dia seguinte.
- **Dia 0:** o primeiro dia inteiro depois do deploy do 30.4.99. A data entra no arquivo
  `a3-calendario.json` **antes** do dia 0, num commit próprio; esse commit é a hora do
  calendário.
- **Sequência primária:** 2 blocos ABBA × BAAB = 8 dias: `A B B A · B A A B`. O
  contrabalanceamento cancela tendência linear e efeito de dia da semana de primeira ordem.
- **Extensão (só se a regra do §6 mandar):** mais 2 blocos, `A B B A · B A A B`.
- **Invalidação:** um deploy do Touring, um `update-touring` ou uma mudança de
  `settings.json` fora do mecanismo do §5 durante um bloco invalida o bloco inteiro, que é
  repetido no fim da sequência. **Esclarecimento de 28/10, antes do dia 0:** editar o
  script vivo de um hook que injeta contexto (texto, condição ou família do que ele
  emite), ou os DADOS que ele lê para decidir o que emitir (p. ex. o índice
  `analise/.claude/portfolio/indice.json` do `script_portfolio_gate.py`, reexportado pelo
  `portfolio_export.py`), é da mesma classe e também invalida o bloco, porque muda o
  próprio tratamento. O que estiver no ar antes do dia 0 é ambiente fixo; mudança em modo
  sombra que não altera o que chega ao modelo não conta. Os blocos invalidados ficam
  listados em `a3-calendario.json` (`blocos_invalidados`) com o motivo.
- **Ambiente no dia 0 (registrado em 28/10):** Touring 30.4.99 (Rust de `b46b68ab`);
  `script_portfolio_gate.py` na versão P2 da analise-40 — P1 (`87e007cb`, ~11:10) mudou o
  texto e a condição do `additionalContext` (estado do veredito e distâncias em bits;
  emite quando a recuperação nova tem candidatos) e P2 (`e5509ffa`, ~11:25) passou a ler o
  índice do projeto analise (7.893 scripts), emitindo em casos em que antes calava. P3
  (`2286b45a`, decisão do Gabriel de entrar antes do dia 0, confirmada por ele direto na
  analise-40 às ~11:51): o mesmo hook passou a ser registrado em `Write|Bash` (11:52:34) e
  dispara no Bash só em heredoc `cat` que cria script vigiado; família `plugin-hook`, canal
  `additionalContext` em PreToolUse. Congelados até 06/11 04:00 (hora da última mudança em
  28/10): `hooks/script_portfolio_gate.py` 11:42:19, `portfolio/recuperar.py` 11:45:50,
  `portfolio/heredoc.py` 11:39:38, `portfolio/federacao.py` 11:21:04,
  `portfolio/simhash.py` 10:53:39, `~/.claude/scripts-portfolio/MANIFEST.json` 10:53:52 e
  `analise/.claude/portfolio/indice.json` 11:19:31 (7.893 entradas). Os dois braços
  recebem esse mesmo hook; a política de injeção decide o que dele chega.
- **Correção do ambiente (29/10 ~13:20, dia 0, antes da 1ª troca e de qualquer análise):**
  a linha acima diz "Touring 30.4.99"; medido nos processos vivos, o ambiente é misto. O
  rito do 30.4.100 (`propagate-release.sh`) instalou o binário global e reiniciou o daemon
  global às 18:35–18:37 de 28/10 e morreu na suspensão da máquina às 18:42:07, antes de
  instalar a toolchain, torná-la default e propagar aos projetos. Resultado, fixo até
  06/11 porque o congelamento proíbe completar o deploy:
  - sessões em `projects/touring` (pin `dev`): cliente e daemon global **30.4.100**
    (`target/release`, commit `704fbf72` + `b6f747c8` só de teste). O `6a89128d` leva o nível
    de injeção do cliente ao daemon por requisição: o **vazamento 1 está fechado** aqui;
  - sessões em `projects/analise` (lock `30.4.99`): cliente e daemon do projeto **30.4.99**:
    o **vazamento 1 segue aberto**;
  - o **vazamento 2 está fechado para todos**: os hooks Python são symlink vivo de
    `client/hooks`, e o journal dos cortes (`96113cd4`) estava no ar antes do dia 0.
  A mistura é a mesma nos dois braços e nos 8 dias, então não confunde A×B; dilui o efeito
  na fatia analise, o que numa não-inferioridade favorece concluir "não pior". Por isso,
  **sensibilidade adicional** (registrada agora, antes de ver dado do braço B): a análise
  primária repetida por projeto (`touring` × `analise`); se as duas fatias divergirem na
  decisão do §6, o relatório dá as duas e não só o pool.
- **Mínimo por célula (bloco × braço):** 10 sessões com pelo menos 40 chamadas de
  ferramenta. Célula abaixo do mínimo invalida o bloco.

## 4. Desfecho

**Primário:** eficácia do canal de ferramentas, medida pelo instrumento versionado
`scripts/measure_tool_trio.py` (o mesmo do ponto 0 da régua):
`eficácia = 100 − erro% − vazio% − negado%`, agregada por célula (soma dos desfechos de
todas as sessões da célula sobre a soma das chamadas).

**Secundários (relatados, não decidem):**
- retry% e negado% por célula;
- recuperação depois do deny por gate (`--extras`, `deny→next`);
- dose: bytes cortados no `~/.claude/touring/injection_suppressed.jsonl` (braço B; subconta,
  §2);
- estratificação por modelo (campo `message.model` do primeiro registro de assistente):
  a aderência é modelo × apresentação (M2), e as sessões Kimi/MiniMax compartilham o
  mesmo `settings.json`.

**Sensibilidade:** a análise primária repetida sem as sessões que atravessam uma troca.

## 5. Mecanismo de troca (exige o sim do Gabriel)

Um script (`a3_arm.py`, no bundle) lê `a3-calendario.json` e, com `apply --yes`, grava o
nível do dia no `env` do `~/.claude/settings.json` (com backup antes; no braço A remove a
chave). Rodaria por cron às 04:00 BRT. É uma mudança diária e durável da configuração do
Gabriel; por isso o experimento **não começa** sem a aprovação explícita dele. Sem cron,
a troca manual às 04:00 também vale, desde que registrada no calendário.

**Emenda de 29/10 (~13:20, dia 0, antes da 1ª troca):** o cron não serve. A máquina dorme
à noite (boots de 27/10 09:14–18:14, 28/10 09:22–18:42, 29/10 a partir de 13:08) e o
cronie não recupera disparo perdido: nenhum cron rodou na madrugada de 29/10 (nem o
`disk-watch` das 03:00), e a troca de 30/10 para o braço B nunca aconteceria. O dia 0 não
foi afetado, porque o braço A é a ausência da chave e ela já estava ausente. O mecanismo
passou a timers do systemd de usuário, `~/.config/systemd/user/a3-{arm,reset}.timer`:
`OnCalendar` 04:01 com `Persistent=true`, que disparam ao religar ou fazer login quando a
hora passou com a máquina dormindo. O `apply` é idempotente, então disparar a mais não
grava nada. O reset de 06/11 virou `a3_arm.py reset`, que **recusa enquanto o calendário
tiver bloco em curso**: acionar a extensão no `a3-calendario.json` basta para mantê-la, sem
ninguém ter de lembrar de apagar uma linha. O one-liner antigo do reset, testado pelo
systemd, falhava com `SyntaxError`. Os dois foram provados pelo systemd contra settings e
calendário falsos (braço B gravado com backup; reset gravado com backup). Risco residual
declarado: uma sessão aberta nos segundos entre religar e o timer rodar recebe o braço da
véspera mas é atribuída ao do dia; o `a3_arm.log` traz a hora de cada disparo, então essas
sessões são identificáveis e o relatório as conta.

## 6. Análise e regra de decisão

1. Para cada bloco válido: `d_b = eficácia(B) − eficácia(A)` em pontos percentuais, com
   as células pooladas.
2. Estimativa: média dos `d_b`. Intervalo de 95% por bootstrap de sessões dentro de cada
   célula (2.000 reamostragens, semente fixa 20261027), reagregando por célula a cada
   reamostra.
3. **Margem de não-inferioridade: −1,0 pp.** É do tamanho da variação entre classes no
   ponto 0. Um efeito menor que isso não é distinguível do ruído entre dias, que só o
   pareamento ABBA cancela (memória `p95-de-rodada-mede-a-maquina`).
4. Depois dos 2 blocos primários:
   - limite inferior do IC > −1,0 → **não-inferior**: `no-advisory` vira o default;
   - limite superior do IC < −1,0 → **inferior**: fica `full`;
   - IC cruzando −1,0 → **extensão** (§3). Depois dela decide a estimativa pontual: ≥ −1,0
     adota `no-advisory`; abaixo, fica `full`.
5. O relatório traz os números das quatro células, o IC, os secundários e a lista de
   blocos invalidados, com o motivo.

## 7. O que este experimento não responde

- Se os avisos ajudam em tarefas específicas: o desfecho é agregado por dia, e o mix de
  tarefas confunde (a validade é média, PLANO S-12).
- O efeito do S-13 (rubrica dos gates): ele entra no 30.4.99 **antes** do dia 0 e fica
  constante nos dois braços.
- Desfecho por tarefa com juiz (testes verdes por tarefa): é o piloto pago (c), que só
  roda com sinal daqui e orçamento aprovado.

## Encerramento (29/10/2026 17:26, dia 0)

O Gabriel encerrou o A3 no próprio dia 0: o congelamento (nenhum deploy, `settings.json` nem
hook vivo que injeta contexto, em todos os projetos, até 06/11) travava várias frentes, e a
correção de uma falha aberta do gate P0 de segredos (F2.4, `4a93eac0`) estava parada sem deploy.
Os timers `a3-{arm,reset}` foram desligados (`systemctl --user disable --now`); o `settings.json`
ficou no nível `full`, a chave ausente, exatamente como estava. `a3-calendario.json` volta a
`day0: null`, com a data original e o motivo em `encerrado`.

**Nenhuma conclusão sobre eficácia sai deste A3.** Só houve braço A, por cerca de quatro horas
com a máquina ligada (13:08–17:26). O dia 0 fica como piloto do mecanismo: ele mostrou que o
cron não disparava com a máquina dormindo (emenda do §5) e que o ambiente estava misto
(correção do ambiente no §3). O default de injeção continua `full`. Refazer o A3 exige uma
janela planejada de 8 dias sem deploy, com o ambiente homogêneo (todos os projetos na mesma
versão) e o mecanismo por timer já pronto (`a3_arm.py`, `a3-{arm,reset}.{service,timer}` em
`~/.config/systemd/user/`, hoje desabilitados).
