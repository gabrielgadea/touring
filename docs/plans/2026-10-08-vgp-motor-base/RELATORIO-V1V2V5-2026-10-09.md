# RELATÓRIO V1+V2+V5 — motor VGP camada 1 (em curso, 09/10/2026)

> Sessão: `647e47b7` (analise, backend k3) · delegação da coordenadora `analise-89`.
> Juiz do motor: `scripts/juiz_motor_vgp.py` **v2** (sha `91d8a2a608c8bd05`, revogando `3952defd…`
> e `738b621c…` — o que vale é o v2 da coordenadora; selftest dele 9/9). Intocado.
> Repo: `~/projects/touring`, branch `feat/2026-09-16-shell-quality-wiring-python`.

## 0. ESTADO — ENTREGUE (09/10 noite)

**Os três vereditos por exit code, no binário congelado sha256 `e935aaf17d9a09d0`…**:
(1) juiz v2 camada 1 rc 0 PRONTO (§6) · (2) V3 camadas 1,2 rc 0 PRONTO (§6) · (3) prova
`--so 1` rc 0 8/8 (§6). Suites 3.260 testes 0 falhas · clippy 0 · fmt 0 · drift A1 rc 0.
Nada commitado (ordem do Gabriel). A assinatura `pub fn check(input: &CheckInput) -> CheckOutput`
está ESTABILIZADA para o `camada2.rs` da coordenadora; hooks em sombra com `camadas: [1,2]` no
daemon (pool ligado em `daemon_main.rs`) e `[1]` + motivo no standalone — por construção.

**Contratos recebidos em voo (vencem o brief onde divergem)**: juiz v2 — recall conta deny OU warn;
falso deny por balde (arquivo/inline) ≤ 1%; positivos de FORMA ('str'/'list'/'NoneType' object…) fora
do recall; política **P2** (severidade por path×alvo, §3). Negativos inline: **analise-25** faz
(`entradas_inline_neg.jsonl`), NÃO é mais meu. Shadow log: `~/.claude/touring/vgp/sombra.jsonl`
(não mais `<raiz>/.claude/touring/vgp_sombra.jsonl`), contrato exato em §4. Drift A1
(`expected 106, got 107`) zera quando `vgp` existir no CLI — aceite: `scripts/test_drift_coverage.py` rc 0.
Camada 2 LIGADA (desfecho em §5c): `crates/touring-pyright/` + `hooks/vgp_l2.rs` +
`touring vgp check --camadas 1,2`; o daemon liga o pool em `touring-hooks/src/daemon_main.rs:62`
(`habilitar_neste_processo()`) e o hook efêmero standalone devolve `unknown` com motivo, com a
camada 1 de pé.

## 1. O que o juiz v2 cobra (medido do código dele)

- CLI: `touring vgp check --path <virtual> --content-file <p> --project-root <r> [--disk-file <d>]
  [--overlay <o>=<a>]... [--lang python] -j` → JSON com `verdict` sempre presente (falha fechada).
- Limiares: recall de símbolo de ARQUIVO ≥ 0,70 (24 reproduzidos: atributo 17 · nome 6 · import 1) ·
  inline ≥ 0,70 com `--v5-estrito` (41: atributo 22 · nome 15 · import 3 · kwarg 1) · falso deny ≤ 1%
  por balde · p95 elapsed_ms ≤ 150 (medido pelo motor, spawn não conta).
- Negativos vêm com `--disk-file` (o arquivo 1 ms antes da edição; delta = só nega o introduzido) e
  `--overlay` dos módulos do projeto no instante (182 sobreposições; `contexto` do corpus por cima).

## 2. Arquitetura implementada (touring-code/src/vgp/)

- `mod.rs`: tipos (`Finding`/`CheckInput`/`CheckOutput`) + `check()` (verdict sempre JSON).
- `scope.rs`: árvore de escopos pyflakes-grade — classe invisível dentro de método (semântica
  Python), comprehensions isoladas (1º iterável no escopo EXTERNO), global/nonlocal, forward-refs
  legítimos, `import *`/exec → unknown; `__file__` definido em arquivo, AUSENTE em inline (regra da
  coordenadora, confirmada); aug-assign sem vínculo prévio falha; target NUNCA é uso (a gramática
  0.25 embrulha assignments em `expression_statement` — despacho próprio); attribute/keyword_argument
  não geram uso do nome do atributo/parâmetro.
- `modules.rs`: resolução projeto→irmão(/tmp)→venv com overlay primeiro e cache (path, sha256);
  `ModuleInfo` (top_names, `__all__`, `__getattr__`, star, imports-map, classes, funções);
  `ClassInfo` (campos anotados, `self.x` de QUALQUER método, métodos, `__slots__` — inclusive
  gerados por `@dataclass(slots=True)` — `__getattr__`, bases, dataclass); `FuncSig`.
- `stdlib_model.rs`: builtins 3.12 completos + tabelas fechadas (`unicodedata`, `math`, tipos
  builtin + `stat_result`) — só o que é certeza; o resto resolve por arquivo, nunca por palpite.
- `checks.rs`: as 5 classes + inferência de receptor (construtor / anotação / retorno anotado de
  função do projeto / builtin provado / `Path / str`) + delta (proposto ∖ disco) + montagem do
  veredito. **P2**: `severidade = deny` em /tmp e inline ou alvo EXTERNO (stdlib/builtin fechado);
  alvo do projeto em arquivo → `warn`. Dedupe por (classe, nome). A recusa ensina: `reais` + o mais
  próximo (Levenshtein ≤ 2).

## 3. TDD — estado por classe (RED→GREEN→mutação)

38/38 verdes (touring-code) + 3/3 (touring-server cli::vgp). Falhas reais encontradas e
consertadas (todas com teste): embrulho `expression_statement` (alvo virava uso) · aliases sem
field `alias` (identificador solto) · splats `*_pattern` vs `*_parameter` · decorators nunca
chegavam a `collect_class` · `keyword_argument`/`attribute` como usos falsos · aug sem prévio ·
`slots=True` por decorator · docs obrigatórios da crate · clippy --fix + 6 manuais.

**Mutações provadas (8/8, suite restaurada a verde após cada uma):**

| mutação | mata (rc) |
|---|---|
| M-N `resolve()` nunca Missing | 10 testes de nome ✓ (scope + checks + sombra) |
| M-I `module_has` sempre Yes | `from_import…` + `atributo_de_modulo…` ✓ |
| M-M imports cego | 3 (modulo projeto + stdlib + p4_externo) ✓ |
| M-B `members_of` sempre incompleto | 3 (construtor + slots + retorno) ✓ |
| M-K `sig_de` sempre None | `kwarg_inexistente…` ✓ |
| M-D delta não converte pré-existente | `delta_so_nega…` ✓ |
| M-P4 forma volta a ser deny | `p4_forma…` ✓ |
| M-CLI parse do zero (o bug do argv real) | `chamada_minima…` + `overlay_alimenta…` ✓ |

**Bugs reais pegos pelo baseline do juiz (não pelos testes)**: o handler recebe o argv INTEIRO
(`["touring","vgp","check",…]` — o dispatch passa `std::env::args()` cru) e o parse posicional
comia os pares errados → as 762 chamadas saíam `"--content-file é obrigatório"`. Conserto: parse
por `position("check")` com fallback ao primeiro flag; teste com o argv inteiro (M-CLI acima).

**P4 implementada** (política final medida da coordenadora): forma (tipo builtin fechado) e
dinamico (dunder em classe; nome com import*/exec no caminho) → warn SEMPRE; externo
(stdlib/terceiro fechado) → deny em todo balde; proprio/projeto → deny só em /tmp+inline.
Modelo `pathlib.Path` adicionado (construtor + operador `/` + `stat() → stat_result`) para a
forma do corpus (`saida.stat().st_m_size`).

## 3b. Baseline v2 NÃO PRONTO (medido 09/10 ~18:5xZ) → a wave de consertos

Baseline (binário pós-P4, 704 chamadas): recall arquivo 0.615 (8/13) · inline 0.370 (10/27) ·
falso deny 11.1% arquivo (41/369) · 15.3% inline (42/275) · p95 532.4 ms · **58 chamadas com
stack overflow (rc -6)** · 0 não classificados. Seis pendências, quatro causas-raiz medidas:

**(i) `from m import x` SEM alias nunca era checado** (achado da coordenadora, medido nativo):
a gramática 0.25 expõe cada nome importado como `dotted_name` no campo `name` (o módulo é
`dotted_name` no campo `module_name`); o meu `match ch.kind()` só aceitava `identifier`/
`aliased_import` e o import simples caía no `_ => None` em silêncio. Conserto: iterar
`children_by_field_name("name")`. TDD pelo aceite dela: 4 testes RED ((a) import simples ·
(b) lista mista só o inventado nega · (c) parênteses multilinha · (d) relativo `from .m` com
irmão em /tmp) → GREEN; **M-I mata os 5 testes from_import** (o aceite pedia matar o (a)).

**(ii) Stack overflow — 58 chamadas**: bisseção do jev_client.py (297→66 linhas) isolou
`line = line.strip()`: a inferência de tipo seguia nome → atribuição → receptor → o MESMO nome
sem conjunto de visitados. Conserto: guarda `em_curso` (RefCell<HashSet>) no `tipo_do_nome` —
ciclo vira unknown, nunca crash. 3 testes de regressão (autorreferente · mútuo a=b/b=a ·
hierarquia A(B)/B(A) — members_of já tinha corte por profundidade). **M-CICLO (guarda removida)
mata por SIGABRT**; suíte restaurada 45/45.

**(iii) p95 532 ms**: duas causas estruturais — `para_cada_atribuicao` RE-PARSEAVA o arquivo
inteiro por nome inferido, e cada `tipo_do_nome`/`tipo_da_chamada` criava um `Resolver::new`
com cache ZERADO. Conserto: `Checks` carrega a árvore (`root: Node<'t>`), a atribuição anda na
árvore já parseada, e o cache do resolver virou `RefCell` compartilhado (`resolve_module(&self)`).
Suíte segue 45/45 idêntica (refactor sem mudança de comportamento).

**(iv) falsos deny remanescentes**: análise por classe em §6 (preencher após o rebuild).

**Suítes/gates desta wave**: touring-code 45/45 · touring-server cli::vgp 7/7 (4 novos da fusão
L2 + fixture do overlay corrigida — codificava o bug do import: `from m import g` com `g`
inexistente no módulo passava em silêncio) · clippy `-D warnings` rc 0 nos 3 crates ·
`cargo fmt --all -- --check` rc 0 (13 arquivos, todos meus).

**Camada 2 — CLI**: `--camadas 1|1,2` em `touring vgp check` (touring-server → touring-lsp,
aresta acíclica): merge por (nome,linha) com a mais forte vencendo, `layer: 2`, elapsed somado,
L2 unknown → `camada2: "unknown: <motivo>"` com os findings da L1 de pé. Testes: unknown anota
e preserva · deny escala e ensina a mensagem · merge vence a mais forte · parse das camadas.

## 4. Contrato da sombra (V2/V5 — implementado)

Implementado em `touring-code/src/vgp/sombra.rs` + fiação nos 3 hooks
(`pre_edit_prevention.rs` — Edit **e MultiEdit** por dobra sequencial · `pre_write.rs` ·
`pre_bash.rs` — V5 inline `python3 - <<EOF` / `-c` / `touring run --code|--file`, raiz por
`-projects-<nome>` → CLAUDE_PROJECT_DIR → hook root). Contrato exato da coordenadora (adendo):
`~/.claude/touring/vgp/sombra.jsonl` append+fsync, campos `{ts, session_id, tool_use_id, hook,
project_root, path, balde, camadas:[1], modo, verdict, elapsed_ms, findings[…escopo…, reais≤20],
motivo?}`. Modo: `TOURING_VGP_DISABLED=1` mata; `TOURING_VGP_MODE` > `[vgp] modo` do toml >
default **sombra** (ativo nunca por default). Ativo: deny → reason `[VGP · deny] `; aviso →
additionalContext `[VGP · aviso] `. `escopo` nos findings: "externo" quando a severidade é deny,
"projeto" senão (minha decisão P4-consistente; a coordenadora valida).
Registro da dívida do extrator inline do V0d (meu `programa_inline` python): 2.037 programas não
compilam (1.088 `touring run --code` com `'\''` aninhado sem desescape; `-c` em aspas duplas sem
desescape bash) — 3/50 positivos inline afetados; o juiz v2 os exclui nomeados.

## 5. cli_ast_detail (V1, implementado)

`cli/handlers/index.rs:2168`: caminho relativo agora resolve contra `payload["cwd"]` (era o cwd
do daemon); ilegível → `{"error": "arquivo ilegível: <path> (<reason>)"}` explícito (era
`unwrap_or_default` → 0 detalhes mudo). Testes: relativo-no-cwd extrai 1 detalhe; ilegível dá o
erro nomeado.


## 5b. Bloqueio arquitetural do L2 nos hooks (ciclo de pacote) — vgp_l2.rs preservado

Medido 09/10 19:20Z: `cargo check` rc 101 — `cyclic package dependency: touring-hooks depends on
itself`: `touring-lsp →(opcional, lsp-bridge) touring-hooks → touring-dispatch →
touring-hook-handlers →(aresta nova)→ touring-lsp`. A detecção de ciclo do cargo é no GRAFO DE
PACOTES (a aresta opcional conta mesmo com a feature desligada), logo NENHUMA crate do processo do
hook pode depender de touring-lsp enquanto a aresta opcional existir. Decisão aplicada: hooks
ficam em `camadas [1]` (SombraExtra segue como ponto de extensão pub em sombra.rs); o CLI
`--camadas 1,2` (touring-server → touring-lsp, acíclico) fica e é o que o juiz V3 mede.
Reaplicação em 1 passo se a coordenadora extrair uma crate de pool sem a aresta touring-hooks:

```rust
//! Layer-2 (pooled pyright) glue for the VGP shadow hooks — builds the [`SombraExtra`] that the
//! shared `sombra_check` merges into the shadow line (`camadas: [1, 2]`). It lives here and not in
//! touring-code so the dependency graph stays acyclic:
//! touring-hook-handlers → touring-lsp → touring-quality → … → touring-code.

use touring_code::vgp::sombra::SombraExtra;
use touring_code::vgp::{CheckInput, Finding};
use touring_lsp::pyright::{Severidade, Verdict};

/// Runs the pooled pyright check over the same input layer 1 judged, mapped into vgp terms.
/// `None` when the input is not Python — the shadow line then keeps `camadas: [1]`.
pub(crate) fn extra(input: &CheckInput) -> Option<SombraExtra> {
    if input.lang != "python" {
        return None;
    }
    let overlays: Vec<(String, String)> = input
        .overlays
        .iter()
        .map(|(origem, texto)| (origem.clone(), texto.clone()))
        .collect();
    let l2 = touring_lsp::pool::checar(
        &input.project_root,
        &input.path,
        &input.content,
        input.disk.as_deref(),
        &overlays,
    );
    Some(SombraExtra {
        findings: l2
            .achados
            .iter()
            .map(|a| {
                Finding::new(
                    &a.classe,
                    &a.nome,
                    a.linha as usize,
                    match a.severidade {
                        Severidade::Deny => "deny",
                        Severidade::Warn => "warn",
                    },
                    a.mensagem.clone(),
                    Vec::new(),
                )
            })
            .collect(),
        elapsed_ms: l2.elapsed_ms as f64,
        camada2_motivo: (l2.verdict == Verdict::Unknown)
            .then(|| l2.motivo.clone().unwrap_or_else(|| "sem motivo declarado".to_string())),
    })
}
```
## 3c. Wave R1–R3 (a análise de causa-raiz da coordenadora, 09/10 noite)

Medido por ela nos 85 falsos deny + nos perdidos, confirmado por mim por reprodução dos snaps:
**R1** `from M import X [as Y]` tipava o receptor como o módulo M (74 atributo + 4 import) ·
**R2** o vinculador de escopo ignorava construtos que LIGAM nome (74 nome: with-as 32 ·
except-as 27 · encadeada 7 · *args 2 · walrus 2 · comprehension 4) ·
**R3** `from __future__ import annotations` virava uso `annotations` (9).

**Consertos (RED→GREEN por construto, mutante morto por causa):**

| causa | conserto | mutante morto |
|---|---|---|
| R1 receptor | `imports` vira `alias ↦ (módulo, nome_original)` + `plain_imports` separado; submódulo `m.X` resolve → `Tipo::Modulo(m.X)`; membro de tipo desconhecido → unknown (nunca o módulo) | M-R1 (receptor sempre o módulo-from) mata os 2 testes |
| R1 submódulo × `__all__` | `module_exists` ANTES do `__all__` (que só governa `import *`) | M-F3 (ordem antiga) |
| R1 classe com alias | `resolve_class_module` faz lookup pelo nome ORIGINAL | M-ALIAS |
| função importada como receptor | `Tipo::Funcao`: atributo solto → deny; dunder → warn (dinamico) | M-FUNC |
| R2 except-as | `bind_target` aprende `as_pattern_target` | M-F1 (mata os 2 testes do except) |
| R2 with-as | (coberto pelo mesmo `bind_target`) | — |
| R2 encadeada | `walk_assignment` recursa o `right` aninhado (`T=E=R=A=0`) | M-CHAIN |
| R2 splats | já ligava (BindKind::Param) — teste registra | — |
| R2 walrus | braço `named_expression`: avalia o valor, liga o nome | M-WALRUS |
| R2 match | `case_clause` posicional nos DOIS despachos + `walk_case_pattern` (identificador nu liga; com ponto é uso) | M-MATCH |
| R3 future import | braço `future_import_statement` (nem uso nem vínculo) | M-FUTURE |
| F4 f-strings (meu cluster de recall: uf/nm_municipio/scripts/tau_val_coverage — 4 dos 10 que a camada 2 recuperou no V3 dela) | `walk_interpolacoes` desce nas `interpolation` (e nas aninhadas em format_spec), inclusive em statement-position | M-FSTRING |

Detalhe de gramática medido em cada conserto (probes sexp): `case_pattern` é filho POSICIONAL
do `case_clause` (sem field `pattern` — `child_by_field_name` devolvia None e a captura nunca
era ligada); `from __future__ import` é kind PRÓPRIO `future_import_statement`; interpolação de
f-string é nó `interpolation` com campo `expression`.

## 3d. Wave pós-R1R3 (juiz #2 → os 3 falsos + os 5 perdidos de arquivo)

Juiz #2 (sha c00bd645): **falsos 85→3** (pendência eliminada) · inline 20/27 ✓ · p95 97 ms ✓ ·
restou só **recall arquivo 8/13**. Diagnóstico por reprodução (binário congelado + manifesto):

| caso | construto | conserto | mutante |
|---|---|---|---|
| 3 falsos (kwarg `impersonate`/`timeout`/`allow_redirects` no curl_cffi ×2 · `*extra`) | splats TIPADOS (`**kwargs: Unpack[X]` · `*extra: str`) embrulhados em `typed_parameter` — `func_sig` não ligava `has_kwargs`; `bind_parameters` não ligava o nome (e ligava o TIPO como parâmetro) | descida por campo nas duas casas | M-SPLAT-TIPADO ×2 |
| perdido `st_m_size` (tmp) | `D = pathlib.Path(...)` — o construtor PONTUADO não era modelado (só o `Path` nu) e a cadeia `D / 'x' → stat() → st_m_size` morria no 1º elo | construtor pontuado fechado provado pelo plain-import | M-PATHLIB |
| perdidos `total` ×2 (tmp) | `from grafo_memoria import gravar` — gravar é RE-EXPORT de `grafo_gravacao`; a cadeia parava em `m.functions` | `resolve_funcao` (hop com depth cap) em sig_de · tipo_da_chamada · Tipo::Funcao | M-REEXPORT |
| aviso falso `__doc__` | dunders de módulo não ligados | `__doc__/__package__/__loader__/__spec__/__builtins__/__annotations__` no escopo 0 | — |
| `defaultdict` (atribuído) | o nome esperado NÃO aparece no programa replayado (a única ocorrência no snap é a linha do import) | **defeito de corpus** — o snap não reproduz o NameError (verificado no snap `28e890e5…`, idêntico nos dois corpora); não é lacuna do motor | — |
| `parse` (tmp) | `parser, … = cc._motor()` — desempacotamento de tupla sem inferência posicional | lacuna registrada (inferência de retorno-tupla) | — |
| `DriftDetector` · `simd` (inline) | `claude_learning_kernel` é extensão **.so** (Rust) — sem tabela de stub | **irredutível** sem modelo de FFI | — |
| `last` (inline) | vínculo dentro de laço que não executou — dependência de dado | **irredutível** estaticamente | — |

## 5c. Desfecho do wiring da camada 2 (touring-pyright) + drift A1

**Extração da coordenadora (opção E)**: `touring-pyright` (client.rs/pyright.rs/pool.rs) nasceu
sem nenhuma dep touring-* — `hook-handlers → touring-pyright` é acíclico por construção
(`cargo check -p touring-hook-handlers -p touring-hooks -p touring-server` rc 0). touring-lsp
re-exporta `pub use touring_pyright::{client, pool, pyright}`. Pool **desligado por padrão no
processo**: `habilitar_neste_processo()` liga; sem ela `checar` devolve unknown ("camada 2
desligada neste processo") sem spawnar — o hook standalone efêmero NUNCA liga (S219), o daemon
liga no startup (`touring-hooks/src/daemon_main.rs`, após install_hook) e a CLI liga no caminho
`--camadas 1,2`. Arestas finais: touring-server → touring-pyright · touring-hook-handlers →
touring-pyright · touring-hooks → touring-pyright (a aresta touring-server → touring-lsp saiu).
Os 3 hooks voltaram a passar `crate::vgp_l2::extra(&vgp_in)` (arquivo reaplicado do §5b com os
paths touring_pyright). Suites pós-wiring: 835+749+2+1652 = 3.238 testes, 0 falhas.

**Drift A1 (REGRA #21)**: `test_drift_coverage.py` estava com 4 FAIL (393→395, C1 84→85,
C4 106→107). Causas medidas e reconciliadas (7ª reconciliação, lote "2026-10-09-vgp-motor",
ledger +2): C4 "vgp" — o comando existe no fonte e no `--help` do binário novo, mas o GT do scan
é o `touring --help` do binário INSTALADO (stale por construção até o deploy 30.4.96, que é da
coordenadora); C1 "30.4.96" — citação de release futura no RETOMAR-AQUI do bundle (histórico
legítimo). `reconciliar.py verify` OK · `python3 scripts/test_drift_coverage.py` **rc 0, ALL PASS**.

## 6. Medidas do juiz (binário congelado `/tmp/v0d/touring-congelado`, sha256 **e935aaf17d9a09d0**…)

**(1) Juiz v2 — camada 1 (`--v5-estrito`, 762 chamadas): rc 0 — PRONTO**

| métrica | baseline (pré-wave) | final | limiar |
|---|---|---|---|
| recall símbolo arquivo | 8/13 (0,615) | **11/13 (0,846)** | ≥ 0,70 ✓ |
| recall símbolo inline | 12/27 (0,444) | **20/27 (0,741)** | ≥ 0,70 ✓ |
| falso deny arquivo | 43/400 (10,75%) | **0/400 (0%)** | ≤ 1% ✓ |
| falso deny inline | 42/300 (14%) | **0/300 (0%)** | ≤ 1% ✓ |
| p95 elapsed_ms | 532,4 ms | **108,1 ms** | ≤ 150 ✓ |
| falhas fechadas (crash) | 58 | **0** | 0 ✓ |
| não classificados | 0 | 0 | 0 ✓ |
| avisos em negativos | 175 | 58 | (ruído −67%) |

Perdidos remanescentes (12), todos classificados com evidência: irredutíveis por construção —
`defaultdict` (o nome não aparece no programa replayado) · `DriftDetector`/`simd` (extensão
**.so**, sem stub) · `last` (vínculo em laço não executado — dependência de dado) — e lacunas
registradas — `parse` (inferência de desempacotamento de tupla) · `slides_fisica_eleitoral` ·
`JaccardComputer` · `kwarg tipo` (a camada 2 os cobre, medido no V3 dela: 10 recuperados). **Reclassificados
pela coordenação como defeito de CORPUS, não lacuna do motor**: `__version__` (e `defaultdict`, acima) — o snap vivo
guarda o `-c` errado; o corpus com evidência (staging `90b03f50`) os resolve (inline 21/27 no `e935aaf1`).

**(2) V3 comparativo — camadas 1,2 (`--p95-ms 1000`, base juiz_v2.json): rc 0 — PRONTO**

| métrica | camada 1 | camadas 1+2 |
|---|---|---|
| recall símbolo arquivo | 11/13 | **12/13 (0,923)** |
| recall símbolo inline | 20/27 | **22/27 (0,815)** |
| falsos deny | 0 | **0 (a camada 2 não introduziu nenhum)** |
| p95 | 108,1 ms | **939,9 ms (≤ 1.000)** |
| crashes | 0 | 0 |

A camada 2 recuperou +3 positivos sobre a camada 1 sem perder nenhum e sem negar errado — o
desenho "deny só com cadeia fechada; L2 unknown nunca derruba L1" segura as duas.

**(3) Prova prática da coordenadora (`prova_vgp_motor.py --so 1`): rc 0 — 8/8 TUDO AFIRMADO**

inline externo → DENY · inline import de módulo do projeto → DENY · arquivo externo → DENY ·
arquivo símbolo do projeto ainda inexistente → só AVISA · inline forma (builtin) → só AVISA ·
inline nome sob `import *` → só AVISA · programa limpo → ALLOW · defeito pré-existente no disco
→ ALLOW (delta correto).

**Gates finais**: suites 857+749+2+1652 = 3.260 testes 0 falhas · clippy `-D warnings` rc 0 nos
4 crates · `cargo fmt --all -- --check` rc 0 · drift A1 `test_drift_coverage.py` rc 0 (7ª
reconciliação) · `reconciliar.py verify` OK · pyright-langserver sem órfão (3→3 no aceite CLI).

**Mutações mortas (23 no total da entrega — 22 nomes distintos, `M-SPLAT-TIPADO` em dois sítios)**: M-CLI · M-N ·
M-I · M-M · M-B · M-K · M-D · M-P4 · M-CICLO (SIGABRT) · M-R1 · M-F3 · M-ALIAS · M-FUNC · M-F1 · M-CHAIN · M-WALRUS ·
M-MATCH · M-FUTURE · M-FSTRING · M-SPLAT-TIPADO ×2 (`func_sig` e `bind_parameters`) · M-PATHLIB · M-REEXPORT.

> **Nota da coordenadora touring-09 (09/10)**: (1) o número «17» corrigido por contagem da própria lista (a analise-0e
> confirma 23). (2) Os «4 crates» dos gates acima não incluíam o `touring-cli`, onde ficaram os 2 testes novos do
> `cli_ast_detail` (§5): eles **não compilavam** (`crate::HookRuntime::default()` — `HookRuntime` mora em
> `crate::runtime` e não tem `Default`); o clippy `--workspace --all-targets` do release 30.4.96 pegou (rc 101) e a
> coordenadora corrigiu para `HookRuntime::new(tempdir)`. (3) O held-out disjunto 20261010 reprovou o «≤ 1%» pelo
> limite superior (`diagnostics/holdout/RELATORIO.md`) — o «PRONTO» da §0 vale para o corpus de ajuste, não como
> generalização.

