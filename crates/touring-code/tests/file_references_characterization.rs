//! Characterization of "what a file refers to" (C2 of the touring-code deepening).
//!
//! Each fixture is a source file in one language; `snapshot` answers, for it, every
//! question the wiring consumers ask: its imports, the ones that are USES, the paths a
//! re-export forwards, the inferred references, the module paths it names, the modules
//! it declares and the uses of its own symbols. The expected values were fixed from the
//! behaviour BEFORE the passes moved behind `FileReferences`, and are the proof that the
//! move changed nothing: only `snapshot`'s body changed, never a literal below it.

use touring_code::ast::graph::{FileReferences, ImportInfo};
use touring_code::ast::{Lang, extract_symbols};

#[derive(Debug, Default, PartialEq)]
struct Snapshot {
    imports: Vec<(String, Vec<String>)>,
    consumed: Vec<(String, Vec<String>)>,
    forwarded: Vec<String>,
    method_names: Vec<String>,
    type_refs: Vec<String>,
    qualified_calls: Vec<(String, String)>,
    python_uses: Vec<(String, String)>,
    module_paths: Vec<String>,
    declared: Vec<String>,
    declarations: Vec<(String, String)>,
    self_refs: Vec<String>,
}

fn sorted(mut names: Vec<String>) -> Vec<String> {
    names.sort();
    names
}

// ── The questions a wiring consumer asks, through the one interface ───────────────────
fn snapshot(source: &str, lang: Lang) -> Snapshot {
    let refs = FileReferences::new(source, lang);

    let tuples = |imports: Vec<ImportInfo>| -> Vec<(String, Vec<String>)> {
        let mut pairs: Vec<_> = imports
            .into_iter()
            .map(|import| (import.module_path, import.symbols))
            .collect();
        // The extractor's order is not part of what is characterized: Python's comes out of a
        // `HashMap`, and two runs of the same source disagree.
        pairs.sort_by(|a, b| a.0.cmp(&b.0));
        pairs
    };
    let inferred = refs.inferred();
    let modules = refs.module_references();
    let mut forwarded: Vec<String> = refs.forwarded_paths().into_iter().collect();
    forwarded.sort();
    let mut qualified_calls = inferred.qualified_calls;
    qualified_calls.sort();
    let symbols = extract_symbols(source, lang).unwrap_or_default();

    Snapshot {
        imports: tuples(refs.imports()),
        consumed: tuples(refs.consumed_imports()),
        forwarded,
        method_names: sorted(inferred.method_names),
        type_refs: sorted(inferred.type_refs),
        qualified_calls,
        python_uses: refs.python_qualified_uses(),
        module_paths: modules.paths,
        declared: modules.declared.into_iter().collect(),
        declarations: refs
            .module_declarations()
            .into_iter()
            .map(|(name, declaration)| (name, format!("{declaration:?}")))
            .collect(),
        self_refs: refs.self_references(&symbols).into_iter().collect(),
    }
}
// ── END of the part that changes ──────────────────────────────────────────────────────

fn pairs(items: &[(&str, &[&str])]) -> Vec<(String, Vec<String>)> {
    items
        .iter()
        .map(|(module, symbols)| {
            (
                (*module).to_string(),
                symbols.iter().map(|s| (*s).to_string()).collect(),
            )
        })
        .collect()
}

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| (*s).to_string()).collect()
}

fn calls(items: &[(&str, &str)]) -> Vec<(String, String)> {
    items
        .iter()
        .map(|(q, n)| ((*q).to_string(), (*n).to_string()))
        .collect()
}

/// A Rust file with every shape: re-exports (forwarded, used, deep), a plain use, a type alias, the three `mod` forms, calls, a macro path.
#[test]
fn rust_rich() {
    assert_eq!(
        snapshot(RUST_RICH, Lang::Rust),
        Snapshot {
            imports: pairs(&[
                ("alpha", &["Forwarded"]),
                ("beta", &["Shown", "Hidden"]),
                ("crate::fwd::inner", &["Gone"]),
                ("crate::kept::deep", &["Back"]),
                ("crate::shapes", &["Circle"]),
                ("delta", &["Table"]),
                ("gamma", &["Plain"]),
                ("std::collections", &["HashMap"])
            ]),
            consumed: pairs(&[
                ("alpha", &[]),
                ("beta", &["Shown"]),
                ("crate::fwd::inner", &[]),
                ("crate::kept::deep", &["Back"]),
                ("crate::shapes", &["Circle"]),
                ("delta", &[]),
                ("gamma", &["Plain"]),
                ("std::collections", &["HashMap"])
            ]),
            forwarded: strings(&[
                "alpha::Forwarded",
                "beta::Hidden",
                "beta::Shown",
                "crate::fwd::inner::Gone",
                "crate::kept::deep::Back",
                "delta::Table"
            ]),
            method_names: strings(&[
                "assoc",
                "from_child",
                "go",
                "method_call",
                "run",
                "used_internally"
            ]),
            type_refs: strings(&[
                "Back",
                "Circle",
                "Declared",
                "LIMIT",
                "assoc",
                "backup",
                "from_child",
                "go",
                "helper",
                "run",
                "shapes",
                "util"
            ]),
            module_paths: strings(&[
                "child",
                "crate::kept",
                "crate::kept::deep",
                "crate::shapes",
                "crate::util",
                "crate::util::helper",
                "std::collections",
                "super::backup"
            ]),
            declared: strings(&["child", "inline", "moved"]),
            self_refs: strings(&["Declared", "used_internally"]),
            qualified_calls: calls(&[
                ("Declared", "assoc"),
                ("backup", "run"),
                ("child", "from_child"),
                ("helper", "go")
            ]),
            declarations: calls(&[
                ("child", "Elsewhere"),
                ("inline", "Inline"),
                ("moved", "AtPath(\"elsewhere/x.rs\")")
            ]),
            ..Default::default()
        }
    );
}

/// `handlers/mod.rs` style: one re-export the file also uses in its table (a use), one it only forwards.
#[test]
fn rust_handlers_table() {
    assert_eq!(
        snapshot(RUST_TABLE, Lang::Rust),
        Snapshot {
            imports: pairs(&[
                ("auto_import", &["AUTO_IMPORT"]),
                ("auto_wire", &["AUTO_WIRE"])
            ]),
            consumed: pairs(&[("auto_import", &["AUTO_IMPORT"]), ("auto_wire", &[])]),
            forwarded: strings(&["auto_import::AUTO_IMPORT", "auto_wire::AUTO_WIRE"]),
            type_refs: strings(&["Handler"]),
            ..Default::default()
        }
    );
}

/// A Python file with a façade `__all__`, an aliased import, `import m as x` + `x.Nome`, and a method call.
#[test]
fn python_rich() {
    assert_eq!(
        snapshot(PYTHON_RICH, Lang::Python),
        Snapshot {
            imports: pairs(&[
                ("modulo", &[]),
                ("os", &[]),
                ("pkg.gravacao", &["supersede", "grava"]),
                ("pkg.modelo", &["No"])
            ]),
            consumed: pairs(&[
                ("modulo", &[]),
                ("os", &[]),
                ("pkg.gravacao", &["grava"]),
                ("pkg.modelo", &["No"])
            ]),
            method_names: strings(&["Nome", "save"]),
            python_uses: calls(&[("modulo", "Nome")]),
            ..Default::default()
        }
    );
}

/// Python with no `__all__`: a module constant read by its own function, and a method reached through `self`.
#[test]
fn python_self() {
    assert_eq!(
        snapshot(PYTHON_SELF, Lang::Python),
        Snapshot {
            method_names: strings(&["load"]),
            self_refs: strings(&["PUB", "load"]),
            ..Default::default()
        }
    );
}

/// TypeScript keeps every import: the language has no forwarding rule.
#[test]
fn typescript() {
    assert_eq!(
        snapshot(TYPESCRIPT, Lang::TypeScript),
        Snapshot {
            imports: pairs(&[("./x", &["a", "b"]), ("y", &[])]),
            consumed: pairs(&[("./x", &["a", "b"]), ("y", &[])]),
            ..Default::default()
        }
    );
}

/// JavaScript, the same.
#[test]
fn javascript() {
    assert_eq!(
        snapshot(JAVASCRIPT, Lang::JavaScript),
        Snapshot {
            imports: pairs(&[("x", &["x"])]),
            consumed: pairs(&[("x", &["x"])]),
            ..Default::default()
        }
    );
}

/// Bash has no reference rule at all.
#[test]
fn bash() {
    assert_eq!(
        snapshot(BASH, Lang::Bash),
        Snapshot {
            ..Default::default()
        }
    );
}

/// Prose that quotes a `use` is not code: a document makes no reference.
#[test]
fn markdown_prose() {
    assert_eq!(
        snapshot(MARKDOWN, Lang::Markdown),
        Snapshot {
            ..Default::default()
        }
    );
}

/// An empty Rust source says nothing on any pass.
#[test]
fn rust_empty() {
    assert_eq!(
        snapshot("", Lang::Rust),
        Snapshot {
            ..Default::default()
        }
    );
}

/// An empty Python source says nothing on any pass.
#[test]
fn python_empty() {
    assert_eq!(
        snapshot("", Lang::Python),
        Snapshot {
            ..Default::default()
        }
    );
}

const RUST_RICH: &str = "\
pub use alpha::Forwarded;
pub use beta::{Shown, Hidden};
pub(crate) use delta::Table;
pub use crate::fwd::inner::Gone;
pub use crate::kept::deep::Back;
use gamma::Plain;
use std::collections::HashMap;

mod child;
#[path = \"elsewhere/x.rs\"]
mod moved;
pub mod inline { pub fn f() {} }

type Alias = crate::shapes::Circle;

pub struct Declared;
struct Unused;

pub const SHOWN_TABLE: &[(&str, Shown)] = &[(\"s\", Shown)];

pub fn used_internally() {}
pub fn never_used() {}

pub fn caller(p: Plain, m: HashMap<u8, u8>, b: Back) -> Declared {
    used_internally();
    let v = p.method_call();
    Declared::assoc();
    crate::util::helper::go();
    super::backup::run();
    child::from_child();
    assert_eq!(v, tags::LIMIT);
    Declared
}
";

const RUST_TABLE: &str = "\
pub use auto_import::AUTO_IMPORT;
pub use auto_wire::AUTO_WIRE;

pub const ALL: &[(&str, Handler)] = &[(\"auto_import\", AUTO_IMPORT)];
";

const PYTHON_RICH: &str = "\
import modulo as gm
import os
from pkg.gravacao import supersede, grava
from pkg.modelo import No as N

__all__ = [\"supersede\", \"grava\"]

LIMIT = 10

class Repo:
    def save(self):
        return gm.Nome(LIMIT)

    def load(self):
        return self.save()

def f():
    return grava(N())
";

const PYTHON_SELF: &str = "\
PUB = 1
MORTO = 2
_PRIV = 3

def helper():
    return PUB + _PRIV

def unused_helper():
    return 0

class Repo:
    def save(self):
        return self.load()

    def load(self):
        return 1
";

const TYPESCRIPT: &str = "\
import { a, b as c } from './x';
import * as ns from \"y\";
export { z } from './z';

class K {
    m() {
        return a.b() + c.d();
    }
}
";

const JAVASCRIPT: &str = "\
import x from 'x';
const fs = require('fs');
export function g() { return x.run(fs); }
";

const BASH: &str = "\
source ./lib.sh
. ./other.sh
foo() { bar; }
";

const MARKDOWN: &str = "\
A travessia credita o modulo: `use crate::a::b::Z;`

```rust
use crate::a::b::Z;
```
";
