# 0001 — touring-web dashboard binds loopback by default

- Status: accepted
- Date: 2026-06-21
- Deciders: Gabriel Gadea (via the `.full-review` SEC-02 finding)

## Context

`touring-web-server` (the `bind-web` feature, default-off) serves a dashboard +
an `/api/mcp/call` endpoint that can run whitelisted `touring` subcommands
server-side and read codebase/RL state. It previously:

- bound `0.0.0.0:3000` (every interface, reachable from the LAN), and
- applied `CorsLayer::allow_origin(Any)`,

with **no authentication**. On a shared network this let any unauthenticated peer
(or any web page in the user's browser, via the permissive CORS) drive the API.
The MCP server proper is stdio-only and unaffected; this is purely the optional
web binary.

## Decision

1. **Bind loopback (`127.0.0.1:3000`) by default.** The address is overridable
   via `TOURING_WEB_BIND` for operators who deliberately want LAN exposure; a
   **loud warning** fires whenever a non-loopback address is chosen.
2. **Restrict CORS to localhost origins** via an `AllowOrigin::predicate` that
   matches `http://localhost`/`http://127.0.0.1`/`http://[::1]` with an exact
   host-boundary check (rejecting prefix-injection like `http://localhost.evil.com`).
3. **Defense-in-depth headers**: `X-Frame-Options: DENY` (anti-clickjacking, valid
   even on loopback) + `X-Content-Type-Options: nosniff`. CSP is intentionally
   omitted — the dashboard's inline assets would break under a strict policy.

Authentication (a bearer token) is **not** added now: with a loopback default and
a localhost-only CORS allowlist, the network attack surface is closed for the
common case. A token gate remains the recommended follow-up for the explicit
`TOURING_WEB_BIND=0.0.0.0` opt-in path.

## Consequences

- **Positive:** the dangerous default is eliminated; the dashboard is private by
  default; clickjacking and cross-origin reads are blocked. The bind/CORS logic is
  extracted into `resolve_web_bind` / `is_localhost_origin` pure functions with
  unit tests (incl. a prefix-injection regression).
- **Negative / trade-off:** users who relied on LAN access must now set
  `TOURING_WEB_BIND` explicitly (and heed the warning). The opt-in LAN path is
  still unauthenticated until the follow-up token gate lands.

## References

- `.full-review/02a-security.md` (SEC-02), `.full-review/06-goal-implementation.md` (F-3)
- `crates/touring-bindings/src/web/server/mod.rs` — `resolve_web_bind`, `is_localhost_origin`, `build_app`, `run`
