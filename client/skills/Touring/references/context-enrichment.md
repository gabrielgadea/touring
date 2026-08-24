# Context Enrichment — the doctrine elaborated in Touring (2026-08-08)

> Canonical body. Pointed at from `Touring/SKILL.md`, `loop-engineering/SKILL.md`
> and `TACO-cross-audit/SKILL.md`. Every claim here was **measured**, not assumed;
> the evidence is in `~/projects/touring/docs/plans/2026-08-08-portfolio-de-capacidades/`.

**Context enrichment** is everything that puts information in front of the agent
that the agent did not ask for: hook `additionalContext`, an ADW node's summary,
a lens on the CCE ledger, a prior-art block before a `Write`. It is the highest-
leverage and the most dangerous surface in the stack — high leverage because it
reaches the decision before it is made, dangerous because the agent cannot tell
a fabricated enrichment from a measured one.

The nine strategies below are the discipline that makes it the first and not the
second.

---

## E1 — Index the field that states purpose, not the field that names things

The symbol index is keyed by identifier. `touring tantivy search "prior art"`
returns `art_root` (a fuzz shell) and `with_prior` (a Bayesian predictor):
lexically perfect, semantically unrelated. Purpose lives in a different field —
module docstrings, `//!` headers, `SKILL.md` frontmatter, `[adw] description` —
and **96% of 3.881 Python scripts carry it** (mean 411 chars).

> Before adding a retrieval model, ask whether the key is simply wrong.
> `touring portfolio` indexes purpose prose; that alone answered intents no
> embedding would have rescued, because the corpus was never being read.

## E2 — Enrichment must be structurally inescapable, not persuasive

Measured in this workspace, twice: *"I built the master commands and still
reached for atomic tools"* (`touring-4-pillars.md`), and MUST nudges at
confidence 0.95 ignored in the very session that emitted them
(`memory recall "protocol-adherence-diagnosis"`).

**Affordance changes `U(a)=P·V−C(tokens)`; persuasion does not.** So enrichment
is wired where the flow already passes:

| Surface | Why it is inescapable |
|---|---|
| a lens in `explore`'s `AUTOMATED_LENSES` | the CCE contract only counts a round as dry when **every** lens ran |
| a `code` node in an ADW spec | the runner calls it by construction |
| `cli_suggest` PreToolUse on `Write` | fires before the file exists |

A new command nobody calls is debt, not capability.

## E3 — Three sections, never a ranked list (the anti-anchor contract)

A bare list makes the agent reuse whatever ranked first. Every enrichment payload
carries:

```
prior_art[]  — candidates WITH provenance and evidence
gaps[]       — what the prior art does NOT cover for this intent
external[]   — the outside lens to consult (library + the exact question)
```

plus a **required verdict** (`reuse | extend | supersede | create_new`) with a
justification. Naming the gap is what invites superseding; without `gaps` the
injection is an anchor that suppresses better solutions.

## E4 — Absence is displayed, never hidden

Every evidence field is an `Option`, and a `None` is *rendered*: "sem teste
conhecido", "idade desconhecida", "nunca escolhido antes". An empty result says
so **and reports the corpus size**, so a thin answer reads as thin instead of
authoritative.

The worst outcome of enrichment is reusing something broken because it merely
ranked well.

## E5 — Never fabricate to fill a gap

`touring find-code search` returned `doc_kw_1..5` and `doc_sem_1..5` — hardcoded
ids with constant scores, identical for every query, in any language. The
semantic leg even computed a real embedding and **discarded it**. Two existing
tests asserted `!results.is_empty()`, i.e. they *encoded the bug*.

An empty answer plus an honest status beats a plausible fabricated one. The
corollary is structural: expose *which* corpus answered
(`BackendStatus { keyword_backend, embedding_provider, vector_store }`) so
"searched and found nothing" is distinguishable from "never had a corpus".

And do not wire an **empty** backend: an empty store reports `wired: true` and
turns "no corpus" into "no match" — the same lie, one level down.

## E6 — A gap claim is an assertion about absence

Ranking may be exact; an absence claim may not. Saying *"no candidate mentions
professional"* when a candidate says *"professionally formatted"* is a false
statement, and enrichment that makes false statements is worse than none.
Absence checks compare by word family (shared prefix ≥ 4 chars); relevance
scoring stays exact.

## E7 — Meet the operator's language, symmetrically

The corpus is overwhelmingly English; the intents are frequently Portuguese.
`touring search-tools "gerar PDF profissional"` → `No matching tool`, while the
English phrasing of the same intent ranked a result.

Fix: normalize **both** the corpus and the query to one canonical term set, so
"mapa" (doc) and "map" (query) meet in the middle. A curated offline table beats
a translation model here — total, deterministic, microseconds, and unmapped
terms (PDF, SIMD, tantivy) pass through intact.

## E8 — Derived values, never placeholders

The injection-density invariant (`touring-4-pillars.md`) applied: an enrichment
must carry the **real** value whenever it is derivable. The Write nudge derives
its intent from the docstring being written, falling back to the file stem —
never `<intent>`.

Two corollaries measured on 2026-08-08:
- **boilerplate is not purpose**: a licence banner clears any length floor and
  would send the portfolio hunting for "copyright … all rights reserved" on
  every Write. Filter it, then keep looking.
- **a stub is not purpose**: `"Command-line interface."` (23 chars) outranked
  real artifacts, because BM25 length normalization favours short documents. A
  finer grain needs a *higher* prose floor than a coarse one.

## E9 — Enrichment that ages silently is worse than none

A cached index behind a `OnceLock` in a long-lived daemon never sees
`touring portfolio refresh`, and keeps asserting stale prior art with full
confidence. Invalidate on the source's mtime — a `stat` is microseconds and is
exact, so no TTL guesswork.

---

## Applying it

| You are | Do this |
|---|---|
| about to create an artifact | `touring portfolio "<intento>"` → record a verdict |
| adding an enrichment surface | wire it where the flow already passes (E2), with the 3-section contract (E3) |
| auditing an enrichment | check E4/E5/E6/E8/E9 — the honesty axes, each provable by execution |
| writing a retrieval feature | ask first whether the *key* is wrong (E1) |

```bash
touring portfolio "<intento>"                     # prior-art por propósito
touring portfolio inspect <arquivo>               # o que o minerador extrai
touring portfolio verdict "<intento>" --choice extend --why "<razão>"
touring portfolio history                         # decisões acumuladas
```

## Cross-references

| Topic | Local |
|---|---|
| Measured evidence, per phase | `~/projects/touring/docs/plans/2026-08-08-portfolio-de-capacidades/` |
| Injection-density invariant | `~/.claude/rules/touring-4-pillars.md` |
| Affordance vs persuasion (origin) | `touring memory recall "protocol-adherence-diagnosis"` |
| Fabricated corpus post-mortem | `memory/find-code-fabricava-resultados.md` |
| The portfolio itself | `memory/portfolio-de-capacidades.md` |

---

_v1.0 — 2026-08-08 | Nine strategies, each with a measurement behind it._
