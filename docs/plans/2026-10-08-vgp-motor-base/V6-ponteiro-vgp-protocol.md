---
paths:
  - "**/*.rs"
  - "**/*.py"
  - "**/*.ts"
  - "**/*.tsx"
  - "**/*.html"
---

# VGP — Verified Generation Protocol (ponteiro)

**Quem verifica é o motor, no momento da edição — não este texto.** `touring vgp check` (camada 1 nativa, tree-sitter;
camada 2 = o verificador do projeto, pyright-langserver persistente) roda em `pre-edit`, `pre-write`, MultiEdit,
`pre-bash` (inline) e `touring run`. A recusa ensina: nomeia o símbolo inventado, os membros reais e o mais próximo.

**Cobertura por linguagem.** Python: motor ativo (camadas 1 e 2). Rust, TS, TSX, JS e HTML (com versões de CDN e
classes CSS): fases VRS · VTS · VTSX · VHTML · VCDN · VCSS do plano — até cada uma passar no próprio juiz, o motor
**ainda não verifica** essas linguagens (silêncio dele ali não é aprovação) e **quem verifica é você**: `touring ast find <Símbolo> -j` /
`touring index find <Símbolo>` antes de usar campo, método, import ou prop; o `cargo check`/`tsc` do projeto segue
como juiz final depois da edição.

| situação | o que acontece |
|---|---|
| código que roda AGORA (`python3 -c`, heredoc, `touring run --code`, script em `/tmp`) com nome/atributo/import/kwarg inexistente | **deny** com os membros reais |
| edição de arquivo do projeto que cita alvo EXTERNO inexistente (stdlib, terceiro, builtin) | **deny** |
| edição de arquivo do projeto que cita símbolo do PROJETO ainda inexistente (consumidor antes do provedor, RED de TDD) | **aviso** — medido: 10/10 eram estados intermediários |
| membro de valor builtin (`int.add`, `str.get` — forma do dado) · atributo dunder · nome sob `from X import *` | **aviso** |
| receptor `Unknown`/união, `__getattr__`, `json.load` → `Any` | nada — fora do alcance estático |

Ao ESCREVER spec ou plano que cita API (PLAN-AS-CODE, P98), o motor não vê o texto: verifique com
`touring ast find <Símbolo> -j` / `touring index find <Símbolo>` antes de citar campo, assinatura ou valor de enum.

Manual do motor (modo `sombra|ativo`, kill switches, registro): `~/projects/touring/docs/vgp-motor.md`.
Medição, política e juiz: `~/projects/touring/docs/plans/2026-10-08-vgp-motor-base/proposta.md` §5–§6.
