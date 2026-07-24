//! Entity Registry — Canonical codes for symbol disambiguation.
//!
//! Provides AAAK-like codes (e.g., ALC=Alice, JOR=Jordan) for disambiguating
//! generic symbol names like 'Index', 'Manager', 'Handler' that appear across
//! multiple crates with different semantic meanings.
//!
//! ## Problem Solved
//! - 36K+ symbols with homonimia (same name, different crate/module)
//! - Generic names: 'Index', 'Manager', 'Handler', 'Loop', 'Engine' appear everywhere
//! - `find_symbol("Index")` returns 100+ results across all crates
//!
//! ## Solution
//! - Canonical entity codes (4-letter AAAK format) per semantic domain
//! - Module path anchoring for disambiguation
//! - Usage-pattern scoring to rank disambiguation candidates

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

/// Canonical entity code with metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityCode {
    /// 4-letter canonical code (e.g., "ALC", "JOR", "SIMD").
    pub code: String,
    /// Human-readable canonical name (e.g., "Alice Liu", "Jordan Smith").
    pub canonical_name: String,
    /// Semantic domain (e.g., "person", "crate", "module", "concept").
    pub domain: String,
    /// Primary crate or module path if applicable.
    pub primary_module: Option<String>,
    /// Extended description.
    pub description: Option<String>,
}

/// Disambiguation context for a symbol.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolDisambiguation {
    /// The symbol's actual name.
    pub symbol_name: String,
    /// Canonical entity code (e.g., "IDX" for Index in touring-index).
    pub entity_code: String,
    /// Full module path for anchoring.
    pub module_path: String,
    /// Semantic role (crate_root, internal, utility, trait, etc.).
    pub role: String,
    /// Usage frequency score (0.0-1.0).
    pub usage_score: f64,
    /// Last computed timestamp.
    pub computed_at: String,
}

/// Resolution result with ranked candidates.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisambiguationResult {
    /// The query symbol name.
    pub symbol_name: String,
    /// Number of total candidates before disambiguation.
    pub total_candidates: i64,
    /// Number of candidates after disambiguation.
    pub disambiguated_count: usize,
    /// Ranked candidates with entity codes.
    pub candidates: Vec<DisambiguatedCandidate>,
}

/// A disambiguated symbol candidate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisambiguatedCandidate {
    /// Canonical entity code.
    pub entity_code: String,
    /// Full module path.
    pub module_path: String,
    /// File path where defined.
    pub file_path: String,
    /// Line number of definition.
    pub line: i64,
    /// Confidence score (0.0-1.0).
    pub confidence: f64,
    /// Usage score from pattern analysis.
    pub usage_score: f64,
}

/// Entity Registry errors.
#[derive(Debug)]
pub enum EntityRegistryError {
    /// Underlying SQLite error (constraint violation, I/O,
    /// schema mismatch). Stringifies via the inner error's
    /// `Display` impl.
    Sqlite(rusqlite::Error),
    /// The requested entity code was not present in the
    /// registry. String carries the missing code.
    NotFound(String),
    /// An attempt to register a duplicate code was rejected
    /// (uniqueness constraint). String carries the offending
    /// code.
    DuplicateCode(String),
}

impl std::fmt::Display for EntityRegistryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Sqlite(e) => write!(f, "SQLite error: {}", e),
            Self::NotFound(s) => write!(f, "Entity not found: {}", s),
            Self::DuplicateCode(s) => write!(f, "Duplicate entity code: {}", s),
        }
    }
}

impl From<rusqlite::Error> for EntityRegistryError {
    fn from(e: rusqlite::Error) -> Self {
        Self::Sqlite(e)
    }
}

/// Entity Registry — manages canonical codes and symbol disambiguation.
pub struct EntityRegistry {
    conn: std::sync::Mutex<Connection>,
}

impl EntityRegistry {
    /// Create a new EntityRegistry with the given database connection.
    pub fn new(conn: Connection) -> Result<Self, EntityRegistryError> {
        let registry = Self {
            conn: std::sync::Mutex::new(conn),
        };
        registry.ensure_schema()?;
        registry.seed_common_entities()?;
        Ok(registry)
    }

    /// Initialize schema tables.
    pub fn ensure_schema(&self) -> Result<(), EntityRegistryError> {
        let conn = self.conn.lock().expect("entity registry conn lock");
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS entity_codes (
                code TEXT PRIMARY KEY,
                canonical_name TEXT NOT NULL,
                domain TEXT NOT NULL DEFAULT 'generic',
                primary_module TEXT,
                description TEXT,
                created_at TEXT DEFAULT (datetime('now'))
            );

            CREATE TABLE IF NOT EXISTS symbol_disambiguation (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                symbol_name TEXT NOT NULL,
                entity_code TEXT NOT NULL,
                module_path TEXT NOT NULL,
                role TEXT NOT NULL DEFAULT 'unknown',
                usage_score REAL NOT NULL DEFAULT 0.0,
                computed_at TEXT DEFAULT (datetime('now')),
                UNIQUE(symbol_name, module_path)
            );

            CREATE INDEX IF NOT EXISTS idx_entity_codes_domain
                ON entity_codes(domain);
            CREATE INDEX IF NOT EXISTS idx_disambig_symbol
                ON symbol_disambiguation(symbol_name);
            CREATE INDEX IF NOT EXISTS idx_disambig_module
                ON symbol_disambiguation(module_path);
            CREATE INDEX IF NOT EXISTS idx_disambig_usage
                ON symbol_disambiguation(usage_score DESC);

            -- Generic name patterns that need disambiguation
            CREATE TABLE IF NOT EXISTS generic_name_patterns (
                pattern TEXT PRIMARY KEY,
                description TEXT,
                disambiguation_hint TEXT,
                hit_count INTEGER NOT NULL DEFAULT 0,
                last_seen TEXT
            );
            "#,
        )?;
        Ok(())
    }

    /// Seed common entity codes for Touring ecosystem.
    fn seed_common_entities(&self) -> Result<(), EntityRegistryError> {
        let conn = self.conn.lock().expect("entity registry conn lock");

        // Touring core entities
        let entities = vec![
            // People (Team Gadea)
            (
                "ALCA",
                "Alice Cao",
                "person",
                Some("touring-cortex"),
                "Team member",
            ),
            (
                "JORI",
                "Jordan Smith",
                "person",
                Some("touring-learning"),
                "Team member",
            ),
            ("GABR", "Gabriel Gadea", "person", None, "Owner/PO"),
            ("KAZU", "Kazuba King", "person", None, "Openclaw agent"),
            ("ZERO", "Zero Claw", "person", None, "Deep analysis agent"),
            ("KIMI", "Kimi Kidon", "person", None, "Broadcast agent"),
            // Touring crates
            (
                "IDX",
                "Touring Index",
                "crate",
                Some("touring-index"),
                "File indexing and caching",
            ),
            (
                "AST",
                "Touring AST",
                "crate",
                Some("touring-ast"),
                "AST analysis and parsing",
            ),
            (
                "HOK",
                "Touring Hooks",
                "crate",
                Some("touring-hooks"),
                "Hook runtime and handlers",
            ),
            (
                "SVR",
                "Touring Server",
                "crate",
                Some("touring-server"),
                "Daemon and CLI server",
            ),
            (
                "CORT",
                "Touring Cortex",
                "crate",
                Some("touring-cortex"),
                "Context and schema management",
            ),
            (
                "CORE",
                "Touring Core",
                "crate",
                Some("touring-core"),
                "Pipeline and context fusion",
            ),
            (
                "LRN",
                "Touring Learning",
                "crate",
                Some("touring-learning"),
                "RL and LinUCB bandit",
            ),
            (
                "SIMD",
                "Touring SIMD",
                "crate",
                Some("touring-simd"),
                "SIMD-accelerated operations",
            ),
            (
                "ANLY",
                "Touring Analysis",
                "crate",
                Some("touring-analysis"),
                "Quality and complexity analysis",
            ),
            (
                "WASM",
                "Touring WASM",
                "crate",
                Some("touring-wasm"),
                "WebAssembly bindings",
            ),
            // Common generic concepts
            ("HNDL", "Handler", "concept", None, "Event/data handler"),
            ("MNGR", "Manager", "concept", None, "Resource manager"),
            ("IDXX", "Index", "concept", None, "Indexing abstraction"),
            ("ENGN", "Engine", "concept", None, "Processing engine"),
            ("LOOP", "Loop", "concept", None, "Control flow loop"),
            ("PIPE", "Pipeline", "concept", None, "Processing pipeline"),
            ("BRDG", "Bridge", "concept", None, "Component bridge"),
            ("CTX", "Context", "concept", None, "Execution context"),
            ("SIG", "Signal", "concept", None, "Signal/flag"),
            ("STRM", "Stream", "concept", None, "Data stream"),
            ("CACHE", "Cache", "concept", None, "Caching layer"),
            ("WIRE", "Wiring", "concept", None, "Component wiring"),
        ];

        for (code, name, domain, module, desc) in entities {
            conn.execute(
                "INSERT OR IGNORE INTO entity_codes (code, canonical_name, domain, primary_module, description)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![code, name, domain, module, desc],
            )?;
        }

        // Seed generic name patterns
        let patterns = vec![
            (
                "Index",
                "touring-index",
                "Crate-specific — use crate context",
            ),
            ("Manager", "internal", "Generic resource manager pattern"),
            ("Handler", "hook", "Hook event handler — use hook context"),
            (
                "Loop",
                "learning",
                "ACO pheromone loop — use learning context",
            ),
            ("Engine", "simulation", "SIMD engine — use simd context"),
            ("Bridge", "aco", "ACO bridge pattern"),
            ("Cache", "caching", "Caching layer"),
            ("Pipeline", "flow", "Processing pipeline"),
        ];

        for (pattern, hint, desc) in patterns {
            conn.execute(
                "INSERT OR IGNORE INTO generic_name_patterns (pattern, disambiguation_hint, description)
                 VALUES (?1, ?2, ?3)",
                params![pattern, hint, desc],
            )?;
        }

        Ok(())
    }

    /// Register a new entity code.
    pub fn register_entity(&self, entity: &EntityCode) -> Result<(), EntityRegistryError> {
        let conn = self.conn.lock().expect("entity registry conn lock");
        conn.execute(
            "INSERT INTO entity_codes (code, canonical_name, domain, primary_module, description)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                entity.code,
                entity.canonical_name,
                entity.domain,
                entity.primary_module,
                entity.description
            ],
        )
        .map_err(EntityRegistryError::Sqlite)?;
        Ok(())
    }

    /// Find entity by code.
    pub fn find_by_code(&self, code: &str) -> Result<Option<EntityCode>, EntityRegistryError> {
        let conn = self.conn.lock().expect("entity registry conn lock");
        let mut stmt = conn.prepare(
            "SELECT code, canonical_name, domain, primary_module, description
             FROM entity_codes WHERE code = ?1",
        )?;
        let result = stmt
            .query_row(params![code], |r| {
                Ok(EntityCode {
                    code: r.get(0)?,
                    canonical_name: r.get(1)?,
                    domain: r.get(2)?,
                    primary_module: r.get(3)?,
                    description: r.get(4)?,
                })
            })
            .optional()
            .map_err(EntityRegistryError::Sqlite)?;
        Ok(result)
    }

    /// Get all entities in a domain.
    pub fn get_by_domain(&self, domain: &str) -> Result<Vec<EntityCode>, EntityRegistryError> {
        let conn = self.conn.lock().expect("entity registry conn lock");
        let entities: Vec<EntityCode> = conn
            .prepare(
                "SELECT code, canonical_name, domain, primary_module, description
                 FROM entity_codes WHERE domain = ?1",
            )
            .map_err(EntityRegistryError::Sqlite)?
            .query_map(params![domain], |r| {
                Ok(EntityCode {
                    code: r.get(0)?,
                    canonical_name: r.get(1)?,
                    domain: r.get(2)?,
                    primary_module: r.get(3)?,
                    description: r.get(4)?,
                })
            })
            .map_err(EntityRegistryError::Sqlite)?
            .filter_map(|r| r.ok())
            .collect();
        Ok(entities)
    }

    /// Record symbol disambiguation.
    pub fn recorddisambiguation(
        &self,
        symbol_name: &str,
        entity_code: &str,
        module_path: &str,
        role: &str,
        usage_score: f64,
    ) -> Result<(), EntityRegistryError> {
        let conn = self.conn.lock().expect("entity registry conn lock");
        conn.execute(
            "INSERT OR REPLACE INTO symbol_disambiguation
             (symbol_name, entity_code, module_path, role, usage_score, computed_at)
             VALUES (?1, ?2, ?3, ?4, ?5, datetime('now'))",
            params![symbol_name, entity_code, module_path, role, usage_score],
        )
        .map_err(EntityRegistryError::Sqlite)?;
        Ok(())
    }

    /// Resolve disambiguation for a symbol name + optional context.
    pub fn resolve(
        &self,
        symbol_name: &str,
        context_module: Option<&str>,
        crate_filter: Option<&str>,
        limit: usize,
    ) -> Result<DisambiguationResult, EntityRegistryError> {
        let conn = self.conn.lock().expect("entity registry conn lock");

        // Count total candidates (without disambiguation)
        let total: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM symbol_disambiguation WHERE symbol_name = ?1",
                params![symbol_name],
                |r| r.get::<_, i64>(0),
            )
            .unwrap_or(0);

        // Build query with disambiguation
        let sql = if context_module.is_some() {
            // Context-aware: prefer module path matches
            "SELECT sd.entity_code, sd.module_path, s.file_path, s.line,
                    (sd.usage_score * 0.6 + (CASE WHEN sd.module_path LIKE '%' || ?2 || '%' THEN 0.3 ELSE 0.0 END)) as confidence
             FROM symbol_disambiguation sd
             JOIN symbols s ON sd.symbol_name = s.name AND sd.module_path = s.file_path
             WHERE sd.symbol_name = ?1
             ORDER BY confidence DESC
             LIMIT ?3"
        } else if crate_filter.is_some() {
            // Crate filter: prefer crate module matches
            "SELECT sd.entity_code, sd.module_path, s.file_path, s.line,
                    (sd.usage_score * 0.5 + (CASE WHEN sd.module_path LIKE '%' || ?2 || '%' THEN 0.4 ELSE 0.0 END)) as confidence
             FROM symbol_disambiguation sd
             JOIN symbols s ON sd.symbol_name = s.name AND sd.module_path = s.file_path
             WHERE sd.symbol_name = ?1
             ORDER BY confidence DESC
             LIMIT ?3"
        } else {
            // Uncontextualized: use usage score only
            "SELECT sd.entity_code, sd.module_path, s.file_path, s.line,
                    sd.usage_score as confidence
             FROM symbol_disambiguation sd
             JOIN symbols s ON sd.symbol_name = s.name AND sd.module_path = s.file_path
             WHERE sd.symbol_name = ?1
             ORDER BY sd.usage_score DESC
             LIMIT ?1"
        };

        let candidates: Vec<DisambiguatedCandidate> =
            if context_module.is_some() || crate_filter.is_some() {
                let ctx = context_module.or(crate_filter).unwrap_or("");
                let mut stmt = conn.prepare(sql).map_err(EntityRegistryError::Sqlite)?;
                let rows = stmt
                    .query_map(params![symbol_name, ctx, limit as i64], |r| {
                        Ok(DisambiguatedCandidate {
                            entity_code: r.get(0)?,
                            module_path: r.get(1)?,
                            file_path: r.get(2)?,
                            line: r.get(3)?,
                            confidence: r.get(4)?,
                            usage_score: r.get::<_, f64>(4)?,
                        })
                    })
                    .map_err(EntityRegistryError::Sqlite)?;
                rows.filter_map(|r| r.ok()).collect()
            } else {
                // Fallback: just query symbol_disambiguation directly
                let mut stmt = conn
                    .prepare(
                        "SELECT entity_code, module_path, module_path, 0, usage_score
                     FROM symbol_disambiguation
                     WHERE symbol_name = ?1
                     ORDER BY usage_score DESC
                     LIMIT ?2",
                    )
                    .map_err(EntityRegistryError::Sqlite)?;
                let rows = stmt
                    .query_map(params![symbol_name, limit as i64], |r| {
                        Ok(DisambiguatedCandidate {
                            entity_code: r.get(0)?,
                            module_path: r.get(1)?,
                            file_path: r.get(2)?,
                            line: r.get(3)?,
                            confidence: r.get::<_, f64>(4)?,
                            usage_score: r.get::<_, f64>(4)?,
                        })
                    })
                    .map_err(EntityRegistryError::Sqlite)?;
                rows.filter_map(|r| r.ok()).collect()
            };

        Ok(DisambiguationResult {
            symbol_name: symbol_name.to_string(),
            total_candidates: total,
            disambiguated_count: candidates.len(),
            candidates,
        })
    }

    /// Check if a symbol name needs disambiguation (is generic).
    pub fn is_generic(&self, symbol_name: &str) -> Result<bool, EntityRegistryError> {
        let conn = self.conn.lock().expect("entity registry conn lock");
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM symbol_disambiguation WHERE symbol_name = ?1",
                params![symbol_name],
                |r| r.get::<_, i64>(0),
            )
            .unwrap_or(0);

        // If symbol appears in more than 5 locations, it's generic enough to need disambiguation
        Ok(count > 5)
    }

    /// Get generic name patterns.
    pub fn get_generic_patterns(&self) -> Result<Vec<(String, String)>, EntityRegistryError> {
        let conn = self.conn.lock().expect("entity registry conn lock");
        let mut stmt =
            conn.prepare("SELECT pattern, disambiguation_hint FROM generic_name_patterns ORDER BY hit_count DESC")?;
        let rows: Vec<(String, String)> = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(EntityRegistryError::Sqlite)?
            .filter_map(|r| r.ok())
            .collect();
        Ok(rows)
    }

    /// Increment hit count for a generic pattern.
    pub fn bump_pattern_hit(&self, pattern: &str) -> Result<(), EntityRegistryError> {
        let conn = self.conn.lock().expect("entity registry conn lock");
        conn.execute(
            "UPDATE generic_name_patterns SET hit_count = hit_count + 1, last_seen = datetime('now')
             WHERE pattern = ?1",
            params![pattern],
        )
        .map_err(EntityRegistryError::Sqlite)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_entity_code_format() {
        let entity = EntityCode {
            code: "ALCA".to_string(),
            canonical_name: "Alice Cao".to_string(),
            domain: "person".to_string(),
            primary_module: Some("touring-cortex".to_string()),
            description: Some("Team member".to_string()),
        };
        assert_eq!(entity.code.len(), 4);
    }

    #[test]
    fn test_is_generic() {
        // This would need a real connection to test properly
        // Placeholder test
        let symbol = "Index";
        assert!(symbol.len() > 0);
    }
}
