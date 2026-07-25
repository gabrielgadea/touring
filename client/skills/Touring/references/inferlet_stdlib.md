# Inferlet Standard Library — Reference

> **Version**: 1.0 | **Scope**: 18 inferlets | **Touring**: v8.0
> **Location**: `crates/inferlets/src/`

All inferlets follow the same execution contract:
- **Input**: JSON string via `stdin` or positional arg
- **Output**: JSON to `stdout` (structured result)
- **Return**: `i32` exit code — `1` = match/active, `0` = no-match/inactive
- **Binary matcher**: WASM inferlets carry output via `LAST_ERROR` thread-local buffer

---

## Rust WASM Inferlets (Wave A — unified `libinferlets.wasm`)

### `always_success` — Always-Match Benchmark

**Input**: `{}` (empty object)

**Output**: `{"matched": true, "purpose": "always_success benchmark"}`

**Return**: Always `1`

---

### `memory` — Contextual Memory Recall

**Input**: `{"query": "<string>", "tier": "semantic"}`

**Output**: `{"found": true, "value": "...", "tier": "semantic"}`

**Return**: `1` if recall hits, `0` if miss

---

### `pattern` — Regex Pattern Matcher

**Input**: `{"text": "<string>", "pattern": "<regex>"}`

**Output**: `{"matched": true, "groups": ["...", "..."]}`

**Return**: `1` if pattern matches, `0` otherwise

---

### `classifier` — Naive Bayes Text Classifier

**Input**: `{"text": "<string>", "labels": ["label1", "label2"]}`

**Output**: `{"label": "label1", "confidence": 0.87}`

**Return**: `1` if classification confidence > threshold, `0` otherwise

---

### `tantivy_query_builder` — Build Boolean Tantivy Queries

**Input**: `{"query_text": "<string>", "fields": ["name", "description"], "boost": [2.0, 1.0]}`

**Output**: `{"query_json": "{...}", "estimated_terms": 4}`

**Return**: `1` if query builds successfully, `0` on parse error

---

### `synergy_health_check` — Validate WiredPairs Health

**Input**: `{"threshold": 0.8}` (optional, default 0.5)

**Output**: `{"score": 0.73, "pairs_checked": 45, "healthy_pairs": 38, "alerts": []}`

**Return**: `1` if score >= threshold, `0` otherwise

---

### `unused_pub_symbols` — Detect Orphan Pub Symbols

**Input**: `{"threshold": 0.05, "module_filter": "touring-hooks"}` (both optional)

**Output**: `{"orphans": [{"symbol": "foo", "crate": "bar", "consumers": 0}], "total_checked": 120}`

**Return**: `1` if orphans > threshold fraction, `0` otherwise

---

### `dependency_diff` — Compare Cargo.lock Revisions

**Input**: `{"before_lock": "/path/before.lock", "after_lock": "/path/after.lock"}`

**Output**: `{"added": ["crate-1.2.0"], "removed": ["crate-0.9.0"], "changed": [{"name": "tokio", v1: "1.0", v2: "1.1"}]}`

**Return**: `1` if diff non-empty, `0` if identical

---

### `tdg_grade_distribution` — Parse TDG Grade Distributions

**Input**: `{"tdg_output": "A+ B C- D F"}` or `{"file_path": "/path/to/tdg.txt"}`

**Output**: `{"grades": {"A+": 3, "B": 12, "C-": 2, "D": 1, "F": 0}, "avg_grade": "B+"}`

**Return**: `1` if grades parsed, `0` if parse error

---

### `composite_health_trend` — Analyze Health Delta Over Time

**Input**: `{"metric_name": "pre_edit_fast_path", "window_hours": 24}`

**Output**: `{"trend": "increasing", "delta_pct": 12.3, "current": 0.85, "baseline": 0.72}`

**Return**: `1` if trend calculable, `0` if insufficient data

---

## Rust CLI Wrapper Inferlets (Wave B — subprocess dispatch)

### `flaky_test_pattern_detector` — Detect Flaky Tests from Cargo Output

**Input**: `{"test_log": "/path/to/test.log", "threshold": 0.3}`

**Output**: `{"flaky_tests": [{"name": "test_session_resume", "failure_rate": 0.45, "runs": 20}], "total_runs": 100}`

**Return**: `1` if flaky tests found (rate > threshold), `0` otherwise

---

### `count_files_via_cli_wrapper` — Count Files by Extension

**Input**: `{"extensions": [".rs", ".py"], "exclude_dirs": ["target", ".git"]}`

**Output**: `{"counts": {".rs": 342, ".py": 87}, "total": 429}`

**Return**: `1` if files found, `0` if none

---

### `top_n_complex_files` — Find Most Complex Files via Radon

**Input**: `{"extensions": [".rs", ".py"], "top_n": 10, "min_complexity": 10}`

**Output**: `{"files": [{"path": "foo/bar.rs", "complexity": 23, "rank": 1}], "total_scanned": 120}`

**Return**: `1` if files found, `0` if none exceed threshold

---

### `find_circular_imports` — Detect Python Circular Import Chains

**Input**: `{"target_dir": "/path/to/project", "max_depth": 10}`

**Output**: `{"cycles": [["a.py", "b.py", "a.py"], ["c.py", "d.py", "c.py"]], "total_modules": 87}`

**Return**: `1` if cycles found, `0` if clean

---

## Python CLI Inferlets (Wave B — ctx_execute sandbox)

### `event_seq_gap_detector` — Validate Event Sequence Monotonicity

**Input**: `{"activity_log": "/path/to/activity.jsonl", "task_id": "t-123"}`

**Output**: `{"valid": true, "gaps": [], "last_seq": 42}` or `{"valid": false, "gaps": [{"expected": 5, "found": 7}], "last_seq": 6}`

**Return**: `1` if valid (no gaps), `0` if gaps found

---

### `entity_homonymy_detector` — Find Homonymic Entities Across Crates

**Input**: `{"entity_name": "CognitiveMCTS"}`

**Output**: `{"homonyms": [{"crate": "touring-cognitive", "module": "cognitive_mcts", "definition": "type alias"}], "count": 3}`

**Return**: `1` if homonyms found (potential conflict), `0` if unique

---

### `manter_drift_summary` — Summarize PARCER Drift Between Sessions

**Input**: `{"before": "/path/before.parcer.yaml", "after": "/path/after.parcer.yaml"}`

**Output**: `{"drift_detected": true, "changed_dims": ["Audience", "Rules"], "summary": "Audience changed from junior to senior"}`

**Return**: `1` if drift detected, `0` if identical

---

### `wave_velocity_report` — Compute Delivery Velocity from Diary Entries

**Input**: `{"days": 30, "project": "touring"}`

**Output**: `{"velocity": 3.2, "units": "deliverables/week", "trend": "increasing", "samples": 14}`

**Return**: `1` if velocity > 0 (data available), `0` if no data

---

### `confidence_decay_audit` — Evaluate VP-Scout Confidence Decay

**Input**: `{"chain_results": {"feature_trace": {"confidence": 0.95}, "homonimia": {"confidence": 0.82}}, "threshold": 0.7}`

**Output**: `{"decay_detected": true, "current_avg": 0.65, "baseline_avg": 0.87, "slope": -0.034}`

**Return**: `1` if decay detected (avg < threshold), `0` if stable

---

### `top_orphan_clusters` — Community Detection on Orphan Symbol Graph

**Input**: `{"orphan_symbols": [{"name": "foo", "crate": "bar", "consumers": 0}], "top_n": 5, "min_cluster_size": 2}`

**Output**: `{"clusters": [{"size": 4, "members": ["s1", "s2", "s3", "s4"]}], "total_orphans": 12}`

**Return**: `1` if clusters found, `0` if no clustering

---

## JavaScript/Node CLI Inferlets (Wave B — ctx_execute sandbox)

### `boundary_violation_summary` — Parse S1 Boundary Violation Events

**Input**: `{"events": [{"type": "S1_BOUNDS_VIOLATION", "file": "foo.rs", "line": 10}]}`

**Output**: `{"violations": [{"file": "foo.rs", "line": 10, "type": "S1_BOUNDS_VIOLATION"}], "total": 1}`

**Return**: `1` if violations found, `0` if clean

---

### `crates_size_via_cli_wrapper` — Crate Size Distribution via CLI

**Input**: `{"paths": ["/path/to/crates"]}` (optional array of paths)

**Output**: `{"total_crates": 12, "median_loc": 5420, "largest": {"name": "touring-hooks", "loc": 28400}}`

**Return**: `1` if crates found, `0` if path empty

---

## Execution Guide

```bash
# Rust WASM (via touring inferlets run)
touring inferlets run always_success '{}'

# Python CLI (direct execution)
python3 crates/inferlets/src/event_seq_gap_detector.py '{"activity_log": "/tmp/log.jsonl", "task_id": "t-1"}'

# Node.js CLI (direct execution)
node crates/inferlets/src/boundary_violation_summary.js '{"events": [...]}'

# Rust CLI wrappers (direct execution)
cargo run --release -p inferlets --bin flaky_test_pattern_detector -- '{"test_log": "/tmp/test.log", "threshold": 0.3}'
```

## Dispatch Table (lib.rs)

| Key | Module | Language |
|---|---|---|
| `always_success` | `always_success` | Rust |
| `memory` | `memory` | Rust |
| `pattern` | `pattern` | Rust |
| `classifier` | `classifier` | Rust |
| `tantivy_query_builder` | `tantivy_query_builder` | Rust |
| `synergy_health_check` | `synergy_health_check` | Rust |
| `unused_pub_symbols` | `unused_pub_symbols` | Rust |
| `dependency_diff` | `dependency_diff` | Rust |
| `tdg_grade_distribution` | `tdg_grade_distribution` | Rust |
| `composite_health_trend` | `composite_health_trend` | Rust |
| `flaky_test_pattern_detector` | `flaky_test_pattern_detector` | Rust (CLI) |
| `count_files_via_cli_wrapper` | `count_files_via_cli_wrapper` | Rust (CLI) |
| `top_n_complex_files` | `top_n_complex_files` | Rust (CLI) |
| `find_circular_imports` | `find_circular_imports` | Rust (CLI) |
| `event_seq_gap_detector` | `event_seq_gap_detector` | Python |
| `entity_homonymy_detector` | `entity_homonymy_detector` | Python |
| `manter_drift_summary` | `manter_drift_summary` | Python |
| `wave_velocity_report` | `wave_velocity_report` | Python |
| `confidence_decay_audit` | `confidence_decay_audit` | Python |
| `top_orphan_clusters` | `top_orphan_clusters` | Python |
| `boundary_violation_summary` | `boundary_violation_summary` | JavaScript |
| `crates_size_via_cli_wrapper` | `crates_size_via_cli_wrapper` | JavaScript |