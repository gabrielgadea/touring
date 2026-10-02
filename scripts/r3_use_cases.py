"""r3 use-case REGISTRY — one entry per decision the shadow observes (S-1).

Adding a use case in r4 (A3 destructive command, A4 model routing) means adding ONE entry here plus
one arm: the emitter, the evaluator and the KPI group by `use_case` and need no change.

The floors below are the numbers PRE-REGISTERED in
docs/plans/2026-10-02-jev-harness-r3/CRITERIOS-ACEITACAO.md (12:45:17 BRT, Errata 1 12:48:12 BRT).
They are frozen by sha256 in data/MANIFEST.sha256 at S-4; S-14 computes every verdict from THESE
constants, never from a hand-edited report. Changing one after the first replay is not allowed.
"""

SEED = 20261002
K_WINDOW = 3                    # Bash calls after an offer (E1.4)
SPLIT = (0.6, 0.2, 0.2)         # temporal, by session
AUDIT_N = 240                   # stratified by the B0 decision (E1.3): 120 followed / 120 not-followed
HARM_N = 100                    # would-drop blocks judged cross-vendor (C-A1-2)
BOOTSTRAP = 10_000
PREREGISTERED_AT = "2026-10-02T12:45:17-03:00"
ERRATA_1_AT = "2026-10-02T12:48:12-03:00"

ARMS_A2 = ("B0", "B-L", "B-J")
ARMS_A1 = ("B0", "B-D", "B-L", "B-J")

REGISTRY = {
    "a2_outcome": {
        "event_kind": "a2_offer_closed",
        "arms": ARMS_A2,
        "metric": "binary agreement on `followed` vs `not-followed` against the cross-vendor gold",
        "floors": {
            "delta_agreement": 0.05,          # C-A2-1: best model arm − B0, paired-bootstrap CI lower bound > 0
            "stratum_regression": -0.03,      # C-A2-2: no B0-stratum (n_test >= 20) loses more than this
            "precision": 0.90,                # C-A2-3: precision@covered with τ chosen on audit-val
            "coverage": 0.25,                 # C-A2-3
            "kappa_gold": 0.60,               # gold is INSUFFICIENT below this inter-rater κ
            "b0_sufficient_agreement": 0.95,  # B0 already this good ⇒ "regex is enough", no model adopted
            "p95_ms_local": 50,               # C-A2-4 (reported, not a quality failure)
            "p95_ms_api": 800,
        },
    },
    "a1_relevance": {
        "event_kind": "a1_block",
        "arms": ARMS_A1,
        "metric": "bytes saved over Touring-owned injection at followed-recall",
        "floors": {
            "bytes_saved_rule": 0.25,         # C-A1-1: B-D saves at least this share of Touring-owned bytes
            "followed_recall_rule": 1.00,     # C-A1-1: …dropping ZERO followed nudges
            "exempt_retained": 1.00,          # a single EXEMPT block dropped invalidates the arm
            "harm_lower_ci": 0.90,            # C-A1-2: harmless-to-drop rate, lower 95% CI, worst of the two raters
            "model_increment": 0.10,          # C-A1-3: model adds this many points of bytes over B-D …
            "followed_recall_model": 0.95,    # …at this followed-recall
            "budget_ceiling_bytes_per_turn": 3000,  # C-A1-4 (informative): context_budget ceiling
        },
    },
}
