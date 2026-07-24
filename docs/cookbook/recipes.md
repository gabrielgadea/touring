# Touring Cookbook — Wave W7 of the 47to13-residual UPGRADE plan

> **Date**: 2026-06-04
> **Status**: 3 of 13 recipes delivered in this session. Remaining 10 in future waves.

This cookbook contains 3 high-leverage recipes for working with Touring.
Each recipe is a 50-150 LOC executable example with expected output.

## Recipe 01 — Measure doc coverage in 30 seconds

**Problem**: You want to know what fraction of your code's public API is
documented.

**Solution**: Use `scripts/doc-coverage.py` (W6 deliverable).

```bash
cd /home/gabrielgadea/.claude/rust
python3 scripts/doc-coverage.py              # all crates (28 measured)
python3 scripts/doc-coverage.py --crate touring-hooks  # one crate
python3 scripts/doc-coverage.py --top 10    # top 10 by coverage %
python3 scripts/doc-coverage.py --json      # JSON-only stdout
```

**Expected output** (truncated):

```
======================================================================
DOC-COVERAGE — Touring (28 crates)
======================================================================
CRATE                                  PUB    DOC       %  STATUS
----------------------------------------------------------------------
touring-license                         14     12  85.71%  PASS
touring-server-reasoning                98     66  67.35%  WARN
...
TOTAL                                 6948   3323  29.45%  P:1 W:8 F:19
```

**Integration with CI** (add to `.github/workflows/quality.yml`):

```yaml
- name: Doc coverage gate
  run: |
    python3 scripts/doc-coverage.py --json | jq '.summary'
    python3 scripts/doc-coverage.py --json | jq '.crates[] | select(.coverage_pct < 60) | {crate, coverage_pct}'
```

**Pass criterion**: mean coverage ≥ 60% (target after W6 execution).

## Recipe 02 — Add a new lifecycle hook in 5 steps

**Problem**: You want to register a new Claude Code lifecycle hook
(e.g. `pre-task-scout`, `post-terraform-plan`, etc.).

**Solution**: Use the canonical hook registration flow.

```bash
# 1. Define the handler in touring-hooks/src/my_new_hook.rs:
cat > crates/touring-hooks/src/my_new_hook.rs <<'EOF'
//! My new hook — handles <event_name> lifecycle event.

use crate::HookContext;
use crate::HookResponse;

pub fn handle(ctx: &HookContext) -> HookResponse {
    // Read ctx.event_kind, ctx.input, ctx.session_id
    // Decide: Allow / Warn / Deny
    HookResponse::allow()
}
EOF

# 2. Register in crates/touring-hooks/src/lib.rs:
grep -q "pub mod my_new_hook" crates/touring-hooks/src/lib.rs || \
  echo "pub mod my_new_hook;" >> crates/touring-hooks/src/lib.rs

# 3. Add to ALL_DAEMON_HOOK_NAMES in crates/touring-hooks/src/hook_registry.rs:
#    (Edit the file, add "my-new-hook" to the const)

# 4. Wire in settings.json:
#    Add to PreToolUse (or other event) section:
#    {"matcher": "<event>", "hooks": [{"type": "command", "command": "$HOME/.claude/hooks/touring-hook <event>"}]}

# 5. Verify:
cargo check --workspace
touring doctor -j | jq '.daemon_health.healthy_count'  # 8 (was 7)
```

**Pass criterion**: hook fires on the event, response is correct, no
regressions in other hooks.

## Recipe 03 — Tier-gate a feature in 3 lines

**Problem**: You want a feature to be **tier-premium** (only available
to Premium/Enterprise subscribers, not Free/Standard).

**Solution**: Use the `touring-license` tier feature.

```rust
// In your crate's Cargo.toml:
[features]
default = ["tier-free"]
tier-free = []
tier-standard = ["tier-free"]
tier-premium = ["tier-standard"]
tier-enterprise = ["tier-premium"]

// In your source code:
#[cfg(feature = "tier-premium")]
pub fn premium_feature() {
    // Available at compile time when tier-premium (or higher) is enabled.
}

#[cfg(not(feature = "tier-premium"))]
pub fn premium_feature() {
    panic!("This feature requires tier-premium or higher. See touring.dev/pricing.");
}

// At runtime, check the actual tier:
use touring_license::{License, Tier};

pub fn is_premium_unlocked(license: &License) -> bool {
    license.tier().at_least(Tier::Premium)
}
```

**Pattern**: compile-time `#[cfg(feature)]` for the binary's **maximum**
tier; runtime `License::tier().at_least()` for the user's **actual**
tier. The 30-day offline grace in `License::is_valid_at` is automatic.

**Pass criterion**: `cargo check -p <your-crate> --features tier-premium`
succeeds; without the feature, the function panics with a clear message.

---

## Recipes 04-13 (deferred to future sessions)

Per the upgrade plan Section V, the remaining 10 recipes are:
04. add-a-crate, 05. add-a-cli-command, 06. add-an-mcp-tool,
07. add-an-rl-arm, 08. add-a-language, 09. add-a-jwt-license,
10. debug-a-cycle, 11. debug-an-orphan, 12. production-deploy,
13. chaos-test.

Each is 50-150 LOC with executable examples. Authored in future waves
when the relevant tool/feature is exercised.

---

_Cookbook W7 partial (3/13 recipes). 2026-06-04. Future waves to add 10 more._
