# External Client — `curl install.touring.dev | sh`

> **Audience**: someone outside Gabriel's internal workspace who wants Touring
> on their machine. Status: **W12.8 NOT YET SHIPPED** — this guide is a forward
> spec describing the intended UX. Until W12.8 lands, follow the "Manual
> install (interim)" section below.

## What the future UX looks like (W12.8 target)

```bash
curl --proto '=https' --tlsv1.2 -sSf https://install.touring.dev | sh
```

This is the same shape as rustup-init.sh (`curl ... | sh`). The script will:

1. **Detect OS + arch** — Linux x86_64 / aarch64, macOS x86_64 / arm64 (Windows
   is W14).
2. **Download the binary tarball** from `https://releases.touring.dev/<ver>/`
   along with its SHA-256 and sigstore signature.
3. **Verify SHA-256 + signature** — refuses to install if either fails.
4. **Create `~/.touring/`** (or `$TOURING_HOME` if set) — calls
   `touring toolchain init` after extracting the binary.
5. **Install the binary** into `~/.touring/toolchains/<ver>/bin/` and write
   `~/.touring/default` to that version.
6. **Optionally** add a `source ~/.touring/env` line to your `~/.bashrc` /
   `~/.zshrc` so `$PATH` picks up `~/.touring/toolchains/<default>/bin/`.

## Dry-run (when W12.8 ships)

```bash
curl https://install.touring.dev | sh -- --dry-run
```

Prints the steps it WOULD take without mutating disk. Use this if you are
nervous about a one-liner from the internet (you should be).

## Flags (proposed)

| Flag | Effect |
|------|--------|
| `--dry-run` | List actions, mutate nothing |
| `--version <ver>` | Pin to a specific Touring version (default: latest stable) |
| `--no-modify-path` | Skip the `~/.bashrc` / `~/.zshrc` modification |
| `--toolchain-home <dir>` | Use `<dir>` instead of `~/.touring/` |
| `--profile minimal\|default\|complete` | What components to install (rustup-pattern) |

## Manual install (interim — until W12.8 ships)

Until W12.8 ships, install by hand. The end state is identical.

### 1. Get the binary

For now, the only supported source is Gabriel's internal build:

```bash
# On Gabriel's machine, the binary is at:
ls ~/.claude/rust/target/release/touring
# Copy it to wherever you have write access:
cp ~/.claude/rust/target/release/touring ~/bin/touring
chmod +x ~/bin/touring
```

Public binaries from `releases.touring.dev` are not yet published — see
W13 (publishing pipeline) for the release infrastructure.

### 2. Initialize the toolchain root manually

```bash
touring toolchain init
```

Creates `~/.touring/{toolchains/, config.toml}`.

### 3. Drop the binary into a toolchain version directory

```bash
ver="0.30.0"   # or whatever your binary self-reports — check `touring --version`
mkdir -p ~/.touring/toolchains/$ver/bin
cp ~/bin/touring ~/.touring/toolchains/$ver/bin/touring
cp ~/bin/touring-hook ~/.touring/toolchains/$ver/bin/touring-hook   # if you have it
cp ~/bin/touring-daemon ~/.touring/toolchains/$ver/bin/touring-daemon

touring toolchain default $ver
# touring toolchain: default → 0.30.0
```

### 4. (Recommended) add to `$PATH`

```bash
# In ~/.bashrc or ~/.zshrc:
export PATH="$HOME/.touring/toolchains/$(cat ~/.touring/default 2>/dev/null)/bin:$PATH"
```

Or use a shim that resolves dynamically (rustup uses this — Touring may
implement equivalent in W12.3).

### 5. Verify

```bash
touring toolchain list
# 0.30.0 (default)

touring doctor -j
# ... expects 5/6 ok (daemon may need a separate spawn — see touring-rebuild.md)
```

## Security model (W12.8 target)

When W12.8 lands, the install script will follow rustup's hardening pattern:

1. **Pin to TLS 1.2+** via `curl --proto '=https' --tlsv1.2` (the recommended
   incantation, same as `rustup-init.sh`).
2. **Verify SHA-256** of the downloaded tarball against an embedded checksum.
3. **Verify sigstore signature** using a known public-key fingerprint hard-coded
   in the script.
4. **Refuse to write outside `~/.touring/`** without an explicit `--no-confine`
   flag (which prints a big warning).
5. **No `sudo`** unless `~/.touring/` is on a path that requires elevated
   permissions (which it shouldn't, by default).

Until W12.8 ships, **do not** install Touring from any URL claiming to be
`install.touring.dev` — the domain may not yet be controlled by Gabriel.

## Where to go next

After install (manual or via W12.8):

1. `cd ~/projects/<your-project>`
2. `touring init-project` — see [getting-started.md](getting-started.md) Step 2
3. `touring migrate-from-global` (if you have existing data) — see
   [migration.md](migration.md)
4. Open the project in Claude Code; verify hooks fire via
   `touring doctor -j`

## Frequently asked questions

### Can I install multiple versions side-by-side?

Yes — that's the whole point of `~/.touring/toolchains/<version>/`. Once W12.3
ships, `touring toolchain install <ver>` will support this directly. Until
then, manually drop each version into its own subdirectory and switch via
`touring toolchain default <ver>`.

### How do I uninstall?

```bash
rm -rf ~/.touring/
# And remove the PATH modification from your ~/.bashrc / ~/.zshrc.
```

If you used the future W12.8 installer with `--modify-path`, it will register
the modification in a way that's easy to remove (line marker comment, like
rustup's `# rustup` block).

### Why not `apt install touring` / `brew install touring`?

That's W14 (distro packages). The W13 publishing pipeline lays the
infrastructure (release artifact server, SBOM, signing); W14 wires up
Debian/Homebrew/Chocolatey packaging.

### Can my CI install Touring?

After W13 ships, yes — via a stable binary URL. Until then, vendor the binary
yourself into your CI image.

## Reference

- W12.8 plan: `W12-per-project-deployment.md` § W12.8
- W13 plan: `W13-publishing-pipeline.md` (release infrastructure)
- W14 plan: `W14-product-tiers--distribution.md` (distro packages)
- Rustup install script (reference implementation): https://github.com/rust-lang/rustup
