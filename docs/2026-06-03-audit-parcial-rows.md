# REGRA #0 Audit — 13 PARCIAL rows inventory

**Date**: 2026-06-03 | **Source**: matrix.json 2026-06-03T12-26-41 (post-P5)
**DAG**: task_1780502437901336373 | **Wave**: 4.5ed

## Classification framework

| Bucket | Meaning | Action |
|---|---|---|
| **REAL-PARCIAL** | Code exists but genuinely partial (gap is real, not theatre) | Report to next strategic wave |
| **PROSE-PARTIAL** | Code is fuller than spec_compat_reason claims; prose overstates the gap | Update spec_compat_reason (bump prior) — like A-A1, ES4 P1, P5 did |
| **THEATER** | Code absent or stubbed but spec_compat_reason describes non-existent work | Spec-down (lower prior) + flag for future wave |
| **P3-NOOP** | Struct/counter exists but no consumer (P3-style orphan) | Wire to a real consumer (or remove) |

## Inventory (sorted by score asc = most concerning first)

### `interface.formal-verify` — Formal verification / executable contracts (§§2.1.2)

- **Score**: 0.65 (impl=1.0, result=1.0, spec_compat=0.65)
- **Category**: interface
- **Severity**: MEDIUM

**Reason** (verbatim from oracle):

> PARCIAL [impl=1 result=1 spec_compat=0.65] | The EXECUTABLE-CONTRACTS half of §2.1.2 is LIVE: 'touring change-contract' returns pass/fail with per-claim evidence (verified 2026-05-30: regression → committed:false ['composite regressed 0.80->0.75','dimension inspectable regressed 0.80->0.60']; improvement → committed:true) — exactly the success_metric 'verifier returns pass/fail with evidence per claim'. Prior reason (2026-05-29) under-counted by considering only VGP. Held well below CONFORME because the PROOF-ASSISTANT half (Lean/VERINA/SMT/dependent-types — machine-checkable proofs of arbitrary claims) is genuinely ABSENT and is an epic, not a session: invariant contracts prove no-regression, not arbitrary correctness.

**Remediation suggested**: Deepen 'ChangeContract + VgpReport': The EXECUTABLE-CONTRACTS half of §2.1.2 is LIVE: 'touring change-contract' returns pass/fail with per-claim evidence (verified 2026-05-30: regression → committed:false ['composite regressed 0.80->0.75','dimension inspectable regressed 0.80->0.60']; improvement → committed:true) — exactly the success_metric 'verifier returns pass/fail with evidence per claim'. Prior reason (2026-05-29) under-counted by considering only VGP. Held well below CONFORME because the PROOF-ASSISTANT half (Lean/VERINA/SMT/dependent-types — machine-checkable proofs of arbitrary claims) is genuinely ABSENT and is an epic, not a session: invariant contracts prove no-regression, not arbitrary correctness.

---

### `mech.evolution-agent` — Evolution agent (§§3.5.2)

- **Score**: 0.65 (impl=1.0, result=1.0, spec_compat=0.65)
- **Category**: mechanisms
- **Severity**: MEDIUM

**Reason** (verbatim from oracle):

> PARCIAL [impl=1 result=1 spec_compat=0.65] | Drift/insights present; agentic_rl reported active=False, updates=0 -> meta-loop not yet self-driving.

**Remediation suggested**: Deepen 'touring evolution': Drift/insights present; agentic_rl reported active=False, updates=0 -> meta-loop not yet self-driving.

---

### `multiagent.shared-rep` — Shared harness representation (§§4.3.1)

- **Score**: 0.75 (impl=1.0, result=1.0, spec_compat=0.75)
- **Category**: multi-agent
- **Severity**: MEDIUM

**Reason** (verbatim from oracle):

> PARCIAL [impl=1 result=1 spec_compat=0.75] | CRDT shared representation present and healthy; convergence semantics partial vs OP4.

**Remediation suggested**: Deepen 'crdt_graph': CRDT shared representation present and healthy; convergence semantics partial vs OP4.

---

### `multiagent.exec-feedback-sync` — Execution-feedback synchronization (§§4.2)

- **Score**: 0.75 (impl=1.0, result=1.0, spec_compat=0.75)
- **Category**: multi-agent
- **Severity**: MEDIUM

**Reason** (verbatim from oracle):

> PARCIAL [impl=1 result=1 spec_compat=0.75] | Cold-start ADDRESSED (2026-05-30): the thin-update-volume limit is now lifted by an opt-in cross-project warm-start ('touring rl-warmstart', cli-rl-warmstart) that replays REAL bash_outcomes through process_immediate_reward — verified LIVE: update_count 1->201 from a representative sample (measured_bash_success=0.559, honest vs the recency-tail). post_tool_rl feedback sync then continues from a real-data-grounded prior rather than near-zero. Prior reason 'thin update volume' (2026-05-29) addressed for opted-in projects. Held below CONFORME: multi-agent sync convergence is broader than single-loop warm-start, and the warm-start is opt-in/per-session, not continuous.

**Remediation suggested**: Deepen 'post_tool_rl': Cold-start ADDRESSED (2026-05-30): the thin-update-volume limit is now lifted by an opt-in cross-project warm-start ('touring rl-warmstart', cli-rl-warmstart) that replays REAL bash_outcomes through process_immediate_reward — verified LIVE: update_count 1->201 from a representative sample (measured_bash_success=0.559, honest vs the recency-tail). post_tool_rl feedback sync then continues from a real-data-grounded prior rather than near-zero. Prior reason 'thin update volume' (2026-05-29) addressed for opted-in projects. Held below CONFORME: multi-agent sync convergence is broader than single-loop warm-start, and the warm-start is opt-in/per-session, not continuous.

---

### `interface.rlef` — Iterative code-grounded reasoning (RLEF) (§§2.1.3)

- **Score**: 0.78 (impl=1.0, result=1.0, spec_compat=0.78)
- **Category**: interface
- **Severity**: MEDIUM

**Reason** (verbatim from oracle):

> PARCIAL [impl=1 result=1 spec_compat=0.78] | Cold-start ADDRESSED (2026-05-30): the loop can now warm-start from REAL accumulated history via 'touring rl-warmstart' (cli-rl-warmstart handler, experience replay of real bash_outcomes through the genuine process_immediate_reward path). Verified LIVE 2026-05-30: replaying a representative random sample of the home corpus lifted update_count 1->201 with measured_bash_success=0.559 (faithful to the corpus's true ~58.6% rate, NOT the optimistic recent-tail — a recency-bias bug was caught + fixed via ORDER BY RANDOM()). Opt-in (TOURING_RL_WARMSTART_CORPUS) wired into rl_warmup.sh at session-start; default preserves per-project isolation. Prior reason 'underpopulated, signal thin' (2026-05-29) addressed for any project that opts in. Held below CONFORME: it is a warm-start PRIOR from real data, not continuous convergence, and cross-project relevance is heuristic.

**Remediation suggested**: Deepen 'cli_learning_reward': Cold-start ADDRESSED (2026-05-30): the loop can now warm-start from REAL accumulated history via 'touring rl-warmstart' (cli-rl-warmstart handler, experience replay of real bash_outcomes through the genuine process_immediate_reward path). Verified LIVE 2026-05-30: replaying a representative random sample of the home corpus lifted update_count 1->201 with measured_bash_success=0.559 (faithful to the corpus's true ~58.6% rate, NOT the optimistic recent-tail — a recency-bias bug was caught + fixed via ORDER BY RANDOM()). Opt-in (TOURING_RL_WARMSTART_CORPUS) wired into rl_warmup.sh at session-start; default preserves per-project isolation. Prior reason 'underpopulated, signal thin' (2026-05-29) addressed for any project that opts in. Held below CONFORME: it is a warm-start PRIOR from real data, not continuous convergence, and cross-project relevance is heuristic.

---

### `mech.semantic-memory` — Semantic memory (recallable knowledge) (§§3.2)

- **Score**: 0.78 (impl=1.0, result=1.0, spec_compat=0.78)
- **Category**: mechanisms
- **Severity**: MEDIUM

**Reason** (verbatim from oracle):

> PARCIAL [impl=1 result=1 spec_compat=0.78] | ANN/vector corpus now POPULATED (live: ann_results=11, RRF fusion from 3 sources, count=20) — semantic axis active, no longer FTS5-only; held below CONFORME only by modest corpus size (~6 docs), mechanism (ANN+TF-IDF+RRF) is complete. Prior reason 'ann_results=0' (2026-05-29) falsified by live recall 2026-05-30.

**Remediation suggested**: Deepen 'memory recall (FTS5 + cosine)': ANN/vector corpus now POPULATED (live: ann_results=11, RRF fusion from 3 sources, count=20) — semantic axis active, no longer FTS5-only; held below CONFORME only by modest corpus size (~6 docs), mechanism (ANN+TF-IDF+RRF) is complete. Prior reason 'ann_results=0' (2026-05-29) falsified by live recall 2026-05-30.

---

### `interface.pot` — Program-of-Thought (program-delegated reasoning) (§§2.1.1)

- **Score**: 0.8 (impl=1.0, result=1.0, spec_compat=0.8)
- **Category**: interface
- **Severity**: MEDIUM

**Reason** (verbatim from oracle):

> PARCIAL [impl=1 result=1 spec_compat=0.8] | PoT exists at harness level (inferlets + ctx_execute); historically underused (ctx_execute_file_count=0).

**Remediation suggested**: Deepen 'InferletPool': PoT exists at harness level (inferlets + ctx_execute); historically underused (ctx_execute_file_count=0).

---

### `interface.lifelong-lyra` — Lifelong agents (corrections -> skills, LYRA) (§§2.2.3)

- **Score**: 0.8 (impl=1.0, result=1.0, spec_compat=0.8)
- **Category**: interface
- **Severity**: MEDIUM

**Reason** (verbatim from oracle):

> PARCIAL [impl=1 result=1 spec_compat=0.8] | Faithful LYRA analogue; miner exists and is wired (gated TOURING_TRANSCRIPT_MINER, 5min sweep).

**Remediation suggested**: Deepen 'extract_error_resolution_pairs': Faithful LYRA analogue; miner exists and is wired (gated TOURING_TRANSCRIPT_MINER, 5min sweep).

---

### `mech.planning-orchestration` — Multi-agent planning / decomposition (§§3.1.4)

- **Score**: 0.8 (impl=1.0, result=1.0, spec_compat=0.8)
- **Category**: mechanisms
- **Severity**: MEDIUM

**Reason** (verbatim from oracle):

> PARCIAL [impl=1 result=1 spec_compat=0.8] | Faithful DAG decomposition with dependency edges and ready-set computation.

**Remediation suggested**: Deepen 'TaskDecomposer': Faithful DAG decomposition with dependency edges and ready-set computation.

---

### `mech.working-memory` — Working memory (session context) (§§3.2)

- **Score**: 0.8 (impl=1.0, result=1.0, spec_compat=0.8)
- **Category**: mechanisms
- **Severity**: MEDIUM

**Reason** (verbatim from oracle):

> PARCIAL [impl=1 result=1 spec_compat=0.8] | Session-scoped working memory present and assessable.

**Remediation suggested**: Deepen 'SessionManager': Session-scoped working memory present and assessable.

---

### `mech.deep-telemetry` — Deep telemetry (§§3.5.1)

- **Score**: 0.8 (impl=1.0, result=1.0, spec_compat=0.8)
- **Category**: mechanisms
- **Severity**: MEDIUM

**Reason** (verbatim from oracle):

> PARCIAL [impl=1 result=1 spec_compat=0.8] | A4 DONE: the gate-metrics counters now have an OTel/GenAI export representation via 'gate-metrics --otel' (verified 2026-05-30: 155 metrics emitted in OTLP/JSON shape under scope touring.ceg.gate_metrics with gen_ai.* resource attributes — the envelope a GenAI-aware OTLP collector ingests). Prior reason 'counters lack OTel/GenAI export' (2026-05-29) falsified. Held below CONFORME: this is the export *representation*; a live network push to an external OTLP collector endpoint is external infrastructure (out of harness scope, like the deferred span-exporter).

**Remediation suggested**: Deepen 'gate-metrics counters': A4 DONE: the gate-metrics counters now have an OTel/GenAI export representation via 'gate-metrics --otel' (verified 2026-05-30: 155 metrics emitted in OTLP/JSON shape under scope touring.ceg.gate_metrics with gen_ai.* resource attributes — the envelope a GenAI-aware OTLP collector ingests). Prior reason 'counters lack OTel/GenAI export' (2026-05-29) falsified. Held below CONFORME: this is the export *representation*; a live network push to an external OTLP collector endpoint is external infrastructure (out of harness scope, like the deferred span-exporter).

---

### `mech.experiential-memory` — Experiential memory (outcomes / edits) (§§3.2)

- **Score**: 0.82 (impl=1.0, result=1.0, spec_compat=0.82)
- **Category**: mechanisms
- **Severity**: MEDIUM

**Reason** (verbatim from oracle):

> PARCIAL [impl=1 result=1 spec_compat=0.82] | B-5 DONE: the experiential substrate is now distilled into an action predictor via 'touring predict-action' (cli-predict-action handler trains a LearnedOutcomeModel over recent_bash_outcomes, predicts via predict_from_features). Verified 2026-05-30: 'cargo check --workspace' → distilled 64 historical outcomes, success_probability=0.197, confidence High (64 matched) — grounded in real history, not a prior. Distinct from S-11's online model (new outcomes only). Prior reason 'not yet distilled' (2026-05-29) falsified. Held just below CONFORME: the distillation is recomputed on demand per query rather than a persistent incrementally-updated model.

**Remediation suggested**: Deepen 'BashOutcomeRecord': B-5 DONE: the experiential substrate is now distilled into an action predictor via 'touring predict-action' (cli-predict-action handler trains a LearnedOutcomeModel over recent_bash_outcomes, predicts via predict_from_features). Verified 2026-05-30: 'cargo check --workspace' → distilled 64 historical outcomes, success_probability=0.197, confidence High (64 matched) — grounded in real history, not a prior. Distinct from S-11's online model (new outcomes only). Prior reason 'not yet distilled' (2026-05-29) falsified. Held just below CONFORME: the distillation is recomputed on demand per query rather than a persistent incrementally-updated model.

---

### `multiagent.state-convergence` — Harness-state convergence (§§4.3.2)

- **Score**: 0.82 (impl=1.0, result=1.0, spec_compat=0.82)
- **Category**: multi-agent
- **Severity**: MEDIUM

**Reason** (verbatim from oracle):

> PARCIAL [impl=1 result=1 spec_compat=0.82] | OP4 read/write-set + dependency-aware locking now PRESENT + LIVE (txn.rs TxnLockManager, exercised by 'touring txn-acquire' — verified 2026-05-30: 2 disjoint granted concurrent, 1 hazard deferred + serialized after drain). Prior reason 'ABSENT' (2026-05-29) falsified. Held just below CONFORME because the locking is invocable/available, not yet ENFORCED in a live concurrent supervised-exec path (no concurrency exists today; ExecPool::acquire_txn enforcement is the opt-in txn_lock_enforcement layer awaiting a concurrent consumer).

**Remediation suggested**: Deepen 'crdt merge + TxnLockManager': OP4 read/write-set + dependency-aware locking now PRESENT + LIVE (txn.rs TxnLockManager, exercised by 'touring txn-acquire' — verified 2026-05-30: 2 disjoint granted concurrent, 1 hazard deferred + serialized after drain). Prior reason 'ABSENT' (2026-05-29) falsified. Held just below CONFORME because the locking is invocable/available, not yet ENFORCED in a live concurrent supervised-exec path (no concurrency exists today; ExecPool::acquire_txn enforcement is the opt-in txn_lock_enforcement layer awaiting a concurrent consumer).

---

