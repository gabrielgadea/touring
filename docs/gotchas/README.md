# Gotchas — YAML Source-of-Truth (Wave Q3)

This directory is the **canonical source** for gotcha entries.
SQLite is a derived cache populated via `touring gotcha sync`.

## Format

Every YAML file describes one gotcha:

```yaml
id: <lang>:<short-name>          # stable, unique
language: rust|python|typescript|multi-lang
pattern: <substring or regex>     # used by gotcha_match
description: <one-line summary>
severity: low|medium|high
resolution: |
  Multi-line suggestion explaining the fix.
metadata:
  introduced: YYYY-MM-DD
  references:
    - https://example.com/docs
```

## Workflow

1. **Add a new gotcha**: drop a YAML file under the appropriate subdir
2. **Sync to cache**: `touring gotcha sync`
3. **Bootstrap from existing DB**: `touring gotcha init` (one-shot export)

## Layout

```
gotchas/
├── README.md              # this file
├── _schema.json           # JSON schema for validators
├── rust/
├── python/
├── typescript/
└── multi-lang/
```
