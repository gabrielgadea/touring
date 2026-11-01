# Verification Before Completion — Rationale + Escape Patterns

> **Origin**: 2026-09-01, Rank #1 do plano `docs/plans/2026-09-01-skill-aprimoramento/`. Extracted from Touring SKILL.md Rule #12 to comply with REGRA #13 (body <500L) — TACO-skilling Rule #2 (ADD and PRUNE).

## Rationalization table (apply verbatim — borrowed from `superpowers:verification-before-completion`)

| Excuse | Reality |
|---|---|
| "Should work now" | RUN the verification |
| "I'm confident" | Confidence ≠ evidence |
| "Just this once" | No exceptions |
| "Linter passed" | Linter ≠ compiler |
| "Agent said success" | Verify independently |
| "I'm tired" | Exhaustion ≠ excuse |
| "Partial check is enough" | Partial proves nothing |

## Red flags — STOP

- Using "should", "probably", "seems to"
- Expressing satisfaction before verification ("Great!", "Perfect!", "Done!", etc.)
- About to commit/push/PR without verification
- Trusting agent success reports
- Relying on partial verification
- Thinking "just this once"
- Tired and wanting work over
- **ANY wording implying success without having run verification**

## Gate function (verbatim `superpowers:verification-before-completion`)

```
BEFORE claiming any status or expressing satisfaction:

1. IDENTIFY: What command proves this claim?
2. RUN: Execute the FULL command (fresh, complete)
3. READ: Full output, check exit code, count failures
4. VERIFY: Does output confirm the claim?
   - If NO: State actual status with evidence
   - If YES: State claim WITH evidence
5. ONLY THEN: Make the claim

Skip any step = lying, not verifying
```

## Common failures (claim vs requirement)

| Claim | Requires | Not Sufficient |
|---|---|---|
| Tests pass | Test command output: 0 failures | Previous run, "should pass" |
| Linter clean | Linter output: 0 errors | Partial check, extrapolation |
| Build succeeds | Build command: exit 0 | Linter passing, logs look good |
| Bug fixed | Test original symptom: passes | Code changed, assumed fixed |
| Regression test works | Red-green cycle verified | Test passes once |
| Agent completed | VCS diff shows changes | Agent reports "success" |
| Requirements met | Line-by-line checklist | Tests passing |

## Red-green cycle (verbatim)

```
Write → Run (pass) → Revert fix → Run (MUST FAIL) → Restore → Run (pass)
```

> A regression test that hasn't been proven to fail is not a regression test — it's a hypothesis.

## When to apply

**ALWAYS before**:
- ANY variation of success/completion claims
- ANY expression of satisfaction
- ANY positive statement about work state
- Committing, PR creation, task completion
- Moving to next task
- Delegating to agents

**Rule applies to**:
- Exact phrases
- Paraphrases and synonyms
- Implications of success
- ANY communication suggesting completion/correctness

## Cross-references

- **Source**: obra `superpowers:verification-before-completion` (Jesse Vincent / obra-superpowers)
- **Cross-pollination**: mattpocock/skills `code-review` ("A bad ref or empty diff should fail here, not inside two parallel sub-agents")
- **Universal MUST E**: same MUST lives in 7 skills (Marcel Point #1 do Gabriel, single commit)
- **Plan reference**: `docs/plans/2026-09-01-skill-aprimoramento/plan.md` §3.1 Rank #1
