# touring-generator

> LLM-as-Planner / Touring-as-Generator: VGP-verified, typestate code generation.
> Master Plan D.W3.P2.

## Purpose

`touring-generator` turns a plan into code through a **verified pipeline** rather
than a raw write. Every symbol the generated artifact cites is checked against
the index (VGP) before rendering, the render is speculatively validated, and only
then is it committed atomically. This is the L2 layer (see
`docs/explanation/architecture.md`) and the engine behind the
`taco-forge perfect-create-*` workflows.

## The typestate pipeline

```
Draft ──verify──▶ Verified ──render──▶ Rendered ──speculate──▶ Speculated ──submit──▶ Committed
        (VGP)              (template)            (shadow)               (atomic)
```

Each transition is a distinct type, so a stage cannot be skipped at compile time.

## Key commands

```bash
touring generate list-kinds -j               # discover artifact kinds (~36)
touring generate verify --symbol <Symbol>    # VGP gate
touring generate render <kind> --vars '{}'   # preview a render
touring generate plan-speculate --file <p>   # shadow-validate before commit
touring generate plan-submit --file <p>      # atomic commit
```

## Example

```bash
# Verify a symbol exists, then preview a module skeleton that uses it
touring generate verify --symbol HookRuntime
touring generate render RustModule --vars '{"name":"example"}'
```

## Caveats

- **VGP is non-negotiable.** A cited symbol that is not in the index is removed
  from the plan or marked `to_be_created` — never rendered on faith. This is the
  anti-hallucination guarantee.
- The kind registry is the source of truth for `docs/reference/generators.md`;
  after adding a kind, regenerate (`docs/gen_reference.py`) so the anti-drift CI
  gate stays green.
- For *creating files* in practice, prefer the `taco-forge perfect-create-*`
  workflows (REGRA #14) — they drive this generator plus the surrounding 12–17
  stage gates (blast, format, TDG, atomic snapshot, RL reward).
