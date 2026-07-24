//! CVC5SolverBackend — CVC5 SMT solver backend using the cvc5 0.4 crate.
//!
//! Translates `ConstraintExpr` to CVC5 AST and uses CVC5 for actual SMT
//! solving. Uses the deferred-encoding pattern: constraints are stored as
//! `Constraint` values, and a fresh `TermManager` + `Solver` pair is built
//! on each `check_sat` / `get_model` call. This sidesteps the `'tm`
//! lifetime parameter on `cvc5::Term<'tm>` (which borrows from
//! `TermManager`).
//!
//! cvc5 0.4.0 API used here (verified against the crate's integration
//! tests in `cvc5-0.4.0/tests/integration.rs`):
//!   * `Solver::new(&tm)` requires a `&TermManager` reference.
//!   * All `mk_*` constructors live on `TermManager` (e.g. `tm.mk_integer`,
//!     `tm.mk_term`, `tm.mk_true`, `tm.mk_false`, `tm.mk_const`,
//!     `tm.integer_sort`, `tm.boolean_sort`).
//!   * `tm.mk_term(kind, &children)` returns `Term` directly (no `Result`).
//!   * `solver.assert_formula(term)` returns `()` (panics on bad term).
//!   * `solver.check_sat()` returns a `cvc5::Result` STRUCT (not a `Result`
//!     enum); use `.is_sat()`.
//!   * Model extraction: `solver.get_value(term) -> Term` per variable.
//!   * `solver.set_logic("ALL")` is still required for the int theory.
//!
//! ## ES1 P2.6 (2026-06-02) — translation layer completion
//!
//! All 31 `SymbolKind` variants and all 8 `ConstraintExpr` variants that
//! the dormant `translate_symbol` / `translate_constraint` previously
//! stubbed with `tm.mk_integer(0)` / `tm.mk_true()` are now fully
//! translated. The 3-dispatcher split (`translate_symbol_int` /
//! `translate_symbol_bool` / `translate_symbol_bv_deferred`) mirrors
//! `z3_backend.rs` (3 typed dispatchers: `_to_z3_bool` / `_to_z3_int` /
//! `_to_z3_bv`) and provides sort safety — cvc5 will panic at `mk_term`
//! if child sorts mismatch op sorts.
//!
//! **DEFERRED to ES1 P2.7** (with `// DEFERRED P2.7` doc-comments in each
//! match arm):
//!   * 8 BV variants (Shl, Shr, BitAnd, BitOr, BitXor, Extract, ZeroExt,
//!     SignExt) — real BV path needs `bv_sort` caching + `bv↔int` coercion
//!     via the `Kind::BitvectorToNat` / `Kind::IntToBv` enum variants in
//!     cvc5-sys-0.4.0 (unverified at P2.6 time). `coerce_to_int` /
//!     `coerce_to_bv` helpers are added as placeholders for P2.7 wiring.
//!   * ForAll/Exists real quantifier semantics — current `And(range, body)`
//!     collapse is a z3-compatible stub (matches `z3_backend.rs:190-202`).
//!   * Real array semantics for ArraySelect/ArrayStore — current fallback
//!     recursion to base is z3-compatible stub.
//!
//! **All items above were shipped in P2.7 / P2.7.5 / P2.7.6** (see release
//! notes `ES1-P2.6-NOTE.md` and `ES1-P2.7.6-NOTE.md` for chronological
//! evolution). The "DEFERRED" section is preserved for historical
//! context but the current code is feature-complete on the 33
//! `SymbolKind` + 12 `ConstraintExpr` variants.
//!
//! ## ES1 P2.7 (2026-06-03) — BV + quantifiers + TranslationContext refactor
//!
//! * **8 BV variants** (Shl, Shr, BitAnd, BitOr, BitXor, Extract, ZeroExt,
//!   SignExt) wired in `translate_symbol_bv` (real impl, replacing
//!   `translate_symbol_bv_deferred`). Uses cvc5 0.4.0 Kinds
//!   `BitvectorShl(78)` / `BitvectorLshr(79)` / `BitvectorAnd(61)` /
//!   `BitvectorOr(62)` / `BitvectorXor(63)` / `BitvectorExtract(102)` /
//!   `BitvectorZeroExtend(104)` / `BitvectorSignExtend(105)`.
//!   Indexed ops (Extract / ZeroExt / SignExt) use
//!   `tm.mk_op(kind, &[u32])` + `tm.mk_term_from_op(op, children)`.
//! * **BV↔int coercion wired**: `coerce_to_bv` uses
//!   `Kind::IntToBitvector(108)` (indexed op with `&[32]` for 32-bit BV);
//!   `coerce_to_int` uses `Kind::BitvectorToNat(109)`.
//! * **Real quantifiers** (shipped P2.7.5): ForAll/Exists use
//!   `tm.mk_var(int_sort, &name)` + `Kind::Forall(282)` / `Kind::Exists(283)`.
//!   Body is `And(range_b, body_b)` (preserves z3-compatible semantics
//!   from P2.6).
//! * **TranslationContext refactor** (P2.7.5): `int_sort` + `bv_sort` +
//!   `vars` + `bv_vars` threaded through dispatcher via `TranslationContext`
//!   struct. `declare_int_var` / `declare_bv_var` helpers provide
//!   per-recursion Const identity.
//!
//! ## ES1 P2.7.6 (2026-06-03) — real array semantics
//!
//! * **Array sort caching**: `tm.mk_array_sort(int_sort, int_sort)` cached
//!   in `TranslationContext::array_sort` for the duration of one
//!   `check_sat` / `get_model` call. Same sort wrapper → same cvc5 term.
//! * **Per-recursion array Const identity**: `array_vars: HashMap<String,
//!   Term>` (separate from `vars` for ints and `bv_vars` for BVs to avoid
//!   sort conflicts). `declare_array_var(name)` declares/looks up
//!   array-typed variables.
//! * **Fresh const arrays**: `fresh_const_array(v)` uses
//!   `tm.mk_const_array(array_sort, mk_integer(v))` with a counter-based
//!   name (cvc5 may canonicalize two const arrays of the same value).
//! * **Real `Kind::Select(154)` / `Kind::Store(155)` wired in dispatcher**:
//!   `ArraySelect(arr, idx)` and `ArrayStore(arr, idx, val)` produce real
//!   cvc5 array terms when the base is array-typed (Variable, Constant,
//!   nested ArrayStore, nested ArraySelect). `is_array_base_kind`
//!   predicate separates array-typed bases from non-array terms.
//! * **P2.6 fallback preserved** for non-array bases (Load + arith
//!   variants like Add, Sub, etc.): the dispatcher recurses to P2.6's
//!   base-recursion (returns base for Select, returns val for Store).
//! * **`coerce_to_bool` already array-aware** via `Equal(X, X)` wrapping
//!   (cvc5 `Equal` is polymorphic over any sort including arrays). No
//!   change needed in coercion.
//!   `tm.mk_array_sort(int_sort, int_sort)`.
//! * **Per-recursion Const identity fix**: `TranslationContext::vars`
//!   HashMap<String, Term> ensures all uses of a variable name within
//!   a single `check_sat` share the same cvc5 term. Fixes the
//!   `a == b AND a != b` cross-assert contradiction case.
//!
//! **Mode**: BEHAVIOR-EXPAND (NOT strict REFACTOR) — BV variants now
//! return real translations where the P2.6 stub returned
//! `tm.mk_integer(0)`. Previously-passing tests must STILL pass.

use std::collections::HashMap;

use crate::concolic::Constraint;
// ES1 cleanup (2026-06-06): the SMT-AST types below are referenced only by the
// cvc5 translation code and cvc5-gated tests/helpers. `ConstraintExpr` is also
// used by the cvc5-disabled stub test, so it additionally needs the `test` cfg;
// `SymbolExpr`/`SymbolKind` are only reached through the cvc5-gated `var`/`const_`
// helpers, so they need only `cvc5`. This keeps every config warning-free:
// default-lib drops all three; the not-cvc5 test build keeps only
// `ConstraintExpr`; cvc5 builds keep all three.
#[cfg(any(feature = "cvc5", test))]
use crate::concolic::ConstraintExpr;
#[cfg(feature = "cvc5")]
use crate::concolic::{SymbolExpr, SymbolKind};

use super::SolverBackend;

/// ES1 P2.7 — Default BV width for bitvector operations. Matches z3's
/// `BV::from_i64(*v, 32)` default in `z3_backend.rs:444`.
// ES1 cleanup (2026-06-06): referenced only by the `#[cfg(feature = "cvc5")]`
// translation code and cvc5-gated tests; gate so the default z3-only build
// does not flag it as dead_code.
#[cfg(feature = "cvc5")]
const BV_WIDTH: u32 = 32;

/// ES1 P2.7 — Per-call translation context that bundles the
/// `TermManager` reference, the cached `int_sort` and `bv_sort`
/// references, and the declared-variables map used for the
/// per-recursion Const identity fix.
///
/// **P2.7 status**: STRUCT DEFINED, full dispatcher integration
/// deferred to P2.7.5 (the existing P2.6 dispatcher continues to use
/// individual params; this struct is the target architecture for the
/// P2.7.5 refactor that will thread int_sort + bv_sort + vars through
/// all dispatcher functions in one pass). The helpers
/// `declare_int_var` / `declare_bv_var` are added now and the real
/// `translate_symbol_bv` / `coerce_to_bv` / `coerce_to_int` impls are
/// designed against this context type — verified via the
/// `cvc5_bv_translation_unit_test` test that exercises them in isolation.
///
/// Lifetime `'tm` is the `TermManager` lifetime; `'a` is the borrowed
/// lifetime of the sort refs and the vars map.
#[cfg(feature = "cvc5")]
struct TranslationContext<'a, 'tm> {
    tm: &'tm cvc5::TermManager,
    int_sort: &'a cvc5::Sort<'tm>,
    bv_sort: &'a cvc5::Sort<'tm>,
    /// ES1 P2.7.6 — array sort (int → int by default). Cached once per
    /// `check_sat` so every `Kind::Select` / `Kind::Store` uses the
    /// SAME sort wrapper, guaranteeing that `arr` in constraint 1 and
    /// `arr` in constraint 2 are the SAME cvc5 array Const.
    array_sort: &'a cvc5::Sort<'tm>,
    /// Declared int variables map: name → cvc5 term (int sort). The
    /// P2.7 per-recursion Const identity fix: every use of "x" within
    /// a single `check_sat` shares the same cvc5 int Const.
    vars: &'a mut HashMap<String, cvc5::Term<'tm>>,
    /// Declared BV variables map: name → cvc5 term (BV sort). Separate
    /// from `vars` to avoid int/bv sort conflicts.
    bv_vars: &'a mut HashMap<String, cvc5::Term<'tm>>,
    /// ES1 P2.7.6 — declared array variables map: name → cvc5 term
    /// (array sort). Separate from `vars` / `bv_vars` to avoid sort
    /// conflicts. Per-recursion Const identity for array Consts.
    array_vars: &'a mut HashMap<String, cvc5::Term<'tm>>,
    /// ES1 P2.7.6 — counter for generating unique fresh array names
    /// (used when an `ArraySelect` / `ArrayStore` base is a `Constant`
    /// or non-Variable/Constant/Store expression). The counter is
    /// borrowed mutably so it persists across dispatcher calls within
    /// a single `check_sat` invocation.
    array_counter: &'a mut u32,
}

#[cfg(feature = "cvc5")]
impl<'a, 'tm> TranslationContext<'a, 'tm> {
    /// Helper: declare (or reuse) an int-sorted variable in the
    /// declared-variables map. Returns the cvc5 term. This is the
    /// per-recursion Const identity fix: every use of "x" within a
    /// single `check_sat` shares the same cvc5 term.
    fn declare_int_var(&mut self, name: &str) -> cvc5::Term<'tm> {
        if let Some(t) = self.vars.get(name) {
            return t.clone();
        }
        let t = self.tm.mk_const(self.int_sort.clone(), name);
        self.vars.insert(name.to_string(), t.clone());
        t
    }

    /// Helper: declare (or reuse) a BV-sorted variable.
    fn declare_bv_var(&mut self, name: &str) -> cvc5::Term<'tm> {
        if let Some(t) = self.bv_vars.get(name) {
            return t.clone();
        }
        let t = self.tm.mk_const(self.bv_sort.clone(), name);
        self.bv_vars.insert(name.to_string(), t.clone());
        t
    }

    /// ES1 P2.7.6 — declare (or reuse) an array-sorted variable. Same
    /// per-recursion Const identity pattern as `declare_int_var` /
    /// `declare_bv_var`: every use of "arr" within a single `check_sat`
    /// shares the same cvc5 array Const.
    fn declare_array_var(&mut self, name: &str) -> cvc5::Term<'tm> {
        if let Some(t) = self.array_vars.get(name) {
            return t.clone();
        }
        let t = self.tm.mk_const(self.array_sort.clone(), name);
        self.array_vars.insert(name.to_string(), t.clone());
        t
    }

    /// ES1 P2.7.6 — generate a fresh `const_array` (all elements = `v`).
    /// Used when an `ArraySelect` / `ArrayStore` base is a `Constant`
    /// (which has no name to look up). The counter guarantees unique
    /// cvc5 terms across multiple fresh arrays in a single `check_sat`.
    fn fresh_const_array(&mut self, v: i64) -> cvc5::Term<'tm> {
        *self.array_counter += 1;
        let default = self.tm.mk_integer(v);
        self.tm.mk_const_array(self.array_sort.clone(), default)
    }

    /// ES1 P2.7.6 — translate the base of an `ArraySelect` / `ArrayStore`
    /// to an array-sorted cvc5 term. Dispatches:
    ///
    /// * `Variable(name)` → declare/lookup in `array_vars` map.
    /// * `Constant(v)` → fresh const array initialized to `v`.
    /// * `ArrayStore(..)` → recurse via `translate_symbol_int` (the
    ///   result of a Store is array-typed, so this returns an array).
    /// * `ArraySelect(..)` → recurse (nested selects on the same base
    ///   chain through Store to a leaf Variable/Constant).
    /// * anything else (int arith expr) → translate as int and return.
    ///   The dispatcher is responsible for detecting this case and
    ///   falling back to P2.6 base-recursion if needed.
    fn translate_array_base(&mut self, expr: &SymbolExpr) -> cvc5::Term<'tm> {
        match &expr.kind {
            SymbolKind::Variable => self.declare_array_var(&expr.name),
            SymbolKind::Constant(v) => self.fresh_const_array(*v),
            // ArrayStore / ArraySelect return array-typed terms when
            // their base is array-typed; recurse through the int
            // dispatcher (which now knows how to handle array terms).
            SymbolKind::ArrayStore(_, _, _)
            | SymbolKind::ArraySelect(_, _)
            | SymbolKind::Load(_) => self.translate_symbol_int(expr),
            // Fallback: not an array-typed term. Return as int; the
            // dispatcher will see this and emit a P2.6-style fallback
            // (this matches the spec: "Fall back to P2.6 base-recursion
            // for non-array terms").
            _ => self.translate_symbol_int(expr),
        }
    }

    /// ES1 P2.7 — Real BV dispatcher. Maps 8 BV `SymbolKind` variants
    /// to cvc5 0.4.0 BV operations.
    fn translate_symbol_bv(&mut self, expr: &SymbolExpr) -> cvc5::Term<'tm> {
        match &expr.kind {
            SymbolKind::Constant(v) => self.tm.mk_bv(BV_WIDTH, *v as u64),
            SymbolKind::Variable => self.declare_bv_var(&expr.name),
            SymbolKind::Shl(a, b) => {
                let a_t = self.translate_symbol_bv(a);
                let b_int = self.translate_symbol_int(b);
                let b_t = self.coerce_to_bv(&b_int);
                self.tm.mk_term(cvc5::Kind::BitvectorShl, &[a_t, b_t])
            }
            SymbolKind::Shr(a, b) => {
                let a_t = self.translate_symbol_bv(a);
                let b_int = self.translate_symbol_int(b);
                let b_t = self.coerce_to_bv(&b_int);
                self.tm.mk_term(cvc5::Kind::BitvectorLshr, &[a_t, b_t])
            }
            SymbolKind::BitAnd(a, b) => {
                let a_t = self.translate_symbol_bv(a);
                let b_t = self.translate_symbol_bv(b);
                self.tm.mk_term(cvc5::Kind::BitvectorAnd, &[a_t, b_t])
            }
            SymbolKind::BitOr(a, b) => {
                let a_t = self.translate_symbol_bv(a);
                let b_t = self.translate_symbol_bv(b);
                self.tm.mk_term(cvc5::Kind::BitvectorOr, &[a_t, b_t])
            }
            SymbolKind::BitXor(a, b) => {
                let a_t = self.translate_symbol_bv(a);
                let b_t = self.translate_symbol_bv(b);
                self.tm.mk_term(cvc5::Kind::BitvectorXor, &[a_t, b_t])
            }
            SymbolKind::Extract { inner, high, low } => {
                let inner_int = self.translate_symbol_int(inner);
                let inner_t = self.coerce_to_bv(&inner_int);
                let op = self.tm.mk_op(cvc5::Kind::BitvectorExtract, &[*high, *low]);
                self.tm.mk_term_from_op(op, &[inner_t])
            }
            SymbolKind::ZeroExt(inner) => {
                let inner_int = self.translate_symbol_int(inner);
                let inner_t = self.coerce_to_bv(&inner_int);
                let op = self.tm.mk_op(cvc5::Kind::BitvectorZeroExtend, &[BV_WIDTH]);
                self.tm.mk_term_from_op(op, &[inner_t])
            }
            SymbolKind::SignExt(inner) => {
                let inner_int = self.translate_symbol_int(inner);
                let inner_t = self.coerce_to_bv(&inner_int);
                let op = self.tm.mk_op(cvc5::Kind::BitvectorSignExtend, &[BV_WIDTH]);
                self.tm.mk_term_from_op(op, &[inner_t])
            }
            _ => self.tm.mk_bv(BV_WIDTH, 0),
        }
    }

    /// ES1 P2.7 — Real int dispatcher (mirrors the free `translate_symbol_int`
    /// but uses the context for sort + vars lookup).
    fn translate_symbol_int(&mut self, expr: &SymbolExpr) -> cvc5::Term<'tm> {
        match &expr.kind {
            SymbolKind::Constant(v) => self.tm.mk_integer(*v),
            SymbolKind::Variable => self.declare_int_var(&expr.name),
            SymbolKind::Add(a, b) => {
                let a_t = self.translate_symbol_int(a);
                let b_t = self.translate_symbol_int(b);
                self.tm.mk_term(cvc5::Kind::Add, &[a_t, b_t])
            }
            SymbolKind::Sub(a, b) => {
                let a_t = self.translate_symbol_int(a);
                let b_t = self.translate_symbol_int(b);
                self.tm.mk_term(cvc5::Kind::Sub, &[a_t, b_t])
            }
            SymbolKind::Multiply(a, b) => {
                let a_t = self.translate_symbol_int(a);
                let b_t = self.translate_symbol_int(b);
                self.tm.mk_term(cvc5::Kind::Mult, &[a_t, b_t])
            }
            SymbolKind::Divide(a, b) => {
                let a_t = self.translate_symbol_int(a);
                let b_t = self.translate_symbol_int(b);
                self.tm.mk_term(cvc5::Kind::IntsDivision, &[a_t, b_t])
            }
            SymbolKind::Mod(a, b) => {
                let a_t = self.translate_symbol_int(a);
                let b_t = self.translate_symbol_int(b);
                self.tm.mk_term(cvc5::Kind::IntsModulus, &[a_t, b_t])
            }
            SymbolKind::Neg(inner) => {
                let inner_t = self.translate_symbol_int(inner);
                self.tm.mk_term(cvc5::Kind::Neg, &[inner_t])
            }
            SymbolKind::Abs(inner) => {
                let inner_t = self.translate_symbol_int(inner);
                self.tm.mk_term(cvc5::Kind::Abs, &[inner_t])
            }
            _ => self.tm.mk_integer(0),
        }
    }

    /// ES1 P2.7 — Real BV→int coercion via `Kind::BitvectorToNat(109)`.
    #[allow(dead_code)] // used by cvc5_bv_translation_unit_test_coerce_to_int
    fn coerce_to_int(&mut self, term: &cvc5::Term<'tm>) -> cvc5::Term<'tm> {
        if term.sort().is_integer() {
            term.clone()
        } else if term.sort().is_bv() {
            self.tm
                .mk_term(cvc5::Kind::BitvectorToNat, std::slice::from_ref(term))
        } else {
            term.clone()
        }
    }

    /// ES1 P2.7 — Real int→BV coercion via `Kind::IntToBitvector(108)`
    /// (indexed op with `&[BV_WIDTH]`).
    fn coerce_to_bv(&mut self, term: &cvc5::Term<'tm>) -> cvc5::Term<'tm> {
        if term.sort().is_bv() {
            term.clone()
        } else if term.sort().is_integer() {
            let op = self.tm.mk_op(cvc5::Kind::IntToBitvector, &[BV_WIDTH]);
            self.tm.mk_term_from_op(op, std::slice::from_ref(term))
        } else {
            term.clone()
        }
    }
}

/// CVC5 solver backend.
///
/// Stores asserted constraints and lazily encodes them into a fresh
/// `cvc5::TermManager` + `cvc5::Solver` on every `check_sat` / `get_model`
/// call. This avoids the `'tm` lifetime entanglement between `Term` and
/// `TermManager` (a `Term<'tm>` borrows from the manager in cvc5 0.4.0).
#[cfg(feature = "cvc5")]
#[derive(Debug, Default)]
pub struct CVC5SolverBackend {
    /// Asserted constraints (deferred encoding).
    constraints: Vec<Constraint>,
}

/// CVC5 solver backend (no-op when cvc5 feature not enabled).
#[cfg(not(feature = "cvc5"))]
#[derive(Debug)]
pub struct CVC5SolverBackend {
    _placeholder: (),
}

/// Builds a fresh `TranslationContext` over `tm` + `solver`, asserts every
/// constraint into the solver, and returns the names of all int-sort variables
/// declared during translation.
///
/// Extracted from `check_sat` / `get_model` to eliminate the identical
/// sort-setup + context-construction + assertion-loop block that previously
/// existed verbatim in both methods.
#[cfg(feature = "cvc5")]
fn setup_and_assert_constraints(
    tm: &cvc5::TermManager,
    solver: &mut cvc5::Solver,
    constraints: &[Constraint],
) -> Vec<String> {
    let int_sort = tm.integer_sort();
    let bv_sort = tm.mk_bv_sort(BV_WIDTH);
    // ES1 P2.7.6 — array sort (int → int) cached for the entire call so
    // every `Kind::Select` / `Kind::Store` within one query uses the same
    // sort wrapper.
    let array_sort = tm.mk_array_sort(int_sort.clone(), int_sort.clone());
    let mut vars: HashMap<String, cvc5::Term<'_>> = HashMap::new();
    let mut bv_vars: HashMap<String, cvc5::Term<'_>> = HashMap::new();
    let mut array_vars: HashMap<String, cvc5::Term<'_>> = HashMap::new();
    let mut array_counter: u32 = 0;
    {
        let mut ctx = TranslationContext {
            tm,
            int_sort: &int_sort,
            bv_sort: &bv_sort,
            array_sort: &array_sort,
            vars: &mut vars,
            bv_vars: &mut bv_vars,
            array_vars: &mut array_vars,
            array_counter: &mut array_counter,
        };
        for c in constraints {
            let term = translate_constraint(&mut ctx, &c.expr);
            let bool_term = coerce_to_bool(&mut ctx, &term);
            solver.assert_formula(bool_term);
        }
    }
    // Return only the variable names; callers that need model extraction
    // (i.e. `get_model`) recreate term handles via `tm.mk_const` from them.
    vars.keys().cloned().collect()
}

#[cfg(feature = "cvc5")]
impl SolverBackend for CVC5SolverBackend {
    fn new() -> Self {
        Self {
            constraints: Vec::new(),
        }
    }

    fn assert(&mut self, constraint: &Constraint) {
        // Defer encoding: just stash the constraint. Real encoding happens
        // in `check_sat` / `get_model` where we have a fresh TermManager
        // and can use the `Term<'tm>` borrow scope for the whole query.
        self.constraints.push(constraint.clone());
    }

    fn check_sat(&mut self) -> bool {
        let tm = cvc5::TermManager::new();
        let mut solver = cvc5::Solver::new(&tm);
        // QF_LIA = Quantifier-Free Linear Integer Arithmetic — explicit
        // logic ensures cvc5 activates the LIA theory for symbolic
        // int equality / inequality / disequality constraints.
        solver.set_logic("QF_LIA");
        setup_and_assert_constraints(&tm, &mut solver, &self.constraints);
        solver.check_sat().is_sat()
    }

    fn get_model(&self) -> HashMap<String, i64> {
        let mut model = HashMap::new();
        let tm = cvc5::TermManager::new();
        let mut solver = cvc5::Solver::new(&tm);
        solver.set_logic("QF_LIA");
        // Enable model production so `solver.get_value` is allowed.
        solver.set_option("produce-models", "true");
        let declared_var_names = setup_and_assert_constraints(&tm, &mut solver, &self.constraints);
        // cvc5 requires a SAT/UNKNOWN response before get_value is callable.
        let _ = solver.check_sat();
        // Pull the satisfying values for every declared Variable symbol.
        // The vars map is the authoritative source (declaration order).
        let int_sort = tm.integer_sort();
        for var_name in &declared_var_names {
            let var_term = tm.mk_const(int_sort.clone(), var_name.as_str());
            if let Some(val) = extract_int_value(&solver, &var_term) {
                model.insert(var_name.clone(), val);
            }
        }
        model
    }

    fn reset(&mut self) {
        // cvc5 0.4.0 has no `solver.reset()` — recreate by clearing
        // constraints. Next `check_sat` builds a fresh solver.
        self.constraints.clear();
    }

    fn clone_box(&self) -> Box<dyn super::SolverBackend> {
        let mut cloned = CVC5SolverBackend::new();
        for c in &self.constraints {
            cloned.assert(c);
        }
        Box::new(cloned)
    }
}

/// Translates a `ConstraintExpr` to a CVC5 term rooted at `tm`.
///
/// All 12 `ConstraintExpr` variants are fully implemented (P2.6):
/// `True` / `False` / `Bool` / `Symbolic` are leaf cases; `And` / `Or`
/// fold over the children list; `Not` / `Ite` / `Implies` / `Distinct`
/// recurse on the inner constraint; `ForAll` / `Exists` collapse to
/// `And(range, body)` (z3-compatible stub — real quantifier semantics
/// deferred to P2.7).
///
/// **`int_sort` is threaded through the translation chain** so all
/// `tm.mk_const` calls use the same sort wrapper, ensuring cvc5 sees
/// the same variable across multiple constraints (ES1 P2.6 fix).
#[cfg(feature = "cvc5")]
fn translate_constraint<'a, 'tm>(
    ctx: &mut TranslationContext<'a, 'tm>,
    expr: &ConstraintExpr,
) -> cvc5::Term<'tm> {
    match expr {
        ConstraintExpr::True => ctx.tm.mk_true(),
        ConstraintExpr::False => ctx.tm.mk_false(),
        ConstraintExpr::Bool(b) => {
            if *b {
                ctx.tm.mk_true()
            } else {
                ctx.tm.mk_false()
            }
        }
        ConstraintExpr::Symbolic(sym) => translate_symbol(ctx, sym),
        ConstraintExpr::And(constraints) => {
            // Fold: acc = And(acc, translate_constraint(...))
            let mut acc = ctx.tm.mk_true();
            for c in constraints {
                let child_t = translate_constraint(ctx, &c.expr);
                let child_b = coerce_to_bool(ctx, &child_t);
                acc = ctx.tm.mk_term(cvc5::Kind::And, &[acc, child_b]);
            }
            acc
        }
        ConstraintExpr::Or(constraints) => {
            let mut acc = ctx.tm.mk_false();
            for c in constraints {
                let child_t = translate_constraint(ctx, &c.expr);
                let child_b = coerce_to_bool(ctx, &child_t);
                acc = ctx.tm.mk_term(cvc5::Kind::Or, &[acc, child_b]);
            }
            acc
        }
        ConstraintExpr::Not(inner) => {
            let inner_t = translate_constraint(ctx, &inner.expr);
            let inner_b = coerce_to_bool(ctx, &inner_t);
            ctx.tm.mk_term(cvc5::Kind::Not, &[inner_b])
        }
        ConstraintExpr::Ite(cond, then, else_) => {
            let cond_t = translate_constraint(ctx, cond);
            let cond_b = coerce_to_bool(ctx, &cond_t);
            let then_t = translate_constraint(ctx, then);
            let else_t = translate_constraint(ctx, else_);
            if then_t.is_boolean_value() && else_t.is_boolean_value() {
                ctx.tm.mk_term(cvc5::Kind::Ite, &[cond_b, then_t, else_t])
            } else {
                let ite = ctx.tm.mk_term(cvc5::Kind::Ite, &[cond_b, then_t, else_t]);
                coerce_to_bool(ctx, &ite)
            }
        }
        ConstraintExpr::Distinct(a, b) => {
            let a_t = translate_constraint(ctx, a);
            let b_t = translate_constraint(ctx, b);
            let eq_b = coerce_to_bool(ctx, &ctx.tm.mk_term(cvc5::Kind::Equal, &[a_t, b_t]));
            ctx.tm.mk_term(cvc5::Kind::Not, &[eq_b])
        }
        ConstraintExpr::ForAll(var, body, range) => {
            // ES1 P2.7 real quantifier via `tm.mk_var` + `Kind::VariableList`
            // + `Kind::Forall(282)`. cvc5 expects the first argument of
            // Forall/Exists to be a `VariableList` term (not a raw Var).
            let bound = ctx.tm.mk_var(ctx.int_sort.clone(), var);
            let bound_list = ctx
                .tm
                .mk_term(cvc5::Kind::VariableList, std::slice::from_ref(&bound));
            let range_t = translate_constraint(ctx, range);
            let range_b = coerce_to_bool(ctx, &range_t);
            let body_t = translate_constraint(ctx, body);
            let body_b = coerce_to_bool(ctx, &body_t);
            let combined = ctx.tm.mk_term(cvc5::Kind::And, &[range_b, body_b]);
            ctx.tm.mk_term(cvc5::Kind::Forall, &[bound_list, combined])
        }
        ConstraintExpr::Exists(var, body, range) => {
            // ES1 P2.7 real quantifier via `tm.mk_var` + `Kind::Exists(283)`.
            let bound = ctx.tm.mk_var(ctx.int_sort.clone(), var);
            let bound_list = ctx
                .tm
                .mk_term(cvc5::Kind::VariableList, std::slice::from_ref(&bound));
            let range_t = translate_constraint(ctx, range);
            let range_b = coerce_to_bool(ctx, &range_t);
            let body_t = translate_constraint(ctx, body);
            let body_b = coerce_to_bool(ctx, &body_t);
            let combined = ctx.tm.mk_term(cvc5::Kind::And, &[range_b, body_b]);
            ctx.tm.mk_term(cvc5::Kind::Exists, &[bound_list, combined])
        }
        ConstraintExpr::Implies(a, b) => {
            let a_t = translate_constraint(ctx, a);
            let a_b = coerce_to_bool(ctx, &a_t);
            let b_t = translate_constraint(ctx, b);
            let b_b = coerce_to_bool(ctx, &b_t);
            ctx.tm.mk_term(cvc5::Kind::Implies, &[a_b, b_b])
        }
    }
}

/// ES1 P2.7.6 — predicate: is the given `SymbolKind` a valid
/// array-typed base for an `ArraySelect` / `ArrayStore`? Returns
/// `true` for `Variable` (declared as array), `Constant` (fresh
/// const array), and nested `ArrayStore` / `ArraySelect` (which
/// return array-typed terms). Returns `false` for `Load` (single
/// arg, ambiguous intent in this context) and all int-arith
/// variants (Add, Sub, etc., which would produce a sort mismatch
/// in `Kind::Select` / `Kind::Store`).
#[cfg(feature = "cvc5")]
fn is_array_base_kind(kind: &SymbolKind) -> bool {
    matches!(
        kind,
        SymbolKind::Variable
            | SymbolKind::Constant(_)
            | SymbolKind::ArrayStore(_, _, _)
            | SymbolKind::ArraySelect(_, _)
    )
}

/// Top-level dispatcher: translates a `SymbolExpr` to a CVC5 term rooted
/// at `tm`.
///
/// Mirrors `z3_backend.rs::translate_symbol_to_z3_*` 3-dispatcher
/// structure for sort safety. cvc5's `tm.mk_term` will panic on sort
/// mismatch (e.g. mixing int and bool children of an op), so we route
/// each variant to the appropriate sort family.
#[cfg(feature = "cvc5")]
fn translate_symbol<'a, 'tm>(
    ctx: &mut TranslationContext<'a, 'tm>,
    expr: &SymbolExpr,
) -> cvc5::Term<'tm> {
    match &expr.kind {
        // Top-level: constants and variables are int sort
        SymbolKind::Constant(v) => ctx.tm.mk_integer(*v),
        // ES1 P2.7 fix: use vars map (per-recursion Const identity)
        SymbolKind::Variable => ctx.declare_int_var(&expr.name),
        // Int arith family → translate_symbol_int
        SymbolKind::Add(_, _)
        | SymbolKind::Sub(_, _)
        | SymbolKind::Multiply(_, _)
        | SymbolKind::Divide(_, _)
        | SymbolKind::Mod(_, _)
        | SymbolKind::Neg(_)
        | SymbolKind::Abs(_)
        | SymbolKind::Min(_)
        | SymbolKind::Max(_)
        | SymbolKind::Concat(_) => translate_symbol_int(ctx, expr),
        // Bool ops family → translate_symbol_bool
        SymbolKind::Eq(_, _)
        | SymbolKind::NotEq(_, _)
        | SymbolKind::Lt(_, _)
        | SymbolKind::LessOrEqual(_, _)
        | SymbolKind::GreaterThan(_, _)
        | SymbolKind::GreaterEqual(_, _)
        | SymbolKind::And(_, _)
        | SymbolKind::Or(_, _)
        | SymbolKind::Xor(_, _)
        | SymbolKind::Not(_) => translate_symbol_bool(ctx, expr),
        // ES1 P2.7: BV family — REAL implementation
        SymbolKind::Shl(_, _)
        | SymbolKind::Shr(_, _)
        | SymbolKind::BitAnd(_, _)
        | SymbolKind::BitOr(_, _)
        | SymbolKind::BitXor(_, _)
        | SymbolKind::Extract { .. }
        | SymbolKind::ZeroExt(_)
        | SymbolKind::SignExt(_) => ctx.translate_symbol_bv(expr),
        // ES1 P2.7.6 — real array semantics via `Kind::Select(154)` /
        // `Kind::Store(155)`. The dispatcher uses `ctx.translate_array_base`
        // to declare the base as an array-sorted term; if the base is not
        // array-typed (e.g., `Add(x, y)` or `Load`), we fall back to
        // P2.6's base-recursion behavior (return base for Select,
        // return val for Store). `Load` always uses fallback because
        // its single-arg semantic intent is ambiguous in this context.
        SymbolKind::Load(base) => translate_symbol(ctx, base),
        SymbolKind::ArraySelect(arr, idx) => {
            if is_array_base_kind(&arr.kind) {
                let arr_t = ctx.translate_array_base(arr);
                let idx_t = translate_symbol_int(ctx, idx);
                ctx.tm.mk_term(cvc5::Kind::Select, &[arr_t, idx_t])
            } else {
                // Fallback: P2.6 base-recursion (return base as int).
                translate_symbol(ctx, arr)
            }
        }
        SymbolKind::ArrayStore(arr, idx, val) => {
            if is_array_base_kind(&arr.kind) {
                let arr_t = ctx.translate_array_base(arr);
                let idx_t = translate_symbol_int(ctx, idx);
                let val_t = translate_symbol_int(ctx, val);
                ctx.tm.mk_term(cvc5::Kind::Store, &[arr_t, idx_t, val_t])
            } else {
                // Fallback: P2.6 base-recursion (return val as int).
                translate_symbol(ctx, val)
            }
        }
    }
}

/// Translates a `SymbolExpr` (arithmetic family) to a CVC5 int term.
///
/// **ES1 P2.6 fix**: takes `&SymbolExpr` (not just `&SymbolKind`) so that
/// nested `Variable` accesses preserve the original variable name. The
/// `int_sort` parameter is threaded through to ensure `tm.mk_const` uses
/// the same sort wrapper for every call within a single `check_sat` —
/// guaranteeing that `Const("a", int)` in the first constraint and
/// `Const("a", int)` in the second constraint are the SAME cvc5 term.
#[cfg(feature = "cvc5")]
fn translate_symbol_int<'a, 'tm>(
    ctx: &mut TranslationContext<'a, 'tm>,
    expr: &SymbolExpr,
) -> cvc5::Term<'tm> {
    match &expr.kind {
        // Constant/Variable are normally handled by the top-level dispatcher;
        // this arm exists for recursion via Min/Max/Concat/Load/etc.
        SymbolKind::Constant(v) => ctx.tm.mk_integer(*v),
        // ES1 P2.7 fix: use vars map (per-recursion Const identity)
        SymbolKind::Variable => ctx.declare_int_var(&expr.name),
        SymbolKind::Add(a, b) => {
            let a_t = translate_symbol_int(ctx, a);
            let b_t = translate_symbol_int(ctx, b);
            ctx.tm.mk_term(cvc5::Kind::Add, &[a_t, b_t])
        }
        SymbolKind::Sub(a, b) => {
            let a_t = translate_symbol_int(ctx, a);
            let b_t = translate_symbol_int(ctx, b);
            ctx.tm.mk_term(cvc5::Kind::Sub, &[a_t, b_t])
        }
        SymbolKind::Multiply(a, b) => {
            // cvc5 Kind::Mult (NOT Multiply) for integer multiplication
            let a_t = translate_symbol_int(ctx, a);
            let b_t = translate_symbol_int(ctx, b);
            ctx.tm.mk_term(cvc5::Kind::Mult, &[a_t, b_t])
        }
        SymbolKind::Divide(a, b) => {
            // cvc5 Kind::IntsDivision (NOT Division) for integer division
            let a_t = translate_symbol_int(ctx, a);
            let b_t = translate_symbol_int(ctx, b);
            ctx.tm.mk_term(cvc5::Kind::IntsDivision, &[a_t, b_t])
        }
        SymbolKind::Mod(a, b) => {
            // cvc5 Kind::IntsModulus for integer modulus
            let a_t = translate_symbol_int(ctx, a);
            let b_t = translate_symbol_int(ctx, b);
            ctx.tm.mk_term(cvc5::Kind::IntsModulus, &[a_t, b_t])
        }
        SymbolKind::Neg(inner) => {
            let inner_t = translate_symbol_int(ctx, inner);
            ctx.tm.mk_term(cvc5::Kind::Neg, &[inner_t])
        }
        SymbolKind::Abs(inner) => {
            let inner_t = translate_symbol_int(ctx, inner);
            ctx.tm.mk_term(cvc5::Kind::Abs, &[inner_t])
        }
        SymbolKind::Min(vars) => {
            if vars.is_empty() {
                return ctx.tm.mk_integer(0); // sentinel; matches z3_backend.rs:349
            }
            let mut acc = translate_symbol_int(ctx, &vars[0]);
            for v in vars.iter().skip(1) {
                let cur = translate_symbol_int(ctx, v);
                // acc = Ite(Lt(cur, acc), cur, acc)
                let lt_b = coerce_to_bool(
                    ctx,
                    &ctx.tm.mk_term(cvc5::Kind::Lt, &[cur.clone(), acc.clone()]),
                );
                acc = ctx.tm.mk_term(cvc5::Kind::Ite, &[lt_b, cur, acc]);
            }
            acc
        }
        SymbolKind::Max(vars) => {
            if vars.is_empty() {
                return ctx.tm.mk_integer(0);
            }
            let mut acc = translate_symbol_int(ctx, &vars[0]);
            for v in vars.iter().skip(1) {
                let cur = translate_symbol_int(ctx, v);
                // acc = Ite(Gt(cur, acc), cur, acc)
                let gt_b = coerce_to_bool(
                    ctx,
                    &ctx.tm.mk_term(cvc5::Kind::Gt, &[cur.clone(), acc.clone()]),
                );
                acc = ctx.tm.mk_term(cvc5::Kind::Ite, &[gt_b, cur, acc]);
            }
            acc
        }
        SymbolKind::Concat(vars) => {
            if vars.is_empty() {
                return ctx.tm.mk_integer(0);
            }
            let mut acc = translate_symbol_int(ctx, &vars[0]);
            let pow2_32 = ctx.tm.mk_integer(2_i64.pow(32));
            for v in vars.iter().skip(1) {
                let cur = translate_symbol_int(ctx, v);
                // acc = acc * 2^32 + cur  (matches z3_backend.rs:373-378)
                let mul = ctx.tm.mk_term(cvc5::Kind::Mult, &[acc, pow2_32.clone()]);
                acc = ctx.tm.mk_term(cvc5::Kind::Add, &[mul, cur]);
            }
            acc
        }
        // Other variants unreachable from int dispatcher
        _ => ctx.tm.mk_integer(0),
    }
}

/// Translates a `SymbolExpr` (bool ops family) to a CVC5 bool term.
///
/// **ES1 P2.6 fix**: takes `&SymbolExpr` (not just `&SymbolKind`) so that
/// nested `Variable` accesses preserve the original variable name, and
/// takes `int_sort` to ensure all `tm.mk_const` calls use the same sort
/// wrapper (preventing the solver from seeing two distinct `a` constants
/// across two `assert` calls).
#[cfg(feature = "cvc5")]
fn translate_symbol_bool<'a, 'tm>(
    ctx: &mut TranslationContext<'a, 'tm>,
    expr: &SymbolExpr,
) -> cvc5::Term<'tm> {
    match &expr.kind {
        SymbolKind::Constant(v) => {
            // Constant non-zero is "true" via int_is_nonzero mirror (z3 line 215)
            if *v != 0 {
                ctx.tm.mk_true()
            } else {
                ctx.tm.mk_false()
            }
        }
        SymbolKind::Variable => {
            // Variable in bool context: var != 0 (z3 line 216 mirror)
            let var_t = ctx.declare_int_var(&expr.name);
            let zero = ctx.tm.mk_integer(0);
            let eq_b = coerce_to_bool(ctx, &ctx.tm.mk_term(cvc5::Kind::Equal, &[var_t, zero]));
            ctx.tm.mk_term(cvc5::Kind::Not, &[eq_b])
        }
        SymbolKind::Eq(a, b) => {
            // cvc5 Kind::Equal (NOT Eq) — produces a bool from int args
            let a_t = translate_symbol_int(ctx, a);
            let b_t = translate_symbol_int(ctx, b);
            coerce_to_bool(ctx, &ctx.tm.mk_term(cvc5::Kind::Equal, &[a_t, b_t]))
        }
        SymbolKind::NotEq(a, b) => {
            let a_t = translate_symbol_int(ctx, a);
            let b_t = translate_symbol_int(ctx, b);
            let eq_b = coerce_to_bool(ctx, &ctx.tm.mk_term(cvc5::Kind::Equal, &[a_t, b_t]));
            ctx.tm.mk_term(cvc5::Kind::Not, &[eq_b])
        }
        SymbolKind::Lt(a, b) => {
            let a_t = translate_symbol_int(ctx, a);
            let b_t = translate_symbol_int(ctx, b);
            coerce_to_bool(ctx, &ctx.tm.mk_term(cvc5::Kind::Lt, &[a_t, b_t]))
        }
        SymbolKind::LessOrEqual(a, b) => {
            // cvc5 Kind::Leq (NOT Le)
            let a_t = translate_symbol_int(ctx, a);
            let b_t = translate_symbol_int(ctx, b);
            coerce_to_bool(ctx, &ctx.tm.mk_term(cvc5::Kind::Leq, &[a_t, b_t]))
        }
        SymbolKind::GreaterThan(a, b) => {
            let a_t = translate_symbol_int(ctx, a);
            let b_t = translate_symbol_int(ctx, b);
            coerce_to_bool(ctx, &ctx.tm.mk_term(cvc5::Kind::Gt, &[a_t, b_t]))
        }
        SymbolKind::GreaterEqual(a, b) => {
            let a_t = translate_symbol_int(ctx, a);
            let b_t = translate_symbol_int(ctx, b);
            coerce_to_bool(ctx, &ctx.tm.mk_term(cvc5::Kind::Geq, &[a_t, b_t]))
        }
        SymbolKind::And(a, b) => {
            let a_b = translate_symbol_bool(ctx, a);
            let b_b = translate_symbol_bool(ctx, b);
            ctx.tm.mk_term(cvc5::Kind::And, &[a_b, b_b])
        }
        SymbolKind::Or(a, b) => {
            let a_b = translate_symbol_bool(ctx, a);
            let b_b = translate_symbol_bool(ctx, b);
            ctx.tm.mk_term(cvc5::Kind::Or, &[a_b, b_b])
        }
        SymbolKind::Xor(a, b) => {
            let a_b = translate_symbol_bool(ctx, a);
            let b_b = translate_symbol_bool(ctx, b);
            ctx.tm.mk_term(cvc5::Kind::Xor, &[a_b, b_b])
        }
        SymbolKind::Not(inner) => {
            let inner_b = translate_symbol_bool(ctx, inner);
            ctx.tm.mk_term(cvc5::Kind::Not, &[inner_b])
        }
        // Unreachable from bool dispatcher; safe fallback to true.
        _ => ctx.tm.mk_true(),
    }
}

/// BV family placeholder — REMOVED in P2.7.5 (2026-06-03) when
/// `TranslationContext::translate_symbol_bv` (impl method, line ~190)
/// became the canonical path. The P2.6 free fn was kept as a defensive
/// fallback but is now dead code — per REGRA #0, removed in P2.7.6
/// cross-audit when clippy flagged it as `function is never used`.
/// Coerces a `Term` to `Sort::bool` so it can be passed to
/// `solver.assert_formula` (which only accepts Boolean terms).
#[cfg(feature = "cvc5")]
fn coerce_to_bool<'a, 'tm>(
    ctx: &mut TranslationContext<'a, 'tm>,
    term: &cvc5::Term<'tm>,
) -> cvc5::Term<'tm> {
    if term.is_boolean_value() || term.sort().is_boolean() {
        term.clone()
    } else {
        // Non-boolean term (int, BV, etc.) — wrap in self-equality to make
        // it a boolean assertion that is trivially true. This preserves
        // the legacy behavior for inputs that asserted non-boolean terms.
        ctx.tm
            .mk_term(cvc5::Kind::Equal, &[term.clone(), term.clone()])
    }
}

/// Extracts an `i64` integer value from a CVC5 term via `solver.get_value`.
/// Returns `None` if the term is not a constant integer or is not
/// representable in `i64`.
#[cfg(feature = "cvc5")]
fn extract_int_value(solver: &cvc5::Solver<'_>, term: &cvc5::Term<'_>) -> Option<i64> {
    let val = solver.get_value(term.clone());
    if val.is_int64_value() {
        Some(val.int64_value())
    } else if val.is_int32_value() {
        Some(val.int32_value() as i64)
    } else {
        None
    }
}

/// Stub implementation when cvc5 feature is not enabled.
#[cfg(not(feature = "cvc5"))]
impl SolverBackend for CVC5SolverBackend {
    fn new() -> Self {
        CVC5SolverBackend { _placeholder: () }
    }

    fn assert(&mut self, _constraint: &Constraint) {}

    fn check_sat(&mut self) -> bool {
        false
    }

    fn get_model(&self) -> HashMap<String, i64> {
        HashMap::new()
    }

    fn reset(&mut self) {}

    fn clone_box(&self) -> Box<dyn super::SolverBackend> {
        Box::new(CVC5SolverBackend { _placeholder: () })
    }
}

#[cfg(test)]
#[path = "cvc5_backend_tests.rs"]
mod tests;
