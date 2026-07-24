# RFC-000 — RFC Process

**Status**: NORMATIVE (meta-document)
**Version**: 1.0.0
**Date**: 2026-04-24
**Editor**: Gabriel Gadea
**Depends on**: THSF-SPEC-v1.0.0
**Scope**: How to propose, review, version, deprecate, and retire RFCs
that extend or modify THSF.

---

## 1. Purpose

Every public spec eventually needs to evolve. This RFC defines the
self-governing process by which THSF itself evolves — including changes
to THSF-SPEC and to individual RFCs. It is meta-normative: breaking RFC-000
is a breaking change to THSF even if no other RFC moves.

---

## 2. RFC States

Every RFC moves through at most six states:

```
     ┌──────────┐
     │ DRAFT    │   ← newly proposed, not reviewed
     └────┬─────┘
          │ author requests review
     ┌────▼─────┐
     │ REVIEW   │   ← editor + community comment (≥72 h window)
     └────┬─────┘
          │ objections resolved
     ┌────▼─────┐   (reject fork)
     │ NORMATIVE│──────────┐
     └────┬─────┘          │
          │ new evidence   │
     ┌────▼─────┐          │
     │DEPRECATED│          │
     └────┬─────┘          │
          │ 1 MAJOR cycle  │
     ┌────▼─────┐     ┌────▼─────┐
     │ RETIRED  │     │ REJECTED │
     └──────────┘     └──────────┘
```

| State | Definition | Consumers |
|---|---|---|
| DRAFT | Author still editing — nothing relies on content | May read; MUST NOT cite |
| REVIEW | Editor + community gate; comment window open | May read + comment; MAY cite as "RFC-xxx (under review)" |
| NORMATIVE | Editor merged + version bumped | MUST implement if profile applies |
| DEPRECATED | Superseded but still valid for 1 MAJOR cycle | MUST warn via diagnostic when used |
| RETIRED | Removed from normative set | MUST NOT implement |
| REJECTED | Closed during review without merge | MAY cite as historical reference |

State transitions are recorded in Appendix A of each RFC under a
`## Version history` table with `| version | date | state | summary |`.

---

## 3. Proposing an RFC

### 3.1 File layout

An RFC proposal is a single Markdown file at:

```
docs/thsf/rfcs/RFC-NNN-<short-slug>.md
```

- `NNN` is the next free integer (monotonic, no gaps).
- `<short-slug>` is lowercase-kebab, ≤ 40 chars.

### 3.2 Required header

Every RFC starts with the following frontmatter-style header:

```markdown
# RFC-NNN — <Title Case>

**Status**: DRAFT
**Version**: 0.1.0
**Date**: YYYY-MM-DD
**Editor**: <name or handle>
**Depends on**: THSF-SPEC-vX.Y.Z[, RFC-xxx, ...]
**Supersedes**: [RFC-xxx or — ]

---
```

### 3.3 Required sections

Every RFC MUST include the following top-level `##` sections in order:

1. `## 1. Purpose` — why this RFC exists
2. `## 2. Non-goals` — what is deliberately out of scope
3. `## 3. Normative content` — the MUST/SHOULD/MAY clauses
4. `## 4. Diagnostic codes` — table of `thsf-<area>-NNN` if applicable
5. `## 5. Examples` — at least 2 concrete examples
6. `## 6. Conformance tests` — how to verify an implementation
7. `## 7. Security considerations` — or explicit "N/A and why"
8. `## 8. Version history` — table with state transitions

RFCs MAY add extra sections; the 8 above are the floor.

### 3.4 RFC-001, -002, -003, -004 exceptions

The four existing RFCs pre-date this process doc. They are grandfathered
into NORMATIVE state by the editor (Gabriel Gadea) without going through
REVIEW. Their next revision MUST add the missing `## 8. Version history`
section if not already present.

---

## 4. Review workflow

1. **Open review** — author moves `Status: DRAFT` → `REVIEW` and dates
   the change in Version history.
2. **Comment window** — MUST stay open for ≥ 72 hours. Comments go into
   the PR (if GitHub-hosted) or inline markdown at the bottom of the
   RFC file.
3. **Editor decision** — editor explicitly marks either:
   - `Status: NORMATIVE` + bump version per §5, OR
   - `Status: REJECTED` with a closing paragraph describing the reasons.
4. **No silent merges** — every transition from REVIEW MUST carry a
   decision paragraph.

If the author and editor disagree, the author may fork the RFC under a
new number; parallel RFCs are allowed (community selects).

---

## 5. Versioning RFCs

RFCs follow **semver 2.0.0** independently of THSF-SPEC. Bump rules
mirror RFC-002 §3.2 applied to document content:

| Change type | Bump | Example |
|---|---|---|
| Clarify ambiguous wording | PATCH | "MUST" → "MUST (see note)" |
| Add new MAY or SHOULD clause | MINOR | New optional diagnostic code |
| Change or remove MUST clause | MAJOR | Tighter validation of field X |
| Rename a diagnostic code | MAJOR | Code stability is a contract |

The `Version:` line MUST reflect the current aggregate version at the
top of the document. The `## 8. Version history` table MUST have an
entry for every change.

---

## 6. Deprecation and retirement

### 6.1 Marking deprecation

To deprecate an RFC:
1. Bump to the next MAJOR version.
2. Move `Status: NORMATIVE` → `Status: DEPRECATED`.
3. Append to the `## 8. Version history` table with a line pointing at
   the replacement (if any).
4. Implementations MUST emit a diagnostic `thsf-rfc-deprecated` whenever
   a deprecated RFC's features are exercised.

### 6.2 Retirement window

A DEPRECATED RFC MUST remain readable and machine-reachable for at
least one full MAJOR version cycle of THSF-SPEC before moving to
RETIRED. Retired RFCs keep their filename but add a banner at the top:

```markdown
> **RETIRED 2026-YY-ZZ** — superseded by RFC-<new>. Kept for reference only.
```

### 6.3 No silent deletions

RFCs MUST NOT be deleted from the repository. Retirement preserves the
historical record — other RFCs may cite retired numbers as lineage.

---

## 7. Relation to THSF-SPEC

### 7.1 Scope separation

- **THSF-SPEC** defines the framework's four layers and invariants.
  Changes MUST follow §11.1 of that document.
- **RFCs** specify the concrete protocols, schemas, and algorithms that
  implement the layers. RFC-000 governs their lifecycle.

### 7.2 When an RFC needs a SPEC change

An RFC that can only be honored by changing THSF-SPEC invariants MUST:
1. Open alongside a proposed SPEC diff.
2. Track the SPEC diff as a required dependency in its header.
3. Move to NORMATIVE only after the SPEC diff itself moves.

### 7.3 When THSF-SPEC changes require an RFC

A SPEC MAJOR bump MUST be accompanied by either:
- A new RFC documenting the breaking change, OR
- Coordinated MAJOR bumps of every existing RFC impacted.

---

## 8. Diagnostic codes

| Code | Meaning | Severity |
|---|---|---|
| `thsf-rfc-001` | Missing required header field | error |
| `thsf-rfc-002` | State transition skipped (e.g. DRAFT → NORMATIVE without REVIEW) | error |
| `thsf-rfc-003` | Version bump missing for content change | warning |
| `thsf-rfc-004` | `## 8. Version history` absent | warning |
| `thsf-rfc-005` | Deprecated RFC feature invoked at runtime | warning |
| `thsf-rfc-006` | File name does not match `RFC-NNN-<slug>.md` pattern | error |
| `thsf-rfc-007` | RFC cites a RETIRED RFC without lineage note | warning |

These codes are emitted by a linter (future scope — see §12).

---

## 9. Examples

### 9.1 Brand-new RFC (MINOR bump to conformance suite)

A community member proposes RFC-005 adding an OR-Set CRDT type.

1. Author writes `docs/thsf/rfcs/RFC-005-or-set-crdt.md`, status DRAFT,
   version 0.1.0, listing `Depends on: RFC-003`.
2. Moves to REVIEW; comment window opens 2026-05-01 at 10:00 BRT.
3. After 72h + address comments, editor bumps to 1.0.0 NORMATIVE.
4. RFC-003 bumps to 1.1.0 adding a forward-pointer: "See RFC-005 for
   OR-Set extension".

### 9.2 Retiring a feature

RFC-004 §13.2 "No streaming" becomes obsolete when WASI 0.3 ships.

1. New RFC-006 defines streaming interfaces.
2. RFC-004 bumps to 2.0.0 DEPRECATED, cross-references RFC-006.
3. After next THSF-SPEC MAJOR cycle (e.g. 2.0.0), RFC-004 moves to
   RETIRED. Its banner cites 2026-MM-DD retirement date.

---

## 10. Conformance tests

A linter (to be written as `tools/holon/scripts/rfc_lint.py`) MUST:

1. Scan every `docs/thsf/rfcs/RFC-*.md`.
2. Verify the header contains all required fields (§3.2).
3. Verify all 8 required sections are present (§3.3) — RFCs v0.1.0
   through v0.9.x are exempt to allow gradual adoption; v1.0.0+ MUST
   conform.
4. Verify filename matches `^RFC-[0-9]{3}-[a-z0-9-]+\.md$`.
5. Emit diagnostics from §8 as structured JSON.

Until the linter exists, editors MUST eyeball compliance at review time.

---

## 11. Security considerations

RFC-000 does not affect runtime behavior directly. Its security concerns
are governance-level:

- **Silent bypass**: someone skipping REVIEW → NORMATIVE corrupts the
  review record. Mitigated by §4 (explicit decision paragraph required).
- **Retroactive deletion**: someone removing a RETIRED RFC breaks
  citation chains. Mitigated by §6.3 (no-silent-delete rule).
- **Version rollback**: bumping a version backward misleads consumers.
  Mitigated by §5 (only monotonic bumps).

---

## 12. Future scope

- `rfc_lint.py` — automate conformance-test checking (§10).
- `rfc_index.json` — machine-readable catalog of all RFCs + states,
  regenerated on every merge.
- `RFC-template.md` — scaffold file for new RFCs (§3).
- GitHub Actions workflow to verify RFC-000 compliance on every PR
  touching `docs/thsf/rfcs/`.

None of these block the normative content above — they are tooling that
makes the process easier.

---

## 13. Version history

| Version | Date | State | Summary |
|---|---|---|---|
| 1.0.0 | 2026-04-24 | NORMATIVE | Initial (Fase 8 follow-up) |
