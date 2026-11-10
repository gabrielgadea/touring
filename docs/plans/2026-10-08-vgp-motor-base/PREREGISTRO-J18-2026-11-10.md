---
type: PreRegistration
title: "J18 — o binário em produção (30.4.108) contra o último sem a correção da camada 2 (30.4.104), em held-out novo"
description: "Pré-registro escrito antes de construir o corpus e antes de qualquer medição. Muda o instrumento quente e a ordem das corridas em relação ao J17; corte, juiz e critério de aceite são os do J15."
plan_id: 2026-10-08-vgp-motor-base
okf_version: 1
timestamp: 2026-11-10
---

# PRÉ-REGISTRO — J18 (10/11/2026)

> Decisão do Gabriel (10/11/2026, canvas item 4a, opção A): «Julgar o que já roda vem antes de julgar o que ainda
> não existe, e julgar separado mantém as causas legíveis.» O J18 julga **só** o binário instalado. As mudanças
> que alteram veredito e ficaram pendentes (`bfce/DECISOES.md`: D1 `.cache` em Execução na L2, D2 raiz sob /tmp,
> D3 `<…>`/TMPDIR, D4 TS com contexto real, `AmbientePython` na L1) **não entram**: vão para um J19.

## 1. Pergunta

O 30.4.108 — que carrega `90d491ec` (camada 2: módulo do projeto visto dos ancestrais, PEP 420) — é PRONTO pelo
critério do J15 num held-out que nenhuma correção viu?

## 2. Braços (verificado em 10/11/2026)

| braço | binário | por quê |
|---|---|---|
| candidato | toolchain `~/.touring/toolchains/30.4.108/bin/touring` (`touring 30.4.108`), congelado em `~/.claude/touring/vgp/binarios/30.4.108/` byte-idêntico | é o que roda em produção |
| controle | `~/.claude/touring/vgp/binarios/30.4.104/touring` (`touring 30.4.104`) | último release sem `90d491ec` (o commit é ancestral do release 30.4.105, verificado com `git merge-base --is-ancestor`) |

O daemon vivo tem de ser o `touring-daemon` da toolchain 30.4.108 (checagem de identidade do J17, sem mudança).

## 3. Held-out novo, nunca o 20261101c

O caso `b81d9fa3c3e5` do 20261101c originou a correção `90d491ec`. Julgar a correção no corpus de onde ela saiu
mede o próprio ajuste, não a generalização: o 20261101c fica fora como evidência e entra só na lista de exclusão.

- Nome: `20261110`; semente `20261110`.
- Construtor: `diagnostics/holdout-20261031/rodar_construtor_holdout.sh` com o reconstrutor de `8d166db3`
  (dependências locais congeladas pelo Git no `ts`), igual ao 20261101c.
- `--excluir-corpus` de **todos** os anteriores: `20261031`, `20261031b`, `20261101`, `20261101c`.
- `congelados.json` (os instrumentos fixados por sha) é gravado **antes** da construção; selo e cruzamento de
  disjunção com controle positivo antes da primeira medição, como no 20261101c.
- Se o universo não fechar o denominador do J15 (600 arquivos + inline), o J18 é **INCONCLUSIVO por corpus**,
  nunca julgado num corpus menor.

## 4. O que muda no instrumento (por isso este pré-registro é novo)

1. **Ordem ABBA na fase V3.** No J17 os timeouts da camada 2 foram 80 (controle) contra 170 (candidato) com
   código idêntico: era a carga da máquina caindo num bloco. A V3 roda candidato, controle, controle, candidato
   (quatro corridas, mesmo híbrido). O veredito da V3 de cada braço é o da **primeira** corrida dele; a segunda
   serve só para separar máquina de binário: um efeito de timeout só é atribuído ao binário se aparecer nos dois
   pares. A v2 (camada 1) roda uma vez por braço, intercalada (candidato, controle).
2. **Aquecimento do quente.** Antes da primeira entrada, uma chamada de aquecimento ao `touring-hook` é feita e
   **descartada** até o trace mostrar rota daemon (no máximo 5 tentativas; sem rota daemon = INCONCLUSIVO). Uma
   chamada medida que caia no standalone é reparada pela regra 5 do `PREREGISTRO-V3-QUENTE-2026-10-31.md`
   (repetição da entrada, original preservado). O critério continua exigindo 2× o denominador pela rota daemon.

Nada mais muda: juiz `scripts/juiz_motor_vgp.py`, instrumento `rodar_juiz_instrumentado.py`, adjudicação, p95 por
posto mais próximo, limite de 1.000 ms e a regra de aceite `consolidar_j15.avaliar` (fonte única, sem cópia).

## 5. Regra de decisão (escrita antes da medição)

- **Veredito** do candidato: `consolidar_j15.avaliar` sobre as primeiras corridas e o quente → PRONTO / NÃO PRONTO
  / INCONCLUSIVO (entrada ausente ou ilegível = INCONCLUSIVO, nunca zero).
- **Regressão**: todo falso deny do candidato que o controle não tem é listado com id e mensagem; um só basta para
  NÃO PRONTO, qualquer que seja o resto.
- **Efeito da correção**: falsos deny que o controle tem e o candidato não, listados com id. Reportados como
  evidência, não como critério.
- Timeouts da camada 2: reportados por corrida; não decidem o aceite (como no J17).

## 6. O que falsificaria a leitura «a correção generaliza»

- O candidato ter falso deny de camada 2 com a mesma assinatura do `b81d9fa3c3e5` («unknown import symbol» de
  módulo que existe no projeto) em entrada nova.
- O candidato perder recall frente ao controle em controle positivo (`json.loadz`/`os.pathx` deixarem de negar).

## 7. Proibições

Nenhum `cargo build --release`, `update-touring` ou troca de daemon durante o run. Nenhum conteúdo de corpus no
repositório. Nenhuma alteração de corte, semente, juiz ou critério depois da primeira medição; um defeito de
instrumento achado no meio do run encerra o J18 como INCONCLUSIVO e abre um J18b com pré-registro próprio.

## Adendo 1 — 10/11/2026, ANTES de qualquer medição

A construção do held-out `20261110` abortou (rc 1) em `scripts/replay.py:48`
(`JSONDecodeError: Expecting ',' delimiter`): 3 linhas `tool_use`/`tool_result` de transcript estão
corrompidas — gravação interrompida, a linha cortada no meio de uma string com o registro seguinte colado
nela (`analise/5ff8eadd…jsonl:22504`, `analise/ad6c086f…jsonl:41191`, `touring/0fbe5042…jsonl:11847`).
Nenhum corpus foi produzido e nada foi medido, então o §7 (que veda mudança DEPOIS da primeira medição) não
se aplica, e o J18 segue como J18 — não J18b.

Única mudança: `replay._records` pula a linha malformada, registra `<arquivo>:<linha>` em
`replay.MALFORMED` e avisa em stderr, em vez de abortar (teste `scripts/test_replay_records.py`). Uma
entrada que dependa de uma linha perdida simplesmente não vira candidata, como toda entrada não
reconstruível (§3). Corte, semente, braços, juiz, instrumento de medição e critério **não mudam**.
O `congelados.json` é regravado com o sha novo do `replay.py`; o anterior fica preservado ao lado
(`congelados.json.antes-adendo-1`), e o log da construção abortada também.
