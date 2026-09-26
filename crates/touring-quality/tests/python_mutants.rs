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

/// m13 — non-idiomatic Python (F4_1). F4_1 measures the ruff idiom subset (E711 `== None`,
/// E712 `== True`, E721 `type(a) == type(b)`); the first fixture held only `print` calls,
/// which no idiom rule flags, so the mutant never carried the defect the dim measures
/// (found 25/09/2026 when the dead constant got its test: clean 1.000 = mutant 1.000).
const M13_PRINT_IDIOMS: &str = r#"
"""Probe: non-idiomatic patterns."""


def check(a, b, flag):
    if a == None:
        return False
    if flag == True:
        return type(a) == type(b)
    return b != None
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

// ---------------------------------------------------------------------------
// S51 — every sink class ALONE drops F2_1; safe forms stay clean
// ---------------------------------------------------------------------------

/// Shared prologue: an input SOURCE (`input()`) the sink bodies below consume.
const SINK_HEADER: &str = "\"\"\"Probe.\"\"\"\nimport ast\nimport os\nimport pickle\nimport subprocess\nimport sys\n\nimport yaml\n\n\ndef handler(conn, blob):\n    \"\"\"Handler.\"\"\"\n    usuario = input(\"nome: \")\n";

/// One sink per case. `m04` holds six sinks, so it passed with ANY one detected — measured
/// 25/09/2026: eval/exec and SQL built in the `execute` call were each scored 1.000 alone.
const SINK_CASES: &[(&str, &str)] = &[
    ("os_system_fstring", "    os.system(f\"ls {usuario}\")\n"),
    ("os_system_argv", "    os.system(\"ls \" + sys.argv[1])\n"),
    (
        "subprocess_shell",
        "    subprocess.call(\"grep \" + usuario, shell=True)\n",
    ),
    ("eval_input", "    eval(input(\"expr: \"))\n"),
    ("exec_variable", "    exec(usuario)\n"),
    ("eval_fstring", "    eval(f\"{usuario} + 1\")\n"),
    ("pickle_loads", "    pickle.loads(blob)\n"),
    ("yaml_load", "    yaml.load(usuario)\n"),
    (
        "sql_fstring",
        "    conn.execute(f\"SELECT * FROM t WHERE nome = '{usuario}'\")\n",
    ),
    (
        "sql_percent",
        "    conn.execute(\"SELECT * FROM t WHERE nome = '%s'\" % usuario)\n",
    ),
    (
        "sql_concat",
        "    conn.execute(\"DELETE FROM t WHERE nome = '\" + usuario + \"'\")\n",
    ),
    (
        "sql_format",
        "    conn.execute(\"UPDATE t SET a = '{}'\".format(usuario))\n",
    ),
    (
        "sql_fstring_triple",
        "    conn.executemany(f\"\"\"\n        INSERT INTO t VALUES ('{usuario}')\n    \"\"\", [])\n",
    ),
];

/// Forms that carry no injection — each must score as the clean prologue does.
const SAFE_CASES: &[(&str, &str)] = &[
    (
        "subprocess_list",
        "    subprocess.run([\"ls\", usuario], check=True)\n",
    ),
    ("yaml_safe_load", "    yaml.safe_load(usuario)\n"),
    (
        "sql_parameterized",
        "    conn.execute(\"SELECT * FROM t WHERE nome = ?\", (usuario,))\n",
    ),
    ("sql_constant_fstring", "    conn.execute(f\"SELECT 1\")\n"),
    (
        "execute_non_sql_fstring",
        "    conn.execute(f\"job-{usuario}\")\n",
    ),
    ("method_named_eval", "    conn.eval()\n"),
    ("literal_eval", "    ast.literal_eval(usuario)\n"),
    ("eval_constant", "    eval(\"1 + 1\")\n"),
    (
        "sink_in_comment",
        "    # eval(usuario) and conn.execute(f\"SELECT {usuario}\") would be unsafe\n    return usuario\n",
    ),
    (
        "sink_in_string",
        "    doc = \"exec(usuario); conn.execute(f'SELECT {usuario}')\"\n    return doc\n",
    ),
];

// ---------------------------------------------------------------------------
// S53 — every misconfiguration ALONE drops F2_6; generic lookalikes stay clean
// ---------------------------------------------------------------------------

/// Django settings markers: `DEBUG = True` is a finding only in such a file.
const DJANGO_SETTINGS: &str = "\"\"\"Settings.\"\"\"\n\nINSTALLED_APPS = [\"django.contrib.admin\"]\nROOT_URLCONF = \"site.urls\"\n";

/// Measured 25/09/2026, one pattern per file: the four settings forms scored 1.000.
const MISCONFIG_CASES: &[(&str, &str)] = &[
    ("django_debug_true", "DEBUG = True\n"),
    ("django_allowed_hosts_star", "ALLOWED_HOSTS = [\"*\"]\n"),
    ("cors_allow_all_origins", "CORS_ALLOW_ALL_ORIGINS = True\n"),
    ("cors_origin_allow_all", "CORS_ORIGIN_ALLOW_ALL = True\n"),
    ("flask_cors_origins_star", "CORS_ORIGINS = [\"*\"]\n"),
    ("flask_debug_run", "app.run(debug=True, host=\"0.0.0.0\")\n"),
    ("cors_middleware_star", "app.add_middleware(CORSMiddleware, allow_origins=[\"*\"])\n"),
];

/// Lookalikes that are not a production misconfiguration.
const CONFIG_SAFE_CASES: &[(&str, &str, bool)] = &[
    // (name, body, inside Django settings?)
    ("debug_true_in_a_plain_script", "DEBUG = True\n", false),
    ("django_debug_from_env", "import os\n\nDEBUG = os.environ.get(\"DEBUG\") == \"1\"\n", true),
    ("django_specific_hosts", "ALLOWED_HOSTS = [\"api.example.org\"]\n", true),
    ("debug_true_quoted", "DOC = \"never ship DEBUG = True\"\n", true),
];

fn f2_6_of(body: &str, in_settings: bool) -> f32 {
    let prefix = if in_settings { DJANGO_SETTINGS } else { "\"\"\"Script.\"\"\"\n\n" };
    let f = py_file(&format!("{prefix}{body}"));
    dim_value(&score_py(f.path()), "F2_6")
}

#[test]
fn f2_6_detects_each_misconfiguration_alone() {
    let clean = f2_6_of("DEBUG = False\n", true);
    let blind: Vec<&str> = MISCONFIG_CASES
        .iter()
        .filter(|(_, body)| f2_6_of(body, true) >= clean)
        .map(|(name, _)| *name)
        .collect();
    assert!(blind.is_empty(), "F2_6 blind to {blind:?} (clean={clean:.3})");
}

#[test]
fn f2_6_lookalikes_stay_clean() {
    let clean = f2_6_of("DEBUG = False\n", true);
    assert!(clean >= 0.99, "control: the clean settings file scores {clean:.3}");
    let flagged: Vec<(&str, f32)> = CONFIG_SAFE_CASES
        .iter()
        .map(|(name, body, in_settings)| (*name, f2_6_of(body, *in_settings)))
        .filter(|(_, v)| *v < clean)
        .collect();
    assert!(flagged.is_empty(), "F2_6 flagged lookalikes {flagged:?} (clean={clean:.3})");
}

fn f2_1_of_body(body: &str) -> f32 {
    let f = py_file(&format!("{SINK_HEADER}{body}"));
    dim_value(&score_py(f.path()), "F2_1")
}

#[test]
fn f2_1_detects_each_sink_class_alone() {
    let clean = f2_1_of_body("    return usuario\n");
    let blind: Vec<&str> = SINK_CASES
        .iter()
        .filter(|(_, body)| f2_1_of_body(body) >= clean)
        .map(|(name, _)| *name)
        .collect();
    assert!(
        blind.is_empty(),
        "F2_1 blind to {blind:?} (clean={clean:.3})"
    );
}

#[test]
fn f2_1_safe_forms_stay_clean() {
    let clean = f2_1_of_body("    return usuario\n");
    assert!(
        clean >= 0.99,
        "control: the clean prologue itself scores {clean:.3}"
    );
    let flagged: Vec<(&str, f32)> = SAFE_CASES
        .iter()
        .map(|(name, body)| (*name, f2_1_of_body(body)))
        .filter(|(_, v)| *v < clean)
        .collect();
    assert!(
        flagged.is_empty(),
        "F2_1 flagged safe forms {flagged:?} (clean={clean:.3})"
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
    let content = format!("TOKEN = \"{prefix}{body}\"  # pragma: allowlist secret\n");
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

/// The three GREEN dims whose fixtures were written in the S50 baseline but never
/// exercised (dead constants under clippy, 25/09/2026): the module doc CLAIMED them GREEN;
/// these tests make the claim evidence. One mutant, one dimension, strictly below the
/// clean baseline — the status travels in the failure message.
fn assert_dim_drops(dim: &str, mutant_src: &str, what: &str) {
    let f_clean = py_file(M00_CLEAN);
    let f_mutant = py_file(mutant_src);
    let clean = score_py(f_clean.path());
    let mutant = score_py(f_mutant.path());
    let (clean_v, mutant_v) = (dim_value(&clean, dim), dim_value(&mutant, dim));
    assert!(
        mutant_v < clean_v,
        "{dim} should drop on the {what} mutant: clean={clean_v:.3} ({}) mutant={mutant_v:.3} ({})",
        dim_status(&clean, dim),
        dim_status(&mutant, dim)
    );
}

#[test]
fn f3_8_missing_docs_drops() {
    assert_dim_drops("F3_8", M09_NODOCS, "missing-docs");
}

#[test]
fn f1_5_tech_debt_drops() {
    assert_dim_drops("F1_5", M10_DEBT, "TODO/FIXME debt");
}

#[test]
fn f4_1_non_idiomatic_code_drops() {
    assert_dim_drops("F4_1", M13_PRINT_IDIOMS, "non-idiomatic");
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

// ---------------------------------------------------------------------------
// S52 - F2.4 blind spot: env-style assignment inside JSON string arg
// ---------------------------------------------------------------------------

/// Write a temp .json file and return the NamedTempFile guard.
fn json_file(content: &str) -> tempfile::NamedTempFile {
    let mut f = tempfile::Builder::new()
        .suffix(".json")
        .tempfile()
        .expect("create temp .json");
    f.write_all(content.as_bytes()).expect("write .json");
    f
}

/// F2.4 - GEMINI_API_KEY=sk-<48hex> inside a JSON string arg must be
/// detected (blind spot identified by orchestrator probe f24_shapes.py).
///
/// The sk- prefix followed by 48 lowercase hex chars (entropy ~4.0 < 4.5
/// threshold) was not caught by the generic high-entropy literal path, and
/// the env-style NAME=VALUE scanner only checked unquoted assignments, not
/// values embedded inside JSON string literals.
///
/// Secret values generated at runtime — no literal with the shape of a
/// credential appears in this source file.
#[test]
fn f2_4_env_assignment_in_json_string_is_detected() {
    // sk- + 48 hex chars: each part alone is not a secret shape
    let sk_prefix = "sk-";
    let hex_tail = "a1b2c3d4e5f6".repeat(4); // 48 hex chars, not a secret on its own
    let key_ctx = "GEMINI_API_KEY";
    let json_content = format!(
        "{{\"mcpServers\":{{\"x\":{{\"command\":\"bash\",\"args\":[\"-c\",\"{key_ctx}={sk_prefix}{hex_tail} srv\"]}}}}}}"
    );
    let jf = json_file(&json_content);
    let jjson = score_py(jf.path());
    let _keep_j = jf;
    let v_json = dim_value(&jjson, "F2_4");
    assert!(
        v_json < 0.5,
        "F2_4 must detect {key_ctx}=sk-<48hex> inside JSON string arg: value={v_json:.3}"
    );

    // regression: AIza in JSON arg form (already detected)
    let aiza = "AIza".to_owned() + &"A".repeat(35);
    let jf_aiza = json_file(&format!("{{\"args\":[\"-c\",\"GOOGLE_KEY={aiza} srv\"]}}"));
    let v_aiza = dim_value(&score_py(jf_aiza.path()), "F2_4");
    let _keep_aiza = jf_aiza;
    assert!(
        v_aiza < 0.5,
        "AIza in JSON must still score <0.5: {v_aiza:.3}"
    );

    // regression: sk-ant-api03 in JSON arg form (already detected)
    let ant_prefix = "sk-ant-api03-";
    let ant = ant_prefix.to_owned() + &"B".repeat(40);
    let jf_ant = json_file(&format!("{{\"args\":[\"-c\",\"ANTHROPIC_KEY={ant}\"]}}"));
    let v_ant = dim_value(&score_py(jf_ant.path()), "F2_4");
    let _keep_ant = jf_ant;
    assert!(
        v_ant < 0.5,
        "sk-ant in JSON must still score <0.5: {v_ant:.3}"
    );

    // regression: ghp_ in JSON arg form (already detected)
    let ghp = "ghp_".to_owned() + &"C".repeat(36);
    let jf_ghp = json_file(&format!("{{\"args\":[\"-c\",\"TOKEN={ghp}\"]}}"));
    let v_ghp = dim_value(&score_py(jf_ghp.path()), "F2_4");
    let _keep_ghp = jf_ghp;
    assert!(
        v_ghp < 0.5,
        "ghp_ in JSON must still score <0.5: {v_ghp:.3}"
    );

    // regression: sk-<48hex> as Python assignment (already detected)
    let py_content = format!("API_KEY = \"{sk_prefix}{hex_tail}\"\n");
    let pf = py_file(&py_content);
    let v_py = dim_value(&score_py(pf.path()), "F2_4");
    let _keep_p = pf;
    assert!(
        v_py < 0.5,
        "sk-hex48 in Python assignment must still score <0.5: {v_py:.3}"
    );
}
