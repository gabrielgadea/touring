# Touring Integration — Commands, Tiers, Fallback

The intelligence layer that the bare skill-creator has no access to. Every
command here is read-cheap (CLI, < 10 ms) unless noted. If the daemon is down,
see [Fallback](#fallback-when-the-daemon-is-down) — the workflow never blocks.

## Table of contents

- [By phase](#by-phase)
- [Memory tiers](#memory-tiers)
- [Learning reward semantics](#learning-reward-semantics)
- [Diary](#diary)
- [Evolution & gotcha](#evolution--gotcha)
- [Fallback when the daemon is down](#fallback-when-the-daemon-is-down)

## By phase

| Phase | Capability | Command |
|-------|------------|---------|
| 0 | health gate | `touring doctor -j` |
| 1 | past lessons | `touring memory recall "<intent>"` |
| 1 | symbol / pattern search | `touring tantivy search "<query>"` |
| 1 | known pitfalls for a file | `touring gotcha match <file>` |
| 2 | task DAG for a complex skill | `touring decompose create skill "<name>"` |
| 4 | VGP — does a symbol exist? | `touring index find <symbol>` |
| 4 | VGP gate before codegen | `touring generate verify --symbol <name>` |
| 4 | generate a bundled script | `Write tool (script Python) --path <p> --intent "<i>"` |
| 5 | store the lesson | `touring memory store <key> "<value>" --tier semantic` |
| 5 | reward the outcome | `touring learning reward <tool> <value> "<ctx>"` |
| 5 | agent diary entry | `touring diary write taco-skilling "<entry>" --aaak` |
| refine | pattern drift over time | `touring evolution insights -j` |

## Memory tiers

`touring memory store` accepts `--tier`. For skill engineering:

- **`semantic`** — durable lessons that should survive across sessions: "skill X
  created for Y", "refine of Z routed the fix to a reference because the body hit
  480 lines". This is the default tier for CREATE Phase 5 and REFINE Phase 5.
- Working/episodic tiers exist but are not used by this skill — skill lessons are
  meant to be long-lived.

Key naming convention (so `recall` and future audits find them):

- `skill:create:<name>` — a creation event.
- `skill:refine:<name>:<date>` — a refinement event.
- `skill:lesson:<topic>` — a cross-skill lesson (applies when building any skill).

## Learning reward semantics

`touring learning reward <tool> <value> "<ctx>"` feeds the RL loop. Use it to
record whether a TACO-skilling run actually produced a good outcome:

- `+1.0` — skill created/refined, passed the hygiene gate and (if applicable) the
  eval loop.
- `0.0` — neutral / declined (e.g. discovery found the task was a one-off).
- `-1.0` — the run failed a gate or the user rejected the result.

Reward honestly. A reward loop fed only `+1.0` learns nothing. Use the `<ctx>`
string to say what happened, so `evolution insights` can later attribute drift.

## Diary

`touring diary write taco-skilling "<entry>" --aaak` records the run in the
agent diary under the `taco-skilling` agent. Use it once per CREATE and once per
REFINE. The `--aaak` flag uses the structured AAAK entry format. Read history
with `touring diary read taco-skilling`.

## Evolution & gotcha

- `touring evolution insights -j` — during REFINE, surfaces which tools/patterns
  have been effective, which have drifted. Use it to decide whether a recurring
  problem is skill-specific or systemic.
- `touring gotcha match <file>` — during CREATE Phase 1, surfaces known pitfalls
  for files the skill will touch, so the draft can pre-empt them.

## Fallback when the daemon is down

`touring doctor -j` at Phase 0 may report the daemon unreachable
(`daemon_socket: error`). The workflow continues — every Touring step has a
degraded path:

| Touring step | Fallback |
|--------------|----------|
| `memory recall` | skip; note "no memory recall — daemon_degraded" in the report |
| `tantivy search` | use `grep`/`Glob` over `~/.claude/skills/` |
| `index find` (VGP) | `grep` for the symbol; if not found, flag it as unverified |
| `memory store` / `learning reward` / `diary` | skip; tell the user the lesson was not persisted |
| transcript mining | unaffected — `mine_transcripts.py` reads `.jsonl` files directly, no daemon needed |

Always mark daemon-degraded fields explicitly in the final report. A silent skip
is a lie about how grounded the result is.
