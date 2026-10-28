---
fase: S-12 (A3) — desfecho por quase-experimento (d) [T-A3]
pre_registrado_em: 2026-10-27 (BRT) — a hora vale pelo commit deste arquivo
status: pré-registrado; ativação depende do deploy 30.4.99 e do sim do Gabriel ao mecanismo de troca (§5)
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
  emite) é da mesma classe e também invalida o bloco, porque muda o próprio tratamento.
  O que estiver no ar antes do dia 0 é ambiente fixo; mudança em modo sombra que não
  altera o que chega ao modelo não conta. Os blocos invalidados ficam listados em
  `a3-calendario.json` (`blocos_invalidados`) com o motivo.
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
