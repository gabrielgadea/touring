# adw.py: a typed node interface (aprofundamento #9)

**Verdict: implement, narrow. Apply after 08/11.** The live file `~/.claude/skills/Touring/scripts/adw.py`
was NOT touched (nothing under `~/.claude/` or `client/` was edited; all work ran on a copy in a
scratch dir). The review marked this item Speculative, and half of that holds: the drift measurement
found no change at all on the 99 flows that ship today (495/495 outputs byte-identical), so this is
prevention, not a rescue. It is still worth shipping because the same measurement found one dead
key in a shipped fragment, four lint-vs-runtime disagreements about `{{nodes.X}}`, and four test
fixtures with a mistyped key, none of which any existing check could see.

## What is in this directory

| File | What |
|---|---|
| `adw.patch` | unified diff against the live `adw.py` (`patch -p1` from `~/.claude/skills/Touring`), base sha256 `91f8ac2a87d49ab8` |
| `test_adw.patch` | 4 fixture lines in the existing `test_adw.py` (a `human` node written with `prompt`, which the runner ignores) |
| `skilling-pack.patch` | decision B' (02/11): `should_skill` receives the count as argv[1] instead of the dead `env` (`patch -p1` from `~/.claude/skills/Touring`) |
| `test_adw_node.py` | new tests (144 cases; the strict xfail for `skilling-pack` is gone, its entry in `KNOWN_FINDINGS` removed) |
| `evidence/results/` | every number below, as produced; `evidence/tools/` reproduces them |

## How to apply (after the AB34 window closes, 08/11 00:10)

```bash
cd ~/.claude/skills/Touring
sha256sum scripts/adw.py | cut -c1-16        # expect 91f8ac2a87d49ab8; if not, run the --dry-run and rebase
patch -p1 --dry-run < <this dir>/adw.patch && patch -p1 < <this dir>/adw.patch
patch -p1 < <this dir>/test_adw.patch
patch -p1 < <this dir>/skilling-pack.patch
cp <this dir>/test_adw_node.py scripts/
cd scripts && python3 -m pytest test_adw_node.py test_adw.py test_adw_gate_rejection.py -q
#   expect: 509 passed (365 in the two old files + 144 in test_adw_node.py)
# then the generated mirror (CLAUDE.md item 6), from ~/projects/touring:
python3 scripts/sync-client-skills.py --apply
```

Rollback: `patch -R -p1` in reverse order (skilling-pack, tests, then adw.patch); the patches add no state.
The old copy of the fragment under `~/omarchy-kit/` carries the same dead `env`; it is outside this
repository and is not patched here.

**Decisions taken by Gabriel on 02/11/2026** (canvas in the session): `env` is fixed in the fragment
by passing the count as a positional argument (B'), and an unknown key stays an ERROR (E). Apply
`skilling-pack.patch` together with `adw.patch`: without it, a flow composing the fragment fails
`adw lint` (verified: `unknown key env` on a probe flow under the patched runner and the live library;
with the patched library, `valid: true`).

## The measurement (baseline = live adw.py, sha 91f8ac2a87d49ab8, 5093 lines)

| Metric | Value | How |
|---|---|---|
| tests | 365 passed (`test_adw.py` + `test_adw_gate_rejection.py`), 2.67 s | pytest on the copy |
| `.raw` reads | 181 reads, 49 distinct constant keys, 65 functions | AST scan (`scan_raw.py`); the review's "~181 / ~49 / ~43" holds, the function count differs by how you count helpers |
| keys the runtime consumes | 55 (49 constant + 6 via named families: `type`, edges, `good_input`/`bad_input`) | `closure.py` |
| keys the lint can see (reachable from `lint_spec`) | 45 | call-graph closure |
| keys used by real specs | 42 | 161 spec/fragment files, 508 nodes: shipped library + project + `analise` + 4 others |
| runtime reads the lint never validates | 10: `add_dirs budget_usd dry_rounds max_retries message stable_rounds stagnation_rounds timeout_ms zte zte_warmup` | the drift |
| spec keys the runtime never reads | **1: `env`** | the drift that matters |
| keys read with more than one default or access style | 20 of 49 (e.g. `timeout_ms` is 0, 120000 and 600000 at four sites; `max_iters` 0 vs 1; `merge` `""` vs `"collect"`) | `defaults.py`; mostly legitimate (lint demands, runtime falls back) and left alone |

### Regexes: they do not accept the same language

Six places parse `{{nodes.X}}`: four regexes (fragment namespacer, data-graph `NODE_REF_RE`,
sandbox-text `_AGENT_TEXT_REF_RE`, runtime `render_template`) and two bare-substring tests
(`f"nodes.{hidden}" in prompt` in `_lint_persona`, `f"nodes.{name}" in prompt` in `_lint_gate_feedback`).
The runtime and the namespacer agree with each other; the other four disagree with the runtime:

| Site | Disagreement with the runtime | Concrete wrong verdict (`evidence/results/demo.json`) |
|---|---|---|
| data graph | `X.full_ref`, `X.omitted_bytes`, `X.facts.k` are read as a node literally called `X.full_ref`... | a read-only node consumed only through `full_ref` gets "fake waiting" and "nothing downstream reads it, drop the node" |
| sandbox-text lint | bare `{{nodes.X}}` (= `summary`, i.e. agent prose) is not matched | a sandboxed gate interpolating agent text through the bare form gets no warning |
| `blind_to` | `nodes.rev` matches inside `nodes.review` | hard ERROR "blindness is fiction" on a critic that reads `review` |
| gate feedback | `nodes.check` matches inside `nodes.check2` | the "retry runs blind to the verdict" warning is suppressed |

41 reference forms were run through all parsers: 33 produce at least one disagreement. The forms
are not hypothetical: the real specs use `X.summary` 133 times, `X.full_ref` 11 times,
`X.facts.k` 2 times. Today none of those 13 uses lands on a node whose verdict flips (the replay
below is identical), so this is a latent defect with a demonstrated trigger, not an active one.

### The one dead key, and the fixtures

* **`env` in `adw-library/fragments/skilling-pack.toml`** (also in the older copy under
  `~/omarchy-kit/`): node `should_skill` is
  `command = ["bash","-c","test \"${RECURRENCE:-0}\" -ge 2"]` with `env = { RECURRENCE = "..." }`.
  `run_code_node` never reads `env`, so the subprocess never sees `RECURRENCE`; the gate answers
  "one-off" even for `recurrence = "5"` (measured: exit 1) and the `skilling` agent can never run.
  Old lint: valid. New lint: `unknown key 'env'`. The patch deliberately does not change the
  fragment (a finding is reported, not silently fixed).
* **Decision for Gabriel, two options**, both leave this patch unchanged:
  (A) implement `env`: pass `env={**os.environ, **rendered}` in `run_code_node` and add one row
  `"env": _field(dict, "code", "gate", "probe")` to `NODE_FIELDS`; or (B) express it without `env`,
  e.g. `command = ["bash","-c","test \"{{inputs.recurrence}}\" -ge 2"]`.
* **Chosen: B'** (02/11). (B) as written splices a rendered value into the `-c` string, which is
  shell injection (`recurrence = '2" ] || touch X ; [ "'` executes); (A) with `os.environ` hands
  the whole runner environment, secrets included, to the child. B' keeps (B)'s "no runtime
  surface" and passes the value as argv: `["bash","-c","test \"$1\" -ge 2","_","{{inputs.recurrence}}"]`.
  `test_should_skill_gate_answers_on_recurrence_and_never_runs_it_as_shell` pins it (5/2 pass,
  1/`abc` fail, two injection payloads fail and create nothing); against the old fragment it fails
  (`assert 1 == 0` for `recurrence = "5"`), as does the corpus test.
* **Fixtures**: four `human` nodes in `test_adw.py` declare `prompt` (the runner reads `message`).
  Two of them make existing tests fail under the new lint (the lint's own warning is correct);
  `test_adw.patch` renames all four.
* **`control_waived`**: my first schema declared it boolean and 7 library tests failed at once,
  because shipped flows pass the REASON as a string (`control_waived = "quorum de ..."`) and the
  runtime reads it by truthiness. The table now says `(bool, str)`. That is the kind of knowledge
  that used to live only in whoever wrote the flows.

## The design (what the patch adds)

1. **One grammar.** `REF_RE` (one regex), `RESULT_FIELDS`, `NodeRef`, `parse_node_ref`, `node_refs`.
   `render_template`, `_namespace_node_refs`, `node_data_reads`, the sandbox-text lint, `blind_to` and
   gate-feedback all go through them; `NODE_REF_RE`, `_AGENT_TEXT_REF_RE` and both substring tests
   are gone. `{{inputs.k}}` and `{{branch.k}}` stay separate on purpose: they are other
   namespaces resolved at other times (fragment inlining, fan-out cloning).
2. **One table of node keys.** `NODE_FIELDS: dict[str, NodeField]`, 56 rows (the 55 keys the runtime
   reads + the documented-but-unimplemented `max_turns`). Each row: TOML kinds, element kind for
   lists, the node types that read it, an optional closed vocabulary (only `session`), and three
   flags: `dedicated` (a lint that already validates this key's shape/placement owns it, so the
   table only vouches that it exists: 30 rows, which is what keeps one defect from producing two
   messages), `reserved` (accepted, not honoured, warns) and `refused` (`policy`: the old ad-hoc
   block in `_lint_parallel` moved into its row, same message text).
3. **`schema_findings(node)`**, called first for each node in `lint_spec`: unknown key is an error
   with a `difflib` suggestion; wrong TOML kind is an error (bool is never an int, so `retries =
   true` and `sandbox = "false"` are refused; the latter would have switched the sandbox ON by
   truthiness); a key another node type reads is a warning; a reserved key is a warning.
4. **The table is tied to the code both ways** (`test_adw_node.py`): every constant key read from
   `Node.raw` anywhere in `adw.py` must be a row (an AST scan), and every non-reserved row must
   appear as a string literal somewhere in `adw.py` outside the table (a dead row fails). `env`
   would have failed the second one.

Not done, and why: the 181 `.raw[...]` reads were NOT migrated to accessors. The two guard tests
detect the same drift (a key read but undeclared, a row nobody reads) with no behaviour change to
181 sites; migrating them is risk without extra detection. Defaults were not moved into the table
for the same reason (the 20 multi-default keys are mostly intentional: lint demands what runtime
tolerates, and `timeout_ms` is legitimately per node type).

### The honest cost

`adw.py` grows 5093 to 5298 lines (+224/-46 in the diff). Net surface that shrinks: 4 regexes
and 2 substring tests become 1 regex and 1 parser; one ad-hoc refusal moves into the table. What
grows: every new field now needs one table row (B9 would have needed two: `mcp`,
`session_persistence`); the guard test makes forgetting it a build failure instead of a silent
no-op. Counter-evidence acknowledged: B9 touched only `adw.py` and the node fingerprint already
covers the whole `raw`, so nothing here changes run identity (the fingerprint code is untouched).

## Equivalence evidence

* **Replay of every flow**: 99 flows (library 23, `touring` 25, `analise` 40, `omarchy-kit` 8,
  `kazuba`, `konverter`, `~/.touring` 1 each), each through `lint`, `explain`, `explain --cost`,
  `explain --mermaid` and `test` (mock agents; `subprocess.run` stubbed, `time.sleep` and run ids
  made deterministic), on a private copy of each corpus: **495/495 byte-identical** old vs new,
  zero justified differences needed (`replay_compare.txt`, hashes in `replay_summary.json`).
  The one real difference, `skilling-pack`, is not composed by any of these flows, so it shows
  only in the crafted case below and in the corpus schema scan (2 findings: the same fragment in
  two copies, `corpus_schema.txt`).
* **Fuzz** (40 000 random strings of `{{nodes.*}}`, `{{vars.*}}`, malformed braces, odd names):
  `render_template` and `_namespace_node_refs` are identical old vs new (0 mismatches). For data
  reads, the new parser equals what the runtime itself resolves on 40 000/40 000; the old one is
  wrong on 7 367 of the 34 845 samples that carry a `full_ref`/`omitted_bytes`/`facts` form
  (`equiv_fuzz.txt`).
* **Crafted defects** (`demo.json`): `env` in a composed `skilling-pack` (old valid / new error);
  `blind_to` prefix (old error / new clean); gate-feedback prefix (old silent / new warns);
  full_ref-only consumer (old two false warnings / new none); bare ref in a sandboxed command (old
  silent / new warns); `timeuot_ms` and `idempotent = "false"` (old valid / new two errors).
* **Live tests on the new code**: 363 pass, 2 fail (the fixtures `test_adw.patch` fixes);
  new suite: 507 passed, 1 xfailed.

## Mutation: 28 mutants, 28 killed

Each is one exact text replacement (asserted to match once), run against all three test files
(`mutation.json`): a row dropped; kind check always true; bool accepted as int; hyphen dropped from
the regex class; facts/suffix order swapped; `omitted_bytes` dropped; bare-reference default
changed; unknown keys accepted; `session` vocabulary shrunk; namespacer touching `vars`; sandbox
text fields narrowed; placement warning removed; refusal removed; `control_waived` back to bool;
both substring tests restored; `dedicated` flag dropped / dedicated rows revalidated (double
report); suggestion cutoff 0; reserved key as error; list elements unchecked; data graph ignoring
non-summary fields; runtime ignoring facts; schema not called from `lint_spec`; a dead row added;
two more rows dropped; an edge family narrowed; a node type narrowed. One mutant (an integer
accepted as a number) first survived and showed that branch was dead code; it was removed
instead of tested.

## Open issues

1. `env` (above): decided, B' (02/11), shipped as `skilling-pack.patch`.
2. Unknown key is an ERROR (exit 1, blocks `run`), confirmed by Gabriel 02/11 (E). Reverting is one line in
   `schema_findings` (`errors.append` to `warnings.append`) plus the matching assertions in
   `test_adw_node.py`. Specs outside this machine are unmeasured: the 161 files here are the
   whole corpus I could reach.
3. `flow_cost` reads `timeout_ms` with default 0 where the runner uses 120 000 (code) or 600 000
   (agent), so `explain --cost` under-reports nodes without a declared timeout. Observed, not
   touched.
4. `blind_to` and gate-feedback now require braces: a prompt that merely MENTIONS `nodes.X`
   without `{{ }}` is no longer a read (the runner never interpolates it). Pinned by a test.
5. No documentation enumerated the node keys before; the table is the first enumeration. The
   module docstring's `lint` line now mentions it; no SKILL.md/reference was edited (live).
6. The A/B experiment AB34 is untouched: the live `adw.py` hash was re-verified equal to the base
   at the end of this work.

## Reproduce

All scripts take paths as arguments (see each docstring); the order is `scan_raw.py`, `closure.py`,
`regex_diff.py`, `replay.py` (old, then new) + `compare.py`, `equiv_fuzz.py`, `demo.py`,
`corpus_schema.py`, `mutate.py`. Run the heavy ones under `scripts/hw-slot --`.
