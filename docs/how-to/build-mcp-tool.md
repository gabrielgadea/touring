# How to build an MCP tool

> A **how-to** (Diátaxis): task-oriented. You want to expose a new capability to
> MCP clients (editors, agents). Master Plan D.W4.P3. For the catalog of existing
> tools see `docs/reference/mcp-tools.md`.

## Goal

Add a tool to the MCP bridge so it appears to clients connected via
`touring serve` and can be invoked like the existing `touring_*` tools.

## Context: CLI vs MCP

Touring exposes capability through two channels (see
`docs/explanation/architecture.md`):

| Channel | Latency | Use for |
|---|---|---|
| CLI (`touring …`) | <10ms | read-only queries |
| MCP (`mcp__touring__*`) | ~200ms | write operations + structured tool-calls |

An MCP tool is the right choice when an external client needs a structured,
schema'd operation — not for a read you could do with a CLI subcommand.

## Steps

1. **Survey the existing tools** to match conventions (naming, schema shape,
   the `_next_tools` hints used by the token-efficient workflow):
   ```bash
   # the generated catalog (161 tools)
   sed -n '1,40p' docs/reference/mcp-tools.md
   ```

2. **Define the tool** in the MCP bridge layer (`touring-server`, the MCP/serve
   path): its name (`touring_<verb>`), input JSON schema, and handler. Reuse the
   daemon RPC — the tool is a thin schema'd wrapper over an existing capability,
   not a reimplementation.

3. **Wire it into the dispatch** so `touring serve` advertises it in the tool
   list and routes calls to your handler. Keep the handler fail-safe: structured
   error out, never panic the bridge.

4. **Rebuild and reconnect:**
   ```bash
   update-touring
   ```
   In an open editor session, reload MCP (e.g. the `/mcp` dialog) so the client
   re-reads the tool list.

## Verify

```bash
# The tool appears in the generated reference after regeneration
python3 docs/gen_reference.py
grep '<your_tool>' docs/reference/mcp-tools.md

# Anti-drift gate stays green (code ↔ docs in sync)
python3 docs/gen_reference.py --validate
```

From a connected client, the new `mcp__touring__<verb>` should be listed and
return the structured result your schema declares.

## Pitfall: headless sessions

Interactively-authenticated MCP servers may be absent in headless/cron runs.
Design the tool so its capability is *also* reachable via the CLI/daemon path,
so automation that cannot use MCP is not locked out.
