---
type: Audit
title: "F2.4 (secrets) falha ABERTO em padrão de destructuring — bypass de gate P0 BLOCK"
description: "Secret-named binding que nao e o ultimo segmento de um padrao de destructuring escapa do tier Fail do F2.4 e passa pelo gate pre-Write. Provado empiricamente no binario real."
date: 2026-10-29
severity: MEDIUM (gate P0 BLOCK / FailClosedLoc)
status: CORRIGIDO em 4a93eac0 (29/10/2026); vale no binário a partir do próximo deploy
timestamp: 2026-10-29T14:26:35-03:00
---

<!-- touring-quality:allow-secrets — este documento cita fixtures fictícias do bypass -->

# F2.4 — bypass por padrão de destructuring (29/10/2026)

## Veredito

**Real, reproduzido, e vivo no caminho de produção.** O gate F2.4 (P0 BLOCK, `FailClosedLoc`)
classifica como **Warn (0,500)** — logo **não bloqueia** — uma linha que atribui um segredo a um
binding cujo nome é secret-natured mas **não é o último segmento** de um padrão de destructuring.
A mesma linha na forma de nome único é **Fail (0,000)** e **bloqueia**.

O detector é `crates/touring-quality/src/verifications/f2_4_secrets.rs`, função
`assigned_name` (L1179-1183), chamada em `line_strong_rule` (L1127-1128).

## Reprodução (binário real, sem tocar no repo)

Fixtures em `/tmp/f24-check/`, scored com `target/debug/touring-quality score <arquivo> --dims F2.4 --format json`:

| fixture | linha | veredito F2.4 |
|---|---|---|
| `single.rs` | `let password = "hunter2";` | **Fail 0.000** — `secret-named-assignment` |
| `tuple.rs` | `let (password, other) = ("hunter2", "x");` | **Warn 0.500** — "keyword present (no assigned value)" |
| `single_rand.rs` | `let password = "Xk3mQ9pL2vR7tZ";` | **Fail 0.000** — `secret-named-assignment` |
| `tuple_rand.rs` | `let (password, other) = ("Xk3mQ9pL2vR7tZ", "x");` | **Warn 0.500** — não bloqueia |
| `single.py` | `password = "hunter2"` | **Fail 0.000** |
| `tuple.py` | `password, other = "hunter2", "x"` | **Warn 0.500** — não bloqueia |

**Prova end-to-end no caminho vivo**: o hook `PreToolUse`/`Write` (que é o próprio F2.4)
**BLOQUEOU** a escrita de `single.rs` ("50-dim P0 regression … F2.4 (0.00) … secret-named-assignment")
e **deixou passar** `tuple.rs`, `tuple_rand.rs`, `tuple.py` — o segredo foi para o disco.

**Atribuição**: o binário **release** (28/10 17:51, anterior ao commit `20d2b05a` de 29/10 14:16)
mostra o **mesmo** veredito para o caso tuple ⇒ **o bypass é anterior**; o commit de hoje não o
introduziu. O autor do commit de hoje já registra o gaps da mesma família na própria assinatura do
teste: *"Known gap, older than this test: `cfg = connect(password="…")` and `creds = {"api_key": "…"}`
split at the FIRST `=`, so the secret name is never the LHS."*

## Causa

`assigned_name(lhs)` devolve o trecho do LHS **depois do último** `,` `(` `{` `[`:

```rust
fn assigned_name(lhs: &str) -> &str {
    lhs.rfind([',', '(', '{', '[']).map_or(lhs, |i| &lhs[i + 1..])
}
```

Em `let (password, other) = …` o último delimitador é a vírgula ⇒ o nome devolvido é
` other) ` ⇒ `names_secret` é falso ⇒ o tier Fail não dispara. Resta o tier fraco
("keyword present"), que é **Warn** e **não bloqueia**.

## Alcance

- Linguagens: Rust e Python (o detector é multilíngue; o mesmo shapings).
- Segredos: fracos (`hunter2`) **e aleatórios** (`Xk3mQ9pL2vR7tZ`) — o escape não depende de
  entropia, e sim do **nome do binding**.
- Formas que escapam por este mecanismo: tuple/destructuring (`let (password, other) = …`),
  binding não-último em qualquer padrão composto.
- Formas que escapam pelo mecanismo **vizinho** já documentado no commit de hoje:
  `cfg = connect(password="…")` e `creds = {"api_key": "…"}` (o split no **primeiro** `=` faz o
  nome do segredo cair no RHS).
- Impacto no gate: F2.4 é P0 BLOCK e `FailClosedLoc` — um arquivo que reprova reprova o escopo
  inteiro. Com o bypass, um segredo real **entra no disco** sem bloquear.

## Correção proposta (NÃO aplicada — ver "Por que não corrigi aqui")

Trocar `assigned_name` por um extrator que devolve **todos** os nomes realmente **ligados** pelo
padrão do LHS, e exigir `names_secret` em qualquer um deles.

**Restrição que a correção precisa respeitar**: `20d2b05a` existe porque a busca pelo LHS inteiro
gerava falso-positivo que **bloqueava edições legítimas**. O caso que motivou a correção:

```
dict(_use(2, "Bash", {"command": "pwd"}), timestamp="2026-11-01T23:59:59Z")
```

O LHS inteiro continha `pwd` (que é secret-keyword) **dentro de um literal de string**; o LHS
inteiro foi substituído pelo último segmento. Uma correção que volte a varrer todos os
identificadores **sem excluir o conteúdo de literais** reintroduz esse bloqueio.

Portanto, o extrator precisa:
1. reconhecer apenas **identificadores ligados** (não valores, não literais, não chaves de string);
2. em `let (password, other) = …` devolver `password` e `other`;
3. em `dict(_use(2, "Bash", {"command": "pwd"}), timestamp=…)` **não** devolver `pwd` nem `command`.

**Validação obrigatória**: o mesmo censo de corpus que o autor aplicou ("a census over 592 files
(skills + docs + hook crates) found 4 live blockers") — porque F2.4 é `FailClosedLoc`, um
falso-positivo novo bloqueia `Write` em todo arquivo do corpus.

## Por que não corrigi aqui

1. **Mandato de leitura.** A sessão que coordena este trabalho pediu auditoria **somente leitura**,
   sem editar código, sem commit. Este arquivo é o achado; a correção é trabalho de dono.
2. **Arquivo sob refino ativo de outra sessão.** `f2_4_secrets.rs` foi alterado hoje
   (`20d2b05a`, 29/10 14:16, sessão Claude Opus 5.5 — `claude.ai/code/session_01UACRt9HeKKu33uQd5m4yxW`),
   e o autor documenta na própria assinatura do teste o *gap* residual desta mesma família.
   REGRA #19 (exceção de sessão concorrente): coordenar em vez de colidir.
3. **Risco assimétrico.** Uma correção apressada num gate P0 BLOCK pode bloquear `Write` em
   centenas de arquivos legítimos; o bypass atual exige um shape de código incomum. Medir o
   FP depois do conserto é barato; desfazer um bloqueio em massa no corpus é caro.

## Estado

**ABERTO** — roteado ao dono de `f2_4_secrets.rs` e à sessão coordinadora
(`touring-cb`, pedido de "verifique tudo que ainda precisa ser feito", 29/10).
Nada foi editado, commitado ou implantado na verificação deste achado; as fixtures vivem em
`/tmp/f24-check/` e são descartáveis.

## Correção (29/10/2026, touring-cb, `4a93eac0`)

O alvo da atribuição passa a ser secreto pelo nome logo antes do delimitador (inclusive chave
entre aspas) **ou** por qualquer identificador do lado esquerdo com as strings em branco (a
projeção de código que o detector já calcula). Atrás de um lado esquerdo de desestruturação, um
lado direito em tupla ou lista é lido pelos seus literais. A correção revelou um segundo defeito,
este introduzido pelo `20d2b05a`: a forma Python `password, other = "…", "x"` deixara de
bloquear, porque aquele commit olhava só o último nome.

Medido com o detector novo (`target/debug/touring-quality`): `tuple.rs`, `tuple_rand.rs` e
`tuple.py` → Fail 0,000; `single*` seguem Fail; `strong_tuple.rs` segue Warn 0,500, corretamente,
porque `AKIAIOSFODNN7EXAMPLE` é a chave de exemplo da documentação da AWS, mascarada de propósito.
Censo do workspace: 0,985, 0 bloqueios em 4.101 arquivos, igual ao de antes da correção. Teste:
`test_keyword_as_an_earlier_value_is_not_the_assigned_name` (quatro formas de desestruturação e
dois controles negativos). O hook vivo de pre-Write só muda no próximo deploy, depois do
congelamento do A3 (06/11 04:00).
