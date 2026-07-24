# `touring toolchain` — Subcommand Spec (W12.x)

## Subcommands

| Cmd | Description |
|---|---|
| `touring toolchain list` | List installed toolchains |
| `touring toolchain install <channel>` | Install channel (stable\|nightly\|<version>) |
| `touring toolchain remove <channel>` | Remove a toolchain |
| `touring toolchain default <channel>` | Set default toolchain |
| `touring toolchain link <name> <path>` | Link a local build as toolchain |
| `touring update` | Update default toolchain to latest stable |

## Filesystem layout

```
~/.touring/
├── toolchains/
│   ├── stable-1.0.0/        (active)
│   ├── nightly-2026-05-11/
│   └── pinned-0.9.5/
├── cache/                    (shared downloads)
├── settings.toml             ({active_channel = "stable-1.0.0"})
└── bin/touring               (symlink to active toolchain)
```

## Per-project override

```toml
# <project>/.touring/touring.toml
[toolchain]
channel = "nightly-2026-05-11"   # overrides ~/.touring/settings.toml
```
