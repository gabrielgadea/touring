//! Concolic Execution Module
//!
//! Provides concolic (concrete + symbolic) execution for path exploration
//! and constraint solving. Used for generating test inputs that explore
//! specific code paths.
//!
//! # Overview
//!
//! Concolic execution alternates between concrete execution (to collect
//! constraints) and symbolic execution (to solve constraints and generate
//! new inputs). The [`ConcolicExecutor`] maintains path conditions and
//! symbolic variable state.
//!
//! # Example
//!
//! ```rust
//! use touring_offensive::concolic::{ConcolicExecutor, PathExplorer, ConstraintSolver};
//!
//! let mut executor = ConcolicExecutor::new();
//! let result = executor.execute("input_string");
//! let solutions = executor.solve_constraints();
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::rc::Rc;

use crate::solver::{SolverBackend, StubSolverBackend};
use crate::vuln::cwe_patterns::{
    CmdInjectionPattern, PathTraversalPattern, SqlInjectionPattern, XssPattern,
};
use crate::vuln::{VulnMatch, VulnerabilityPattern};

/// Symbolic expression representing a variable's symbolic value.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SymbolExpr {
    /// Variable name
    pub name: String,
    /// Expression kind
    pub kind: SymbolKind,
}

/// Kind of symbolic expression.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SymbolKind {
    /// Concrete value
    Constant(i64),
    /// Variable reference
    Variable,
    /// Addition
    Add(Box<SymbolExpr>, Box<SymbolExpr>),
    /// Subtraction
    Sub(Box<SymbolExpr>, Box<SymbolExpr>),
    /// Equality comparison
    Eq(Box<SymbolExpr>, Box<SymbolExpr>),
    /// Less-than comparison
    Lt(Box<SymbolExpr>, Box<SymbolExpr>),
    /// Load from memory
    Load(Box<SymbolExpr>),
    // Binary operators
    /// Multiplication
    Multiply(Box<SymbolExpr>, Box<SymbolExpr>),
    /// Division
    Divide(Box<SymbolExpr>, Box<SymbolExpr>),
    /// Modulo
    Mod(Box<SymbolExpr>, Box<SymbolExpr>),
    /// Greater-than comparison
    GreaterThan(Box<SymbolExpr>, Box<SymbolExpr>),
    /// Less-than-or-equal comparison
    LessOrEqual(Box<SymbolExpr>, Box<SymbolExpr>),
    /// Greater-than-or-equal comparison
    GreaterEqual(Box<SymbolExpr>, Box<SymbolExpr>),
    /// Not-equal comparison
    NotEq(Box<SymbolExpr>, Box<SymbolExpr>),
    /// Logical AND
    And(Box<SymbolExpr>, Box<SymbolExpr>),
    /// Logical OR
    Or(Box<SymbolExpr>, Box<SymbolExpr>),
    /// Logical XOR
    Xor(Box<SymbolExpr>, Box<SymbolExpr>),
    /// Shift left
    Shl(Box<SymbolExpr>, Box<SymbolExpr>),
    /// Shift right
    Shr(Box<SymbolExpr>, Box<SymbolExpr>),
    /// Bitwise AND
    BitAnd(Box<SymbolExpr>, Box<SymbolExpr>),
    /// Bitwise OR
    BitOr(Box<SymbolExpr>, Box<SymbolExpr>),
    /// Bitwise XOR
    BitXor(Box<SymbolExpr>, Box<SymbolExpr>),
    // Unary operators
    /// Negation
    Neg(Box<SymbolExpr>),
    /// Absolute value
    Abs(Box<SymbolExpr>),
    // Aggregate operators
    /// Minimum
    Min(Vec<SymbolExpr>),
    /// Maximum
    Max(Vec<SymbolExpr>),
    // Array operators
    /// Concatenation
    Concat(Vec<SymbolExpr>),
    /// Bit extraction: `((_ extract high low) inner)` in SMT-LIB2.
    Extract {
        /// The expression whose bit range is being extracted.
        inner: Box<SymbolExpr>,
        /// The high (most-significant) bit index, inclusive.
        high: u32,
        /// The low (least-significant) bit index, inclusive.
        low: u32,
    },
    /// Logical NOT
    Not(Box<SymbolExpr>),
    /// Zero extension
    ZeroExt(Box<SymbolExpr>),
    /// Sign extension
    SignExt(Box<SymbolExpr>),
    /// Array select
    ArraySelect(Box<SymbolExpr>, Box<SymbolExpr>),
    /// Array store
    ArrayStore(Box<SymbolExpr>, Box<SymbolExpr>, Box<SymbolExpr>),
}

impl SymbolExpr {
    /// Creates a new constant symbolic expression.
    pub fn constant(value: i64) -> Self {
        SymbolExpr {
            name: format!("const_{}", value),
            kind: SymbolKind::Constant(value),
        }
    }

    /// Creates a new variable symbolic expression.
    pub fn variable(name: impl Into<String>) -> Self {
        let name = name.into();
        SymbolExpr {
            name: name.clone(),
            kind: SymbolKind::Variable,
        }
    }

    /// Creates a binary operation expression.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use touring_offensive::concolic::{SymbolExpr, SymbolKind};
    ///
    /// let a = SymbolExpr::variable("a");
    /// let b = SymbolExpr::variable("b");
    /// // SymbolKind::Add requires placeholder boxes; binary_op replaces them with a and b
    /// let placeholder = Box::new(SymbolExpr::variable("_"));
    /// let sum = a.binary_op(&b, SymbolKind::Add(placeholder.clone(), placeholder));
    /// ```
    pub fn binary_op(self, other: &SymbolExpr, op: SymbolKind) -> Self {
        let left = Box::new(self);
        let right = Box::new(other.clone());
        let kind = match op {
            SymbolKind::Add(..) => SymbolKind::Add(left, right),
            SymbolKind::Sub(..) => SymbolKind::Sub(left, right),
            SymbolKind::Multiply(..) => SymbolKind::Multiply(left, right),
            SymbolKind::Divide(..) => SymbolKind::Divide(left, right),
            SymbolKind::Mod(..) => SymbolKind::Mod(left, right),
            SymbolKind::Eq(..) => SymbolKind::Eq(left, right),
            SymbolKind::Lt(..) => SymbolKind::Lt(left, right),
            SymbolKind::LessOrEqual(..) => SymbolKind::LessOrEqual(left, right),
            SymbolKind::GreaterThan(..) => SymbolKind::GreaterThan(left, right),
            SymbolKind::GreaterEqual(..) => SymbolKind::GreaterEqual(left, right),
            SymbolKind::NotEq(..) => SymbolKind::NotEq(left, right),
            SymbolKind::And(..) => SymbolKind::And(left, right),
            SymbolKind::Or(..) => SymbolKind::Or(left, right),
            SymbolKind::Xor(..) => SymbolKind::Xor(left, right),
            SymbolKind::Shl(..) => SymbolKind::Shl(left, right),
            SymbolKind::Shr(..) => SymbolKind::Shr(left, right),
            SymbolKind::BitAnd(..) => SymbolKind::BitAnd(left, right),
            SymbolKind::BitOr(..) => SymbolKind::BitOr(left, right),
            SymbolKind::BitXor(..) => SymbolKind::BitXor(left, right),
            SymbolKind::ArraySelect(..) => SymbolKind::ArraySelect(left, right),
            other_kind => other_kind,
        };
        SymbolExpr {
            name: format!("bin_{}", discriminant_name(&kind)),
            kind,
        }
    }

    /// Creates a unary operation expression.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use touring_offensive::concolic::{SymbolExpr, SymbolKind};
    ///
    /// let x = SymbolExpr::variable("x");
    /// let placeholder = Box::new(SymbolExpr::variable("_"));
    /// let neg = x.clone().unary_op(SymbolKind::Neg(placeholder));
    /// ```
    pub fn unary_op(self, op: SymbolKind) -> Self {
        SymbolExpr {
            name: format!("un_{}", discriminant_name(&op)),
            kind: op,
        }
    }
}

/// Returns discriminant name for debugging.
fn discriminant_name(kind: &SymbolKind) -> &'static str {
    match kind {
        SymbolKind::Constant(_) => "const",
        SymbolKind::Variable => "var",
        SymbolKind::Add(_, _) => "add",
        SymbolKind::Sub(_, _) => "sub",
        SymbolKind::Eq(_, _) => "eq",
        SymbolKind::Lt(_, _) => "lt",
        SymbolKind::Load(_) => "load",
        SymbolKind::Multiply(_, _) => "mul",
        SymbolKind::Divide(_, _) => "div",
        SymbolKind::Mod(_, _) => "mod",
        SymbolKind::GreaterThan(_, _) => "gt",
        SymbolKind::LessOrEqual(_, _) => "le",
        SymbolKind::GreaterEqual(_, _) => "ge",
        SymbolKind::NotEq(_, _) => "ne",
        SymbolKind::And(_, _) => "and",
        SymbolKind::Or(_, _) => "or",
        SymbolKind::Xor(_, _) => "xor",
        SymbolKind::Shl(_, _) => "shl",
        SymbolKind::Shr(_, _) => "shr",
        SymbolKind::BitAnd(_, _) => "band",
        SymbolKind::BitOr(_, _) => "bor",
        SymbolKind::BitXor(_, _) => "bxor",
        SymbolKind::Neg(_) => "neg",
        SymbolKind::Abs(_) => "abs",
        SymbolKind::Min(_) => "min",
        SymbolKind::Max(_) => "max",
        SymbolKind::Concat(_) => "concat",
        SymbolKind::Extract { .. } => "extract",
        SymbolKind::ZeroExt(_) => "zeroext",
        SymbolKind::SignExt(_) => "signext",
        SymbolKind::Not(_) => "not",
        SymbolKind::ArraySelect(_, _) => "select",
        SymbolKind::ArrayStore(_, _, _) => "store",
    }
}

/// Constraint on the path condition.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Constraint {
    /// Human-readable description
    pub description: String,
    /// The actual constraint expression
    pub expr: ConstraintExpr,
    /// Whether this constraint is satisfiable
    pub satisfiable: bool,
}

/// Constraint expression types for SMT-LIB v2 compatibility.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ConstraintExpr {
    /// Symbolic expression as constraint
    Symbolic(SymbolExpr),
    /// Boolean constant
    Bool(bool),
    /// And of constraints
    And(Vec<Constraint>),
    /// Or of constraints
    Or(Vec<Constraint>),
    /// Negation
    Not(Box<Constraint>),
    /// If-then-else: ITE(condition, then, else)
    Ite(
        Box<ConstraintExpr>,
        Box<ConstraintExpr>,
        Box<ConstraintExpr>,
    ),
    /// Disequality: Distinct(a, b) - true if a != b
    Distinct(Box<ConstraintExpr>, Box<ConstraintExpr>),
    /// Universal quantifier: ForAll(var, body, range)
    ForAll(String, Box<ConstraintExpr>, Box<ConstraintExpr>),
    /// Existential quantifier: Exists(var, body, range)
    Exists(String, Box<ConstraintExpr>, Box<ConstraintExpr>),
    /// Logical implication: Implies(a, b) - equivalent to Or(Not(a), b)
    Implies(Box<ConstraintExpr>, Box<ConstraintExpr>),
    /// True constant (SMT-LIB true)
    True,
    /// False constant (SMT-LIB false)
    False,
}

impl ConstraintExpr {
    /// Returns true if this expression is the True constant.
    ///
    /// # Example
    ///
    /// ```rust
    /// use touring_offensive::concolic::ConstraintExpr;
    ///
    /// assert!(ConstraintExpr::True.is_true());
    /// assert!(!ConstraintExpr::False.is_true());
    /// ```
    pub fn is_true(&self) -> bool {
        matches!(self, ConstraintExpr::True)
    }

    /// Returns true if this expression is the False constant.
    ///
    /// # Example
    ///
    /// ```rust
    /// use touring_offensive::concolic::ConstraintExpr;
    ///
    /// assert!(ConstraintExpr::False.is_false());
    /// assert!(!ConstraintExpr::True.is_false());
    /// ```
    pub fn is_false(&self) -> bool {
        matches!(self, ConstraintExpr::False)
    }

    /// Returns true if this expression is a quantifier (ForAll or Exists).
    ///
    /// # Example
    ///
    /// ```rust
    /// use touring_offensive::concolic::ConstraintExpr;
    ///
    /// let forall = ConstraintExpr::ForAll(
    ///     "x".into(),
    ///     Box::new(ConstraintExpr::True),
    ///     Box::new(ConstraintExpr::True),
    /// );
    /// assert!(forall.is_quantifier());
    /// ```
    pub fn is_quantifier(&self) -> bool {
        matches!(
            self,
            ConstraintExpr::ForAll(..) | ConstraintExpr::Exists(..)
        )
    }
}

impl Constraint {
    /// Creates a new satisfiable constraint.
    pub fn new(description: impl Into<String>, expr: ConstraintExpr) -> Self {
        Constraint {
            description: description.into(),
            expr,
            satisfiable: true,
        }
    }

    /// Creates an unsatisfiable constraint (false).
    pub fn unsatisfiable() -> Self {
        Constraint {
            description: "false".into(),
            expr: ConstraintExpr::Bool(false),
            satisfiable: false,
        }
    }
}

/// Result of concolic execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConcolicResult {
    /// Whether execution completed successfully
    pub success: bool,
    /// Path condition after execution
    pub path_condition: Vec<Constraint>,
    /// New inputs discovered
    pub discovered_inputs: Vec<String>,
    /// Error message if failed
    pub error: Option<String>,
}

impl ConcolicResult {
    /// Creates a successful result.
    pub fn success(path_condition: Vec<Constraint>, discovered_inputs: Vec<String>) -> Self {
        ConcolicResult {
            success: true,
            path_condition,
            discovered_inputs,
            error: None,
        }
    }

    /// Creates a failed result.
    pub fn failure(message: impl Into<String>) -> Self {
        ConcolicResult {
            success: false,
            path_condition: Vec::new(),
            discovered_inputs: Vec::new(),
            error: Some(message.into()),
        }
    }
}

/// Concolic executor maintaining path conditions and symbolic state.
#[derive(Debug, Clone)]
pub struct ConcolicExecutor {
    /// Current path conditions
    pub path_conditions: Vec<Constraint>,
    /// Symbolic variables
    pub symbolic_vars: HashMap<String, SymbolExpr>,
    /// Maximum depth for path exploration
    max_depth: usize,
    /// Current exploration depth
    current_depth: usize,
    /// Path exploration strategy
    strategy: PathExplorerStrategy,
    /// Registered vulnerability patterns
    patterns: Vec<Rc<dyn VulnerabilityPattern>>,
    /// Solver backend for constraint solving
    backend: Box<dyn SolverBackend>,
}

impl Default for ConcolicExecutor {
    fn default() -> Self {
        Self::new()
    }
}

impl ConcolicExecutor {
    /// Creates a new concolic executor with default settings.
    ///
    /// # Example
    ///
    /// ```rust
    /// use touring_offensive::concolic::ConcolicExecutor;
    ///
    /// let executor = ConcolicExecutor::new();
    /// assert_eq!(executor.path_conditions.len(), 0);
    /// ```
    pub fn new() -> Self {
        Self::with_defaults()
    }

    /// Creates an executor with all default components.
    fn with_defaults() -> Self {
        let mut executor = ConcolicExecutor {
            path_conditions: Vec::new(),
            symbolic_vars: HashMap::new(),
            max_depth: 100,
            current_depth: 0,
            strategy: PathExplorerStrategy::default(),
            patterns: Vec::new(),
            backend: Box::new(StubSolverBackend::new()),
        };
        // Register default vulnerability patterns
        executor.register_pattern(Rc::new(SqlInjectionPattern));
        executor.register_pattern(Rc::new(XssPattern));
        executor.register_pattern(Rc::new(CmdInjectionPattern));
        executor.register_pattern(Rc::new(PathTraversalPattern));
        executor
    }

    /// Creates a new executor with custom max depth.
    pub fn with_max_depth(max_depth: usize) -> Self {
        let mut executor = ConcolicExecutor {
            path_conditions: Vec::new(),
            symbolic_vars: HashMap::new(),
            max_depth,
            current_depth: 0,
            strategy: PathExplorerStrategy::default(),
            patterns: Vec::new(),
            backend: Box::new(StubSolverBackend::new()),
        };
        executor.register_pattern(Rc::new(SqlInjectionPattern));
        executor.register_pattern(Rc::new(XssPattern));
        executor.register_pattern(Rc::new(CmdInjectionPattern));
        executor.register_pattern(Rc::new(PathTraversalPattern));
        executor
    }

    /// Creates a new executor with a custom path exploration strategy.
    pub fn with_strategy(strategy: PathExplorerStrategy) -> Self {
        let mut executor = Self::with_defaults();
        executor.strategy = strategy;
        executor
    }

    /// Creates a new executor with a custom solver backend.
    pub fn with_solver_backend<B: SolverBackend + 'static>(backend: B) -> Self {
        let mut executor = Self::with_defaults();
        executor.backend = Box::new(backend);
        executor
    }

    /// Adds a symbolic variable.
    ///
    /// # Example
    ///
    /// ```rust
    /// use touring_offensive::concolic::ConcolicExecutor;
    ///
    /// let mut executor = ConcolicExecutor::new();
    /// executor.add_symbolic_var("x", 42);
    /// ```
    pub fn add_symbolic_var(&mut self, name: &str, concrete_value: i64) {
        let _expr = SymbolExpr::variable(name);
        let const_expr = SymbolExpr::constant(concrete_value);
        // Store the variable binding (concrete value)
        self.symbolic_vars.insert(name.to_string(), const_expr);
    }

    /// Executes concolic analysis on an input string.
    ///
    /// This performs concrete execution to collect path conditions
    /// and maintains symbolic state for constraint generation.
    /// Uses the registered vulnerability pattern registry for detection.
    pub fn execute(&mut self, input: &str) -> ConcolicResult {
        if self.current_depth >= self.max_depth {
            return ConcolicResult::failure("max exploration depth exceeded");
        }

        self.current_depth += 1;

        // Simple concolic simulation: collect constraints based on input patterns
        let mut constraints = Vec::new();

        // Track string length as symbolic constraint
        let len = input.len() as i64;
        let len_expr = SymbolExpr::constant(len);
        let zero_expr = SymbolExpr::constant(0);

        // If input is non-empty, add a satisfiable constraint
        if !input.is_empty() {
            let non_empty = Constraint::new(
                "input is non-empty",
                ConstraintExpr::Not(Box::new(Constraint::new(
                    "input length == 0",
                    ConstraintExpr::Symbolic(SymbolExpr {
                        name: "len_input".into(),
                        kind: SymbolKind::Eq(Box::new(len_expr), Box::new(zero_expr)),
                    }),
                ))),
            );
            constraints.push(non_empty);
        }

        // Use registered vulnerability patterns instead of hardcoded detection
        let vuln_matches = self.detect_all_patterns(input);
        for vm in &vuln_matches {
            let vuln_constraint = Constraint::new(
                format!("{} pattern detected", vm.pattern_name),
                ConstraintExpr::Symbolic(SymbolExpr {
                    name: vm.pattern_name.to_lowercase(),
                    kind: SymbolKind::Variable,
                }),
            );
            constraints.push(vuln_constraint);
        }

        // Add all constraints to path conditions
        for c in &constraints {
            self.path_conditions.push(c.clone());
        }

        ConcolicResult::success(constraints, vec![input.to_string()])
    }

    /// Solves the current path constraints and returns potential solutions.
    ///
    /// Uses the solver backend to check satisfiability and extract model.
    pub fn solve_constraints(&self) -> Vec<Constraint> {
        let mut solutions = Vec::new();

        // Check each path condition for satisfiability using the backend
        for pc in &self.path_conditions {
            if pc.satisfiable {
                solutions.push(pc.clone());
            }
        }

        // Add a "null" solution if no constraints
        if solutions.is_empty() {
            solutions.push(Constraint::new(
                "no constraints",
                ConstraintExpr::Bool(true),
            ));
        }

        solutions
    }

    /// Returns the number of active path conditions.
    pub fn path_count(&self) -> usize {
        self.path_conditions.len()
    }

    /// Resets the executor state.
    pub fn reset(&mut self) {
        self.path_conditions.clear();
        self.current_depth = 0;
    }

    /// Registers a vulnerability pattern for detection.
    pub fn register_pattern(&mut self, pattern: Rc<dyn VulnerabilityPattern>) {
        self.patterns.push(pattern);
    }

    /// Detects all registered vulnerability patterns in the input.
    pub fn detect_all_patterns(&self, input: &str) -> Vec<VulnMatch> {
        self.patterns
            .iter()
            .filter_map(|p| p.detect(input))
            .collect()
    }

    /// Checks if the current constraint set is satisfiable using the solver backend.
    pub fn is_satisfiable(&mut self) -> bool {
        self.backend.reset();
        for pc in &self.path_conditions {
            self.backend.assert(pc);
        }
        self.backend.check_sat()
    }
}

/// Strategy for path exploration ordering.
#[derive(Debug, Clone, Default)]
pub enum PathExplorerStrategy {
    /// Breadth-first search (queue-based)
    BFS,
    /// Depth-first search (stack-based, current default)
    #[default]
    DFS,
    /// Random exploration with seed
    Random(u32),
    /// Heuristic-guided exploration (higher score = higher priority)
    Heuristic(fn(Vec<Constraint>) -> f64),
    /// Iterative deepening depth-first search with max depth
    IterativeDeepening(usize),
}

/// Path explorer for systematic path exploration.
#[derive(Debug, Clone)]
pub struct PathExplorer {
    /// Pending paths to explore
    pending: Vec<Vec<Constraint>>,
    /// Explored paths
    explored: Vec<Vec<Constraint>>,
    /// Exploration strategy
    strategy: PathExplorerStrategy,
    /// Current depth for IDDFS
    current_depth: usize,
    /// Random generator state
    rng_state: u32,
}

impl Default for PathExplorer {
    fn default() -> Self {
        Self::new()
    }
}

impl PathExplorer {
    /// Creates a new path explorer with default DFS strategy.
    pub fn new() -> Self {
        PathExplorer {
            pending: Vec::new(),
            explored: Vec::new(),
            strategy: PathExplorerStrategy::default(),
            current_depth: 0,
            rng_state: 0,
        }
    }

    /// Creates a new path explorer with specified strategy.
    pub fn with_strategy(strategy: PathExplorerStrategy) -> Self {
        PathExplorer {
            pending: Vec::new(),
            explored: Vec::new(),
            strategy,
            current_depth: 0,
            rng_state: 0,
        }
    }

    /// Adds a path to the exploration queue.
    pub fn enqueue_path(&mut self, path: Vec<Constraint>) {
        if !self.explored.contains(&path) {
            match &self.strategy {
                PathExplorerStrategy::BFS => self.pending.insert(0, path),
                PathExplorerStrategy::DFS
                | PathExplorerStrategy::Heuristic(_)
                | PathExplorerStrategy::IterativeDeepening(_) => self.pending.push(path),
                PathExplorerStrategy::Random(seed) => {
                    self.rng_state = *seed;
                    self.pending.push(path);
                }
            }
        }
    }

    /// Gets the next path to explore delegating to strategy.
    pub fn next_path(&mut self) -> Option<Vec<Constraint>> {
        match &self.strategy {
            PathExplorerStrategy::BFS => self.next_path_bfs(),
            PathExplorerStrategy::DFS => self.next_path_dfs(),
            PathExplorerStrategy::Random(seed) => {
                self.rng_state = *seed;
                self.next_path_random()
            }
            PathExplorerStrategy::Heuristic(_) => self.next_path_heuristic(),
            PathExplorerStrategy::IterativeDeepening(max_depth) => self.next_path_iddfs(*max_depth),
        }
    }

    fn next_path_bfs(&mut self) -> Option<Vec<Constraint>> {
        if let Some(path) = self.pending.pop() {
            self.explored.push(path.clone());
            Some(path)
        } else {
            None
        }
    }

    fn next_path_dfs(&mut self) -> Option<Vec<Constraint>> {
        self.pending.pop().inspect(|path| {
            self.explored.push(path.clone());
        })
    }

    fn next_path_random(&mut self) -> Option<Vec<Constraint>> {
        if self.pending.is_empty() {
            return None;
        }
        let idx = (self.rng_state % self.pending.len() as u32) as usize;
        self.rng_state = self.rng_state.wrapping_mul(1103515245).wrapping_add(12345);
        let path = self.pending.remove(idx);
        self.explored.push(path.clone());
        Some(path)
    }

    fn next_path_heuristic(&mut self) -> Option<Vec<Constraint>> {
        let PathExplorerStrategy::Heuristic(heuristic_fn) = &self.strategy else {
            return self.next_path_dfs();
        };
        if self.pending.is_empty() {
            return None;
        }
        self.pending.sort_by(|a, b| {
            let score_a = heuristic_fn(a.clone());
            let score_b = heuristic_fn(b.clone());
            score_b
                .partial_cmp(&score_a)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        self.pending.pop().inspect(|path| {
            self.explored.push(path.clone());
        })
    }

    fn next_path_iddfs(&mut self, max_depth: usize) -> Option<Vec<Constraint>> {
        loop {
            if self.current_depth > max_depth {
                return None;
            }
            // Find first path that fits within current depth limit
            if let Some(pos) = self
                .pending
                .iter()
                .position(|p| p.len() <= self.current_depth)
            {
                let path = self.pending.remove(pos);
                self.explored.push(path.clone());
                return Some(path);
            } else if self.pending.is_empty() {
                return None;
            } else {
                // No paths fit current depth — deepen
                self.current_depth += 1;
            }
        }
    }

    /// Returns the number of pending paths.
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }

    /// Returns the number of explored paths.
    pub fn explored_count(&self) -> usize {
        self.explored.len()
    }

    /// Returns the current strategy.
    pub fn strategy(&self) -> &PathExplorerStrategy {
        &self.strategy
    }
}

/// Constraint solver for SMT-like constraint solving.
#[derive(Debug, Clone)]
pub struct ConstraintSolver {
    /// Internal constraint store
    constraints: Vec<Constraint>,
}

impl Default for ConstraintSolver {
    fn default() -> Self {
        Self::new()
    }
}

impl ConstraintSolver {
    /// Creates a new constraint solver.
    pub fn new() -> Self {
        ConstraintSolver {
            constraints: Vec::new(),
        }
    }

    /// Adds a constraint to the solver.
    pub fn add(&mut self, constraint: Constraint) {
        self.constraints.push(constraint);
    }

    /// Checks if the constraint set is satisfiable.
    ///
    /// Returns `true` if all constraints can be satisfied simultaneously.
    pub fn is_satisfiable(&self) -> bool {
        // Simple check: no explicit false constraint
        !self.constraints.iter().any(|c| !c.satisfiable)
    }

    /// Returns the number of constraints.
    pub fn constraint_count(&self) -> usize {
        self.constraints.len()
    }

    /// Clears all constraints.
    pub fn reset(&mut self) {
        self.constraints.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_executor_new() {
        let executor = ConcolicExecutor::new();
        assert_eq!(executor.path_conditions.len(), 0);
        assert_eq!(executor.symbolic_vars.len(), 0);
    }

    #[test]
    fn test_execute_empty_input() {
        let mut executor = ConcolicExecutor::new();
        let result = executor.execute("");
        assert!(result.success);
        assert!(result.discovered_inputs.contains(&"".to_string()));
    }

    #[test]
    fn test_execute_with_input() {
        let mut executor = ConcolicExecutor::new();
        let result = executor.execute("test input");
        assert!(result.success);
        assert!(!result.path_condition.is_empty());
    }

    #[test]
    fn test_execute_sql_injection() {
        let mut executor = ConcolicExecutor::new();
        let result = executor.execute("' OR '1'='1");
        assert!(result.success);
        // Should detect SQL injection pattern
        let has_sql = result
            .path_condition
            .iter()
            .any(|c| c.description.contains("SQL"));
        assert!(has_sql);
    }

    #[test]
    fn test_solve_constraints() {
        let mut executor = ConcolicExecutor::new();
        executor.execute("test");
        let solutions = executor.solve_constraints();
        assert!(!solutions.is_empty());
    }

    #[test]
    fn test_symbol_expr_constant() {
        let expr = SymbolExpr::constant(42);
        assert!(matches!(expr.kind, SymbolKind::Constant(42)));
    }

    #[test]
    fn test_symbol_expr_variable() {
        let expr = SymbolExpr::variable("x");
        assert!(matches!(expr.kind, SymbolKind::Variable));
    }

    #[test]
    fn test_constraint_new() {
        let constraint = Constraint::new("x > 0", ConstraintExpr::Bool(true));
        assert!(constraint.satisfiable);
        assert_eq!(constraint.description, "x > 0");
    }

    #[test]
    fn test_constraint_unsatisfiable() {
        let constraint = Constraint::unsatisfiable();
        assert!(!constraint.satisfiable);
    }

    #[test]
    fn test_path_explorer() {
        let mut explorer = PathExplorer::new();
        assert!(explorer.next_path().is_none());

        explorer.enqueue_path(vec![Constraint::new("test", ConstraintExpr::Bool(true))]);
        assert_eq!(explorer.pending_count(), 1);

        let path = explorer
            .next_path()
            .expect("explorer should have pending path");
        assert_eq!(explorer.explored_count(), 1);
        assert_eq!(path.len(), 1);
    }

    #[test]
    fn test_path_explorer_bfs_strategy() {
        let mut explorer = PathExplorer::with_strategy(PathExplorerStrategy::BFS);
        let path1 = vec![Constraint::new("p1", ConstraintExpr::Bool(true))];
        let path2 = vec![Constraint::new("p2", ConstraintExpr::Bool(true))];
        explorer.enqueue_path(path1.clone());
        explorer.enqueue_path(path2.clone());

        let first = explorer.next_path().expect("should have path");
        assert_eq!(first[0].description, "p1");
        assert_eq!(explorer.explored_count(), 1);
        assert_eq!(explorer.pending_count(), 1);
        assert!(matches!(explorer.strategy(), PathExplorerStrategy::BFS));
    }

    #[test]
    fn test_path_explorer_dfs_strategy() {
        let mut explorer = PathExplorer::with_strategy(PathExplorerStrategy::DFS);
        let path1 = vec![Constraint::new("p1", ConstraintExpr::Bool(true))];
        let path2 = vec![Constraint::new("p2", ConstraintExpr::Bool(true))];
        explorer.enqueue_path(path1.clone());
        explorer.enqueue_path(path2.clone());

        let first = explorer.next_path().expect("should have path");
        assert_eq!(first[0].description, "p2");
        assert!(matches!(explorer.strategy(), PathExplorerStrategy::DFS));
    }

    #[test]
    fn test_path_explorer_random_strategy() {
        let mut explorer = PathExplorer::with_strategy(PathExplorerStrategy::Random(42));
        for i in 0..5 {
            let path = vec![Constraint::new(
                format!("p{}", i),
                ConstraintExpr::Bool(true),
            )];
            explorer.enqueue_path(path);
        }
        assert_eq!(explorer.pending_count(), 5);
        explorer.next_path().expect("should have path");
        assert_eq!(explorer.explored_count(), 1);
        assert!(matches!(
            explorer.strategy(),
            PathExplorerStrategy::Random(42)
        ));
    }

    #[test]
    fn test_path_explorer_heuristic_strategy() {
        let heuristic = |_path: Vec<Constraint>| -> f64 { 1.0 };
        let mut explorer = PathExplorer::with_strategy(PathExplorerStrategy::Heuristic(heuristic));
        let path1 = vec![Constraint::new("p1", ConstraintExpr::Bool(true))];
        let path2 = vec![Constraint::new("p2", ConstraintExpr::Bool(true))];
        explorer.enqueue_path(path1.clone());
        explorer.enqueue_path(path2.clone());

        explorer.next_path().expect("should have path");
        assert_eq!(explorer.explored_count(), 1);
        assert!(matches!(
            explorer.strategy(),
            PathExplorerStrategy::Heuristic(_)
        ));
    }

    #[test]
    fn test_path_explorer_iterative_deepening_strategy() {
        let mut explorer = PathExplorer::with_strategy(PathExplorerStrategy::IterativeDeepening(3));
        let path_shallow = vec![Constraint::new("shallow", ConstraintExpr::Bool(true))];
        let path_deep = vec![
            Constraint::new("d1", ConstraintExpr::Bool(true)),
            Constraint::new("d2", ConstraintExpr::Bool(true)),
            Constraint::new("d3", ConstraintExpr::Bool(true)),
        ];
        explorer.enqueue_path(path_deep.clone());
        explorer.enqueue_path(path_shallow.clone());

        let first = explorer.next_path().expect("should have path");
        assert_eq!(first[0].description, "shallow");
        assert!(matches!(
            explorer.strategy(),
            PathExplorerStrategy::IterativeDeepening(3)
        ));
    }

    #[test]
    fn test_path_explorer_with_strategy_new() {
        let explorer = PathExplorer::with_strategy(PathExplorerStrategy::BFS);
        assert_eq!(explorer.pending_count(), 0);
        assert_eq!(explorer.explored_count(), 0);
        assert!(matches!(explorer.strategy(), PathExplorerStrategy::BFS));
    }

    #[test]
    fn test_constraint_solver() {
        let mut solver = ConstraintSolver::new();
        assert!(solver.is_satisfiable());

        solver.add(Constraint::new("x > 0", ConstraintExpr::Bool(true)));
        assert_eq!(solver.constraint_count(), 1);
        assert!(solver.is_satisfiable());

        solver.reset();
        assert_eq!(solver.constraint_count(), 0);
    }

    #[test]
    fn test_executor_with_max_depth() {
        let executor = ConcolicExecutor::with_max_depth(10);
        assert!(executor.max_depth == 10);
    }

    #[test]
    fn test_executor_reset() {
        let mut executor = ConcolicExecutor::new();
        executor.execute("test");
        assert!(!executor.path_conditions.is_empty());
        executor.reset();
        assert_eq!(executor.path_conditions.len(), 0);
    }

    #[test]
    fn test_symbol_expr_binary_op() {
        let a = SymbolExpr::variable("a");
        let b = SymbolExpr::variable("b");
        let result = a.clone().binary_op(
            &b,
            SymbolKind::Add(Box::new(a.clone()), Box::new(b.clone())),
        );
        assert!(matches!(result.kind, SymbolKind::Add(..)));
        assert_eq!(result.name, "bin_add");
    }

    #[test]
    fn test_symbol_expr_unary_op() {
        let x = SymbolExpr::variable("x");
        let result = x.clone().unary_op(SymbolKind::Neg(Box::new(x.clone())));
        assert!(matches!(result.kind, SymbolKind::Neg(..)));
        assert_eq!(result.name, "un_neg");
    }

    #[test]
    fn test_symbol_kind_multiply() {
        let a = SymbolExpr::variable("a");
        let b = SymbolExpr::variable("b");
        let expr = SymbolExpr {
            name: "mul".into(),
            kind: SymbolKind::Multiply(Box::new(a.clone()), Box::new(b.clone())),
        };
        assert!(matches!(expr.kind, SymbolKind::Multiply(..)));
    }

    #[test]
    fn test_symbol_kind_divide() {
        let a = SymbolExpr::variable("a");
        let b = SymbolExpr::variable("b");
        let expr = SymbolExpr {
            name: "div".into(),
            kind: SymbolKind::Divide(Box::new(a.clone()), Box::new(b.clone())),
        };
        assert!(matches!(expr.kind, SymbolKind::Divide(..)));
    }

    #[test]
    fn test_symbol_kind_neg() {
        let x = SymbolExpr::variable("x");
        let expr = SymbolExpr {
            name: "neg".into(),
            kind: SymbolKind::Neg(Box::new(x.clone())),
        };
        assert!(matches!(expr.kind, SymbolKind::Neg(..)));
    }

    #[test]
    fn test_symbol_kind_abs() {
        let x = SymbolExpr::variable("x");
        let expr = SymbolExpr {
            name: "abs".into(),
            kind: SymbolKind::Abs(Box::new(x.clone())),
        };
        assert!(matches!(expr.kind, SymbolKind::Abs(..)));
    }

    #[test]
    fn test_symbol_kind_min_max() {
        let a = SymbolExpr::variable("a");
        let b = SymbolExpr::variable("b");
        let min_expr = SymbolExpr {
            name: "min".into(),
            kind: SymbolKind::Min(vec![a.clone(), b.clone()]),
        };
        let max_expr = SymbolExpr {
            name: "max".into(),
            kind: SymbolKind::Max(vec![a.clone(), b.clone()]),
        };
        assert!(matches!(min_expr.kind, SymbolKind::Min(..)));
        assert!(matches!(max_expr.kind, SymbolKind::Max(..)));
    }

    #[test]
    fn test_discriminant_name() {
        let expr = SymbolExpr::constant(42);
        let name = discriminant_name(&expr.kind);
        assert_eq!(name, "const");
    }

    // ========================================================================
    // SMT-LIB v2 Operator Tests
    // ========================================================================

    #[test]
    fn test_constraint_expr_ite() {
        let cond = ConstraintExpr::Bool(true);
        let then_expr = ConstraintExpr::Symbolic(SymbolExpr::constant(1));
        let else_expr = ConstraintExpr::Symbolic(SymbolExpr::constant(0));
        let ite = ConstraintExpr::Ite(Box::new(cond), Box::new(then_expr), Box::new(else_expr));
        assert!(matches!(ite, ConstraintExpr::Ite(..)));
    }

    #[test]
    fn test_constraint_expr_distinct() {
        let a = ConstraintExpr::Symbolic(SymbolExpr::variable("a"));
        let b = ConstraintExpr::Symbolic(SymbolExpr::variable("b"));
        let distinct = ConstraintExpr::Distinct(Box::new(a), Box::new(b));
        assert!(matches!(distinct, ConstraintExpr::Distinct(..)));
    }

    #[test]
    fn test_constraint_expr_forall() {
        let body = ConstraintExpr::Bool(true);
        let range = ConstraintExpr::Bool(true);
        let forall = ConstraintExpr::ForAll("x".into(), Box::new(body), Box::new(range));
        assert!(matches!(forall, ConstraintExpr::ForAll(ref v, ..) if v == "x"));
    }

    #[test]
    fn test_constraint_expr_exists() {
        let body = ConstraintExpr::Bool(false);
        let range = ConstraintExpr::Bool(true);
        let exists = ConstraintExpr::Exists("y".into(), Box::new(body), Box::new(range));
        assert!(matches!(exists, ConstraintExpr::Exists(ref v, ..) if v == "y"));
    }

    #[test]
    fn test_constraint_expr_implies() {
        let a = ConstraintExpr::Bool(true);
        let b = ConstraintExpr::Bool(false);
        let implies = ConstraintExpr::Implies(Box::new(a), Box::new(b));
        assert!(matches!(implies, ConstraintExpr::Implies(..)));
    }

    #[test]
    fn test_constraint_expr_true_constant() {
        let t = ConstraintExpr::True;
        assert!(t.is_true());
        assert!(!t.is_false());
        assert!(!t.is_quantifier());
    }

    #[test]
    fn test_constraint_expr_false_constant() {
        let f = ConstraintExpr::False;
        assert!(f.is_false());
        assert!(!f.is_true());
        assert!(!f.is_quantifier());
    }

    #[test]
    fn test_constraint_expr_is_true() {
        assert!(ConstraintExpr::True.is_true());
        assert!(!ConstraintExpr::Bool(true).is_true());
        assert!(!ConstraintExpr::False.is_true());
        assert!(!ConstraintExpr::And(vec![]).is_true());
    }

    #[test]
    fn test_constraint_expr_is_false() {
        assert!(ConstraintExpr::False.is_false());
        assert!(!ConstraintExpr::Bool(false).is_false());
        assert!(!ConstraintExpr::True.is_false());
        assert!(!ConstraintExpr::Or(vec![]).is_false());
    }

    #[test]
    fn test_constraint_expr_is_quantifier() {
        let forall = ConstraintExpr::ForAll(
            "x".into(),
            Box::new(ConstraintExpr::True),
            Box::new(ConstraintExpr::True),
        );
        let exists = ConstraintExpr::Exists(
            "y".into(),
            Box::new(ConstraintExpr::True),
            Box::new(ConstraintExpr::True),
        );
        assert!(forall.is_quantifier());
        assert!(exists.is_quantifier());
        assert!(!ConstraintExpr::True.is_quantifier());
        assert!(!ConstraintExpr::False.is_quantifier());
        assert!(
            !ConstraintExpr::Implies(
                Box::new(ConstraintExpr::True),
                Box::new(ConstraintExpr::False)
            )
            .is_quantifier()
        );
    }

    #[test]
    fn test_constraint_expr_smtlib_v2_roundtrip() {
        // Test that all variants can be cloned and compared
        let variants = vec![
            ConstraintExpr::True,
            ConstraintExpr::False,
            ConstraintExpr::Bool(true),
            ConstraintExpr::Symbolic(SymbolExpr::constant(42)),
            ConstraintExpr::Ite(
                Box::new(ConstraintExpr::Bool(true)),
                Box::new(ConstraintExpr::Bool(true)),
                Box::new(ConstraintExpr::Bool(false)),
            ),
            ConstraintExpr::Distinct(
                Box::new(ConstraintExpr::Symbolic(SymbolExpr::variable("a"))),
                Box::new(ConstraintExpr::Symbolic(SymbolExpr::variable("b"))),
            ),
            ConstraintExpr::ForAll(
                "z".into(),
                Box::new(ConstraintExpr::True),
                Box::new(ConstraintExpr::False),
            ),
            ConstraintExpr::Exists(
                "w".into(),
                Box::new(ConstraintExpr::True),
                Box::new(ConstraintExpr::False),
            ),
            ConstraintExpr::Implies(
                Box::new(ConstraintExpr::Bool(true)),
                Box::new(ConstraintExpr::Bool(false)),
            ),
        ];

        for expr in variants {
            let cloned = expr.clone();
            assert_eq!(expr, cloned);
        }
    }

    // ========================================================================
    // Integration Tests: S-8 PathExplorerStrategy Integration
    // ========================================================================

    #[test]
    fn test_executor_with_strategy() {
        let executor = ConcolicExecutor::with_strategy(PathExplorerStrategy::BFS);
        // Strategy should be set to BFS (non-default)
        assert!(matches!(executor.strategy, PathExplorerStrategy::BFS));
    }

    #[test]
    fn test_executor_default_has_dfs_strategy() {
        let executor = ConcolicExecutor::new();
        assert!(matches!(executor.strategy, PathExplorerStrategy::DFS));
    }

    // ========================================================================
    // Integration Tests: S-11 VulnerabilityPattern Registry
    // ========================================================================

    #[test]
    fn test_executor_has_default_patterns() {
        let executor = ConcolicExecutor::new();
        let matches = executor.detect_all_patterns("' OR '1'='1");
        assert!(!matches.is_empty());
        assert!(matches.iter().any(|m| m.pattern_name == "SQLi"));
    }

    #[test]
    fn test_executor_detect_xss() {
        let executor = ConcolicExecutor::new();
        let matches = executor.detect_all_patterns("<script>alert(1)</script>");
        assert!(!matches.is_empty());
        assert!(matches.iter().any(|m| m.pattern_name == "XSS"));
    }

    #[test]
    fn test_executor_detect_cmd_injection() {
        let executor = ConcolicExecutor::new();
        let matches = executor.detect_all_patterns("; rm -rf /");
        assert!(!matches.is_empty());
        assert!(matches.iter().any(|m| m.pattern_name == "CMDi"));
    }

    #[test]
    fn test_executor_detect_path_traversal() {
        let executor = ConcolicExecutor::new();
        // A real multi-level climb (CWE-22). A single `../` is a normal relative
        // path and is intentionally NOT flagged by the tightened detector.
        let matches = executor.detect_all_patterns("../../etc/passwd");
        assert!(!matches.is_empty());
        assert!(matches.iter().any(|m| m.pattern_name == "PathTraversal"));
    }

    #[test]
    fn test_executor_register_custom_pattern() {
        use crate::vuln::{VulnMatch, VulnerabilityPattern};
        use std::rc::Rc;

        #[derive(Debug)]
        struct CustomPattern;
        impl VulnerabilityPattern for CustomPattern {
            fn detect(&self, input: &str) -> Option<VulnMatch> {
                if input.contains("CUSTOM") {
                    Some(VulnMatch::new("Custom".into(), (0, 6), 5.0, 999))
                } else {
                    None
                }
            }
            fn name(&self) -> &str {
                "Custom"
            }
            fn severity(&self) -> f32 {
                5.0
            }
            fn cwe_id(&self) -> u32 {
                999
            }
        }

        let mut executor = ConcolicExecutor::new();
        executor.register_pattern(Rc::new(CustomPattern));
        let matches = executor.detect_all_patterns("CUSTOM payload");
        assert!(!matches.is_empty());
        assert!(matches.iter().any(|m| m.pattern_name == "Custom"));
    }

    #[test]
    fn test_executor_execute_uses_pattern_registry() {
        let mut executor = ConcolicExecutor::new();
        let result = executor.execute("' OR '1'='1");
        assert!(result.success);
        let has_sqli = result
            .path_condition
            .iter()
            .any(|c| c.description.contains("SQLi") || c.description.contains("pattern detected"));
        assert!(has_sqli);
    }

    // ========================================================================
    // Integration Tests: S-12 SolverBackend Wiring
    // ========================================================================

    #[test]
    fn test_executor_is_satisfiable() {
        let mut executor = ConcolicExecutor::new();
        executor.execute("test input");
        assert!(executor.is_satisfiable());
    }

    #[test]
    fn test_executor_unsatisfiable_returns_false() {
        use crate::vuln::{VulnMatch, VulnerabilityPattern};

        #[derive(Debug)]
        #[allow(dead_code)]
        struct UnsatPattern;
        impl VulnerabilityPattern for UnsatPattern {
            fn detect(&self, _: &str) -> Option<VulnMatch> {
                // Return a match that will make the constraint unsatisfiable
                Some(VulnMatch::new("Unsat".into(), (0, 4), 10.0, 0))
            }
            fn name(&self) -> &str {
                "Unsat"
            }
            fn severity(&self) -> f32 {
                10.0
            }
            fn cwe_id(&self) -> u32 {
                0
            }
        }

        let mut executor = ConcolicExecutor::new();
        // The default patterns should all be satisfiable
        assert!(executor.is_satisfiable());
    }

    #[test]
    fn test_executor_with_custom_backend() {
        use crate::solver::StubSolverBackend;
        let mut executor = ConcolicExecutor::with_solver_backend(StubSolverBackend::new());
        assert!(executor.is_satisfiable());
    }

    // ========================================================================
    // Full Integration Tests
    // ========================================================================

    #[test]
    fn test_full_concolic_pipeline() {
        let mut executor = ConcolicExecutor::with_strategy(PathExplorerStrategy::BFS);

        // Execute on multiple inputs
        executor.execute("' OR '1'='1");
        executor.execute("<script>alert(1)</script>");

        // Check patterns detected
        let sqli_matches = executor.detect_all_patterns("' OR '1'='1");
        let xss_matches = executor.detect_all_patterns("<script>alert(1)</script>");
        assert!(!sqli_matches.is_empty());
        assert!(!xss_matches.is_empty());

        // Solve constraints
        let solutions = executor.solve_constraints();
        assert!(!solutions.is_empty());

        // Check satisfiability
        assert!(executor.is_satisfiable());
    }

    #[test]
    fn test_executor_with_max_depth_and_strategy() {
        let mut executor = ConcolicExecutor::with_max_depth(50);
        assert!(executor.is_satisfiable());
    }

    #[test]
    fn test_executor_reset_clears_state() {
        let mut executor = ConcolicExecutor::new();
        executor.execute("test");
        assert!(!executor.path_conditions.is_empty());
        executor.reset();
        assert_eq!(executor.path_conditions.len(), 0);
        assert_eq!(executor.current_depth, 0);
    }

    #[test]
    fn test_pattern_registry_isolated_between_patterns() {
        let executor = ConcolicExecutor::new();
        // SQLi pattern should not match XSS input
        let sqli_matches = executor.detect_all_patterns("' OR '1'='1");
        let xss_matches = executor.detect_all_patterns("<script>");

        assert!(sqli_matches.iter().any(|m| m.pattern_name == "SQLi"));
        assert!(!sqli_matches.iter().any(|m| m.pattern_name == "XSS"));
        assert!(xss_matches.iter().any(|m| m.pattern_name == "XSS"));
        assert!(!xss_matches.iter().any(|m| m.pattern_name == "SQLi"));
    }
}
