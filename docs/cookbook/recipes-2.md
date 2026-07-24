# Touring Cookbook — Recipes 04-08 (W7 expansion)

> **Date**: 2026-06-04
> **Wave**: W7 expansion of the 47to13-residual UPGRADE plan
> **Status**: 5 more recipes (total: 8 of 13). Remaining 5 in future sessions.

This document ships 5 more high-leverage recipes for working with Touring.
The first 3 are in `docs/cookbook/recipes.md`.

## Recipe 04 — Add a new crate to the workspace

**Problem**: You want to create a new crate in the workspace
(e.g. `touring-foo`).

**Solution**: Use the canonical workspace member flow.

```bash
# 1. Create the crate directory + Cargo.toml
mkdir -p crates/touring-foo/src
cat > crates/touring-foo/Cargo.toml <<EOF
[package]
name = "touring-foo"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true

[dependencies]
serde = { workspace = true }
thiserror = { workspace = true }
touring-foundation = { workspace = true }

[lints]
workspace = true
EOF

# 2. Add lib.rs skeleton
cat > crates/touring-foo/src/lib.rs <<'EOF'
//! # Boundary: touring-foo
//!
//! **Inputs**: ...
//! **Outputs**: ...
//! **Invariants**:
//!   - I1: ...
//! **Tier**: free.
//! **Stability**: 2 (stable).

#![warn(missing_docs)]
#![warn(clippy::all)]
```

# 3. Register in root Cargo.toml
sed -i '/members = \[/a \    "crates/touring-foo",' Cargo.toml

# 4. Verify
cargo check --workspace                  # MUST be exit 0
cargo test -p touring-foo                # new crate has 0 tests, OK
touring wiring orphans -j | jq '.count'  # delta <= 0
```

**Pass criterion**: `cargo check --workspace` exit 0; the new crate is
listed in `cargo metadata --no-deps --format-version 1 | jq '.workspace_members'`.

## Recipe 05 — Add a new CLI command in 3 steps

**Problem**: You want a new `touring <command>` subcommand.

**Solution**: Use clap derive + the `clap_handlers` dispatch.

```rust
// 1. In crates/touring-server/src/cli/my_command.rs:
use clap::Args;
use serde::Serialize;

#[derive(Args, Debug)]
pub struct MyCommandArgs {
    /// Path to operate on
    pub path: String,
    /// Output format
    #[arg(long, default_value = "json")]
    pub format: String,
}

#[derive(Serialize)]
pub struct MyCommandResult {
    pub path: String,
    pub count: usize,
}

pub fn run(args: MyCommandArgs) -> Result<MyCommandResult, Box<dyn std::error::Error>> {
    // ... do the work ...
    Ok(MyCommandResult { path: args.path, count: 0 })
}
```

```rust
// 2. In crates/touring-server/src/cli/mod.rs:
#[derive(Subcommand)]
pub enum Cli {
    /// My new command - does X
    MyCommand(MyCommandArgs),
    // ... other commands ...
}
```

```rust
// 3. In crates/touring-server/src/cli_handlers.rs:
match args.command {
    Cli::MyCommand(args) => {
        let result = cli::my_command::run(args)?;
        println!("{}", serde_json::to_string_pretty(&result)?);
    }
    // ...
}
```

```bash
# 4. Verify
cargo check --workspace
touring my-command /tmp/test --format json | jq .
```

**Pass criterion**: `touring my-command <path>` returns valid JSON;
help message includes the new command.

## Recipe 06 — Add a JWT-verify feature in 3 files

**Problem**: You want to enable cryptographic license verification
(the `jwt-verify` feature in `touring-license`).

**Solution**: Add `jsonwebtoken` + `ed25519-dalek` deps; extend `License::parse_verified`.

```rust
// 1. In crates/touring-license/Cargo.toml:
[dependencies]
jsonwebtoken = { version = "9", optional = true }
ed25519-dalek = { version = "2", optional = true }

[features]
jwt-verify = ["dep:jsonwebtoken", "dep:ed25519-dalek"]
```

```rust
// 2. In crates/touring-license/src/lib.rs:
#[cfg(feature = "jwt-verify")]
pub fn parse_verified(token: &str, public_key: &[u8]) -> Result<License, LicenseError> {
    use jsonwebtoken::{decode, DecodingKey, Validation};
    use ed25519_dalek::Verifier;

    let key = DecodingKey::from_ed_der(public_key);
    let mut validation = Validation::new(jsonwebtoken::Algorithm::EdDSA);
    let token = decode::<Claims>(token, &key, &validation)?;
    License::from_claims(token.claims)
}

#[cfg(not(feature = "jwt-verify"))]
pub fn parse_verified(_token: &str, _public_key: &[u8]) -> Result<License, LicenseError> {
    Err(LicenseError::JwtVerifyNotCompiled)
}
```

```bash
# 3. Verify
cargo check -p touring-license --features jwt-verify
cargo test -p touring-license --features jwt-verify
```

**Pass criterion**: `--features jwt-verify` compiles; `parse_verified`
returns `Err(JwtVerifyNotCompiled)` without the feature.

## Recipe 07 — Debug an orphan in 5 steps

**Problem**: `touring wiring orphans -j` reports a symbol as orphan,
but you suspect it's actually consumed somewhere.

**Solution**: Apply Cadeia 7 (VP-Scout) — verify via grep.

```bash
# 1. Get the orphan symbol
ORPHAN=$(touring wiring orphans -j | jq -r '.orphans[0].symbol')
echo "Investigating: $ORPHAN"

# 2. Grep the entire workspace
grep -rn "$ORPHAN" crates/ --include="*.rs" | grep -v "//" | head -10

# 3. If grep finds a consumer, the wiring DB is stale
#    (Cadeia 7 — common 5-10 min after edits)
#    Action: touring index rebuild
#    Then re-check:
touring index rebuild
touring wiring orphans -j | jq '.count'  # delta <= 0?

# 4. If grep is empty, the symbol IS orphan
#    Inspect why it exists:
touring index find "$ORPHAN"  # definition file:line
touring wiring impact "$ORPHAN" --depth 3  # transitive consumers

# 5. Decide: wire it (find a consumer, add the import)
#    OR remove it (if it's truly unused)
touring assist apply auto_wire <file>:<line>
```

**Pass criterion**: orphan count delta ≤ 0 after the action; if the
symbol was wired, the consumer is documented in the change.

## Recipe 08 — Production deployment (per-project)

**Problem**: You want to install Touring for a single project
(per-project deployment, like rustup's `rustup override`).

**Solution**: Use the W12.6 hook walkup shim.

```bash
# 1. Per-project install
mkdir -p /path/to/your-project/.touring/bin
ln -sf ~/.local/bin/touring /path/to/your-project/.touring/bin/touring
ln -sf ~/.local/bin/touring-hook /path/to/your-project/.touring/bin/touring-hook
ln -sf ~/.local/bin/touring-daemon /path/to/your-project/.touring/bin/touring-daemon

# 2. Set CLAUDE_PROJECT_DIR so the hooks find the project
export CLAUDE_PROJECT_DIR=/path/to/your-project

# 3. Verify
/path/to/your-project/.touring/bin/touring --version
/path/to/your-project/.touring/bin/touring doctor
```

**Pass criterion**: `touring --version` from inside the project
returns the version; the daemon is per-project, not system-wide.

---

## Remaining 5 recipes (deferred to future sessions)

09. add-an-mcp-tool, 10. add-an-rl-arm, 11. add-a-language,
12. chaos-test, 13. add-a-jwt-license (full version with revocation).

---

_Cookbook W7 expansion 2026-06-04 (8/13 recipes total). Future waves for 5 more._
