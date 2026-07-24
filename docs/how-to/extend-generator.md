# How to extend the code generator

> A **how-to** (Diátaxis): task-oriented. You want a new generator *kind* (a
> templated, VGP-verified code artifact). Master Plan D.W4.P3. For the catalog of
> existing kinds see `docs/reference/generators.md`.

## Goal

Add a new kind to `touring-generator` so `touring generate render <kind>` and the
`taco-forge perfect-create-*` workflows can produce it through the verified
pipeline (Draft → Verified → Rendered → Speculated → Committed).

## Why a generator (not a Write)

Generators run the **typestate pipeline**: VGP verifies every cited symbol
against the index, the render is speculatively validated (shadow), and only then
is it committed atomically. A raw `Write` skips all of that. This is why creating
code goes through `taco-forge perfect-create-*` rather than the Write tool
(REGRA #14).

## Steps

1. **List the current kinds** to avoid collisions and to copy the closest one:
   ```bash
   touring generate list-kinds -j
   ```

2. **Define the kind** in `touring-generator`: its template, the variables it
   takes, and the symbols it must VGP-verify before rendering. Model it on an
   existing kind of the same shape (module / function / impl / test).

3. **Keep VGP honest.** Any symbol the template references must be verifiable:
   ```bash
   touring generate verify --symbol <Symbol>
   ```
   If a symbol does not exist, it is removed from the plan or marked
   `to_be_created` — never rendered on faith.

4. **Rebuild:**
   ```bash
   update-touring
   ```

## Verify

```bash
# The kind appears in discovery
touring generate list-kinds -j | grep <kind>

# A dry render produces the expected skeleton
touring generate render <kind> --vars '{"name":"Example"}'

# The full pipeline speculates before committing
touring generate plan-speculate --file <plan>
```

A new kind is "done" when `list-kinds` shows it, `render` produces valid output,
and `plan-speculate` passes (shadow score acceptable) before any
`plan-submit`/commit.

## Pitfall: drift between code and the reference doc

`docs/reference/generators.md` is generated from the kind registry
(`docs/gen_reference.py`). After adding a kind, regenerate and let the CI
anti-drift gate confirm sync:

```bash
python3 docs/gen_reference.py            # regenerate reference/*.md
python3 docs/gen_reference.py --validate  # gate: fails if docs drift
```
