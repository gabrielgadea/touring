//! Module-scope namespace collector: every name a module's body can bind at import time, in EVERY
//! branch, plus the dynamic-binding signals that make that set unenumerable.
//!
//! Why it exists (held-out 20261010, 3 of the 7 false denies): the first extractor read only the
//! DIRECT children of the module node, so a name bound inside `if/else`, `try/except`, `with`,
//! `for`/`while` or `match` never reached the namespace — `numpy` binds `array`/`where` inside an
//! `else:` block, `rust_bridge` binds `RUST_AVAILABLE` in a `try/except ImportError`. And a module
//! that binds names at runtime (`setattr(sys.modules[__name__], item.name, …)` in Pillow,
//! `globals()[k] = …`, a PEP 562 module `__getattr__`) has a namespace no static read can close.
//!
//! The contract with the checks: [`Namespace::names`] is a SUPERSET of what the module can bind
//! (a branch that never runs still contributes — fewer denies, never more), and a module whose
//! dynamic binding cannot be closed raises [`Namespace::has_dynamic`] / [`Namespace::has_getattr`]
//! so the caller abstains instead of denying. The dynamic mechanisms that CAN be closed are
//! closed, not abstained on, so a typo against `numpy` (which has all three shapes: composite
//! `else:`, `globals()[ta]` over a literal list, and a literal-guarded `__getattr__`) still denies:
//!
//! - `globals()[K] = …` / `setattr(<module>, K, …)` / `globals().update({…})` with `K` a string
//!   literal, or a loop variable over a literal list/tuple of strings → those names;
//! - a module `__getattr__` is CLOSED when every `return` sits under an `attr == "lit"` /
//!   `attr in ("a", "b")` guard and the fall-through path ends in `raise` (PEP 562: a function that
//!   falls off the end returns `None`, which is a VALID attribute value, not an error) → the
//!   guarded literals;
//! - everything else (non-literal key, unguarded `return`, `sys.modules[...] = obj`, module-level
//!   `exec`, `module.__class__ = …`, `__getattr__ = lazy.attach(…)`) is OPEN.
//!
//! Only module scope counts: names bound inside `def`/`class` bodies are not module attributes,
//! except what a `global` statement declares.

use std::collections::HashSet;

use tree_sitter::Node;

use super::modules::{last_identifier, static_string_list, text};

/// What the collector learned about one module's namespace.
#[derive(Debug, Default, Clone)]
pub struct Namespace {
    /// Every name bound at module scope in ANY branch, plus the names closed dynamic mechanisms
    /// provably add (a superset of the runtime namespace — over-inclusion only costs recall).
    pub names: HashSet<String>,
    /// `from x import *` in any branch — the namespace is then unenumerable.
    pub has_star: bool,
    /// An OPEN module `__getattr__` (unguarded `return`, no final `raise`, or bound by
    /// assignment/import) — any attribute may resolve at runtime.
    pub has_getattr: bool,
    /// An OPEN dynamic binding (`setattr`/`globals()[…]`/`update` with a non-literal key,
    /// `sys.modules[...] = obj`, module-level `exec`, `module.__class__ = …`).
    pub has_dynamic: bool,
}

/// Callees that only READ the namespace/module they are handed — passing `globals()` to them binds
/// nothing (`print(globals())`, `getattr(module, k)`); every other callee may bind names.
const READ_ONLY_CALLEES: &[&str] = &[
    "print",
    "len",
    "list",
    "dict",
    "sorted",
    "set",
    "tuple",
    "frozenset",
    "isinstance",
    "issubclass",
    "getattr",
    "hasattr",
    "repr",
    "str",
    "id",
    "type",
    "iter",
    "next",
    "enumerate",
    "zip",
    "map",
    "filter",
    "sum",
    "min",
    "max",
    "any",
    "all",
    "vars",
    "dir",
    "callable",
    "bool",
    "int",
    "hash",
    "format",
];

/// A `for` target seen on the way down, with the string literals it iterates (when closed).
type LoopVar = (String, Option<Vec<String>>);

/// Collect the namespace of a parsed module (one alias pass, one binding pass, one signal pass).
pub fn collect(source: &str, root: Node) -> Namespace {
    let mut c = Collector {
        source,
        ns: Namespace::default(),
        module_aliases: HashSet::new(),
        dict_aliases: HashSet::new(),
    };
    c.find_aliases(root);
    walk_tree(root, &mut BindWalk { c: &mut c });
    walk_tree(
        root,
        &mut SignalWalk {
            c: &mut c,
            depth: 0,
            loops: Vec::new(),
            pushed: Vec::new(),
        },
    );
    c.ns
}

/// A pre-order visitor over the NAMED nodes of a tree.
trait Visitor<'t> {
    /// Visit `node`; `true` descends into its children.
    fn enter(&mut self, node: Node<'t>) -> bool;
    /// Called once the subtree of `node` is done (or right after `enter` when it is a leaf or was
    /// pruned).
    fn exit(&mut self, _node: Node<'t>) {}
}

/// Pre-order walk with ONE tree cursor. A `Node::walk()` per node is a heap allocation each, and
/// three recursive passes over a 4 000-line module (Pillow's `Image.py`) cost +17 ms in the
/// measured A/B (29 → 46 ms) — the cursor walk allocates nothing per node.
fn walk_tree<'t, V: Visitor<'t>>(root: Node<'t>, v: &mut V) {
    let mut cursor = root.walk();
    loop {
        let node = cursor.node();
        let named = node.is_named();
        if named && v.enter(node) && cursor.goto_first_child() {
            continue;
        }
        if named {
            v.exit(node);
        }
        loop {
            if cursor.goto_next_sibling() {
                break;
            }
            if !cursor.goto_parent() {
                return;
            }
            let parent = cursor.node();
            if parent.is_named() {
                v.exit(parent);
            }
        }
    }
}

/// The named children of an argument list by index (no cursor, no allocation; argument lists are
/// short, so the O(i) `named_child` is cheaper than a `walk()`).
fn args_of(args: Node) -> impl Iterator<Item = Node> {
    (0..args.named_child_count())
        .filter_map(move |i| u32::try_from(i).ok().and_then(|i| args.named_child(i)))
}

struct Collector<'s> {
    source: &'s str,
    ns: Namespace,
    /// Identifiers assigned a module object (`module = sys.modules[__name__]`).
    module_aliases: HashSet<String>,
    /// Identifiers assigned the namespace dict (`ns = globals()`).
    dict_aliases: HashSet<String>,
}

impl Collector<'_> {
    fn add(&mut self, name: &str) {
        self.ns.names.insert(name.to_string());
    }

    fn t(&self, node: Node) -> String {
        text(self.source, node).to_string()
    }

    // ------------------------------------------------------------------ pass 0: aliases

    /// `module = sys.modules[__name__]` / `ns = globals()` anywhere — the receivers that make a
    /// later `setattr(module, …)` / `ns[k] = …` a write into THIS module. One walk collects every
    /// `name = <expr>` candidate; the fixpoint runs over that short list, because an alias may be
    /// defined from another (`b = a`).
    fn find_aliases(&mut self, root: Node) {
        let mut cands = AliasCandidates {
            source: self.source,
            depth: 0,
            found: Vec::new(),
        };
        walk_tree(root, &mut cands);
        for _ in 0..4 {
            let mut changed = false;
            for (nome, rhs, depth) in &cands.found {
                if !self.module_aliases.contains(nome) && self.is_module_expr(*rhs) {
                    self.module_aliases.insert(nome.clone());
                    changed = true;
                }
                if !self.dict_aliases.contains(nome) && self.is_ns_dict(*rhs, *depth) {
                    self.dict_aliases.insert(nome.clone());
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
    }

    /// An expression that may evaluate to THIS module's object. A module named by a string literal
    /// (`sys.modules["yaml"]`, `import_module("yaml")`) is another module; `__name__`, a variable
    /// or any other expression may be this one — conservative, it only ever adds abstentions.
    fn is_module_expr(&self, n: Node) -> bool {
        match n.kind() {
            "parenthesized_expression" => n.named_child(0).is_some_and(|c| self.is_module_expr(c)),
            "identifier" => self.module_aliases.contains(text(self.source, n)),
            "subscript" => {
                n.child_by_field_name("value")
                    .is_some_and(|v| self.is_sys_modules(v))
                    && n.child_by_field_name("subscript")
                        .is_some_and(|k| k.kind() != "string")
            }
            "call" => self.call_yields_module(n),
            _ => false,
        }
    }

    /// `importlib.import_module(…)` · `__import__(…)` · `inspect.getmodule(…)` · `sys.modules.get(…)`.
    fn call_yields_module(&self, call: Node) -> bool {
        let Some(f) = call.child_by_field_name("function") else {
            return false;
        };
        let last = match f.kind() {
            "identifier" => text(self.source, f),
            "attribute" => f
                .child_by_field_name("attribute")
                .map(|a| text(self.source, a))
                .unwrap_or(""),
            _ => "",
        };
        let by_name = matches!(last, "import_module" | "__import__" | "getmodule")
            || (last == "get"
                && f.child_by_field_name("object")
                    .is_some_and(|o| self.is_sys_modules(o)));
        let literal_first = call
            .child_by_field_name("arguments")
            .and_then(|a| a.named_child(0))
            .is_some_and(|a| a.kind() == "string");
        by_name && !literal_first
    }

    /// `sys.modules` (any object spelled before `.modules`: `_sys.modules` counts too).
    fn is_sys_modules(&self, n: Node) -> bool {
        n.kind() == "attribute"
            && n.child_by_field_name("attribute")
                .is_some_and(|a| text(self.source, a) == "modules")
    }

    /// An expression that evaluates to THIS module's namespace dict.
    fn is_ns_dict(&self, n: Node, depth: usize) -> bool {
        match n.kind() {
            "parenthesized_expression" => {
                n.named_child(0).is_some_and(|c| self.is_ns_dict(c, depth))
            }
            "identifier" => self.dict_aliases.contains(text(self.source, n)),
            "call" => self.call_yields_ns_dict(n, depth),
            "attribute" => {
                n.child_by_field_name("attribute")
                    .is_some_and(|a| text(self.source, a) == "__dict__")
                    && n.child_by_field_name("object")
                        .is_some_and(|o| self.is_module_expr(o))
            }
            _ => false,
        }
    }

    /// `globals()` · `vars()`/`locals()` at module scope · `vars(<module>)`.
    fn call_yields_ns_dict(&self, call: Node, depth: usize) -> bool {
        let (Some(f), Some(args)) = (
            call.child_by_field_name("function"),
            call.child_by_field_name("arguments"),
        ) else {
            return false;
        };
        if f.kind() != "identifier" {
            return false;
        }
        match (text(self.source, f), args.named_child_count()) {
            ("globals", 0) => true,
            // vars()/locals() with no argument are the module dict only at module scope
            ("vars" | "locals", 0) => depth == 0,
            ("vars", 1) => args.named_child(0).is_some_and(|a| self.is_module_expr(a)),
            _ => false,
        }
    }

    // ------------------------------------------------------------------ pass 1: bindings

    /// Bindings of the module body in every branch. `def`/`class` bind their NAME only; their
    /// bodies, lambdas and comprehension scopes are not module scope. Returns whether the walk
    /// should descend into `node`.
    fn bind_node(&mut self, node: Node) -> bool {
        match node.kind() {
            "function_definition" | "class_definition" => self.bind_def_name(node),
            "import_statement" => self.bind_import(node),
            "import_from_statement" | "future_import_statement" => self.bind_import_from(node),
            "type_alias_statement" => self.bind_type_alias(node),
            // `with … as x` / `except E as e` / `case … as y`
            "as_pattern_target" => self.bind_target(node),
            "assignment" | "augmented_assignment" => {
                self.bind_assignment(node);
                return true; // chained `a = b = 1` and a walrus on the right
            }
            "for_statement" => {
                if let Some(l) = node.child_by_field_name("left") {
                    self.bind_target(l);
                }
                return true;
            }
            "named_expression" => {
                self.bind_walrus(node);
                return true;
            }
            "decorated_definition" => return true,
            "lambda"
            | "list_comprehension"
            | "set_comprehension"
            | "dictionary_comprehension"
            | "generator_expression" => {}
            _ => return true,
        }
        false
    }

    fn bind_def_name(&mut self, node: Node) {
        if let Some(n) = node.child_by_field_name("name") {
            let nome = self.t(n);
            self.add(&nome);
        }
    }

    fn bind_assignment(&mut self, node: Node) {
        if let Some(l) = node.child_by_field_name("left") {
            self.bind_target(l);
            if l.kind() == "identifier"
                && text(self.source, l) == "__all__"
                && let Some(r) = node.child_by_field_name("right")
                && let Some(all) = static_string_list(self.source, r)
            {
                // `__all__` is a promise that these names exist — a name bound by a mechanism we
                // cannot read still counts as a member
                self.ns.names.extend(all);
            }
        }
    }

    /// `type X = …` / `type Y[T] = …` (3.12): the alias name is the first identifier on the left.
    fn bind_type_alias(&mut self, node: Node) {
        let Some(ty) = node
            .child_by_field_name("left")
            .and_then(|l| l.named_child(0))
        else {
            return;
        };
        let ident = if ty.kind() == "generic_type" {
            ty.named_child(0)
        } else {
            Some(ty)
        };
        if let Some(i) = ident.filter(|i| i.kind() == "identifier") {
            let nome = self.t(i);
            self.add(&nome);
        }
    }

    fn bind_walrus(&mut self, node: Node) {
        if let Some(n) = node.child_by_field_name("name") {
            let nome = self.t(n);
            self.add(&nome);
        }
    }

    /// Names a store target introduces (`x`, `(a, b)`, `*rest`, …); attribute/subscript targets
    /// bind nothing at module scope.
    fn bind_target(&mut self, n: Node) {
        match n.kind() {
            "identifier" => {
                let nome = self.t(n);
                if nome == "__getattr__" {
                    // a module `__getattr__` bound by assignment is not an analysable def
                    self.ns.has_getattr = true;
                }
                self.add(&nome);
            }
            "attribute" | "subscript" => {}
            _ => {
                let mut c = n.walk();
                for ch in n.named_children(&mut c) {
                    self.bind_target(ch);
                }
            }
        }
    }

    fn bind_import(&mut self, node: Node) {
        let mut c = node.walk();
        for ch in node.children_by_field_name("name", &mut c) {
            match ch.kind() {
                "dotted_name" => {
                    let dotted = self.t(ch);
                    if let Some(first) = dotted.split('.').next() {
                        self.add(first);
                    }
                }
                "aliased_import" => {
                    if let Some(alias) = last_identifier(ch) {
                        let a = self.t(alias);
                        self.add(&a);
                    }
                }
                _ => {}
            }
        }
    }

    fn bind_import_from(&mut self, node: Node) {
        let mut c = node.walk();
        for ch in node.children(&mut c) {
            if ch.kind() == "wildcard_import" {
                self.ns.has_star = true;
            }
        }
        let mut c = node.walk();
        for ch in node.children_by_field_name("name", &mut c) {
            let nome = match ch.kind() {
                "dotted_name" => self.t(ch).rsplit('.').next().unwrap_or("").to_string(),
                "aliased_import" => match last_identifier(ch) {
                    Some(alias) => self.t(alias),
                    None => continue,
                },
                _ => continue,
            };
            if nome == "__getattr__" {
                self.ns.has_getattr = true; // re-exported getattr: not analysable here
            }
            self.add(&nome);
        }
    }

    // ------------------------------------------------------------------ pass 2: signals

    /// A module-level `def __getattr__` (PEP 562): closed → its literal names, open → a flag.
    fn signal_function(&mut self, def: Node, depth: usize) {
        let is_getattr = def
            .child_by_field_name("name")
            .is_some_and(|n| text(self.source, n) == "__getattr__");
        if depth != 0 || !is_getattr {
            return;
        }
        match self.closed_getattr(def) {
            Some(nomes) => self.ns.names.extend(nomes),
            None => self.ns.has_getattr = true,
        }
    }

    /// `for k in ("a", "b"): …` pushes `k ↦ ["a", "b"]` while its body is walked, so
    /// `globals()[k] = …` inside is closed; any other iterable pushes `k ↦ None` (open). Returns
    /// how many loop variables were pushed.
    fn enter_for(&self, node: Node, loops: &mut Vec<LoopVar>) -> usize {
        let Some(l) = node.child_by_field_name("left") else {
            return 0;
        };
        let lits = if l.kind() == "identifier" {
            node.child_by_field_name("right")
                .and_then(|r| self.str_collection(r))
        } else {
            None
        };
        self.push_loop_vars(l, lits, loops)
    }

    /// `global A, B` in any function declares module attributes (bound when the function runs).
    fn bind_global(&mut self, node: Node) {
        let mut c = node.walk();
        for ch in node.named_children(&mut c) {
            let nome = self.t(ch);
            self.add(&nome);
        }
    }

    /// Push the loop variable(s) of a `for` target; returns how many were pushed.
    fn push_loop_vars(
        &self,
        target: Node,
        lits: Option<Vec<String>>,
        loops: &mut Vec<LoopVar>,
    ) -> usize {
        if target.kind() == "identifier" {
            loops.push((self.t(target), lits));
            return 1;
        }
        let mut n = 0;
        let mut c = target.walk();
        for ch in target.named_children(&mut c) {
            n += self.push_loop_vars(ch, None, loops);
        }
        n
    }

    /// `globals()[k] = v` / `module.x = v` / `sys.modules[__name__] = obj` as a store target.
    fn signal_store(&mut self, target: Node, depth: usize, loops: &[LoopVar]) {
        match target.kind() {
            "subscript" => self.store_subscript(target, depth, loops),
            "attribute" => self.store_attribute(target),
            "pattern_list" | "tuple_pattern" | "list_pattern" | "tuple" | "list" => {
                let mut c = target.walk();
                for ch in target.named_children(&mut c) {
                    self.signal_store(ch, depth, loops);
                }
            }
            _ => {}
        }
    }

    fn store_subscript(&mut self, target: Node, depth: usize, loops: &[LoopVar]) {
        let (Some(v), Some(k)) = (
            target.child_by_field_name("value"),
            target.child_by_field_name("subscript"),
        ) else {
            return;
        };
        if self.is_sys_modules(v) {
            // `sys.modules[__name__] = obj` REPLACES this module; a registration of another entry
            // (`sys.modules["pkg.alias"] = sub`) leaves this namespace alone
            if matches!(text(self.source, k), "__name__" | "__package__") {
                self.ns.has_dynamic = true;
            }
        } else if self.is_ns_dict(v, depth) {
            self.bind_key(k, loops);
        }
    }

    fn store_attribute(&mut self, target: Node) {
        let (Some(o), Some(a)) = (
            target.child_by_field_name("object"),
            target.child_by_field_name("attribute"),
        ) else {
            return;
        };
        if !self.is_module_expr(o) {
            return;
        }
        let nome = self.t(a);
        match nome.as_str() {
            "__class__" => self.ns.has_dynamic = true,
            "__getattr__" => self.ns.has_getattr = true,
            _ => self.add(&nome),
        }
    }

    /// Calls that write the namespace: `setattr(<module>, k, v)` · `globals().update(…)` ·
    /// module-level `exec` · `__all__.extend(…)` · and any call that HANDS the namespace (or the
    /// module object) to a callee that may bind names (`lazy_import(globals(), …)`).
    fn signal_call(&mut self, call: Node, depth: usize, loops: &[LoopVar]) {
        let (Some(f), Some(args)) = (
            call.child_by_field_name("function"),
            call.child_by_field_name("arguments"),
        ) else {
            return;
        };
        match f.kind() {
            "identifier" => self.call_by_name(text(self.source, f), args, depth, loops),
            "attribute" => self.call_by_method(f, args, depth, loops),
            _ => {}
        }
    }

    fn call_by_name(&mut self, nome: &str, args: Node, depth: usize, loops: &[LoopVar]) {
        match nome {
            "setattr" => {
                if let (Some(target), Some(key)) = (args.named_child(0), args.named_child(1))
                    && self.is_module_expr(target)
                {
                    self.bind_key(key, loops);
                }
            }
            // `exec(code)` at module scope writes the module namespace; deeper, only when the
            // namespace dict is handed over
            "exec" if depth == 0 || args_of(args).any(|a| self.mentions_ns_dict(a, depth)) => {
                self.ns.has_dynamic = true;
            }
            n if READ_ONLY_CALLEES.contains(&n) => {}
            _ => self.hands_namespace(args, depth),
        }
    }

    fn call_by_method(&mut self, f: Node, args: Node, depth: usize, loops: &[LoopVar]) {
        let (Some(obj), Some(m)) = (
            f.child_by_field_name("object"),
            f.child_by_field_name("attribute"),
        ) else {
            return;
        };
        let metodo = text(self.source, m);
        if self.is_ns_dict(obj, depth) {
            match metodo {
                "update" => self.bind_dict_args(args),
                "setdefault" => {
                    if let Some(k) = args.named_child(0) {
                        self.bind_key(k, loops);
                    }
                }
                _ => {}
            }
        } else if obj.kind() == "identifier"
            && text(self.source, obj) == "__all__"
            && matches!(metodo, "extend" | "append")
        {
            for a in args_of(args) {
                if let Some(l) = self.str_collection(a) {
                    self.ns.names.extend(l);
                } else if let Some(s) = self.str_lit(a) {
                    self.add(&s);
                }
            }
        } else {
            self.hands_namespace(args, depth);
        }
    }

    /// An argument that IS the namespace dict or the module object lets the callee bind names we
    /// cannot see (`websockets.lazy_import(globals(), aliases=…)`). Keyword values count.
    fn hands_namespace(&mut self, args: Node, depth: usize) {
        let hands = args_of(args).any(|a| {
            let v = if a.kind() == "keyword_argument" {
                a.child_by_field_name("value")
            } else {
                Some(a)
            };
            v.is_some_and(|v| self.is_ns_dict(v, depth) || self.is_module_expr(v))
        });
        if hands {
            self.ns.has_dynamic = true;
        }
    }

    fn mentions_ns_dict(&self, n: Node, depth: usize) -> bool {
        if self.is_ns_dict(n, depth) {
            return true;
        }
        let mut c = n.walk();
        n.named_children(&mut c)
            .any(|ch| self.mentions_ns_dict(ch, depth))
    }

    /// `update({"a": 1}, b=2)` is closed (the keys are literal); anything else is open.
    fn bind_dict_args(&mut self, args: Node) {
        for a in args_of(args) {
            match a.kind() {
                "dictionary" => {
                    let mut c = a.walk();
                    for pair in a.named_children(&mut c) {
                        let key = (pair.kind() == "pair")
                            .then(|| pair.child_by_field_name("key"))
                            .flatten()
                            .and_then(|k| self.str_lit(k));
                        match key {
                            Some(k) => self.add(&k),
                            None => self.ns.has_dynamic = true,
                        }
                    }
                }
                "keyword_argument" => {
                    if let Some(n) = a.child_by_field_name("name") {
                        let nome = self.t(n);
                        self.add(&nome);
                    }
                }
                _ => self.ns.has_dynamic = true,
            }
        }
    }

    /// The key of a namespace write: a string literal, or a loop variable over a literal list, is
    /// closed; anything else (`item.name`, a call, an f-string) is open.
    fn bind_key(&mut self, k: Node, loops: &[LoopVar]) {
        if let Some(s) = self.str_lit(k) {
            self.add(&s);
            return;
        }
        if k.kind() == "identifier" {
            let nome = text(self.source, k);
            if let Some((_, Some(lits))) = loops.iter().rev().find(|(n, _)| n == nome) {
                for l in lits.clone() {
                    self.add(&l);
                }
                return;
            }
        }
        self.ns.has_dynamic = true;
    }

    // ------------------------------------------------------------------ literals

    /// A plain string literal (no interpolation, no escape sequences — those are not provably
    /// the name they spell).
    fn str_lit(&self, n: Node) -> Option<String> {
        if n.kind() != "string" {
            return None;
        }
        let mut out = String::new();
        let mut c = n.walk();
        for ch in n.children(&mut c) {
            match ch.kind() {
                "string_start" | "string_end" => {}
                "string_content" => out.push_str(text(self.source, ch)),
                _ => return None,
            }
        }
        Some(out)
    }

    /// `["a", "b"]` / `("a", "b")` / `{"a", "b"}` of plain string literals.
    fn str_collection(&self, n: Node) -> Option<Vec<String>> {
        if !matches!(n.kind(), "list" | "tuple" | "set") {
            return None;
        }
        let mut out = Vec::new();
        let mut c = n.walk();
        for ch in n.named_children(&mut c) {
            out.push(self.str_lit(ch)?);
        }
        Some(out)
    }

    // ------------------------------------------------------------------ closed __getattr__

    /// The literal names a module `__getattr__` serves, or `None` when it is open. CLOSED means:
    /// the last statement is a `raise` (falling off the end would return `None`, a valid value),
    /// and every `return` sits in the consequence of an `attr == "lit"` / `attr in (…)` guard.
    fn closed_getattr(&self, def: Node) -> Option<HashSet<String>> {
        let param = self.first_param(def)?;
        let body = def.child_by_field_name("body")?;
        let mut c = body.walk();
        let last = body
            .named_children(&mut c)
            .filter(|s| s.kind() != "comment")
            .last()?;
        if last.kind() != "raise_statement" {
            return None;
        }
        let mut nomes = HashSet::new();
        self.guarded(body, &param, false, &mut nomes)
            .then_some(nomes)
    }

    fn first_param(&self, def: Node) -> Option<String> {
        let first = def.child_by_field_name("parameters")?.named_child(0)?;
        match first.kind() {
            "identifier" => Some(self.t(first)),
            "typed_parameter" => first.named_child(0).map(|n| self.t(n)),
            "default_parameter" | "typed_default_parameter" => {
                first.child_by_field_name("name").map(|n| self.t(n))
            }
            _ => None,
        }
    }

    /// Walk the statements; `false` as soon as a `return` is reachable for names no literal guard
    /// pins down. Nested `def`/`class`/`lambda` are not this function's returns.
    fn guarded(&self, node: Node, param: &str, guarded: bool, nomes: &mut HashSet<String>) -> bool {
        match node.kind() {
            "return_statement" => return guarded,
            "function_definition" | "class_definition" | "lambda" => return true,
            "if_statement" | "elif_clause" => {
                let lits = node
                    .child_by_field_name("condition")
                    .and_then(|c| self.literal_guard(c, param));
                let inner = match lits {
                    Some(l) => {
                        nomes.extend(l);
                        true
                    }
                    None => guarded,
                };
                if let Some(cons) = node.child_by_field_name("consequence")
                    && !self.guarded(cons, param, inner, nomes)
                {
                    return false;
                }
                // `elif`/`else` run for every name the `if` did not take: they inherit `guarded`
                let mut c = node.walk();
                for alt in node.children_by_field_name("alternative", &mut c) {
                    if !self.guarded(alt, param, guarded, nomes) {
                        return false;
                    }
                }
                return true;
            }
            _ => {}
        }
        let mut c = node.walk();
        for ch in node.named_children(&mut c) {
            if !self.guarded(ch, param, guarded, nomes) {
                return false;
            }
        }
        true
    }

    /// The literals a condition pins `param` to, when the condition holds only for those names:
    /// `param == "x"` · `"x" == param` · `param in ("a", "b")` · `and` (either side narrows) ·
    /// `or` (both sides must). `not in`, `!=`, calls on `param` and the rest are not guards.
    fn literal_guard(&self, cond: Node, param: &str) -> Option<Vec<String>> {
        match cond.kind() {
            "parenthesized_expression" => cond
                .named_child(0)
                .and_then(|c| self.literal_guard(c, param)),
            "comparison_operator" => self.guard_comparison(cond, param),
            "boolean_operator" => self.guard_boolean(cond, param),
            _ => None,
        }
    }

    fn guard_comparison(&self, cond: Node, param: &str) -> Option<Vec<String>> {
        let mut c = cond.walk();
        let operands: Vec<Node> = cond.named_children(&mut c).collect();
        if operands.len() != 2 {
            return None;
        }
        let mut c = cond.walk();
        let ops: Vec<&str> = cond
            .children(&mut c)
            .filter(|x| !x.is_named())
            .map(|x| text(self.source, x))
            .collect();
        let is_param = |n: Node| n.kind() == "identifier" && text(self.source, n) == param;
        match ops.join(" ").as_str() {
            "==" if is_param(operands[0]) => self.str_lit(operands[1]).map(|s| vec![s]),
            "==" if is_param(operands[1]) => self.str_lit(operands[0]).map(|s| vec![s]),
            "in" if is_param(operands[0]) => self.str_collection(operands[1]),
            _ => None,
        }
    }

    fn guard_boolean(&self, cond: Node, param: &str) -> Option<Vec<String>> {
        let side = |field: &str| {
            cond.child_by_field_name(field)
                .and_then(|n| self.literal_guard(n, param))
        };
        let (l, r) = (side("left"), side("right"));
        let op = cond
            .child_by_field_name("operator")
            .map(|o| text(self.source, o))
            .unwrap_or("");
        match (op, l, r) {
            ("and" | "or", Some(mut a), Some(b)) => {
                a.extend(b);
                Some(a)
            }
            ("and", Some(a), None) | ("and", None, Some(a)) => Some(a),
            _ => None,
        }
    }
}

/// Pass 0: every `name = <expr>` of the module (any depth) with the scope depth it sits at.
struct AliasCandidates<'s, 't> {
    source: &'s str,
    depth: usize,
    found: Vec<(String, Node<'t>, usize)>,
}

impl<'t> Visitor<'t> for AliasCandidates<'_, 't> {
    fn enter(&mut self, node: Node<'t>) -> bool {
        if is_scope(node) {
            self.depth += 1;
        } else if node.kind() == "assignment"
            && let (Some(l), Some(r)) = (
                node.child_by_field_name("left"),
                node.child_by_field_name("right"),
            )
            && l.kind() == "identifier"
        {
            self.found
                .push((text(self.source, l).to_string(), r, self.depth));
        }
        true
    }

    fn exit(&mut self, node: Node<'t>) {
        if is_scope(node) {
            self.depth -= 1;
        }
    }
}

/// Pass 1: the module-scope bindings.
struct BindWalk<'a, 's> {
    c: &'a mut Collector<'s>,
}

impl<'t> Visitor<'t> for BindWalk<'_, '_> {
    fn enter(&mut self, node: Node<'t>) -> bool {
        self.c.bind_node(node)
    }
}

/// Pass 2: the dynamic-binding signals, with the scope depth and the literal `for` variables in
/// force at each node.
struct SignalWalk<'a, 's> {
    c: &'a mut Collector<'s>,
    depth: usize,
    loops: Vec<LoopVar>,
    /// How many loop variables each enclosing `for` pushed (popped when its subtree ends).
    pushed: Vec<usize>,
}

impl<'t> Visitor<'t> for SignalWalk<'_, '_> {
    fn enter(&mut self, node: Node<'t>) -> bool {
        match node.kind() {
            "function_definition" => {
                self.c.signal_function(node, self.depth);
                self.depth += 1;
            }
            "class_definition" => self.depth += 1,
            "for_statement" => {
                let n = self.c.enter_for(node, &mut self.loops);
                self.pushed.push(n);
            }
            "global_statement" => {
                self.c.bind_global(node);
                return false;
            }
            "assignment" | "augmented_assignment" => {
                if let Some(l) = node.child_by_field_name("left") {
                    self.c.signal_store(l, self.depth, &self.loops);
                }
            }
            "call" => self.c.signal_call(node, self.depth, &self.loops),
            _ => {}
        }
        true
    }

    fn exit(&mut self, node: Node<'t>) {
        match node.kind() {
            "function_definition" | "class_definition" => self.depth -= 1,
            "for_statement" => {
                if let Some(n) = self.pushed.pop() {
                    let keep = self.loops.len() - n;
                    self.loops.truncate(keep);
                }
            }
            _ => {}
        }
    }
}

/// `def` and `class` open a new scope (module-level `exec`/`vars()` only apply at depth 0).
fn is_scope(node: Node) -> bool {
    matches!(node.kind(), "function_definition" | "class_definition")
}

#[cfg(test)]
mod tests;
