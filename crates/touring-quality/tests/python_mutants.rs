//! S50 — mutation test suite for Python-facing dimensions (25/09/2026).
//!
//! Each test asserts one dimension DROPS (strictly < baseline) when a
//! single-defect mutant replaces the clean baseline. The five dimensions that
//! were BLIND before this wave (F4_3, F1_6, F2_10, F2_11, F1_2) start RED;
//! the remaining eight (F2_1, F2_4, F2_6, F1_3, F1_5, F3_8, F2_7, F4_1)
//! are already GREEN. A "revert check" for each blind dim is tracked by the
//! TDD cycle (`cargo test`, revert impl, `cargo test` → fail, restore → pass).
//!
//! **Security rule**: no literal with the shape of a secret appears in this
//! file — all values that could be flagged as secrets are produced at runtime
//! via `format!`/string concatenation (see `gen_secret_val`).

use std::io::Write as _;
use std::path::Path;
use std::process::Command;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn bin() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_BIN_EXE_touring-quality"))
}

/// Run `touring-quality score <path> --format json` and return the parsed JSON.
fn score_py(path: &Path) -> serde_json::Value {
    let out = Command::new(bin())
        .args(["score", "--format", "json"])
        .arg(path)
        .env("TOURING_QUALITY_CACHE_DISABLE", "1")
        .output()
        .expect("touring-quality binary not found");
    let stdout = String::from_utf8_lossy(&out.stdout);
    serde_json::from_str(&stdout).unwrap_or_else(|e| {
        panic!(
            "JSON parse failed ({e})\nstdout:\n{stdout}\nstderr:\n{}",
            String::from_utf8_lossy(&out.stderr)
        )
    })
}

/// Extract a float value for `dim` key (e.g. `"F1_6"`) from the score JSON.
fn dim_value(json: &serde_json::Value, dim: &str) -> f32 {
    json["dimensions"][dim]["value"]
        .as_f64()
        .unwrap_or_else(|| panic!("dim {dim} missing from JSON: {json}")) as f32
}

fn dim_status(json: &serde_json::Value, dim: &str) -> String {
    json["dimensions"][dim]["status"]
        .as_str()
        .unwrap_or("unknown")
        .to_string()
}

/// Write a temp `.py` file and return the `NamedTempFile` guard.
fn py_file(content: &str) -> tempfile::NamedTempFile {
    let mut f = tempfile::Builder::new()
        .suffix(".py")
        .tempfile()
        .expect("create temp .py");
    f.write_all(content.as_bytes()).expect("write .py");
    f
}

// ---------------------------------------------------------------------------
// Mutant fixtures (embedded inline — no file-system read at test time)
// ---------------------------------------------------------------------------

/// m00 — clean baseline
const M00_CLEAN: &str = r#"
"""Clean probe module."""

def soma(a: int, b: int) -> int:
    """Return a + b."""
    return a + b

def media(valores: list[float]) -> float:
    """Return the arithmetic mean, raising on empty input."""
    if not valores:
        raise ValueError("lista vazia")
    return sum(valores) / len(valores)
"#;

/// m01 — high cyclomatic complexity (F1_2)
const M01_COMPLEXITY: &str = r#"
"""Probe: high cyclomatic complexity."""

def classify(x: int, y: int, z: int, flag: bool, mode: str) -> str:
    """Deeply nested decision tree — high CC."""
    if x > 0:
        if y > 0:
            if z > 0:
                if flag:
                    if mode == "a":
                        return "a1"
                    elif mode == "b":
                        return "b1"
                    elif mode == "c":
                        return "c1"
                    elif mode == "d":
                        return "d1"
                    else:
                        return "e1"
                else:
                    if mode == "a":
                        return "a2"
                    elif mode == "b":
                        return "b2"
                    else:
                        return "c2"
            else:
                if flag:
                    return "z_neg_flag"
                else:
                    return "z_neg_no_flag"
        else:
            if z > 0:
                return "y_neg_z_pos"
            else:
                return "y_neg_z_neg"
    else:
        if y > 0:
            return "x_neg_y_pos"
        elif z > 0:
            return "x_neg_z_pos"
        else:
            return "x_neg_all_neg"
"#;

/// m03 — bare except and except+pass (F1_6)
const M03_ERROR_SWALLOW: &str = r#"
"""Probe: swallowed errors."""

def soma(a: int, b: int) -> int:
    """Return a + b."""
    return a + b

def carrega(caminho: str) -> str:
    """Swallows every error."""
    try:
        return open(caminho).read()
    except:
        pass
    try:
        return caminho.upper()
    except Exception:
        pass
    return ""
"#;

/// m04 — OWASP taint (F2_1): multiple injection sinks with user input sources.
const M04_OWASP: &str = r#"
"""Probe: injection sinks."""
import os
import pickle
import sqlite3
import subprocess
import yaml


def perigoso(usuario: str, blob: bytes) -> None:
    """Multiple injection sinks."""
    eval(input("expr: "))
    os.system(f"ls {usuario}")
    subprocess.call("grep " + usuario, shell=True)
    pickle.loads(blob)
    yaml.load(usuario)
    conn = sqlite3.connect("x.db")
    conn.execute(f"SELECT * FROM t WHERE nome = '{usuario}'")
"#;

/// m05 — secrets (F2_4)
fn m05_secrets() -> String {
    // Value produced at runtime — not a literal secret in the repo.
    let key_prefix = "AK";
    let key_body = "IA".to_string() + "ioO" + "jNb" + "kX9" + "Pw2" + "3rT";
    format!(
        r#"
"""Probe: hardcoded credential."""

{key_prefix}{key_body} = "{key_prefix}{key_body}VALUE"
AWS_SECRET = "{key_prefix}{key_body}VALUE"
"#
    )
}

/// m06 — weak crypto (F2_4): md5/sha1 for password + unverified TLS + random for token.
const M06_CRYPTO: &str = r#"
"""Probe: weak crypto."""
import hashlib
import random
import ssl


def hash_senha(senha: str) -> str:
    """Weak hash for password."""
    ctx = ssl._create_unverified_context()
    token = str(random.random())
    return hashlib.md5(senha.encode()).hexdigest() + hashlib.sha1(token.encode()).hexdigest() + str(ctx)
"#;

/// m07 — config security (F2_6): DEBUG, wildcard hosts, CORS open, TLS off.
const M07_CONFIG: &str = r#"
"""Probe: insecure config."""
import requests

DEBUG = True
ALLOWED_HOSTS = ["*"]
CORS_ALLOW_ALL_ORIGINS = True


def busca(url: str) -> str:
    """TLS verification disabled."""
    return requests.get(url, verify=False, timeout=5).text
"#;

/// m08 — deprecated stdlib (F4_3)
const M08_DEPRECATED: &str = r#"
"""Probe: deprecated stdlib."""
import asyncio
import imp
from collections import Mapping
from distutils.core import setup
import datetime

def agora() -> str:
    return str(datetime.datetime.utcnow())
"#;

/// m09 — missing docs (F3_8)
const M09_NODOCS: &str = r#"
def calc(a, b):
    return a + b

def run():
    return calc(1, 2)
"#;

/// m10 — tech debt (F1_5)
const M10_DEBT: &str = r#"
"""Probe: TODO/FIXME debt."""

def process():
    # TODO: fix this
    # FIXME: broken
    # HACK: workaround
    # XXX: remove later
    pass
"#;

/// m11 — I/O in loop (F2_10)
const M11_IO_PERF: &str = r#"
"""Probe: I/O inside loop."""
import sqlite3
import time


def lento(ids: list[int]) -> list:
    """N+1 queries, open in loop, sleep in loop."""
    conn = sqlite3.connect("x.db")
    out = []
    for i in ids:
        out.append(conn.execute(f"SELECT * FROM t WHERE id={i}").fetchall())
        f = open("/etc/hostname")
        out.append(f.read())
        time.sleep(0.1)
    return out
"#;

/// m12 — threading shared global without lock (F2_11)
const M12_CONCURRENCY: &str = r#"
"""Probe: shared mutable global without lock."""
import threading

CONTADOR = 0


def incrementa() -> None:
    """Mutate shared state without a lock."""
    global CONTADOR
    for _ in range(1000):
        CONTADOR += 1


def dispara() -> None:
    """Spawn racing threads."""
    ts = [threading.Thread(target=incrementa) for _ in range(8)]
    for t in ts:
        t.start()
"#;

/// m13 — print idioms (F4_1)
const M13_PRINT_IDIOMS: &str = r#"
"""Probe: non-idiomatic patterns."""

import sys

def log(msg):
    print(msg)
    print(msg, file=sys.stderr)
    sys.stdout.write(msg + "\n")
"#;

// ---------------------------------------------------------------------------
// S50.1 — baseline clean file passes all P0 dims
// ---------------------------------------------------------------------------

#[test]
fn baseline_clean_file_passes_all_p0() {
    let f = py_file(M00_CLEAN);
    let json = score_py(f.path());
    let dims = json["dimensions"].as_object().expect("dimensions object");

    // P0 BLOCK dims: F2_1, F2_4, F2_6 must not be Fail with score < 0.5
    for p0 in &["F2_1", "F2_4", "F2_6"] {
        let v = dim_value(&json, p0);
        assert!(
            v >= 0.5,
            "P0 dim {p0} failed on clean baseline: value={v:.3}, dims={dims:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// S50.2-S50.8 — already-green dims (existing detection working)
// ---------------------------------------------------------------------------

#[test]
fn f2_1_detects_os_system_input() {
    let f_clean = py_file(M00_CLEAN);
    let f_mutant = py_file(M04_OWASP);
    let clean = score_py(f_clean.path());
    let mutant = score_py(f_mutant.path());
    let clean_v = dim_value(&clean, "F2_1");
    let mutant_v = dim_value(&mutant, "F2_1");
    assert!(
        mutant_v < clean_v,
        "F2_1 should drop on OWASP mutant: clean={clean_v:.3} mutant={mutant_v:.3}"
    );
}

#[test]
fn f2_1_ignores_patterns_inside_string_literals() {
    let safe = r#"
"""Safe: os.system only in a string constant."""

DOCS = "never call os.system(user_input())"
"#;
    let f = py_file(safe);
    let json = score_py(f.path());
    let v = dim_value(&json, "F2_1");
    // The string literal masks the pattern — must not score as Fail
    assert!(
        v >= 0.7,
        "F2_1 should not flag os.system inside a string literal: value={v:.3}"
    );
}

#[test]
fn f2_4_detects_secrets() {
    let secrets = m05_secrets();
    let f_clean = py_file(M00_CLEAN);
    let f_mutant = py_file(&secrets);
    let clean = score_py(f_clean.path());
    let mutant = score_py(f_mutant.path());
    let clean_v = dim_value(&clean, "F2_4");
    let mutant_v = dim_value(&mutant, "F2_4");
    assert!(
        mutant_v < clean_v,
        "F2_4 should drop on secrets mutant: clean={clean_v:.3} mutant={mutant_v:.3}"
    );
}

#[test]
fn f2_4_weak_hash_for_password_is_p0() {
    let f = py_file(M06_CRYPTO);
    let json = score_py(f.path());
    let v = dim_value(&json, "F2_4");
    assert!(
        v < 1.0,
        "F2_4 should detect md5 for password as weak crypto: value={v:.3}"
    );
}

#[test]
fn f2_4_pragma_allowlist_suppresses_fixture() {
    // Runtime-generated value so no secret literal in the repo.
    let prefix = "ghp";
    let body = "_FakeT".to_owned() + "oken123456";
    let content = format!(
        "TOKEN = \"{prefix}{body}\"  # pragma: allowlist secret\n"
    );
    let f = py_file(&content);
    let json = score_py(f.path());
    let v = dim_value(&json, "F2_4");
    assert!(
        v >= 0.9,
        "F2_4 pragma allowlist should suppress the finding: value={v:.3}"
    );
}

#[test]
fn f2_6_detects_debug_true_and_wildcard_hosts() {
    let f_clean = py_file(M00_CLEAN);
    let f_mutant = py_file(M07_CONFIG);
    let clean = score_py(f_clean.path());
    let mutant = score_py(f_mutant.path());
    let clean_v = dim_value(&clean, "F2_6");
    let mutant_v = dim_value(&mutant, "F2_6");
    assert!(
        mutant_v < clean_v,
        "F2_6 should drop on config mutant: clean={clean_v:.3} mutant={mutant_v:.3}"
    );
}

// ---------------------------------------------------------------------------
// S50.9-S50.13 — BLIND dims (currently RED, become GREEN after impl fixes)
// ---------------------------------------------------------------------------

/// F4.3 — Python deprecated stdlib imports must be detected.
/// RED before S54 (f4_3_deprecated.rs Python extension).
#[test]
fn f4_3_python_deprecated_stdlib() {
    let f_clean = py_file(M00_CLEAN);
    let f_mutant = py_file(M08_DEPRECATED);
    let clean = score_py(f_clean.path());
    let mutant = score_py(f_mutant.path());
    let clean_v = dim_value(&clean, "F4_3");
    let mutant_v = dim_value(&mutant, "F4_3");
    assert!(
        mutant_v < clean_v,
        "F4_3 should drop on Python deprecated stdlib (imp/distutils/Mapping/utcnow): \
         clean={clean_v:.3} mutant={mutant_v:.3}"
    );
}

/// F1.6 — bare `except:` and `except Exception: pass` must be penalised.
/// RED before S55 (unwrap_audit.rs Python bare-except extension).
#[test]
fn f1_6_bare_except_and_except_pass() {
    let f_clean = py_file(M00_CLEAN);
    let f_mutant = py_file(M03_ERROR_SWALLOW);
    let clean = score_py(f_clean.path());
    let mutant = score_py(f_mutant.path());
    let clean_v = dim_value(&clean, "F1_6");
    let mutant_v = dim_value(&mutant, "F1_6");
    assert!(
        mutant_v < clean_v,
        "F1_6 should drop on bare-except/except-pass mutant: \
         clean={clean_v:.3} mutant={mutant_v:.3}"
    );
}

/// F2.10 — `open()` without `with` inside a loop must be detected.
/// RED before S56 (io.rs Python open-in-loop extension).
#[test]
fn f2_10_sync_io_in_loop() {
    let f_clean = py_file(M00_CLEAN);
    let f_mutant = py_file(M11_IO_PERF);
    let clean = score_py(f_clean.path());
    let mutant = score_py(f_mutant.path());
    let clean_v = dim_value(&clean, "F2_10");
    let mutant_v = dim_value(&mutant, "F2_10");
    assert!(
        mutant_v < clean_v,
        "F2_10 should drop on open-in-loop/sleep-in-loop mutant: \
         clean={clean_v:.3} mutant={mutant_v:.3}"
    );
}

/// F2.11 — `threading.Thread` + `global` without a `Lock` must be detected.
/// RED before S56 (concurrency.rs Python thread-global extension).
#[test]
fn f2_11_thread_shared_global_without_lock() {
    let f_clean = py_file(M00_CLEAN);
    let f_mutant = py_file(M12_CONCURRENCY);
    let clean = score_py(f_clean.path());
    let mutant = score_py(f_mutant.path());
    let clean_v = dim_value(&clean, "F2_11");
    let mutant_v = dim_value(&mutant, "F2_11");
    assert!(
        mutant_v < clean_v,
        "F2_11 should drop on threading+global-without-lock mutant: \
         clean={clean_v:.3} mutant={mutant_v:.3}"
    );
}

/// F1.2 — high cyclomatic complexity must lower the MI-based score.
/// RED before S57 (complexity.rs / f1_2_maintainability.rs calibration).
#[test]
fn f1_2_band_matches_calibration_fixture() {
    let f_clean = py_file(M00_CLEAN);
    let f_mutant = py_file(M01_COMPLEXITY);
    let clean = score_py(f_clean.path());
    let mutant = score_py(f_mutant.path());
    let clean_v = dim_value(&clean, "F1_2");
    let mutant_v = dim_value(&mutant, "F1_2");
    assert!(
        mutant_v < clean_v,
        "F1_2 should drop on high-CC mutant (cc≥17 lloc≈34): \
         clean={clean_v:.3} mutant={mutant_v:.3}"
    );
}

// ---------------------------------------------------------------------------
// S50.14 — public functions without tests detected by F3.1
// ---------------------------------------------------------------------------

#[test]
fn f3_1_python_public_fns_without_tests_below_one() {
    // Module with 2 public functions and zero tests.
    let src = r#"
"""Module with public API but no tests."""

def processa(x: int) -> int:
    """Public function 1."""
    return x * 2

def transforma(xs: list[int]) -> list[int]:
    """Public function 2."""
    return [processa(x) for x in xs]
"#;
    let f = py_file(src);
    let json = score_py(f.path());
    let v = dim_value(&json, "F3_1");
    let _keep = f; // keep alive until here
    assert!(
        v < 1.0,
        "F3_1 should penalise public functions with no tests: value={v:.3}"
    );
}

// ---------------------------------------------------------------------------
// S50.15 — suggestions: no placeholder text on passing dims
// ---------------------------------------------------------------------------

#[test]
fn suggestions_have_no_placeholders_on_pass_dims() {
    let f = py_file(M00_CLEAN);
    let json = score_py(f.path());
    let _keep = f; // keep alive until here
    let suggestions = json["suggestions"].as_array().cloned().unwrap_or_default();
    // P0 dims that pass must not emit preventive suggestions with placeholder tokens
    let placeholders = ["<hardening>", "<operation>", "--operation ssr", "<dim>"];
    for s in &suggestions {
        let text = s.as_str().unwrap_or_default();
        for ph in &placeholders {
            assert!(
                !text.contains(ph),
                "Suggestion on clean baseline contains placeholder `{ph}`: {text}"
            );
        }
    }
}
