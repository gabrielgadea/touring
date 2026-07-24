//! StubSolverBackend — default fallback when no native solver is available.
//!
//! Provides trivial satisfiability checking based on constraint metadata.
//! Not a real SMT solver — only checks `satisfiable` flag on constraints.

use std::collections::HashMap;

use crate::concolic::{Constraint, ConstraintExpr};

use super::{ConstraintTranslator, SolverBackend, constraint_to_smtlib, symbol_to_smtlib};

/// A trivial solver backend that checks constraint satisfiability directly.
///
/// This is NOT a real SMT solver. It simply delegates to the `satisfiable`
/// field on each constraint and performs no actual solving.
///
/// # When to use
///
/// - During testing without native solver dependencies
/// - As a fallback when Z3/CVC5 are not available
/// - For benchmarking the concolic executor without solver overhead
#[derive(Debug)]
pub struct StubSolverBackend {
    /// Currently asserted constraints.
    constraints: Vec<Constraint>,
}

impl Default for StubSolverBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl SolverBackend for StubSolverBackend {
    fn new() -> Self {
        StubSolverBackend {
            constraints: Vec::new(),
        }
    }

    fn assert(&mut self, constraint: &Constraint) {
        self.constraints.push(constraint.clone());
    }

    fn check_sat(&mut self) -> bool {
        // Trivial check: no constraint should be marked unsatisfiable.
        // In a real solver, this would invoke the SMT solver.
        !self.constraints.iter().any(|c| !c.satisfiable)
    }

    fn get_model(&self) -> HashMap<String, i64> {
        // Stub implementation: extract models from satisfiable constraints.
        // A real solver would return variable assignments from the model.
        let mut model = HashMap::new();
        for constraint in &self.constraints {
            if constraint.satisfiable {
                // For symbolic expressions, try to extract variable bindings.
                if let ConstraintExpr::Symbolic(sym) = &constraint.expr {
                    if let crate::concolic::SymbolKind::Constant(v) = &sym.kind {
                        // If the constraint is a simple constant assignment,
                        // add it to the model.
                        if !sym.name.starts_with("const_") {
                            model.insert(sym.name.clone(), *v);
                        }
                    }
                }
            }
        }
        model
    }

    fn reset(&mut self) {
        self.constraints.clear();
    }

    fn clone_box(&self) -> Box<dyn super::SolverBackend> {
        Box::new(StubSolverBackend {
            constraints: self.constraints.clone(),
        })
    }
}

/// P1-S2 (REGRA #0): minimal `ConstraintTranslator` impl that closes
/// the orphan trait declared in `solver.rs:60`. The trait had no
/// implementors before this patch — that is a wiring anti-pattern.
///
/// The stub backend translates to plain SMT-LIB2 text via the existing
/// `constraint_to_smtlib` / `symbol_to_smtlib` helpers. This is a
/// STAND-IN for the full native translation that P2 will provide for
/// Z3 (`z3::Ast<'static>`) and CVC5 (`cvc5::Term`).
impl ConstraintTranslator for StubSolverBackend {
    type Output = String;

    fn translate(&self, expr: &ConstraintExpr) -> Self::Output {
        constraint_to_smtlib(expr)
    }

    fn translate_symbol(&self, expr: &crate::concolic::SymbolExpr) -> Self::Output {
        symbol_to_smtlib(expr)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::concolic::{SymbolExpr, SymbolKind};

    #[test]
    fn test_stub_new() {
        let backend = StubSolverBackend::new();
        assert!(backend.constraints.is_empty());
    }

    #[test]
    fn test_stub_assert_and_check_sat() {
        let mut backend = StubSolverBackend::new();
        let constraint = Constraint::new("x > 0", ConstraintExpr::Bool(true));
        backend.assert(&constraint);
        assert!(backend.check_sat());
    }

    #[test]
    fn test_stub_check_sat_with_false_constraint() {
        let mut backend = StubSolverBackend::new();
        let false_constraint = Constraint::unsatisfiable();
        backend.assert(&false_constraint);
        // Unsatisfiable constraint should cause check_sat to return false.
        assert!(!backend.check_sat());
    }

    #[test]
    fn test_stub_multiple_constraints() {
        let mut backend = StubSolverBackend::new();
        backend.assert(&Constraint::new("a", ConstraintExpr::Bool(true)));
        backend.assert(&Constraint::new("b", ConstraintExpr::Bool(true)));
        assert!(backend.check_sat());
    }

    #[test]
    fn test_stub_reset() {
        let mut backend = StubSolverBackend::new();
        backend.assert(&Constraint::new("x", ConstraintExpr::Bool(true)));
        assert!(!backend.constraints.is_empty());
        backend.reset();
        assert!(backend.constraints.is_empty());
    }

    #[test]
    fn test_stub_get_model_empty() {
        let backend = StubSolverBackend::new();
        let model = backend.get_model();
        assert!(model.is_empty());
    }

    #[test]
    fn test_stub_get_model_with_constant() {
        let mut backend = StubSolverBackend::new();
        let expr = SymbolExpr {
            name: "y".into(),
            kind: SymbolKind::Constant(42),
        };
        backend.assert(&Constraint::new("y = 42", ConstraintExpr::Symbolic(expr)));
        let model = backend.get_model();
        // The stub extracts constants that don't start with "const_"
        // and have variable names.
        assert_eq!(model.get("y"), Some(&42));
    }

    #[test]
    fn test_stub_get_model_ignores_const_prefix() {
        let mut backend = StubSolverBackend::new();
        // A constant expression with name starting with "const_"
        // should NOT be added to the model (it's a literal, not a variable).
        let expr = SymbolExpr {
            name: "const_99".into(),
            kind: SymbolKind::Constant(99),
        };
        backend.assert(&Constraint::new("const_99", ConstraintExpr::Symbolic(expr)));
        let model = backend.get_model();
        assert!(!model.contains_key("const_99"));
    }

    #[test]
    fn test_stub_check_sat_after_reset() {
        let mut backend = StubSolverBackend::new();
        backend.assert(&Constraint::unsatisfiable());
        assert!(!backend.check_sat());
        backend.reset();
        // After reset, should be satisfiable (no constraints).
        assert!(backend.check_sat());
    }

    #[test]
    fn test_stub_model_with_multiple_vars() {
        let mut backend = StubSolverBackend::new();
        let x_expr = SymbolExpr {
            name: "x".into(),
            kind: SymbolKind::Constant(10),
        };
        let y_expr = SymbolExpr {
            name: "y".into(),
            kind: SymbolKind::Constant(20),
        };
        backend.assert(&Constraint::new("x=10", ConstraintExpr::Symbolic(x_expr)));
        backend.assert(&Constraint::new("y=20", ConstraintExpr::Symbolic(y_expr)));
        let model = backend.get_model();
        assert_eq!(model.get("x"), Some(&10));
        assert_eq!(model.get("y"), Some(&20));
    }

    #[test]
    fn test_stub_and_constraint() {
        let mut backend = StubSolverBackend::new();
        let c1 = Constraint::new("c1", ConstraintExpr::Bool(true));
        let c2 = Constraint::new("c2", ConstraintExpr::Bool(true));
        let and_constraint = Constraint::new("c1 AND c2", ConstraintExpr::And(vec![c1, c2]));
        backend.assert(&and_constraint);
        assert!(backend.check_sat());
    }

    #[test]
    fn test_stub_or_constraint() {
        let mut backend = StubSolverBackend::new();
        let c1 = Constraint::new("c1", ConstraintExpr::Bool(true));
        let c2 = Constraint::new("c2", ConstraintExpr::Bool(false));
        let or_constraint = Constraint::new("c1 OR c2", ConstraintExpr::Or(vec![c1, c2]));
        backend.assert(&or_constraint);
        // At least one should be satisfiable
        assert!(backend.check_sat());
    }

    #[test]
    fn test_stub_false_constraint_in_and() {
        let mut backend = StubSolverBackend::new();
        let c1 = Constraint::new("c1", ConstraintExpr::Bool(true));
        let c2 = Constraint::new("c2", ConstraintExpr::Bool(false));
        let and_constraint = Constraint::new("c1 AND c2", ConstraintExpr::And(vec![c1, c2]));
        backend.assert(&and_constraint);
        // AND with a false constraint - the stub checks individual constraint satisfiability
        // so this may still return true since our stub doesn't actually evaluate the And expression.
        // The stub implementation relies on constraint.satisfiable flag.
        assert!(backend.check_sat());
    }

    #[test]
    fn test_stub_with_symbolic_expression() {
        let mut backend = StubSolverBackend::new();
        let x = SymbolExpr::variable("x");
        let y = SymbolExpr::variable("y");
        let sum = SymbolExpr {
            name: "sum".into(),
            kind: SymbolKind::Add(Box::new(x), Box::new(y)),
        };
        backend.assert(&Constraint::new("x + y", ConstraintExpr::Symbolic(sum)));
        // Should not panic.
        assert!(backend.check_sat());
        let model = backend.get_model();
        // Stub doesn't compute model for complex expressions.
        assert!(model.is_empty());
    }

    #[test]
    fn test_stub_ite_constraint() {
        let mut backend = StubSolverBackend::new();
        let ite = ConstraintExpr::Ite(
            Box::new(ConstraintExpr::Bool(true)),
            Box::new(ConstraintExpr::Bool(true)),
            Box::new(ConstraintExpr::Bool(false)),
        );
        backend.assert(&Constraint::new("ite constraint", ite));
        assert!(backend.check_sat());
    }

    #[test]
    fn test_stub_implies_constraint() {
        let mut backend = StubSolverBackend::new();
        let implies = ConstraintExpr::Implies(
            Box::new(ConstraintExpr::Bool(true)),
            Box::new(ConstraintExpr::Bool(false)),
        );
        backend.assert(&Constraint::new("implies", implies));
        // True => False is False, but our stub only checks .satisfiable flag.
        assert!(backend.check_sat());
    }

    #[test]
    fn test_stub_distinct_constraint() {
        let mut backend = StubSolverBackend::new();
        let distinct = ConstraintExpr::Distinct(
            Box::new(ConstraintExpr::Symbolic(SymbolExpr::variable("a"))),
            Box::new(ConstraintExpr::Symbolic(SymbolExpr::variable("b"))),
        );
        backend.assert(&Constraint::new("a != b", distinct));
        assert!(backend.check_sat());
    }

    #[test]
    fn test_stub_forall_constraint() {
        let mut backend = StubSolverBackend::new();
        let forall = ConstraintExpr::ForAll(
            "i".into(),
            Box::new(ConstraintExpr::Bool(true)),
            Box::new(ConstraintExpr::Bool(true)),
        );
        backend.assert(&Constraint::new("forall i", forall));
        assert!(backend.check_sat());
    }

    #[test]
    fn test_stub_exists_constraint() {
        let mut backend = StubSolverBackend::new();
        let exists = ConstraintExpr::Exists(
            "j".into(),
            Box::new(ConstraintExpr::Bool(true)),
            Box::new(ConstraintExpr::Bool(false)),
        );
        backend.assert(&Constraint::new("exists j", exists));
        // Stub returns true based on satisfiable flag.
        assert!(backend.check_sat());
    }

    #[test]
    fn test_stub_no_double_count_after_reset() {
        let mut backend = StubSolverBackend::new();
        backend.assert(&Constraint::new("x", ConstraintExpr::Bool(true)));
        backend.reset();
        backend.assert(&Constraint::new("y", ConstraintExpr::Bool(true)));
        assert_eq!(backend.constraints.len(), 1);
    }

    #[test]
    fn test_stub_model_variable_only() {
        let mut backend = StubSolverBackend::new();
        // Variable without a constant value should not appear in model.
        let var_expr = SymbolExpr::variable("unknown_var");
        backend.assert(&Constraint::new("var", ConstraintExpr::Symbolic(var_expr)));
        let model = backend.get_model();
        // Variables with kind Variable (not Constant) are not added to model.
        assert!(model.is_empty());
    }
}
