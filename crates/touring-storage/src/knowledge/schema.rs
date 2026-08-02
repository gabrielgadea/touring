//! Schema creation and migration DDL for `FileKnowledgeDB`
//! (`ensure_schema` table creation + `migrate_schema` versioned migrations).
//!
//! Method group extracted verbatim from `knowledge.rs` (1A god-file decomposition);
//! a child-module inherent `impl` block. `ensure_schema`/`migrate_schema` are
//! `pub(super)` so the parent `knowledge` constructors (and the test module) keep calling them.

use super::*;
use touring_foundation::schema_guard;

impl FileKnowledgeDB {
    /// Create all tables if they don't exist.
    pub(super) fn ensure_schema(&self) -> Result<(), rusqlite::Error> {
        self.conn
            .execute_batch(
                &format!(
                    "CREATE TABLE IF NOT EXISTS {fk} (
                file_path TEXT PRIMARY KEY,
                language TEXT,
                line_count INTEGER DEFAULT 0,
                symbol_count INTEGER DEFAULT 0,
                read_count INTEGER DEFAULT 0,
                last_read_at TEXT,
                content_hash TEXT,
                imports_json TEXT DEFAULT '[]',
                symbols_json TEXT DEFAULT '[]',
                notes TEXT,
                created_at TEXT DEFAULT (datetime('now')),
                updated_at TEXT DEFAULT (datetime('now'))
            );

            CREATE TABLE IF NOT EXISTS {fr} (
                source_path TEXT NOT NULL,
                target_path TEXT NOT NULL,
                relation_type TEXT NOT NULL DEFAULT 'imports',
                created_at TEXT DEFAULT (datetime('now')),
                PRIMARY KEY (source_path, target_path, relation_type)
            );

            CREATE INDEX IF NOT EXISTS idx_file_relations_source
                ON {fr}(source_path);
            CREATE INDEX IF NOT EXISTS idx_file_relations_target
                ON {fr}(target_path);

            CREATE TABLE IF NOT EXISTS {fal} (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                file_path TEXT NOT NULL,
                session_id TEXT,
                accessed_at TEXT DEFAULT (datetime('now'))
            );

            CREATE INDEX IF NOT EXISTS idx_file_access_path
                ON {fal}(file_path);

            -- S-M1: Composite index for session-ordered access queries
            -- Eliminates SCAN + TEMP B-TREE for:
            --   SELECT file_path FROM file_access_log WHERE session_id = ? ORDER BY accessed_at DESC LIMIT N
            CREATE INDEX IF NOT EXISTS idx_file_access_session_time
                ON {fal}(session_id, accessed_at DESC);

            -- S-M2: Index on read_count DESC for top-N post_compact queries
            -- Eliminates full-table SCAN on file_knowledge (1252+ rows in production)
            CREATE INDEX IF NOT EXISTS idx_file_knowledge_read_count
                ON {fk}(read_count DESC);

            CREATE TABLE IF NOT EXISTS {bo} (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                command TEXT NOT NULL,
                command_short TEXT NOT NULL,
                exit_code INTEGER DEFAULT 0,
                success INTEGER DEFAULT 1,
                error_pattern TEXT,
                file_context TEXT,
                executed_at TEXT DEFAULT (datetime('now'))
            );

            CREATE INDEX IF NOT EXISTS idx_bash_outcomes_short
                ON {bo}(command_short);

            CREATE TABLE IF NOT EXISTS {eh} (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                file_path TEXT NOT NULL,
                edit_type TEXT NOT NULL DEFAULT 'edit',
                summary TEXT,
                error_pattern TEXT,
                edited_at TEXT DEFAULT (datetime('now'))
            );

            CREATE INDEX IF NOT EXISTS idx_file_edit_history_path
                ON {eh}(file_path);

            CREATE TABLE IF NOT EXISTS {fg} (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                pattern TEXT NOT NULL,
                gotcha TEXT NOT NULL,
                severity TEXT NOT NULL DEFAULT 'warning',
                symbol_name TEXT,
                hit_count INTEGER NOT NULL DEFAULT 0,
                prevented_errors INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );

            CREATE INDEX IF NOT EXISTS idx_file_gotchas_pattern
                ON {fg}(pattern);

            CREATE TABLE IF NOT EXISTS {fc} (
                source_path TEXT NOT NULL,
                target_path TEXT NOT NULL,
                coedit_count INTEGER NOT NULL DEFAULT 0,
                last_coedit_at TEXT DEFAULT (datetime('now')),
                PRIMARY KEY (source_path, target_path)
            );

            CREATE INDEX IF NOT EXISTS idx_coedits_source
                ON {fc}(source_path);
            CREATE INDEX IF NOT EXISTS idx_coedits_target
                ON {fc}(target_path);",
                    fk = schema_guard::TABLE_FILE_KNOWLEDGE, fr =
                    schema_guard::TABLE_FILE_RELATIONS, fal =
                    schema_guard::TABLE_FILE_ACCESS_LOG, bo =
                    schema_guard::TABLE_BASH_OUTCOMES, eh =
                    schema_guard::TABLE_EDIT_HISTORY, fg = schema_guard::TABLE_GOTCHAS,
                    fc = schema_guard::TABLE_FILE_COEDITS,
                ),
            )?;
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS failure_counts (
                failure_key TEXT PRIMARY KEY,
                count INTEGER NOT NULL DEFAULT 0,
                last_updated TEXT DEFAULT (datetime('now'))
            );",
        )?;
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS task_decompositions (
                task_id TEXT PRIMARY KEY,
                task_type TEXT NOT NULL,
                description TEXT NOT NULL,
                cila_level INTEGER NOT NULL DEFAULT 3,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                archived_at TEXT,
                status TEXT NOT NULL DEFAULT 'active',
                metrics TEXT
            );
            CREATE TABLE IF NOT EXISTS decomposition_subtasks (
                subtask_id TEXT PRIMARY KEY,
                task_id TEXT NOT NULL,
                description TEXT NOT NULL,
                depends_on TEXT NOT NULL DEFAULT '[]',
                priority INTEGER NOT NULL DEFAULT 255,
                status TEXT NOT NULL,
                deadline TEXT,
                deadline_behavior TEXT DEFAULT 'Fail',
                parallel_group TEXT,
                review_required INTEGER NOT NULL DEFAULT 0,
                complexity_hint TEXT,
                retry_policy TEXT,
                attempts INTEGER NOT NULL DEFAULT 0,
                quality_score REAL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                FOREIGN KEY (task_id) REFERENCES task_decompositions(task_id)
            );
            CREATE INDEX IF NOT EXISTS idx_task_status ON task_decompositions(status);
            CREATE INDEX IF NOT EXISTS idx_subtasks_task ON decomposition_subtasks(task_id);",
        )?;
        self.conn.execute_batch(&format!(
            "CREATE TABLE IF NOT EXISTS {wm} (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                module_file TEXT NOT NULL,
                symbol_name TEXT NOT NULL,
                symbol_kind TEXT NOT NULL DEFAULT 'unknown',
                visibility TEXT NOT NULL DEFAULT 'public',
                consumer_file TEXT,
                import_line INTEGER,
                contract_source TEXT DEFAULT 'ast_read',
                resolved_at TEXT
            );
            CREATE UNIQUE INDEX IF NOT EXISTS idx_wiring_unique
                ON {wm}(module_file, symbol_name, COALESCE(consumer_file, ''));
            CREATE INDEX IF NOT EXISTS idx_wiring_orphans
                ON {wm}(consumer_file) WHERE consumer_file IS NULL;
            CREATE INDEX IF NOT EXISTS idx_wiring_module
                ON {wm}(module_file);

            CREATE TABLE IF NOT EXISTS {me} (
                file_path TEXT PRIMARY KEY,
                module_role TEXT NOT NULL DEFAULT 'internal',
                parent_module TEXT,
                pub_symbol_count INTEGER DEFAULT 0,
                import_count INTEGER DEFAULT 0,
                re_export_count INTEGER DEFAULT 0,
                integration_score REAL DEFAULT 0.0,
                last_scanned_at TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_ecosystem_score
                ON {me}(integration_score);",
            wm = schema_guard::TABLE_WIRING_MAP,
            me = schema_guard::TABLE_MODULE_ECOSYSTEM,
        ))?;
        self.conn
            .execute_batch(
                &format!(
                    "CREATE TABLE IF NOT EXISTS {fff} (
                file_path TEXT NOT NULL,
                feature_name TEXT NOT NULL,
                lang TEXT NOT NULL DEFAULT 'rust',
                detected_at TEXT DEFAULT (datetime('now')),
                PRIMARY KEY (file_path, feature_name)
            );
            CREATE TABLE IF NOT EXISTS {ftd} (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                file_path TEXT NOT NULL,
                line_num INTEGER NOT NULL,
                kind TEXT NOT NULL,
                content TEXT NOT NULL,
                resolved INTEGER NOT NULL DEFAULT 0,
                created_at TEXT DEFAULT (datetime('now'))
            );
            CREATE INDEX IF NOT EXISTS idx_todos_file ON {ftd}(file_path);
            CREATE INDEX IF NOT EXISTS idx_todos_kind ON {ftd}(kind);

            CREATE TABLE IF NOT EXISTS {ecf} (
                source_path TEXT NOT NULL,
                target_path TEXT NOT NULL,
                relation_type TEXT NOT NULL DEFAULT 'imports',
                confidence_level TEXT NOT NULL DEFAULT 'medium',
                computed_at TEXT DEFAULT (datetime('now')),
                PRIMARY KEY (source_path, target_path, relation_type)
            );
            CREATE INDEX IF NOT EXISTS idx_confidence_level ON {ecf}(confidence_level);

            CREATE TABLE IF NOT EXISTS {fcm} (
                file_path TEXT PRIMARY KEY,
                community_id INTEGER NOT NULL,
                modularity_score REAL NOT NULL DEFAULT 0.0,
                assigned_at TEXT DEFAULT (datetime('now'))
            );

            CREATE TABLE IF NOT EXISTS {ftc} (
                file_path TEXT PRIMARY KEY,
                coverage_pct REAL NOT NULL DEFAULT 0.0,
                tested_functions INTEGER NOT NULL DEFAULT 0,
                total_functions INTEGER NOT NULL DEFAULT 0,
                updated_at TEXT DEFAULT (datetime('now'))
            );

            CREATE TABLE IF NOT EXISTS {fbr} (
                file_path TEXT PRIMARY KEY,
                blake3_hash TEXT NOT NULL,
                symbol_count INTEGER NOT NULL DEFAULT 0,
                merkle_parent TEXT,
                last_indexed_at TEXT DEFAULT (datetime('now'))
            );
            CREATE INDEX IF NOT EXISTS idx_blake3_hash ON {fbr}(blake3_hash);

            CREATE TABLE IF NOT EXISTS {sfs} (
                file_path TEXT NOT NULL,
                session_id TEXT NOT NULL,
                skeleton_json TEXT,
                purpose TEXT,
                top_gotchas_json TEXT,
                blast_severity TEXT,
                created_at TEXT DEFAULT (datetime('now')),
                PRIMARY KEY (file_path, session_id)
            );
            CREATE INDEX IF NOT EXISTS idx_session_summary_session ON {sfs}(session_id);

            CREATE TABLE IF NOT EXISTS {sel} (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                sequence_id TEXT UNIQUE NOT NULL,
                file_path TEXT NOT NULL,
                blake3_hash TEXT,
                operation TEXT NOT NULL,
                symbol_name TEXT,
                agent_id TEXT,
                timestamp TEXT DEFAULT (datetime('now'))
            );
            CREATE INDEX IF NOT EXISTS idx_symbol_events_file ON {sel}(file_path);
            CREATE INDEX IF NOT EXISTS idx_symbol_events_symbol ON {sel}(symbol_name);

            CREATE TABLE IF NOT EXISTS {wsg} (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                orphan_symbol TEXT NOT NULL,
                orphan_file TEXT NOT NULL,
                suggested_consumer TEXT,
                similarity_score REAL NOT NULL DEFAULT 0.0,
                community_id INTEGER,
                applied INTEGER NOT NULL DEFAULT 0,
                rejected INTEGER NOT NULL DEFAULT 0,
                created_at TEXT DEFAULT (datetime('now'))
            );
            CREATE INDEX IF NOT EXISTS idx_wiring_suggest_orphan ON {wsg}(orphan_symbol) WHERE applied = 0;

            CREATE TABLE IF NOT EXISTS {mbr} (
                run_id INTEGER PRIMARY KEY AUTOINCREMENT,
                commit_hash TEXT NOT NULL,
                bench_name TEXT NOT NULL,
                p50_ms REAL NOT NULL DEFAULT 0.0,
                p95_ms REAL NOT NULL DEFAULT 0.0,
                p99_ms REAL NOT NULL DEFAULT 0.0,
                samples INTEGER NOT NULL DEFAULT 0,
                ran_at TEXT DEFAULT (datetime('now')),
                UNIQUE (commit_hash, bench_name)
            );

            CREATE TABLE IF NOT EXISTS {cog} (
                file_path TEXT PRIMARY KEY,
                cognitive_score REAL NOT NULL DEFAULT 0.0,
                complexity_signal REAL NOT NULL DEFAULT 0.0,
                fan_in_signal REAL NOT NULL DEFAULT 0.0,
                fan_out_signal REAL NOT NULL DEFAULT 0.0,
                doc_signal REAL NOT NULL DEFAULT 0.0,
                updated_at TEXT DEFAULT (datetime('now'))
            );",
                    fff = schema_guard::TABLE_FILE_FEATURE_FLAGS, ftd =
                    schema_guard::TABLE_FILE_TODOS, ecf =
                    schema_guard::TABLE_EDGE_CONFIDENCE, fcm =
                    schema_guard::TABLE_FILE_COMMUNITIES, ftc =
                    schema_guard::TABLE_FILE_TEST_COVERAGE, fbr =
                    schema_guard::TABLE_FILE_BLAKE3_REGISTRY, sfs =
                    schema_guard::TABLE_SESSION_FILE_SUMMARY, sel =
                    schema_guard::TABLE_SYMBOL_EVENTS_LOG, wsg =
                    schema_guard::TABLE_WIRING_SUGGESTIONS, mbr =
                    schema_guard::TABLE_METADATA_BENCHMARK_RUNS, cog =
                    schema_guard::TABLE_COGNITIVE_ENRICHMENT,
                ),
            )?;
        Ok(())
    }
    /// Apply incremental schema migrations for existing databases.
    ///
    /// Each migration is idempotent — safe to run multiple times.
    pub(super) fn migrate_schema(&self) -> Result<(), rusqlite::Error> {
        let has_error_pattern: bool = self
            .conn
            .prepare(&format!(
                "SELECT error_pattern FROM {} LIMIT 0",
                schema_guard::TABLE_EDIT_HISTORY
            ))
            .is_ok();
        if !has_error_pattern {
            self.conn.execute_batch(&format!(
                "ALTER TABLE {} ADD COLUMN error_pattern TEXT;",
                schema_guard::TABLE_EDIT_HISTORY
            ))?;
        }
        let has_language: bool = self
            .conn
            .prepare(&format!(
                "SELECT language FROM {} LIMIT 0",
                schema_guard::TABLE_EDIT_HISTORY
            ))
            .is_ok();
        if !has_language {
            self.conn.execute_batch(&format!(
                "ALTER TABLE {eh} ADD COLUMN language TEXT;
                 ALTER TABLE {eh} ADD COLUMN symbol_context TEXT;",
                eh = schema_guard::TABLE_EDIT_HISTORY
            ))?;
        }
        let has_session_id: bool = self
            .conn
            .prepare(&format!(
                "SELECT session_id FROM {} LIMIT 0",
                schema_guard::TABLE_EDIT_HISTORY
            ))
            .is_ok();
        if !has_session_id {
            self.conn.execute_batch(&format!(
                "ALTER TABLE {} ADD COLUMN session_id TEXT;",
                schema_guard::TABLE_EDIT_HISTORY
            ))?;
        }
        self.conn.execute_batch(&format!(
            "CREATE INDEX IF NOT EXISTS idx_file_edit_error_ctx
                ON {eh}(error_pattern, language);
             CREATE INDEX IF NOT EXISTS idx_file_edit_session
                ON {eh}(session_id);",
            eh = schema_guard::TABLE_EDIT_HISTORY
        ))?;
        let has_command_hash: bool = self
            .conn
            .prepare(&format!(
                "SELECT command_hash FROM {} LIMIT 0",
                schema_guard::TABLE_BASH_OUTCOMES
            ))
            .is_ok();
        if !has_command_hash {
            self.conn.execute_batch(&format!(
                "ALTER TABLE {} ADD COLUMN command_hash TEXT;",
                schema_guard::TABLE_BASH_OUTCOMES
            ))?;
        }
        self.conn.execute_batch(&format!(
            "CREATE INDEX IF NOT EXISTS idx_bash_outcomes_hash
                ON {}(command_hash);",
            schema_guard::TABLE_BASH_OUTCOMES
        ))?;
        let has_gotcha_language: bool = self
            .conn
            .prepare(&format!(
                "SELECT language FROM {} LIMIT 0",
                schema_guard::TABLE_GOTCHAS
            ))
            .is_ok();
        if !has_gotcha_language {
            self.conn.execute_batch(&format!(
                "ALTER TABLE {} ADD COLUMN language TEXT;",
                schema_guard::TABLE_GOTCHAS
            ))?;
        }
        self.conn.execute_batch(&format!(
            "CREATE UNIQUE INDEX IF NOT EXISTS idx_file_gotchas_pattern_language
                ON {}(pattern, COALESCE(language, ''));",
            schema_guard::TABLE_GOTCHAS
        ))?;
        let has_decay_score: bool = self
            .conn
            .prepare(&format!(
                "SELECT decay_score FROM {} LIMIT 0",
                schema_guard::TABLE_GOTCHAS
            ))
            .is_ok();
        if !has_decay_score {
            self.conn.execute_batch(&format!(
                "ALTER TABLE {fg} ADD COLUMN decay_score REAL NOT NULL DEFAULT 1.0;
                 ALTER TABLE {fg} ADD COLUMN last_occurrence TEXT;
                 ALTER TABLE {fg} ADD COLUMN resolved_at TEXT;",
                fg = schema_guard::TABLE_GOTCHAS
            ))?;
            self.conn.execute_batch(&format!(
                "UPDATE {fg} SET last_occurrence = created_at WHERE last_occurrence IS NULL
                 AND created_at IS NOT NULL;",
                fg = schema_guard::TABLE_GOTCHAS
            ))?;
        }
        self.conn.execute_batch(&format!(
            "CREATE INDEX IF NOT EXISTS idx_file_gotchas_decay
                 ON {}(decay_score DESC) WHERE resolved_at IS NULL;",
            schema_guard::TABLE_GOTCHAS
        ))?;
        let _ = self.conn.execute_batch(&format!(
            "ALTER TABLE {} ADD COLUMN imported_symbols TEXT DEFAULT '[]';",
            schema_guard::TABLE_FILE_RELATIONS
        ));
        let has_quality_score: bool = self
            .conn
            .prepare("SELECT quality_score FROM decomposition_subtasks LIMIT 0")
            .is_ok();
        if !has_quality_score {
            self.conn.execute_batch(
                "ALTER TABLE decomposition_subtasks ADD COLUMN quality_score REAL;",
            )?;
        }
        let has_consumer_type: bool = self
            .conn
            .prepare(&format!(
                "SELECT consumer_type FROM {} LIMIT 0",
                schema_guard::TABLE_WIRING_MAP
            ))
            .is_ok();
        if !has_consumer_type {
            self.conn.execute_batch(&format!(
                "ALTER TABLE {} ADD COLUMN consumer_type TEXT DEFAULT 'rust_import';",
                schema_guard::TABLE_WIRING_MAP
            ))?;
        }
        let has_workspace_root: bool = self
            .conn
            .prepare(&format!(
                "SELECT workspace_root FROM {} LIMIT 0",
                schema_guard::TABLE_WIRING_MAP
            ))
            .is_ok();
        if !has_workspace_root {
            self.conn.execute_batch(&format!(
                "ALTER TABLE {} ADD COLUMN workspace_root TEXT;",
                schema_guard::TABLE_WIRING_MAP
            ))?;
        }
        let daemon_consumer_count = self.conn.execute(
            &format!(
                "UPDATE {wm} SET \
                consumer_type = 'daemon_hook', \
                consumer_file = 'touring-daemon://dispatch', \
                resolved_at = datetime('now') \
             WHERE module_file LIKE '%/touring-hooks/%' \
               AND consumer_file IS NULL \
               AND consumer_type = 'rust_import'",
                wm = schema_guard::TABLE_WIRING_MAP
            ),
            [],
        )?;
        if daemon_consumer_count > 0 {
            tracing::debug!(
                daemon_orphan_corrected = daemon_consumer_count,
                "L4 IPC consumer enrichment: marked {}{} symbols as daemon_hook",
                daemon_consumer_count,
                " touring-hooks"
            );
        }
        Ok(())
    }
}
