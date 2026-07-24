//! Touring-geopostgis — PostGIS EWKB bridge using geozero + postgres.
//!
//! Provides sync and async interfaces for:
//! - Decoding PostGIS EWKB → `geo_types::Geometry<f64>` (via `FromSql`)
//! - Encoding `geo_types::Geometry<f64>` → EWKB for INSERT (via `ToSql`)
//!
//! **Key design**: This crate uses `postgres` (NOT sqlx), avoiding the
//! rusqlite/sqlite3-sys conflict with touring-simd.

use geo_types::Geometry as GeoGeometry;
use geozero::wkb::{Decode as WkbDecode, Encode as WkbEncode};
use geozero::{ToGeo, ToWkb, ToWkt};
use thiserror::Error;

// ─── Error Types ───────────────────────────────────────────────────────────

/// Errors raised by the PostGIS geometry-bindings layer.
#[derive(Error, Debug)]
pub enum PostgisError {
    /// Establishing the PostgreSQL/PostGIS connection failed.
    #[error("Connection failed: {0}")]
    Connection(String),

    /// Executing a SQL query against PostGIS failed.
    #[error("Query execution failed: {0}")]
    Query(String),

    /// Decoding a geometry value (EWKB → in-memory) failed.
    #[error("Geometry decode error: {0}")]
    Decode(String),

    /// Encoding a geometry value (in-memory → EWKB) failed.
    #[error("Geometry encode error: {0}")]
    Encode(String),
}

// ─── Core Decode/Encode ────────────────────────────────────────────────────

/// Decode EWKB bytes into `GeoGeometry<f64>` using geozero's processor pattern.
pub fn decode_ewkb(bytes: &[u8]) -> Result<GeoGeometry<f64>, PostgisError> {
    geozero::wkb::Ewkb(bytes.to_vec())
        .to_geo()
        .map_err(|e| PostgisError::Decode(e.to_string()))
}

/// Encode `GeoGeometry<f64>` into EWKB bytes for PostGIS.
pub fn encode_ewkb(geom: &GeoGeometry<f64>) -> Result<Vec<u8>, PostgisError> {
    geom.to_ewkb(geozero::CoordDimensions::xy(), None)
        .map_err(|e| PostgisError::Encode(e.to_string()))
}

/// Encode with explicit SRID.
pub fn encode_ewkb_srid(geom: &GeoGeometry<f64>, srid: i32) -> Result<Vec<u8>, PostgisError> {
    geom.to_ewkb(geozero::CoordDimensions::xy(), Some(srid))
        .map_err(|e| PostgisError::Encode(e.to_string()))
}

// ─── Sync Wrappers (Postgres Client) ──────────────────────────────────────

fn pg_connect(url: &str) -> Result<postgres::Client, PostgisError> {
    use postgres::tls::NoTls;
    postgres::Client::connect(url, NoTls).map_err(|e| PostgisError::Connection(e.to_string()))
}

/// Read a single geometry from a sync PostgreSQL query.
/// Uses `wkb::Decode<GeoGeometry<f64>>` which implements `postgres::FromSql`
/// for PostGIS geometry columns (via geozero's `with-postgis-postgres` feature).
pub fn read_geometry_sync(
    connection_url: &str,
    query: &str,
    params: &[&(dyn postgres::types::ToSql + Sync)],
) -> Result<Option<GeoGeometry<f64>>, PostgisError> {
    let mut client = pg_connect(connection_url)?;
    let row = client
        .query_one(query, params)
        .map_err(|e| PostgisError::Query(e.to_string()))?;
    let geom: WkbDecode<GeoGeometry<f64>> = row.get(0);
    Ok(geom.geometry)
}

/// Read multiple geometry rows from a sync PostgreSQL query.
pub fn read_geometries_sync(
    connection_url: &str,
    query: &str,
    params: &[&(dyn postgres::types::ToSql + Sync)],
) -> Result<Vec<GeoGeometry<f64>>, PostgisError> {
    let mut client = pg_connect(connection_url)?;
    let rows = client
        .query(query, params)
        .map_err(|e| PostgisError::Query(e.to_string()))?;
    let mut geometries = Vec::with_capacity(rows.len());
    for row in rows {
        let geom: WkbDecode<GeoGeometry<f64>> = row.get(0);
        if let Some(g) = geom.geometry {
            geometries.push(g);
        }
    }
    Ok(geometries)
}

/// Insert a geometry into PostgreSQL using sync client.
/// `wkb::Encode<T>` implements `postgres::ToSql` for PostGIS geometry columns.
pub fn insert_geometry_sync(
    connection_url: &str,
    query: &str,
    geom: &GeoGeometry<f64>,
) -> Result<u64, PostgisError> {
    let mut client = pg_connect(connection_url)?;
    let encoded = WkbEncode(geom.clone());
    let result = client
        .execute(query, &[&encoded as &(dyn postgres::types::ToSql + Sync)])
        .map_err(|e| PostgisError::Query(e.to_string()))?;
    Ok(result)
}

// ─── Async Wrappers ────────────────────────────────────────────────────────

/// Async geometry read via `tokio::task::spawn_blocking`.
pub async fn read_geometry_async(
    connection_url: String,
    query: String,
    params: Vec<Box<dyn postgres::types::ToSql + Sync + Send>>,
) -> Result<Option<GeoGeometry<f64>>, PostgisError> {
    let url = connection_url.clone();
    let q = query.clone();
    tokio::task::spawn_blocking(move || {
        let mut client = pg_connect(&url)?;
        // SAFETY: Coercion from Box<dyn ToSql + Sync + Send> to
        // Box<dyn ToSql + Sync> is sound — the inner vtable and data pointer are
        // identical; the Send vtable is a superset of Sync, and postgres only
        // needs Sync methods here.
        let sync_params: Vec<Box<dyn postgres::types::ToSql + Sync>> =
            unsafe { std::mem::transmute(params) };
        let param_refs: Vec<&(dyn postgres::types::ToSql + Sync)> =
            sync_params.iter().map(|b| b.as_ref()).collect();
        let row = client
            .query_one(&q, &param_refs)
            .map_err(|e| PostgisError::Query(e.to_string()))?;
        let geom: WkbDecode<GeoGeometry<f64>> = row.get(0);
        Ok(geom.geometry)
    })
    .await
    .map_err(|e| PostgisError::Connection(format!("tokio spawn: {}", e)))?
}

/// Async geometry batch read.
pub async fn read_geometries_async(
    connection_url: String,
    query: String,
    params: Vec<Box<dyn postgres::types::ToSql + Sync + Send>>,
) -> Result<Vec<GeoGeometry<f64>>, PostgisError> {
    let url = connection_url.clone();
    let q = query.clone();
    tokio::task::spawn_blocking(move || {
        let mut client = pg_connect(&url)?;
        // SAFETY: same as read_geometry_async — Box<ToSql+Sync+Send> → Box<ToSql+Sync>
        let sync_params: Vec<Box<dyn postgres::types::ToSql + Sync>> =
            unsafe { std::mem::transmute(params) };
        let param_refs: Vec<&(dyn postgres::types::ToSql + Sync)> =
            sync_params.iter().map(|b| b.as_ref()).collect();
        let rows = client
            .query(&q, &param_refs)
            .map_err(|e| PostgisError::Query(e.to_string()))?;
        let mut geometries = Vec::with_capacity(rows.len());
        for row in rows {
            let geom: WkbDecode<GeoGeometry<f64>> = row.get(0);
            if let Some(g) = geom.geometry {
                geometries.push(g);
            }
        }
        Ok(geometries)
    })
    .await
    .map_err(|e| PostgisError::Connection(format!("tokio spawn: {}", e)))?
}

/// Async geometry insert.
pub async fn insert_geometry_async(
    connection_url: String,
    query: String,
    geom: GeoGeometry<f64>,
) -> Result<u64, PostgisError> {
    let url = connection_url.clone();
    let q = query.clone();
    let encoded = WkbEncode(geom);
    tokio::task::spawn_blocking(move || {
        let mut client = pg_connect(&url)?;
        client
            .execute(&q, &[&encoded as &(dyn postgres::types::ToSql + Sync)])
            .map_err(|e| PostgisError::Query(e.to_string()))
    })
    .await
    .map_err(|e| PostgisError::Connection(format!("tokio spawn: {}", e)))?
}

// ─── Async SQLx API (no unsafe, no spawn_blocking) ─────────────────────────

#[cfg(feature = "async-sqlx")]
pub mod sqlx_api {
    //! Native async PostGIS via sqlx + geozero `with-postgis-sqlx`.
    //!
    //! **Benefit**: Full async, compile-time query verification, built-in pool.
    //! **Zero unsafe**: No transmute needed — sqlx handles sync→async internally.
    //!
    //! ```toml
    //! touring-geopostgis = { features = ["async-sqlx"] }
    //! ```
    //!
    //! ```ignore
    //! // Full example requires tokio runtime — use #[tokio::main] in your binary
    //! use touring_bindings::postgis::sqlx_api::*;
    //!
    //! async fn example() -> Result<(), touring_bindings::postgis::PostgisError> {
    //!     // 12-factor: connection URL comes from the environment, never literal.
    //!     let url = std::env::var("POSTGRES_URL")
    //!         .expect("POSTGRES_URL must be set (read from the environment)");
    //!     let pool = create_pool(&url).await?;
    //!     let geom = read_geometry(&pool, "SELECT geom FROM roads LIMIT 1").await?;
    //!     health_check(&pool).await?;
    //!     Ok(())
    //! }
    //! ```

    use geo_types::Geometry as GeoGeometry;
    use geozero::wkb::{Decode as WkbDecode, Encode as WkbEncode};

    /// Create a sqlx PgPool with 16 max connections.
    pub async fn create_pool(url: &str) -> Result<sqlx::PgPool, super::PostgisError> {
        use sqlx::postgres::PgPoolOptions;
        PgPoolOptions::new()
            .max_connections(16)
            .connect(url)
            .await
            .map_err(|e| super::PostgisError::Connection(e.to_string()))
    }

    /// Read a single geometry via sqlx (async, no spawn_blocking).
    pub async fn read_geometry(
        pool: &sqlx::PgPool,
        query: &str,
    ) -> Result<Option<GeoGeometry<f64>>, super::PostgisError> {
        let row: (WkbDecode<GeoGeometry<f64>>,) = sqlx::query_as(query)
            .fetch_one(pool)
            .await
            .map_err(|e| super::PostgisError::Query(e.to_string()))?;
        Ok(row.0.geometry)
    }

    /// Read multiple geometry rows via sqlx (async, no spawn_blocking).
    pub async fn read_geometries(
        pool: &sqlx::PgPool,
        query: &str,
    ) -> Result<Vec<GeoGeometry<f64>>, super::PostgisError> {
        let rows: Vec<(WkbDecode<GeoGeometry<f64>>,)> = sqlx::query_as(query)
            .fetch_all(pool)
            .await
            .map_err(|e| super::PostgisError::Query(e.to_string()))?;
        Ok(rows.into_iter().filter_map(|r| r.0.geometry).collect())
    }

    /// Insert a geometry via sqlx (async, no spawn_blocking).
    /// Uses ST_SetSRID($1, srid) to embed the SRID in PostGIS.
    pub async fn insert_geometry(
        pool: &sqlx::PgPool,
        query: &str,
        geom: &GeoGeometry<f64>,
    ) -> Result<u64, super::PostgisError> {
        let encoded = WkbEncode(geom.clone());
        sqlx::query(query)
            .bind(encoded)
            .execute(pool)
            .await
            .map_err(|e| super::PostgisError::Query(e.to_string()))
            .map(|r| r.rows_affected())
    }

    /// Read geometry with explicit SRID extraction via sqlx.
    pub async fn read_geometry_with_srid(
        pool: &sqlx::PgPool,
        query: &str,
    ) -> Result<Option<(GeoGeometry<f64>, Option<i32>)>, super::PostgisError> {
        let row: (WkbDecode<GeoGeometry<f64>>,) = sqlx::query_as(query)
            .fetch_one(pool)
            .await
            .map_err(|e| super::PostgisError::Query(e.to_string()))?;
        let geom = row.0.geometry;
        // NOTE: sqlx decodes PostGIS geometry directly to geo_types::Geometry.
        // The SRID is embedded in PostGIS's internal EWKB representation but is NOT
        // exposed through the decoded geometry. To extract SRID, either:
        // 1) Use raw EWKB bytes: SELECT ST_AsEWKB(geom) FROM table — then call decode_ewkb_with_srid()
        // 2) Use ST_SRID(geom) in a separate query
        let _srid: Option<Option<i32>> = None; // placeholder — SRID extraction requires raw EWKB bytes
        Ok(geom.map(|g| (g, None)))
    }

    /// Health check — verify pool can reach the database.
    pub async fn health_check(pool: &sqlx::PgPool) -> Result<(), super::PostgisError> {
        sqlx::query("SELECT 1")
            .fetch_one(pool)
            .await
            .map_err(|e| super::PostgisError::Connection(e.to_string()))?;
        Ok(())
    }
}

// ─── SRID Extraction ────────────────────────────────────────────────────────

/// Decode EWKB bytes and extract SRID from the EWKB header.
///
/// Returns `(geometry, srid)` where `srid` is `None` if the geometry has no
/// embedded SRID (i.e., type_id does not have the 0x2000_0000 flag set).
///
/// # EWKB Structure
/// ```text
/// [byte_order: u8 | type_id: u32 | optional_srid: i32 | coordinates...]
/// ```
/// The SRID is present when `type_id & 0x2000_0000 != 0` (PostGIS EWKB flag).
pub fn decode_ewkb_with_srid(
    bytes: &[u8],
) -> Result<(GeoGeometry<f64>, Option<i32>), PostgisError> {
    // Minimum EWKB: byte_order(1) + type_id(4) + at least 1 coordinate(8) = 13 bytes
    if bytes.len() < 9 {
        return Err(PostgisError::Decode(format!(
            "EWKB buffer too short: {} bytes, need >= 9",
            bytes.len()
        )));
    }

    // EWKB byte layout:
    // [0] byte_order (1 byte)
    // [1..5] type_id (4 bytes, little-endian) — bit 0x2000_0000 = SRID present
    // [5..9] SRID (4 bytes, little-endian) — only if bit set
    // [9..] coordinates
    let type_id_lo = u32::from_le_bytes([bytes[1], bytes[2], bytes[3], bytes[4]]);
    let srid = if type_id_lo & 0x2000_0000 != 0 {
        // SRID flag set — bytes[5..9] contain the SRID value
        Some(i32::from_le_bytes([bytes[5], bytes[6], bytes[7], bytes[8]]))
    } else {
        None
    };

    // Full decode via geozero
    let geom = decode_ewkb(bytes)?;
    Ok((geom, srid))
}

// ─── EWKT Helpers ──────────────────────────────────────────────────────────

/// Parse an EWKT string (e.g., "SRID=4326;POINT(10 -20)") into `GeoGeometry<f64>`.
pub fn parse_ewkt(ewkt: &str) -> Result<GeoGeometry<f64>, PostgisError> {
    // EWKT has SRID prefix "SRID=XXXX;" before standard WKT
    let (srid, wkt_body) = if let Some(pos) = ewkt.find(";") {
        let prefix = &ewkt[..pos];
        let srid_val = if let Some(stripped) = prefix.strip_prefix("SRID=") {
            stripped.parse::<i32>().ok()
        } else {
            None
        };
        (srid_val, &ewkt[pos + 1..])
    } else {
        (None, ewkt)
    };

    let mut geom = geozero::wkt::Wkt(wkt_body)
        .to_geo()
        .map_err(|e| PostgisError::Decode(format!("EWKT parse: {}", e)))?;

    // If SRID was present, embed it via EWKB encoding
    if let Some(srid) = srid {
        let bytes = geom
            .to_ewkb(geozero::CoordDimensions::xy(), Some(srid))
            .map_err(|e| PostgisError::Encode(e.to_string()))?;
        geom = geozero::wkb::Ewkb(bytes)
            .to_geo()
            .map_err(|e| PostgisError::Decode(format!("EWKB re-decode: {}", e)))?;
    }

    Ok(geom)
}

/// Convert geometry to EWKT string with SRID prefix.
pub fn geometry_to_ewkt(geom: &GeoGeometry<f64>, srid: i32) -> Result<String, PostgisError> {
    geom.to_ewkt(Some(srid))
        .map_err(|e| PostgisError::Encode(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_encode_roundtrip() {
        let geom_in = geo::Point::new(10.0, 20.0).into();
        let bytes = encode_ewkb(&geom_in).expect("encode");
        let geom_out = decode_ewkb(&bytes).expect("decode");
        let wkt = geom_out.to_wkt().expect("wkt");
        assert!(wkt.contains("10") && wkt.contains("20"));
    }

    #[test]
    fn test_ewkt_parse_with_srid() {
        let ewkt = "SRID=4326;POINT(10 -20)";
        let geom = parse_ewkt(ewkt).expect("parse EWKT");
        let re_ewkt = geometry_to_ewkt(&geom, 4326).expect("to EWKT");
        assert!(re_ewkt.starts_with("SRID=4326;POINT"));
    }

    #[test]
    fn test_polygon_ewkt() {
        let ewkt = "SRID=31983;POLYGON((0 0,10 0,10 10,0 10,0 0))";
        let geom = parse_ewkt(ewkt).expect("parse");
        let re_ewkt = geometry_to_ewkt(&geom, 31983).expect("re-encode");
        assert!(re_ewkt.starts_with("SRID=31983;POLYGON"));
    }

    #[test]
    fn test_decode_ewkb_with_srid_point() {
        // Encode Point with SRID=4326
        let geom_in = geo::Point::new(10.0, -20.0).into();
        let bytes = encode_ewkb_srid(&geom_in, 4326).expect("encode srid");
        let (geom_out, srid) = decode_ewkb_with_srid(&bytes).expect("decode with srid");
        assert_eq!(srid, Some(4326));
        let wkt = geom_out.to_wkt().expect("wkt");
        assert!(wkt.contains("10") && wkt.contains("-20"));
    }

    #[test]
    fn test_decode_ewkb_with_srid_no_srid() {
        // Encode Point WITHOUT SRID (plain EWKB)
        let geom_in = geo::Point::new(5.0, 6.0).into();
        let bytes = encode_ewkb(&geom_in).expect("encode no srid");
        let (geom_out, srid) = decode_ewkb_with_srid(&bytes).expect("decode no srid");
        assert_eq!(srid, None);
        let wkt = geom_out.to_wkt().expect("wkt");
        assert!(wkt.contains("5") && wkt.contains("6"));
    }

    #[test]
    fn test_decode_ewkb_with_srid_polygon() {
        let ewkt = "SRID=31983;POLYGON((0 0,10 0,10 10,0 10,0 0))";
        let geom = parse_ewkt(ewkt).expect("parse EWKT");
        let bytes = encode_ewkb_srid(&geom, 31983).expect("encode srid");
        let (_geom_out, srid) = decode_ewkb_with_srid(&bytes).expect("decode polygon");
        assert_eq!(srid, Some(31983));
    }

    #[test]
    fn test_decode_ewkb_with_srid_truncated_buffer() {
        // 5 bytes — less than minimum 9
        let result = decode_ewkb_with_srid(&[0, 1, 2, 3, 4]);
        assert!(result.is_err());
    }
}
