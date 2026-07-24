# touring-geopostgis -- Architecture

> **Version**: v30.3.5 | **Updated**: 2026-04-30 | **Tests**: 10+ | **LOC**: 435

## Overview

PostGIS EWKB bridge for Touring. Decodes PostGIS `bytea` columns into `geo_types::Geometry<f64>` and encodes geometries back to EWKB for inserts. Provides two API styles: blocking (postgres crate) and native async (sqlx). Explicitly avoids rusqlite/sqlx conflicts by using `geozero` as the serialization layer with `postgres` for sync and `sqlx` only for async-sqlx.

## Key Types

| Type | File | Purpose |
|------|------|---------|
| `PostgisError` | lib.rs | Enum: Connection / Query / Decode / Encode |
| `decode_ewkb` | lib.rs | EWKB bytes -> geo_types::Geometry via geozero processor |
| `encode_ewkb` | lib.rs | geo_types::Geometry -> EWKB bytes |
| `encode_ewkb_srid` | lib.rs | Encode with explicit SRID in EWKB header |
| `decode_ewkb_with_srid` | lib.rs | Decode EWKB + extract SRID from EWKB header bits |
| `read_geometry_sync` | lib.rs | Single-row geometry via blocking postgres client |
| `read_geometries_sync` | lib.rs | Multi-row geometry via blocking postgres client |
| `insert_geometry_sync` | lib.rs | Insert geometry via postgres `ToSql` |
| `read_geometry_async` | lib.rs | Async read via `tokio::spawn_blocking` (boxed params) |
| `read_geometries_async` | lib.rs | Async multi-row via `spawn_blocking` |
| `insert_geometry_async` | lib.rs | Async insert via `spawn_blocking` |
| `parse_ewkt` | lib.rs | Parse EWKT string (SRID=XXXX;WKT) -> Geometry |
| `geometry_to_ewkt` | lib.rs | Geometry -> EWKT string with SRID prefix |
| `sqlx_api::create_pool` | lib.rs | Create sqlx PgPool with 16 max connections |
| `sqlx_api::read_geometry` | lib.rs | Native async read via sqlx (no spawn_blocking, compile-time query check) |
| `sqlx_api::insert_geometry` | lib.rs | Native async insert via sqlx |
| `sqlx_api::health_check` | lib.rs | Pool connectivity probe |

## Dependencies

| Crate | Why |
|-------|-----|
| `geozero` | EWKB/WKB/WKT processing -- `with-geo`, `with-wkb`, `with-wkt`, `with-postgis-postgres` |
| `geo-types` | Geometry<f64> representation (Point, Polygon, etc.) |
| `tokio` | Async runtime for spawn_blocking |
| `postgres` (opt) | Blocking PostgreSQL driver -- avoids rusqlite conflict |
| `sqlx` (opt) | Native async driver -- only via `async-sqlx` feature |
| `thiserror` | Error enum |

## Feature Flags

| Feature | Default | Effect |
|---------|---------|--------|
| `sync-api` | Yes | Sync API via `postgres` crate (no sqlite conflicts) |
| `async-sqlx` | No | Native async API via `sqlx` with compile-time query verification |

**Note**: `async-sqlx` pulls in `geozero/with-postgis-sqlx` which pins a compatible `sqlx ~0.8`. Do NOT pin a separate `sqlx` version -- it will conflict.

## Key Modules

| Module | Purpose |
|--------|---------|
| `sqlx_api` (cfg feature) | Zero-unsafe async API using sqlx native async |

## EWKB Structure

```
[byte_order: u8 | type_id: u32 | optional_srid: i32 | coordinates...]
```

The SRID is embedded when `type_id & 0x2000_0000 != 0` (PostGIS EWKB flag).

## Sync vs Async API Trade-off

| Aspect | Sync (postgres) | Async (sqlx) |
|--------|----------------|--------------|
| Unsafe transmute | Required for boxed params | None |
| Query verification | Runtime only | Compile-time |
| Connection pool | Manual | Built-in PgPool |
| Thread blocking | Yes | No |

## Invariants

1. `decode_ewkb_with_srid` requires a minimum 9-byte buffer (byte_order + type_id + at least 1 coordinate).
2. The SRID flag bit `0x2000_0000` on `type_id` controls whether bytes[5..9] contain the SRID value.
3. `spawn_blocking` async wrappers soundly transmute `Box<ToSql+Sync+Send>` -> `Box<ToSql+Sync>` because the Send vtable is a superset of Sync requirements for the postgres crate.
4. `sqlx_api` is entirely separate from the `spawn_blocking` path -- zero shared state between them.

## Tests

10+ tests (inline in lib.rs). Run with:

```bash
cargo test -p touring-geopostgis
```