//! Z3SolverBackend — Z3 SMT solver backend using the z3 0.20 crate.
//!
//! Migrated from the older z3 0.18-era API (untyped `z3::Expr`) to the
//! typed 0.20 API (Bool/Int/BV/Array).
//!
//! # Design notes
//!
//! - `Solver::new()` (no args) uses z3 0.20's thread_local Context pattern.
//! - `solver.check()` returns `SatResult` (Sat/Unsat/Unknown); we fold
//!   `Unknown` as `Unsat` for the `check_sat() -> bool` trait contract.
//! - z3 0.20's `Solver` is **not Send/Sync** (it holds `NonNull<_Z3_solver>`
//!   and `Rc<ContextInternal>`), so we wrap it in `std::sync::Mutex` to
//!   satisfy the `SolverBackend: Send + Sync` trait bound.
//! - The translation layer dispatches on `SymbolKind` to the appropriate
//!   typed AST (Bool/Int/BV) and converts to `Bool` in constraint context.
//! - `Bool::and` / `Bool::or` are varop! associated functions taking
//!   `&[T] where T: Into<Bool> + Clone`; we use `ite`-based chaining
//!   for clarity (`a AND b = a.ite(&b, &false)`).

use std::collections::HashMap;
use std::sync::Mutex;

use z3::SatResult;
use z3::Solver;
use z3::ast::{BV, Bool, Int};

use crate::concolic::{Constraint, ConstraintExpr, SymbolExpr, SymbolKind};

use super::SolverBackend;

/// Z3 solver backend (active when z3 feature is enabled).
#[cfg(feature = "z3")]
#[derive(Debug)]
pub struct Z3SolverBackend {
    /// Z3 solver instance wrapped in Mutex (z3 0.20 Solver is not Send/Sync).
    solver: Mutex<Solver>,
    /// Asserted constraints (for model extraction).
    constraints: Vec<Constraint>,
}

/// Z3 solver backend (no-op when z3 feature is not enabled).
#[cfg(not(feature = "z3"))]
#[derive(Debug)]
pub struct Z3SolverBackend {
    _placeholder: (),
}

impl Default for Z3SolverBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "z3")]
impl Z3SolverBackend {
    /// Creates a new `Z3SolverBackend` using the thread_local Context.
    pub fn new() -> Self {
        Z3SolverBackend {
            solver: Mutex::new(Solver::new()),
            constraints: Vec::new(),
        }
    }
}

#[cfg(not(feature = "z3"))]
impl Z3SolverBackend {
    /// Stub constructor (no z3 feature).
    pub fn new() -> Self {
        Z3SolverBackend { _placeholder: () }
    }
}

#[cfg(feature = "z3")]
impl SolverBackend for Z3SolverBackend {
    fn new() -> Self {
        Self::new()
    }

    fn assert(&mut self, constraint: &Constraint) {
        self.constraints.push(constraint.clone());
        if let Ok(b) = translate_constraint_to_z3(&constraint.expr) {
            let s = self.solver.lock().expect("z3 solver mutex poisoned");
            s.assert(&b);
        }
    }

    fn check_sat(&mut self) -> bool {
        let s = self.solver.lock().expect("z3 solver mutex poisoned");
        matches!(s.check(), SatResult::Sat)
    }

    fn get_model(&self) -> HashMap<String, i64> {
        let mut model = HashMap::new();
        let s = self.solver.lock().expect("z3 solver mutex poisoned");
        if let Some(z3_model) = s.get_model() {
            for c in &self.constraints {
                extract_model_from_expr(&z3_model, &c.expr, &mut model);
            }
        }
        model
    }

    fn reset(&mut self) {
        self.constraints.clear();
        let mut s = self.solver.lock().expect("z3 solver mutex poisoned");
        *s = Solver::new();
    }

    fn clone_box(&self) -> Box<dyn super::SolverBackend> {
        let mut cloned = Z3SolverBackend::new();
        for c in &self.constraints {
            cloned.assert(c);
        }
        Box::new(cloned)
    }
}

#[cfg(not(feature = "z3"))]
impl SolverBackend for Z3SolverBackend {
    fn new() -> Self {
        Z3SolverBackend { _placeholder: () }
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
        Box::new(Z3SolverBackend { _placeholder: () })
    }
}

// ---------------------------------------------------------------------------
// Translation layer (z3 0.20 typed ASTs)
// ---------------------------------------------------------------------------

/// Translates a `ConstraintExpr` to a Z3 `Bool`.
///
/// Constraints are always boolean; this is the entry point for the
/// `assert` path.
#[cfg(feature = "z3")]
fn translate_constraint_to_z3(expr: &ConstraintExpr) -> Result<Bool, String> {
    match expr {
        ConstraintExpr::True => Ok(Bool::from_bool(true)),
        ConstraintExpr::False => Ok(Bool::from_bool(false)),
        ConstraintExpr::Bool(b) => Ok(Bool::from_bool(*b)),
        ConstraintExpr::Symbolic(sym) => translate_symbol_to_z3_bool(sym),
        ConstraintExpr::And(constraints) => {
            let mut result = Bool::from_bool(true);
            for c in constraints {
                let child = translate_constraint_to_z3(&c.expr)?;
                // a AND b ≡ a.ite(&b, &false)
                result = result.ite(&child, &Bool::from_bool(false));
            }
            Ok(result)
        }
        ConstraintExpr::Or(constraints) => {
            let mut result = Bool::from_bool(false);
            for c in constraints {
                let child = translate_constraint_to_z3(&c.expr)?;
                // a OR b ≡ a.ite(&true, &b)
                result = result.ite(&Bool::from_bool(true), &child);
            }
            Ok(result)
        }
        ConstraintExpr::Not(inner) => {
            let inner_expr = translate_constraint_to_z3(&inner.expr)?;
            Ok(inner_expr.not())
        }
        ConstraintExpr::Ite(cond, then, else_) => {
            let cond_expr = translate_constraint_to_z3(cond)?;
            let then_expr = translate_constraint_to_z3(then)?;
            let else_expr = translate_constraint_to_z3(else_)?;
            Ok(cond_expr.ite(&then_expr, &else_expr))
        }
        ConstraintExpr::Distinct(a, b) => {
            // a != b ≡ a XOR b
            let a_expr = translate_constraint_to_z3(a)?;
            let b_expr = translate_constraint_to_z3(b)?;
            Ok(a_expr.xor(&b_expr))
        }
        ConstraintExpr::ForAll(_var, body, range) => {
            // Quantifier support requires the z3::ast::Quantifier API; for
            // P2 we collapse to (range ⇒ body) — safe over-approximation.
            let range_expr = translate_constraint_to_z3(range)?;
            let body_expr = translate_constraint_to_z3(body)?;
            Ok(range_expr.implies(&body_expr))
        }
        ConstraintExpr::Exists(_var, body, range) => {
            // Symmetric to ForAll: collapse to (range ∧ body) for P2.
            let range_expr = translate_constraint_to_z3(range)?;
            let body_expr = translate_constraint_to_z3(body)?;
            Ok(range_expr.ite(&body_expr, &Bool::from_bool(false)))
        }
        ConstraintExpr::Implies(a, b) => {
            let a_expr = translate_constraint_to_z3(a)?;
            let b_expr = translate_constraint_to_z3(b)?;
            Ok(a_expr.implies(&b_expr))
        }
    }
}

/// Translates a `SymbolExpr` to a Z3 `Bool` (constraint context).
#[cfg(feature = "z3")]
fn translate_symbol_to_z3_bool(expr: &SymbolExpr) -> Result<Bool, String> {
    match &expr.kind {
        SymbolKind::Constant(v) => Ok(int_is_nonzero(Int::from_i64(*v))),
        SymbolKind::Variable => Ok(int_is_nonzero(Int::new_const(expr.name.as_str()))),
        SymbolKind::Eq(a, b) => {
            let a_int = translate_symbol_to_z3_int(a)?;
            let b_int = translate_symbol_to_z3_int(b)?;
            Ok(a_int.eq(&b_int))
        }
        SymbolKind::NotEq(a, b) => {
            let a_int = translate_symbol_to_z3_int(a)?;
            let b_int = translate_symbol_to_z3_int(b)?;
            Ok(a_int.eq(&b_int).not())
        }
        SymbolKind::Lt(a, b) => {
            let a_int = translate_symbol_to_z3_int(a)?;
            let b_int = translate_symbol_to_z3_int(b)?;
            Ok(a_int.lt(&b_int))
        }
        SymbolKind::LessOrEqual(a, b) => {
            let a_int = translate_symbol_to_z3_int(a)?;
            let b_int = translate_symbol_to_z3_int(b)?;
            Ok(a_int.le(&b_int))
        }
        SymbolKind::GreaterThan(a, b) => {
            let a_int = translate_symbol_to_z3_int(a)?;
            let b_int = translate_symbol_to_z3_int(b)?;
            Ok(a_int.gt(&b_int))
        }
        SymbolKind::GreaterEqual(a, b) => {
            let a_int = translate_symbol_to_z3_int(a)?;
            let b_int = translate_symbol_to_z3_int(b)?;
            Ok(a_int.ge(&b_int))
        }
        SymbolKind::And(a, b) => {
            let a_b = translate_symbol_to_z3_bool(a)?;
            let b_b = translate_symbol_to_z3_bool(b)?;
            Ok(a_b.ite(&b_b, &Bool::from_bool(false)))
        }
        SymbolKind::Or(a, b) => {
            let a_b = translate_symbol_to_z3_bool(a)?;
            let b_b = translate_symbol_to_z3_bool(b)?;
            Ok(a_b.ite(&Bool::from_bool(true), &b_b))
        }
        SymbolKind::Xor(a, b) => {
            let a_b = translate_symbol_to_z3_bool(a)?;
            let b_b = translate_symbol_to_z3_bool(b)?;
            Ok(a_b.xor(&b_b))
        }
        SymbolKind::Not(inner) => Ok(translate_symbol_to_z3_bool(inner)?.not()),
        SymbolKind::Add(a, b) => Ok(int_is_nonzero(
            translate_symbol_to_z3_int(a)? + translate_symbol_to_z3_int(b)?,
        )),
        SymbolKind::Sub(a, b) => Ok(int_is_nonzero(
            translate_symbol_to_z3_int(a)? - translate_symbol_to_z3_int(b)?,
        )),
        SymbolKind::Multiply(a, b) => Ok(int_is_nonzero(
            translate_symbol_to_z3_int(a)? * translate_symbol_to_z3_int(b)?,
        )),
        SymbolKind::Divide(a, b) => Ok(int_is_nonzero(
            translate_symbol_to_z3_int(a)? / translate_symbol_to_z3_int(b)?,
        )),
        SymbolKind::Mod(a, b) => Ok(int_is_nonzero(
            translate_symbol_to_z3_int(a)? % translate_symbol_to_z3_int(b)?,
        )),
        SymbolKind::Neg(inner) => Ok(int_is_nonzero(-translate_symbol_to_z3_int(inner)?)),
        SymbolKind::Abs(inner) => {
            let v = translate_symbol_to_z3_int(inner)?;
            let neg = -v.clone();
            let abs = v.ge(Int::from_i64(0)).ite(&v, &neg);
            Ok(int_is_nonzero(abs))
        }
        SymbolKind::Min(vars) => {
            if vars.is_empty() {
                return Ok(Bool::from_bool(false));
            }
            let mut acc = translate_symbol_to_z3_int(&vars[0])?;
            for v in vars.iter().skip(1) {
                let cur = translate_symbol_to_z3_int(v)?;
                acc = cur.lt(&acc).ite(&cur, &acc);
            }
            Ok(int_is_nonzero(acc))
        }
        SymbolKind::Max(vars) => {
            if vars.is_empty() {
                return Ok(Bool::from_bool(false));
            }
            let mut acc = translate_symbol_to_z3_int(&vars[0])?;
            for v in vars.iter().skip(1) {
                let cur = translate_symbol_to_z3_int(v)?;
                acc = cur.gt(&acc).ite(&cur, &acc);
            }
            Ok(int_is_nonzero(acc))
        }
        SymbolKind::Concat(vars) => {
            if vars.is_empty() {
                return Ok(Bool::from_bool(false));
            }
            let mut acc = translate_symbol_to_z3_int(&vars[0])?;
            for v in vars.iter().skip(1) {
                let cur = translate_symbol_to_z3_int(v)?;
                acc = acc * Int::from_i64(32) + cur;
            }
            Ok(int_is_nonzero(acc))
        }
        SymbolKind::Shl(a, b) => Ok(bv_is_nonzero(
            translate_symbol_to_z3_bv(a)?.bvshl(&translate_symbol_to_z3_bv(b)?),
        )),
        SymbolKind::Shr(a, b) => Ok(bv_is_nonzero(
            translate_symbol_to_z3_bv(a)?.bvlshr(&translate_symbol_to_z3_bv(b)?),
        )),
        SymbolKind::BitAnd(a, b) => Ok(bv_is_nonzero(
            translate_symbol_to_z3_bv(a)?.bvand(&translate_symbol_to_z3_bv(b)?),
        )),
        SymbolKind::BitOr(a, b) => Ok(bv_is_nonzero(
            translate_symbol_to_z3_bv(a)?.bvor(&translate_symbol_to_z3_bv(b)?),
        )),
        SymbolKind::BitXor(a, b) => Ok(bv_is_nonzero(
            translate_symbol_to_z3_bv(a)?.bvxor(&translate_symbol_to_z3_bv(b)?),
        )),
        SymbolKind::Extract { inner, high, low } => {
            let bv = translate_symbol_to_z3_bv(inner)?;
            Ok(bv_is_nonzero(bv.extract(*high, *low)))
        }
        SymbolKind::ZeroExt(inner) => Ok(bv_is_nonzero(
            translate_symbol_to_z3_bv(inner)?.zero_ext(32),
        )),
        SymbolKind::SignExt(inner) => Ok(bv_is_nonzero(
            translate_symbol_to_z3_bv(inner)?.sign_ext(32),
        )),
        SymbolKind::Load(base) => translate_symbol_to_z3_bool(base),
        SymbolKind::ArraySelect(arr, _idx) => translate_symbol_to_z3_bool(arr),
        SymbolKind::ArrayStore(arr, _idx, val) => {
            let val_b = translate_symbol_to_z3_bool(val)?;
            let arr_b = translate_symbol_to_z3_bool(arr)?;
            // store semantics: prefer val when present, else arr
            Ok(val_b.ite(&val_b, &arr_b))
        }
    }
}

/// Translates a `SymbolExpr` to a Z3 `Int` (arithmetic context).
#[cfg(feature = "z3")]
fn translate_symbol_to_z3_int(expr: &SymbolExpr) -> Result<Int, String> {
    match &expr.kind {
        SymbolKind::Constant(v) => Ok(Int::from_i64(*v)),
        SymbolKind::Variable => Ok(Int::new_const(expr.name.as_str())),
        SymbolKind::Add(a, b) => {
            Ok(translate_symbol_to_z3_int(a)? + translate_symbol_to_z3_int(b)?)
        }
        SymbolKind::Sub(a, b) => {
            Ok(translate_symbol_to_z3_int(a)? - translate_symbol_to_z3_int(b)?)
        }
        SymbolKind::Multiply(a, b) => {
            Ok(translate_symbol_to_z3_int(a)? * translate_symbol_to_z3_int(b)?)
        }
        SymbolKind::Divide(a, b) => {
            Ok(translate_symbol_to_z3_int(a)? / translate_symbol_to_z3_int(b)?)
        }
        SymbolKind::Mod(a, b) => {
            Ok(translate_symbol_to_z3_int(a)? % translate_symbol_to_z3_int(b)?)
        }
        SymbolKind::Neg(inner) => Ok(-translate_symbol_to_z3_int(inner)?),
        SymbolKind::Abs(inner) => {
            let v = translate_symbol_to_z3_int(inner)?;
            let neg = -v.clone();
            Ok(v.ge(Int::from_i64(0)).ite(&v, &neg))
        }
        SymbolKind::Min(vars) => {
            if vars.is_empty() {
                return Ok(Int::from_i64(0));
            }
            let mut acc = translate_symbol_to_z3_int(&vars[0])?;
            for v in vars.iter().skip(1) {
                let cur = translate_symbol_to_z3_int(v)?;
                acc = cur.lt(&acc).ite(&cur, &acc);
            }
            Ok(acc)
        }
        SymbolKind::Max(vars) => {
            if vars.is_empty() {
                return Ok(Int::from_i64(0));
            }
            let mut acc = translate_symbol_to_z3_int(&vars[0])?;
            for v in vars.iter().skip(1) {
                let cur = translate_symbol_to_z3_int(v)?;
                acc = cur.gt(&acc).ite(&cur, &acc);
            }
            Ok(acc)
        }
        SymbolKind::Concat(vars) => {
            if vars.is_empty() {
                return Ok(Int::from_i64(0));
            }
            let mut acc = translate_symbol_to_z3_int(&vars[0])?;
            for v in vars.iter().skip(1) {
                let cur = translate_symbol_to_z3_int(v)?;
                acc = acc * Int::from_i64(32) + cur;
            }
            Ok(acc)
        }
        SymbolKind::Eq(a, b) => {
            let a_int = translate_symbol_to_z3_int(a)?;
            let b_int = translate_symbol_to_z3_int(b)?;
            Ok(a_int.eq(&b_int).ite(&Int::from_i64(1), &Int::from_i64(0)))
        }
        SymbolKind::NotEq(a, b) => {
            let a_int = translate_symbol_to_z3_int(a)?;
            let b_int = translate_symbol_to_z3_int(b)?;
            Ok(a_int
                .eq(&b_int)
                .not()
                .ite(&Int::from_i64(1), &Int::from_i64(0)))
        }
        SymbolKind::Lt(a, b) => {
            let a_int = translate_symbol_to_z3_int(a)?;
            let b_int = translate_symbol_to_z3_int(b)?;
            Ok(a_int.lt(&b_int).ite(&Int::from_i64(1), &Int::from_i64(0)))
        }
        SymbolKind::LessOrEqual(a, b) => {
            let a_int = translate_symbol_to_z3_int(a)?;
            let b_int = translate_symbol_to_z3_int(b)?;
            Ok(a_int.le(&b_int).ite(&Int::from_i64(1), &Int::from_i64(0)))
        }
        SymbolKind::GreaterThan(a, b) => {
            let a_int = translate_symbol_to_z3_int(a)?;
            let b_int = translate_symbol_to_z3_int(b)?;
            Ok(a_int.gt(&b_int).ite(&Int::from_i64(1), &Int::from_i64(0)))
        }
        SymbolKind::GreaterEqual(a, b) => {
            let a_int = translate_symbol_to_z3_int(a)?;
            let b_int = translate_symbol_to_z3_int(b)?;
            Ok(a_int.ge(&b_int).ite(&Int::from_i64(1), &Int::from_i64(0)))
        }
        SymbolKind::And(a, b) => {
            let a_b = translate_symbol_to_z3_bool(a)?;
            let b_b = translate_symbol_to_z3_bool(b)?;
            Ok(a_b
                .ite(&b_b, &Bool::from_bool(false))
                .ite(&Int::from_i64(1), &Int::from_i64(0)))
        }
        SymbolKind::Or(a, b) => {
            let a_b = translate_symbol_to_z3_bool(a)?;
            let b_b = translate_symbol_to_z3_bool(b)?;
            Ok(a_b
                .ite(&Bool::from_bool(true), &b_b)
                .ite(&Int::from_i64(1), &Int::from_i64(0)))
        }
        SymbolKind::Xor(a, b) => {
            let a_b = translate_symbol_to_z3_bool(a)?;
            let b_b = translate_symbol_to_z3_bool(b)?;
            Ok(a_b.xor(&b_b).ite(&Int::from_i64(1), &Int::from_i64(0)))
        }
        SymbolKind::Not(inner) => Ok(translate_symbol_to_z3_bool(inner)?
            .not()
            .ite(&Int::from_i64(1), &Int::from_i64(0))),
        SymbolKind::Shl(a, b) => Ok(translate_symbol_to_z3_bv(a)?
            .bvshl(&translate_symbol_to_z3_bv(b)?)
            .to_int(false)),
        SymbolKind::Shr(a, b) => Ok(translate_symbol_to_z3_bv(a)?
            .bvlshr(&translate_symbol_to_z3_bv(b)?)
            .to_int(false)),
        SymbolKind::BitAnd(a, b) => Ok(translate_symbol_to_z3_bv(a)?
            .bvand(&translate_symbol_to_z3_bv(b)?)
            .to_int(false)),
        SymbolKind::BitOr(a, b) => Ok(translate_symbol_to_z3_bv(a)?
            .bvor(&translate_symbol_to_z3_bv(b)?)
            .to_int(false)),
        SymbolKind::BitXor(a, b) => Ok(translate_symbol_to_z3_bv(a)?
            .bvxor(&translate_symbol_to_z3_bv(b)?)
            .to_int(false)),
        SymbolKind::Extract { inner, high, low } => Ok(translate_symbol_to_z3_bv(inner)?
            .extract(*high, *low)
            .to_int(false)),
        SymbolKind::ZeroExt(inner) => {
            Ok(translate_symbol_to_z3_bv(inner)?.zero_ext(32).to_int(false))
        }
        SymbolKind::SignExt(inner) => {
            Ok(translate_symbol_to_z3_bv(inner)?.sign_ext(32).to_int(false))
        }
        SymbolKind::Load(base) => translate_symbol_to_z3_int(base),
        SymbolKind::ArraySelect(arr, _idx) => translate_symbol_to_z3_int(arr),
        SymbolKind::ArrayStore(_arr, _idx, val) => translate_symbol_to_z3_int(val),
    }
}

/// Translates a `SymbolExpr` to a Z3 `BV` (32-bit default).
#[cfg(feature = "z3")]
fn translate_symbol_to_z3_bv(expr: &SymbolExpr) -> Result<BV, String> {
    match &expr.kind {
        SymbolKind::Constant(v) => Ok(BV::from_i64(*v, 32)),
        SymbolKind::Variable => Ok(BV::new_const(expr.name.as_str(), 32)),
        SymbolKind::Shl(a, b) => {
            Ok(translate_symbol_to_z3_bv(a)?.bvshl(&translate_symbol_to_z3_bv(b)?))
        }
        SymbolKind::Shr(a, b) => {
            Ok(translate_symbol_to_z3_bv(a)?.bvlshr(&translate_symbol_to_z3_bv(b)?))
        }
        SymbolKind::BitAnd(a, b) => {
            Ok(translate_symbol_to_z3_bv(a)?.bvand(&translate_symbol_to_z3_bv(b)?))
        }
        SymbolKind::BitOr(a, b) => {
            Ok(translate_symbol_to_z3_bv(a)?.bvor(&translate_symbol_to_z3_bv(b)?))
        }
        SymbolKind::BitXor(a, b) => {
            Ok(translate_symbol_to_z3_bv(a)?.bvxor(&translate_symbol_to_z3_bv(b)?))
        }
        SymbolKind::Extract { inner, high, low } => {
            Ok(translate_symbol_to_z3_bv(inner)?.extract(*high, *low))
        }
        SymbolKind::ZeroExt(inner) => Ok(translate_symbol_to_z3_bv(inner)?.zero_ext(32)),
        SymbolKind::SignExt(inner) => Ok(translate_symbol_to_z3_bv(inner)?.sign_ext(32)),
        // Other variants — promote Int to BV via Symbol-based promotion as a
        // best-effort; full mixed-type resolution is a P3 follow-up.
        _ => Ok(BV::new_const(expr.name.as_str(), 32)),
    }
}

/// Helper: convert an Int AST to a Bool (true iff != 0).
#[cfg(feature = "z3")]
fn int_is_nonzero(v: Int) -> Bool {
    v.eq(Int::from_i64(0)).not()
}

/// Helper: convert a BV AST to a Bool (true iff != 0 as unsigned int).
#[cfg(feature = "z3")]
fn bv_is_nonzero(v: BV) -> Bool {
    let zero = BV::from_i64(0, 32);
    v.eq(&zero).not()
}

/// Extracts variable bindings from a Z3 model.
#[cfg(feature = "z3")]
fn extract_model_from_expr(
    model: &z3::Model,
    expr: &ConstraintExpr,
    result: &mut HashMap<String, i64>,
) {
    if let ConstraintExpr::Symbolic(sym) = expr
        && let SymbolKind::Variable = &sym.kind
    {
        let int_var = Int::new_const(sym.name.as_str());
        if let Some(evaluated) = model.eval(&int_var, true)
            && let Some(val) = evaluated.as_i64()
        {
            result.insert(sym.name.clone(), val);
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_z3_stub_when_disabled() {
        let mut backend = Z3SolverBackend::new();
        let constraint = Constraint::new("x", ConstraintExpr::Bool(true));
        backend.assert(&constraint);
        // Without z3 feature, check_sat always returns false (stub).
        #[cfg(not(feature = "z3"))]
        assert!(!backend.check_sat());
        // With z3 feature, asserting (Bool(true)) is trivially satisfiable.
        #[cfg(feature = "z3")]
        assert!(backend.check_sat());
    }

    #[cfg(feature = "z3")]
    #[test]
    fn test_z3_translate_constraint_true() {
        let expr = ConstraintExpr::True;
        let b = translate_constraint_to_z3(&expr).expect("translate true");
        let s = Solver::new();
        s.assert(&b);
        assert_eq!(s.check(), SatResult::Sat);
    }

    #[cfg(feature = "z3")]
    #[test]
    fn test_z3_translate_constraint_false() {
        let expr = ConstraintExpr::False;
        let b = translate_constraint_to_z3(&expr).expect("translate false");
        let s = Solver::new();
        s.assert(&b);
        assert_eq!(s.check(), SatResult::Unsat);
    }

    #[cfg(feature = "z3")]
    #[test]
    fn test_z3_translate_symbolic_variable_eq() {
        // x == 5  → satisfiable (model has x = 5)
        let x = SymbolExpr {
            name: "x".to_string(),
            kind: SymbolKind::Variable,
        };
        let five = SymbolExpr {
            name: String::new(),
            kind: SymbolKind::Constant(5),
        };
        let eq_sym = SymbolExpr {
            name: String::new(),
            kind: SymbolKind::Eq(Box::new(x), Box::new(five)),
        };
        let expr = ConstraintExpr::Symbolic(eq_sym);
        let b = translate_constraint_to_z3(&expr).expect("translate eq");
        let s = Solver::new();
        s.assert(&b);
        assert_eq!(s.check(), SatResult::Sat);
        let m = s.get_model().expect("model");
        let x_var = Int::new_const("x");
        let v = m.eval(&x_var, true).expect("eval x").as_i64();
        assert_eq!(v, Some(5));
    }

    #[cfg(feature = "z3")]
    #[test]
    fn test_z3_translate_constraint_and_or() {
        // (true && (false || true))  → Sat
        let inner = Constraint::new(
            "inner",
            ConstraintExpr::Or(vec![
                Constraint::new("f", ConstraintExpr::False),
                Constraint::new("t", ConstraintExpr::True),
            ]),
        );
        let expr = ConstraintExpr::And(vec![Constraint::new("tt", ConstraintExpr::True), inner]);
        let b = translate_constraint_to_z3(&expr).expect("translate and/or");
        let s = Solver::new();
        s.assert(&b);
        assert_eq!(s.check(), SatResult::Sat);
    }
}
