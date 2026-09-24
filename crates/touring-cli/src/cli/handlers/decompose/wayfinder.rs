//! Reivindicacao atomica e Wayfinder
//!
//! Extraido de `handlers/decompose.rs` em 21/09/2026: o arquivo tinha 2450
//! linhas e reprovava F1.2 (manutenibilidade) desde antes deste trabalho. O
//! corte e por COESAO — este modulo guarda quem PEGA trabalho: claim/release com lease, tickets de
//! decisao, a fronteira e a fila de prontos.

use super::*;
/// The ONE readiness predicate, shared by `claim` (who may take it) and `ready`
/// (what the pool shows): pending, or in_progress only under an EXPIRED lease.
/// A live claim is someone's work in both views — and an in_progress subtask
/// started by hand (no claim) is nobody's to claim, exactly as `claim` treats
/// it. One predicate in one place, because a predicate written twice never
/// meets itself (coordenação 2026-09-23: ready listed live claims as ready).
fn ready_eligible(
    status: &str,
    claimed_by: Option<&str>,
    claim_expires_at: Option<i64>,
    now_epoch: i64,
) -> bool {
    match status {
        "pending" => true,
        "in_progress" => claimed_by.is_some() && claim_expires_at.is_some_and(|e| e < now_epoch),
        _ => false,
    }
}

/// The ONE queue order, shared by `claim` (what it would hand out next) and
/// `ready` (what it lists first): priority ASC, then id ASC. Written twice it
/// diverged — `claim` sorted by (priority, id) while `ready` listed INSERTION
/// order, and a guard trusting `ready[0]` mispredicted the claim (analise-d4,
/// 24/09/2026: all at priority 128, ready showed A6 first, claim delivered
/// A3a because '3' < '6'). One function, one truth about "what comes first".
fn queue_order(
    a_priority: i32,
    a_id: &str,
    b_priority: i32,
    b_id: &str,
) -> std::cmp::Ordering {
    a_priority.cmp(&b_priority).then_with(|| a_id.cmp(b_id))
}

/// Atomically claim the next ready subtask for one owner.
///
/// `ready` only READS. Two sessions polling it concurrently were handed the same
/// subtask and both began it — reproduced 2026-08-18, and the reason the plan
/// rated this the highest-risk delivery in the wave.
///
/// The fix is not a lock around the read but a **conditional write**: SQLite
/// serialises writers, so of N racing `UPDATE ... WHERE status = 'pending'`
/// statements exactly one reports a row changed. The losers see zero rows and
/// move on to the next candidate — no lock, no lease server, no lost update.
///
/// This is deliberately a NEW verb rather than a change to `ready_subtasks`,
/// which has 66 call-sites legitimately asking "what COULD start" for display,
/// planning and convergence checks. Claiming answers a different question —
/// "give me work, exclusively" — so every existing caller keeps its meaning.
///
/// Payload: `{task_id, owner, lease_secs?, subtask_id?}` → `{claimed: bool, subtask_id?, ...}`.
/// With `subtask_id` the claim names ONE subtask instead of taking the next
/// ready one — and every refusal names its reason (24/09/2026, analise-d4:
/// wanting A1, it ran plain claim twice and got A5/A6 with no explanation).
pub fn cli_decompose_claim(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let task_id = payload
        .get("task_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let owner = payload.get("owner").and_then(|v| v.as_str()).unwrap_or("");
    if task_id.is_empty() || owner.is_empty() {
        return serde_json::json!({
            "claimed": false,
            "error": "task_id and owner are both required — an unattributed claim \
                      cannot be released or expired by anyone"
        })
        .to_string();
    }
    let lease_secs = payload
        .get("lease_secs")
        .and_then(serde_json::Value::as_i64)
        .filter(|v| *v > 0)
        .unwrap_or(DEFAULT_CLAIM_LEASE_SECS);

    let db = &rt.ctx.knowledge;
    ensure_decompose_tables(db);

    let rows: Vec<ClaimCandidate> = {
        let Ok(mut stmt) = db.conn_ref().prepare(
            "SELECT subtask_id, status, depends_on, priority, claim_expires_at, claimed_by, autonomy \
             FROM decomposition_subtasks WHERE task_id = ?1",
        ) else {
            return serde_json::json!({"claimed": false, "error": "db error"}).to_string();
        };
        stmt.query_map(params![task_id], |r| {
            let deps_str = r.get::<_, String>(2)?;
            Ok(ClaimCandidate {
                id: r.get::<_, String>(0)?,
                status: r.get::<_, String>(1)?,
                deps: serde_json::from_str(&deps_str).unwrap_or_else(|_| {
                    deps_str
                        .split(',')
                        .filter(|s| !s.is_empty())
                        .map(String::from)
                        .collect()
                }),
                priority: r.get::<_, i32>(3)?,
                claim_expires_at: r.get::<_, Option<i64>>(4)?,
                claimed_by: r.get::<_, Option<String>>(5)?,
                autonomy: r.get::<_, Option<String>>(6)?,
            })
        })
        .map(|it| it.filter_map(Result::ok).collect())
        .unwrap_or_default()
    };

    let completed: std::collections::HashSet<String> = rows
        .iter()
        .filter(|c| {
            matches!(
                c.status.as_str(),
                "completed" | "done" | "complete" | "failed" | "skipped"
            )
        })
        .map(|c| short_subtask_id(&c.id))
        .collect();

    let now = chrono::Utc::now();
    let now_epoch = now.timestamp();
    let try_claim = |subtask_id: &str| {
        db.conn_ref().execute(
            "UPDATE decomposition_subtasks \
                SET status = 'in_progress', claimed_by = ?1, claim_expires_at = ?2, \
                    updated_at = ?3 \
              WHERE subtask_id = ?4 AND task_id = ?5 \
                AND (status = 'pending' \
                     OR (status = 'in_progress' AND claimed_by IS NOT NULL \
                         AND claim_expires_at IS NOT NULL AND claim_expires_at < ?6))",
            params![
                owner,
                now_epoch + lease_secs,
                now.to_rfc3339(),
                subtask_id,
                task_id,
                now_epoch
            ],
        )
    };
    let claimed_ok = |subtask_id: &str| {
        serde_json::json!({
            "claimed": true,
            "task_id": task_id,
            "subtask_id": subtask_id,
            "owner": owner,
            "lease_secs": lease_secs,
            "claim_expires_at": now_epoch + lease_secs,
        })
        .to_string()
    };

    // `--subtask <id>`: claim ONE named subtask instead of the next ready one.
    if let Some(want) = payload
        .get("subtask_id")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
    {
        return claim_named_subtask(
            want,
            task_id,
            owner,
            &rows,
            &completed,
            now_epoch,
            &try_claim,
            &claimed_ok,
        );
    }

    // A candidate is unblocked AND unowned: either never started, or held by a
    // lease that has run out (the session that took it died without releasing).
    // A subtask moved to in_progress by hand carries no claim and is left alone —
    // a human said someone is on it, and no lease of ours expires that.
    //
    // And a HITL ticket is the HUMAN's work (analise-d4, 24/09/2026): the ticket
    // declares `autonomy = 'hitl'` ("human present") and a pool claim must never
    // hand it to an agent by accident — declaration and executor now read the
    // same field. The NAMED path (--subtask) stays open: it is the directed
    // way in, for the human's own session.
    let mut hitl_skipped = 0usize;
    let mut candidates: Vec<&ClaimCandidate> = rows
        .iter()
        .filter(|c| {
            let unblocked = c
                .deps
                .iter()
                .all(|d| completed.contains(&short_subtask_id(d)));
            if !unblocked || completed.contains(&short_subtask_id(&c.id)) {
                return false;
            }
            if !ready_eligible(&c.status, c.claimed_by.as_deref(), c.claim_expires_at, now_epoch) {
                return false;
            }
            if c.autonomy.as_deref() == Some("hitl") {
                hitl_skipped += 1;
                return false;
            }
            true
        })
        .collect();
    candidates.sort_by(|a, b| queue_order(a.priority, &a.id, b.priority, &b.id));

    for ClaimCandidate { id: subtask_id, .. } in &candidates {
        // Exactly one racer sees 1 here; the others see 0 and try the next candidate.
        if matches!(try_claim(subtask_id), Ok(1)) {
            return claimed_ok(subtask_id);
        }
    }

    let reason = if !candidates.is_empty() {
        "every candidate was claimed by another owner first".to_string()
    } else if hitl_skipped > 0 {
        format!(
            "the {hitl_skipped} unblocked subtask(s) left are HITL — the human's own \
             work, which a pool claim never takes; if you ARE that human, the directed \
             path is `touring decompose claim {task_id} --owner <you> --subtask <id>`"
        )
    } else {
        "no unblocked, unclaimed subtask is available".to_string()
    };
    serde_json::json!({
        "claimed": false,
        "task_id": task_id,
        "owner": owner,
        "reason": reason,
        "hitl_skipped": hitl_skipped,
    })
    .to_string()
}

/// One row of the claim scan. A named struct rather than a 6-tuple: the sort
/// reads `priority` instead of `.3`. Module-level since 24/09/2026, when the
/// named-subtask path needed the same shape.
struct ClaimCandidate {
    id: String,
    status: String,
    deps: Vec<String>,
    priority: i32,
    claim_expires_at: Option<i64>,
    claimed_by: Option<String>,
    /// The ticket's `autonomy` (hitl|afk), when the subtask carries a ticket.
    autonomy: Option<String>,
}

/// The `--subtask <id>` path: claim ONE named subtask, and let every refusal
/// name its reason — the owner and lease when the lease is live, the pending
/// deps when blocked, the status when terminal. An unnamed refusal is what
/// sent analise-d4 home with A5/A6 when it wanted A1 (24/09/2026).
/// The write is the same conditional UPDATE as the pool path, so exactly one
/// racer wins even here.
#[allow(clippy::too_many_arguments)]
fn claim_named_subtask(
    want: &str,
    task_id: &str,
    owner: &str,
    rows: &[ClaimCandidate],
    completed: &std::collections::HashSet<String>,
    now_epoch: i64,
    try_claim: &dyn Fn(&str) -> rusqlite::Result<usize>,
    claimed_ok: &dyn Fn(&str) -> String,
) -> String {
    let refuse = |reason: String| {
        serde_json::json!({
            "claimed": false,
            "task_id": task_id,
            "owner": owner,
            "reason": reason,
        })
        .to_string()
    };
    let Some(c) = rows
        .iter()
        .find(|c| c.id == want || short_subtask_id(&c.id) == want)
    else {
        return refuse(format!(
            "subtask '{want}' not found in task '{task_id}' — \
             `touring decompose get {task_id}` lists the ids"
        ));
    };
    if completed.contains(&short_subtask_id(&c.id)) {
        return refuse(format!(
            "subtask '{}' is already '{}' — nothing to claim",
            c.id, c.status
        ));
    }
    let pending_deps: Vec<String> = c
        .deps
        .iter()
        .map(|d| short_subtask_id(d))
        .filter(|d| !completed.contains(d))
        .collect();
    if !pending_deps.is_empty() {
        return refuse(format!(
            "subtask '{}' is blocked by unfinished deps: {}",
            c.id,
            pending_deps.join(", ")
        ));
    }
    if !ready_eligible(&c.status, c.claimed_by.as_deref(), c.claim_expires_at, now_epoch) {
        let reason = match (c.status.as_str(), c.claimed_by.as_deref(), c.claim_expires_at) {
            ("in_progress", Some(by), Some(exp)) if exp >= now_epoch => {
                let when = chrono::DateTime::from_timestamp(exp, 0)
                    .map(|t| t.format("%Y-%m-%d %H:%M UTC").to_string())
                    .unwrap_or_else(|| exp.to_string());
                format!(
                    "subtask '{}' is claimed by '{by}' — live lease until {when}; \
                     `touring decompose release {task_id} {} --owner {by}` or wait it out",
                    c.id, c.id
                )
            }
            ("in_progress", _, _) => format!(
                "subtask '{}' is in_progress with NO claim — moved by hand, \
                 and no lease of ours expires that",
                c.id
            ),
            (s, _, _) => format!("subtask '{}' has status '{s}'", c.id),
        };
        return refuse(reason);
    }
    if matches!(try_claim(&c.id), Ok(1)) {
        claimed_ok(&c.id)
    } else {
        refuse("another owner claimed it between the check and the write".to_string())
    }
}

/// Release a claim back to the pool — only its own owner may.
///
/// A worker that fails should hand the subtask back immediately rather than let
/// the whole lease elapse; letting it expire is the fallback for a session that
/// died, not the normal path.
pub fn cli_decompose_release(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let task_id = payload
        .get("task_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let subtask_id = payload
        .get("subtask_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let owner = payload.get("owner").and_then(|v| v.as_str()).unwrap_or("");
    if task_id.is_empty() || subtask_id.is_empty() || owner.is_empty() {
        return serde_json::json!({
            "released": false,
            "error": "task_id, subtask_id and owner are all required"
        })
        .to_string();
    }

    let db = &rt.ctx.knowledge;
    ensure_decompose_tables(db);
    // Releasing gives up the LEASE, not the WORK. Setting `pending`
    // unconditionally meant the ordinary sequence — claim → work → mark
    // completed → release — silently un-completed the subtask, and the next
    // `claim` handed the finished work to someone else (observed 2026-08-24 on
    // `w3b`, which returned to the ready set minutes after being closed with
    // evidence). A terminal status is a fact about the work; only a
    // non-terminal one describes a lease still being held.
    let changed = db.conn_ref().execute(
        "UPDATE decomposition_subtasks \
            SET status = CASE WHEN status IN ('completed', 'failed', 'skipped') \
                              THEN status ELSE 'pending' END, \
                claimed_by = NULL, claim_expires_at = NULL, updated_at = ?1 \
          WHERE task_id = ?2 \
            AND (subtask_id = ?3 OR subtask_id = ?2 || '::' || ?3) \
            AND claimed_by = ?4",
        params![chrono::Utc::now().to_rfc3339(), task_id, subtask_id, owner],
    );
    match changed {
        Ok(1) => serde_json::json!({
            "released": true, "task_id": task_id, "subtask_id": subtask_id, "owner": owner
        })
        .to_string(),
        Ok(_) => serde_json::json!({
            "released": false, "task_id": task_id, "subtask_id": subtask_id,
            "reason": "not held by this owner"
        })
        .to_string(),
        Err(e) => serde_json::json!({"released": false, "error": e.to_string()}).to_string(),
    }
}

/// The seam switch — hard-FALSE in every shipped binary: the daemon links
/// this crate and never calls `enable_test_seams`, and no RPC payload can
/// reach a Rust static. In production the seam fields are REFUSED with a
/// named error — a property of construction, not of "only tests send them"
/// (touring-36, 24/09/2026 — the convention was a named lease-theft door in
/// the RPC's public contract).
static TEST_SEAMS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Arm the test seams for THIS process. Test binaries call it; production has
/// no path here.
pub fn enable_test_seams() {
    TEST_SEAMS.store(true, std::sync::atomic::Ordering::Relaxed);
}

/// Back to the shipped state (tests assert both sides, in any order).
pub fn disable_test_seams() {
    TEST_SEAMS.store(false, std::sync::atomic::Ordering::Relaxed);
}

fn test_seams_armed() -> bool {
    TEST_SEAMS.load(std::sync::atomic::Ordering::Relaxed)
}

/// Renew a live lease — only its own holder may, and only while it is LIVE.
///
/// The lease exists so a session that DIES mid-work frees its subtask; but
/// with no renewal verb it also fires against a LIVE, slow session: the claim
/// lapses mid-flight and another session can take the in-progress subtask
/// (analise-d4, 24/09/2026 — the L11 claim expiring ~18:50 UTC with the work
/// still running, and `claim` only able to take the NEXT ready; naming the
/// same subtask with `--subtask` REFUSES even for the holder, it does not
/// renew). `renew` is the heartbeat the claim lifecycle was missing:
/// acquire → hold → release.
///
/// The write is conditional in the same way claim's is — exactly one writer,
/// and the condition carries the whole contract: `in_progress`, held by THIS
/// owner, lease still LIVE. A renew never resurrects an expired lease: the
/// expiry already freed the subtask to the pool, and resurrecting would be
/// stealing it back.
pub fn cli_decompose_renew(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let task_id = payload
        .get("task_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let subtask_id = payload
        .get("subtask_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let owner = payload.get("owner").and_then(|v| v.as_str()).unwrap_or("");
    if task_id.is_empty() || subtask_id.is_empty() || owner.is_empty() {
        return serde_json::json!({
            "renewed": false,
            "error": "task_id, subtask_id and owner are all required"
        })
        .to_string();
    }
    let lease_secs = payload
        .get("lease_secs")
        .and_then(serde_json::Value::as_i64)
        .filter(|v| *v > 0)
        .unwrap_or(DEFAULT_CLAIM_LEASE_SECS);

    let db = &rt.ctx.knowledge;
    ensure_decompose_tables(db);
    let now = chrono::Utc::now();
    let now_epoch = now.timestamp();
    let refuse = |reason: String| {
        serde_json::json!({
            "renewed": false,
            "task_id": task_id,
            "subtask_id": subtask_id,
            "owner": owner,
            "reason": reason,
        })
        .to_string()
    };

    // Read the row first: a bare "0 rows changed" teaches nothing — every
    // refusal below names its reason.
    let row: Option<(String, Option<String>, Option<i64>)> = db
        .conn_ref()
        .query_row(
            "SELECT status, claimed_by, claim_expires_at FROM decomposition_subtasks \
             WHERE task_id = ?1 AND (subtask_id = ?2 OR subtask_id = ?1 || '::' || ?2)",
            params![task_id, subtask_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .ok();
    let Some((status, claimed_by, claim_expires_at)) = row else {
        return refuse(format!(
            "subtask '{subtask_id}' not found in task '{task_id}' — \
             `touring decompose get {task_id}` lists the ids"
        ));
    };
    match (status.as_str(), claimed_by.as_deref(), claim_expires_at) {
        ("completed" | "done" | "complete" | "failed" | "skipped", _, _) => {
            return refuse(format!(
                "subtask '{subtask_id}' is already '{status}' — nothing to renew"
            ));
        }
        ("pending", _, _) => {
            return refuse(format!(
                "subtask '{subtask_id}' is 'pending' — nothing to renew; \
                 `touring decompose claim {task_id} --owner <you> --subtask {subtask_id}` first"
            ));
        }
        ("in_progress", None, _) => {
            return refuse(format!(
                "subtask '{subtask_id}' is in_progress with NO claim — moved by hand, \
                 and there is no lease to renew"
            ));
        }
        ("in_progress", Some(by), _) if by != owner => {
            return refuse(format!(
                "subtask '{subtask_id}' is held by '{by}' — only the holder renews a lease"
            ));
        }
        ("in_progress", Some(_), Some(exp)) if exp <= now_epoch => {
            let when = chrono::DateTime::from_timestamp(exp, 0)
                .map(|t| t.format("%Y-%m-%d %H:%M UTC").to_string())
                .unwrap_or_else(|| exp.to_string());
            return refuse(format!(
                "the lease on '{subtask_id}' EXPIRED at {when} — the subtask already \
                 returned to the pool; `touring decompose claim {task_id} --owner <you> \
                 --subtask {subtask_id}` again if it is still free"
            ));
        }
        _ => {}
    }

    if let Some(thief) = payload.get("_test_swap_claimed_by").and_then(|v| v.as_str()) {
        // The field only acts when THIS process armed the seams (a Rust static
        // the daemon never flips and no RPC can reach). Unarmed: refused with a
        // named error, and the owner never moves.
        if !test_seams_armed() {
            return serde_json::json!({
                "renewed": false,
                "error": format!(
                    "seam de teste desativado — `_test_swap_claimed_by` ({thief}) só age em \
                     binários que chamam `enable_test_seams`; em produção o campo é recusado \
                     e o dono NÃO muda")
            })
            .to_string();
        }
        let _ = db.conn_ref().execute(
            "UPDATE decomposition_subtasks SET claimed_by = ?1 \
             WHERE task_id = ?2 AND (subtask_id = ?3 OR subtask_id = ?2 || '::' || ?3)",
            params![thief, task_id, subtask_id],
        );
    }

    let changed = db.conn_ref().execute(
        "UPDATE decomposition_subtasks \
            SET claim_expires_at = ?1, updated_at = ?2 \
          WHERE task_id = ?3 \
            AND (subtask_id = ?4 OR subtask_id = ?3 || '::' || ?4) \
            AND status = 'in_progress' \
            AND claimed_by = ?5 \
            AND claim_expires_at IS NOT NULL AND claim_expires_at > ?6",
        params![
            now_epoch + lease_secs,
            now.to_rfc3339(),
            task_id,
            subtask_id,
            owner,
            now_epoch
        ],
    );
    match changed {
        Ok(1) => serde_json::json!({
            "renewed": true,
            "task_id": task_id,
            "subtask_id": subtask_id,
            "owner": owner,
            "lease_secs": lease_secs,
            "claim_expires_at": now_epoch + lease_secs,
        })
        .to_string(),
        Ok(_) => refuse(
            "the lease expired or changed hands between the check and the write — \
             claim again if the subtask is still free"
                .to_string(),
        ),
        Err(e) => serde_json::json!({"renewed": false, "error": e.to_string()}).to_string(),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// C3: Wayfinder — decision tickets, fog, and a frontier
// ─────────────────────────────────────────────────────────────────────────────

/// A ticket either DECIDES something or BUILDS something. Collapsing the two is
/// what produces plans that look complete while resting on unmade decisions.
const TICKET_KINDS: &[&str] = &["decision", "implementation"];
/// Decision work comes in four shapes; `task` is the ordinary implementation one.
const TICKET_SUBTYPES: &[&str] = &["research", "prototype", "grilling", "task"];
/// Who has to be present: human-in-the-loop, or away-from-keyboard.
const AUTONOMY_MODES: &[&str] = &["hitl", "afk"];
/// How much is unknown. The test for a ticket is whether the question can be
/// STATED clearly now — not whether it can be answered now.
const FOG_LEVELS: &[&str] = &["clear", "hazy", "unknown"];

fn validate_enum(field: &str, value: &str, allowed: &[&str]) -> Option<String> {
    (!allowed.contains(&value))
        .then(|| format!("{field} must be one of {allowed:?}, got `{value}`"))
}

/// Annotate a subtask with its Wayfinder metadata.
///
/// Payload: `{task_id, subtask_id, kind?, subtype?, autonomy?, fog?, origin_ticket?}`.
/// Every field is optional so a ticket can be refined as the fog lifts.
pub fn cli_decompose_ticket(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let task_id = payload
        .get("task_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let subtask_id = payload
        .get("subtask_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if task_id.is_empty() || subtask_id.is_empty() {
        return serde_json::json!({"updated": false, "error": "task_id and subtask_id are required"})
            .to_string();
    }
    let field = |name: &str| {
        payload
            .get(name)
            .and_then(|v| v.as_str())
            .map(str::to_string)
    };
    let (kind, subtype, autonomy, fog, origin) = (
        field("kind"),
        field("subtype"),
        field("autonomy"),
        field("fog"),
        field("origin_ticket"),
    );

    let mut errors: Vec<String> = Vec::new();
    for (name, value, allowed) in [
        ("kind", &kind, TICKET_KINDS),
        ("subtype", &subtype, TICKET_SUBTYPES),
        ("autonomy", &autonomy, AUTONOMY_MODES),
        ("fog", &fog, FOG_LEVELS),
    ] {
        if let Some(v) = value
            && let Some(err) = validate_enum(name, v, allowed)
        {
            errors.push(err);
        }
    }
    // The only decision work that may run unattended is GATHERING evidence.
    // Choosing between options, stress-testing one, or building a throwaway to
    // learn from all end in a judgement, and a judgement wants a human present.
    if kind.as_deref() == Some("decision")
        && autonomy.as_deref() == Some("afk")
        && subtype.as_deref() != Some("research")
    {
        errors.push(
            "a decision ticket may only be `afk` when its subtype is `research` —              gathering evidence can run unattended, reaching a verdict cannot"
                .to_string(),
        );
    }
    if !errors.is_empty() {
        return serde_json::json!({"updated": false, "errors": errors}).to_string();
    }

    let db = &rt.ctx.knowledge;
    ensure_decompose_tables(db);
    // Two things this statement has to get right:
    //   · COALESCE keeps every unset field at its current value, so refining one
    //     facet of a ticket never silently clears the others.
    //   · `subtask_id` is persisted SCOPED (`<task_id>::S-00`) while callers hold
    //     the SHORT id — the same mismatch that once left every dependent subtask
    //     permanently blocked. Matching both forms is what makes a short id update
    //     the row instead of silently touching none.
    let changed = db.conn_ref().execute(
        "UPDATE decomposition_subtasks \
            SET ticket_kind    = COALESCE(?1, ticket_kind), \
                ticket_subtype = COALESCE(?2, ticket_subtype), \
                autonomy       = COALESCE(?3, autonomy), \
                fog            = COALESCE(?4, fog), \
                origin_ticket  = COALESCE(?5, origin_ticket), \
                updated_at     = ?6 \
          WHERE task_id = ?7 \
            AND (subtask_id = ?8 OR subtask_id = ?7 || '::' || ?8)",
        params![
            kind,
            subtype,
            autonomy,
            fog,
            origin,
            chrono::Utc::now().to_rfc3339(),
            task_id,
            subtask_id
        ],
    );
    match changed {
        Ok(1) => serde_json::json!({
            "updated": true, "task_id": task_id, "subtask_id": subtask_id,
            "kind": kind, "subtype": subtype, "autonomy": autonomy,
            "fog": fog, "origin_ticket": origin,
        })
        .to_string(),
        Ok(_) => serde_json::json!({
            "updated": false, "error": "no such subtask in this task"
        })
        .to_string(),
        Err(e) => serde_json::json!({"updated": false, "error": e.to_string()}).to_string(),
    }
}

/// The frontier: the edge of what is known, and what to do about it.
///
/// Two things distinguish this from `ready`. First, it partitions by TICKET KIND,
/// because a decision that has not been made is not one more task to schedule — it
/// gates the work that depends on it, and planning past it produces a plan resting
/// on an unmade choice. Second, it reports the map's integrity: an implementation
/// ticket carries a POINTER back to the decision that produced it (map-as-index —
/// the reasoning lives in the ticket, the map only indexes it), and an
/// implementation ticket with no origin while decisions are still open is work
/// nobody can trace to a choice.
pub fn cli_decompose_frontier(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let task_id = payload
        .get("task_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if task_id.is_empty() {
        return serde_json::json!({"error": "task_id is required"}).to_string();
    }
    let db = &rt.ctx.knowledge;
    ensure_decompose_tables(db);

    #[derive(Clone)]
    struct Ticket {
        id: String,
        description: String,
        status: String,
        deps: Vec<String>,
        priority: i32,
        kind: String,
        subtype: Option<String>,
        autonomy: Option<String>,
        fog: String,
        origin: Option<String>,
        claimed_by: Option<String>,
    }

    let rows: Vec<Ticket> = {
        let Ok(mut stmt) = db.conn_ref().prepare(
            "SELECT subtask_id, description, status, depends_on, priority,                     ticket_kind, ticket_subtype, autonomy, fog, origin_ticket, claimed_by              FROM decomposition_subtasks WHERE task_id = ?1",
        ) else {
            return serde_json::json!({"error": "db error"}).to_string();
        };
        stmt.query_map(params![task_id], |r| {
            let deps_str = r.get::<_, String>(3)?;
            Ok(Ticket {
                id: r.get::<_, String>(0)?,
                description: r.get::<_, String>(1)?,
                status: r.get::<_, String>(2)?,
                deps: serde_json::from_str(&deps_str).unwrap_or_else(|_| {
                    deps_str
                        .split(',')
                        .filter(|s| !s.is_empty())
                        .map(String::from)
                        .collect()
                }),
                priority: r.get::<_, i32>(4)?,
                // An unlabelled ticket is implementation work: that is what a DAG
                // held before Wayfinder, so the default preserves what it meant.
                kind: r
                    .get::<_, Option<String>>(5)?
                    .unwrap_or_else(|| "implementation".to_string()),
                subtype: r.get::<_, Option<String>>(6)?,
                autonomy: r.get::<_, Option<String>>(7)?,
                // Fog is the one axis this feature exists to surface, so an
                // unassessed ticket reports `unknown` — not `clear`. Defaulting to
                // clear made the frontier answer "no fog here" about work nobody
                // had looked at yet, which is the reassuring lie the Wayfinder is
                // meant to prevent. `kind` above may default, because an unlabelled
                // ticket really was implementation work before this existed; there
                // is no equivalent prior meaning for unmeasured fog.
                fog: r
                    .get::<_, Option<String>>(8)?
                    .unwrap_or_else(|| "unknown".to_string()),
                origin: r.get::<_, Option<String>>(9)?,
                claimed_by: r.get::<_, Option<String>>(10)?,
            })
        })
        .map(|it| it.filter_map(Result::ok).collect())
        .unwrap_or_default()
    };

    let is_done = |s: &str| matches!(s, "completed" | "done" | "complete" | "failed" | "skipped");
    let completed: std::collections::HashSet<String> = rows
        .iter()
        .filter(|t| is_done(&t.status))
        .map(|t| short_subtask_id(&t.id))
        .collect();

    let mut open: Vec<&Ticket> = rows
        .iter()
        .filter(|t| {
            !is_done(&t.status)
                && t.deps
                    .iter()
                    .all(|d| completed.contains(&short_subtask_id(d)))
        })
        .collect();
    open.sort_by(|a, b| a.priority.cmp(&b.priority).then_with(|| a.id.cmp(&b.id)));

    let render = |t: &&Ticket| {
        serde_json::json!({
            "subtask_id": t.id,
            "description": t.description,
            "status": t.status,
            "subtype": t.subtype,
            "autonomy": t.autonomy,
            "fog": t.fog,
            "origin_ticket": t.origin,
            "claimed_by": t.claimed_by,
        })
    };
    let (decisions, implementation): (Vec<&Ticket>, Vec<&Ticket>) =
        open.iter().partition(|t| t.kind == "decision");

    let mut fog_counts = std::collections::BTreeMap::new();
    for t in rows.iter().filter(|t| !is_done(&t.status)) {
        *fog_counts.entry(t.fog.clone()).or_insert(0_usize) += 1;
    }

    // Work nobody can trace back to a choice, while choices are still open.
    let untraceable: Vec<&str> = implementation
        .iter()
        .filter(|t| t.origin.is_none())
        .map(|t| t.id.as_str())
        .collect();

    // T3.2 — Pocock names prototypes as the anti-waterfall device: "some folks
    // look at Wayfinder and think, god, that's a lot of planning, doesn't that
    // look like waterfall? The prototypes are the way that you prevent it from
    // becoming waterfall — huge amounts of low-fidelity upfront planning. A
    // prototype is a high-fidelity way to get feedback on what you're actually
    // building." A map carrying fog with no prototype anywhere in it IS waterfall
    // under another name: a plan whose uncertainty will only be tested after the
    // planning is over.
    //
    // Prototypes are counted across ALL tickets, closed included — a prototype
    // that already ran did its job of lifting fog, and demanding a fresh one
    // would punish the map for having worked.
    //
    // This NAMES the risk; it never blocks. Fog is legitimate, and a map may
    // rationally decide that research rather than a prototype is what lifts it.
    let foggy = rows
        .iter()
        .filter(|t| !is_done(&t.status) && t.fog != "clear")
        .count();
    let prototypes = rows
        .iter()
        .filter(|t| t.subtype.as_deref() == Some("prototype"))
        .count();
    let waterfall_risk = foggy > 0 && prototypes == 0;

    // T3.1: the decisions this map has already made, as recorded on the map.
    let resolutions: serde_json::Value = db
        .conn_ref()
        .query_row(
            "SELECT COALESCE(resolutions, '[]') FROM task_decompositions WHERE task_id = ?1",
            params![task_id],
            |r| r.get::<_, String>(0),
        )
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_else(|| serde_json::json!([]));

    let gated = !decisions.is_empty();
    serde_json::json!({
        "task_id": task_id,
        "gated_by_open_decisions": gated,
        "fog": fog_counts,
        "resolutions": resolutions,
        "waterfall_risk": {
            "at_risk": waterfall_risk,
            "foggy_open_tickets": foggy,
            "prototype_tickets": prototypes,
            "why": if waterfall_risk {
                "fog with no prototype ticket anywhere: the uncertainty will only be                  tested after the planning is over. A prototype is the high-fidelity                  way to get feedback on what is actually being built."
            } else if foggy == 0 {
                "no fog left to test"
            } else {
                "fog is present and at least one prototype ticket exists to test it"
            },
        },
        "decisions": decisions.iter().map(render).collect::<Vec<_>>(),
        "implementation": implementation.iter().map(render).collect::<Vec<_>>(),
        "untraceable_implementation": untraceable,
        "next_action": if gated {
            "resolve the decision tickets first — implementation planned past an              unmade decision is a plan resting on a guess"
        } else if implementation.is_empty() {
            "the frontier is empty: nothing is unblocked"
        } else {
            "no open decisions — implementation tickets may be claimed"
        },
    })
    .to_string()
}

/// Return ready-to-execute subtasks (deps satisfied) grouped by `parallel_group`.
///
/// Read-only by contract: this answers "what COULD start", which is why two
/// sessions calling it concurrently both receive the same subtask. To TAKE work,
/// use [`cli_decompose_claim`].
pub fn cli_decompose_ready(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let task_id = payload
        .get("task_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let only_ready = payload
        .get("only_ready")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let db = &rt.ctx.knowledge;
    ensure_decompose_tables(db);

    #[derive(Debug, Clone, serde::Serialize)]
    struct SubtaskInfo {
        subtask_id: String,
        status: String,
        depends_on: Vec<String>,
        parallel_group: Option<String>,
        priority: i32,
        /// Who holds the claim, when one exists (coordenação 2026-09-23: the
        /// views must name the owner, or "claimed by a live session" and
        /// "abandoned work" are indistinguishable).
        claimed_by: Option<String>,
        claim_expires_at: Option<i64>,
        /// Lease live RIGHT NOW (claimed_by set and unexpired at read time).
        claim_live: bool,
        /// The ticket declares `autonomy = 'hitl'` — the HUMAN's work. `ready`
        /// keeps listing it (it is unblocked work), but marked, so a display
        /// caller can tell "a session may take this" from "Gabriel's decision".
        hitl: bool,
    }

    let now_epoch = chrono::Utc::now().timestamp();

    // Load all subtasks for this task
    let subtasks: Vec<SubtaskInfo> = {
        let mut stmt = match db.conn_ref().prepare(
            "SELECT subtask_id, status, depends_on, parallel_group, priority, claimed_by, claim_expires_at, autonomy FROM decomposition_subtasks WHERE task_id = ?1",
        ) {
            Ok(s) => s,
            Err(_) => return serde_json::json!({"error": "db error"}).to_string(),
        };
        stmt.query_map(params![task_id], |r| {
            let deps_str = r.get::<_, String>(2)?;
            let deps: Vec<String> = serde_json::from_str(&deps_str).unwrap_or_else(|_| {
                deps_str
                    .split(',')
                    .filter(|s| !s.is_empty())
                    .map(String::from)
                    .collect()
            });
            let claimed_by: Option<String> = r.get::<_, Option<String>>(5)?;
            let claim_expires_at: Option<i64> = r.get::<_, Option<i64>>(6)?;
            let autonomy: Option<String> = r.get::<_, Option<String>>(7)?;
            Ok(SubtaskInfo {
                subtask_id: r.get::<_, String>(0)?,
                status: r.get::<_, String>(1)?,
                depends_on: deps,
                parallel_group: r.get::<_, Option<String>>(3)?,
                priority: r.get::<_, i32>(4)?,
                claim_live: claimed_by.is_some()
                    && claim_expires_at.is_some_and(|e| e > now_epoch),
                claimed_by,
                claim_expires_at,
                hitl: autonomy.as_deref() == Some("hitl"),
            })
        })
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
    };

    // Build a set of terminal-success subtask IDs, keyed by SHORT id.
    //
    // ROOT CAUSE of the DAG "ready" quirk: `subtask_id` is persisted SCOPED
    // (`<task_id>::S-00`) while `depends_on` holds SHORT ids (`S-00`). Comparing
    // them directly never matched, so every dependent stayed permanently blocked
    // even after its dependency completed. Normalize BOTH sides to the short id.
    // "done"/"complete" are accepted as aliases for "completed" (the loop-engineering
    // `loop_phase_close` marks subtasks "done").
    let short_id = |id: &str| id.rsplit("::").next().unwrap_or(id).to_string();
    let completed: std::collections::HashSet<String> = subtasks
        .iter()
        .filter(|s| {
            matches!(
                s.status.as_str(),
                "completed" | "done" | "complete" | "failed" | "skipped"
            )
        })
        .map(|s| short_id(&s.subtask_id))
        .collect();

    // A subtask is ready if all its deps (normalized to short ids) are completed.
    let is_ready = |deps: &[String]| deps.iter().all(|d| completed.contains(&short_id(d)));

    // Partition into ready (owned) and blocked (owned) — the SAME predicate the
    // claim uses: pending, or in_progress only under an EXPIRED lease. A live
    // claim never lists as ready again (coordenação 2026-09-23).
    let (mut ready, blocked): (Vec<SubtaskInfo>, Vec<SubtaskInfo>) =
        subtasks.into_iter().partition(|s| {
            is_ready(&s.depends_on)
                && ready_eligible(
                    &s.status,
                    s.claimed_by.as_deref(),
                    s.claim_expires_at,
                    now_epoch,
                )
        });

    // The SAME queue order the claim hands out (24/09/2026, analise-d4 via
    // touring-36): ready listed INSERTION order while claim sorted by
    // (priority, id), and `ready[0]` predicted the wrong subtask. One source.
    ready.sort_by(|a, b| queue_order(a.priority, &a.subtask_id, b.priority, &b.subtask_id));

    // Group ready subtasks by parallel_group
    let mut groups_map: std::collections::HashMap<Option<String>, Vec<serde_json::Value>> =
        std::collections::HashMap::new();
    for s in &ready {
        let group = s.parallel_group.clone();
        let entry = groups_map.entry(group).or_default();
        entry.push(serde_json::json!({
            "subtask_id": s.subtask_id,
            "priority": s.priority,
            "status": s.status,
            "hitl": s.hitl
        }));
    }

    let parallel_groups: Vec<serde_json::Value> = groups_map
        .into_iter()
        .map(|(group, members)| {
            serde_json::json!({
                "parallel_group": group,
                "members": members
            })
        })
        .collect();

    if only_ready {
        serde_json::json!({
            "task_id": task_id,
            "ready_subtasks": ready,
            "parallel_groups": parallel_groups
        })
        .to_string()
    } else {
        serde_json::json!({
            "task_id": task_id,
            "ready_subtasks": ready,
            "blocked_subtasks": blocked,
            "parallel_groups": parallel_groups
        })
        .to_string()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// S-3: Decompose-event handler — session-scoped task lifecycle via CLI
// ─────────────────────────────────────────────────────────────────────────────

/// Handle decompose-event subcommand from touring-hook binary.
///
/// Parses stdin JSON with event_type (TaskCreated | TaskCompleted),
/// session_id, and task_data. Maintains in-memory session→task mapping
/// via HookRuntime.decompose_event_state.
///
/// - TaskCreated: calls `touring decompose create <desc>` via Command,
///   stores session_id→task_id, returns {status: "ok", task_id, session_id}
/// - TaskCompleted: looks up task_id from session map, calls
///   `touring decompose add <task_id> <desc>`, removes from map,
///   returns {status: "ok"}
///
/// Fire-and-forget subprocess calls with 10s timeout.
/// Never fails — always returns ok with graceful degradation.
pub fn cli_decompose_event(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let event_type = payload
        .get("event_type")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let session_id = payload
        .get("session_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let task_data = payload
        .get("task_data")
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default();

    match event_type {
        "TaskCreated" => {
            let task_desc = task_data
                .get("description")
                .or_else(|| task_data.get("task_description"))
                .and_then(|v| v.as_str())
                .unwrap_or("");

            let desc = task_desc.to_string();
            // Sprint 2 PB (REGRA #19): reap to prevent <defunct> zombies.
            let _ = std::thread::spawn(move || {
                if let Ok(mut child) = std::process::Command::new("touring")
                    .args(["decompose", "create", "general", &desc])
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .spawn()
                {
                    let _ = child.wait();
                }
            });

            let session_prefix: String = session_id.chars().take(8).collect();
            let task_id = format!("decompose-session-{}", session_prefix);

            rt.decompose_event_state
                .insert(session_id.to_string(), task_id.clone());

            serde_json::json!({
                "status": "ok",
                "task_id": task_id,
                "session_id": session_id
            })
            .to_string()
        }
        "TaskCompleted" => {
            if let Some(task_id) = rt.decompose_event_state.remove(session_id) {
                let completion_desc = task_data
                    .get("description")
                    .or_else(|| task_data.get("result_summary"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("completed");

                let tid = task_id.clone();
                let desc = completion_desc.to_string();
                // Sprint 2 PB (REGRA #19): reap to prevent <defunct> zombies.
                let _ = std::thread::spawn(move || {
                    if let Ok(mut child) = std::process::Command::new("touring")
                        .args(["decompose", "add", &tid, &desc])
                        .stdout(std::process::Stdio::null())
                        .stderr(std::process::Stdio::null())
                        .spawn()
                    {
                        let _ = child.wait();
                    }
                });
            }
            serde_json::json!({
                "status": "ok"
            })
            .to_string()
        }
        _ => serde_json::json!({
            "status": "ok",
            "skipped": "unknown_event_type"
        })
        .to_string(),
    }
}

// ── workflow handlers extracted to cli/handlers/decompose_workflow.rs (F-9) ──
pub use crate::cli_handlers_decompose_workflow::{
    cli_workflow_compare, cli_workflow_resume, cli_workflow_run, cli_workflow_slowest,
    cli_workflow_stats, cli_workflow_status,
};

// ─────────────────────────────────────────────────────────────────────────────
// Wave P1 — Unit tests for priority enum mapping
// ─────────────────────────────────────────────────────────────────────────────

