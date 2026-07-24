#!/usr/bin/env python3
"""build_lite_bench.py - emit a curated SWE-bench-lite dataset for touring-eval.

Master Plan E.W2 (scale-up). Produces a diverse set of self-contained Rust and
Python bug-fix instances in the harness JSONL format. Each instance is a REAL bug
PATTERN (overflow, off-by-one, missing guard, wrong accumulator, ...) but is
self-contained: a tiny crate / script runnable with `cargo test` / `python3` in a
temp dir, with NO network, NO Docker, NO workspace git. Deterministic + fast.

These are honestly labeled "lite" instances (authored real-pattern bugs), NOT raw
GitHub-issue clones. The raw-GitHub path (Multi-SWE-bench Rust) is the heavier
escalation: it needs per-repo Docker envs + minutes-long builds + context-file
selection, and is documented as the next step.

Each instance uses a PARTIAL bug so that the normal case passes in both the buggy
and fixed code (`pass_to_pass`, a genuine regression guard) while an edge case is
red until fixed (`fail_to_pass`). `harness.py validate` mechanically checks this.

Usage:
  eval/swe_bench/datasets/build_lite_bench.py            # -> touring-lite-v1.jsonl
  eval/swe_bench/datasets/build_lite_bench.py --out <p>  # custom path
"""
from __future__ import annotations

import argparse
import json
import re
from pathlib import Path
from typing import Optional

CARGO = ('[package]\nname = "{name}"\nversion = "0.0.0"\nedition = "2021"\n\n'
         '[lib]\npath = "src/lib.rs"\n')


def _crate_name(instance_id: str) -> str:
    """Derive a valid cargo crate name from an instance id."""
    return re.sub(r"[^0-9a-zA-Z]+", "_", instance_id.split("__", 1)[-1]).strip("_")


def rust_lib(fn_code: str, tests: str) -> str:
    """Assemble a lib.rs from a production fn body + a test-module inner body."""
    return f"{fn_code}\n\n#[cfg(test)]\nmod tests {{\n    use super::*;\n{tests}\n}}\n"


def rust_instance(iid: str, problem: str, fn_buggy: str, fn_fixed: str,
                  tests: str, f2p: list, p2p: list) -> dict:
    """Build a Rust inline instance (cargo test)."""
    name = _crate_name(iid)
    return {
        "instance_id": iid,
        "problem_statement": problem,
        "mode": "inline",
        "files": {"Cargo.toml": CARGO.format(name=name), "src/lib.rs": rust_lib(fn_buggy, tests)},
        "gold_files": {"src/lib.rs": rust_lib(fn_fixed, tests)},
        "test_cmd": "cargo test --quiet",
        "fail_to_pass": f2p,
        "pass_to_pass": p2p,
        "aider_resolved": None,
    }


def py_runner(import_line: str, tests: dict) -> str:
    """Build a run_tests.py dispatching argv[1] to a named test function."""
    lines = ["import sys", import_line, ""]
    for name, body in tests.items():
        lines.append(f"def {name}():")
        for b in body.split("\n"):
            lines.append("    " + b)
    reg = ", ".join(f'"{n}": {n}' for n in tests)
    lines += [
        f"TESTS = {{{reg}}}",
        "if __name__ == '__main__':",
        "    _n = sys.argv[1] if len(sys.argv) > 1 else ''",
        "    _f = TESTS.get(_n)",
        "    if _f is None:",
        "        print('no such test', _n)",
        "        sys.exit(2)",
        "    _f()",
        "    print('ok')",
    ]
    return "\n".join(lines) + "\n"


def py_instance(iid: str, problem: str, sol_buggy: str, sol_fixed: str,
                import_line: str, tests: dict, f2p: list, p2p: list) -> dict:
    """Build a Python inline instance (python3 run_tests.py)."""
    runner = py_runner(import_line, tests)
    return {
        "instance_id": iid,
        "problem_statement": problem,
        "mode": "inline",
        "files": {"solution.py": sol_buggy, "run_tests.py": runner},
        "gold_files": {"solution.py": sol_fixed},
        "test_cmd": "python3 run_tests.py",
        "fail_to_pass": f2p,
        "pass_to_pass": p2p,
        "aider_resolved": None,
    }


def rust_instances() -> list:
    """14 Rust instances across distinct bug categories."""
    out = []
    out.append(rust_instance(
        "lite__rust-overflow",
        "safe_add overflows on i32::MAX; it should return None instead of panicking.",
        "pub fn safe_add(a: i32, b: i32) -> Option<i32> { Some(a + b) }",
        "pub fn safe_add(a: i32, b: i32) -> Option<i32> { a.checked_add(b) }",
        "    #[test]\n    fn t_basic() { assert_eq!(safe_add(2, 3), Some(5)); }\n"
        "    #[test]\n    fn t_overflow() { assert_eq!(safe_add(i32::MAX, 1), None); }",
        ["t_overflow"], ["t_basic"]))
    out.append(rust_instance(
        "lite__rust-range-off-by-one",
        "sum_to(n) must include n; it currently sums 1..n instead of 1..=n.",
        "pub fn sum_to(n: u32) -> u32 { (1..n).sum() }",
        "pub fn sum_to(n: u32) -> u32 { (1..=n).sum() }",
        "    #[test]\n    fn t_zero() { assert_eq!(sum_to(0), 0); }\n"
        "    #[test]\n    fn t_five() { assert_eq!(sum_to(5), 15); }",
        ["t_five"], ["t_zero"]))
    out.append(rust_instance(
        "lite__rust-max-of",
        "max_of returns the first element instead of the maximum.",
        "pub fn max_of(v: &[i32]) -> Option<i32> { v.first().copied() }",
        "pub fn max_of(v: &[i32]) -> Option<i32> { v.iter().max().copied() }",
        "    #[test]\n    fn t_single() { assert_eq!(max_of(&[7]), Some(7)); }\n"
        "    #[test]\n    fn t_multi() { assert_eq!(max_of(&[1, 5, 3]), Some(5)); }",
        ["t_multi"], ["t_single"]))
    out.append(rust_instance(
        "lite__rust-div-guard",
        "safe_div panics on division by zero; it should return None.",
        "pub fn safe_div(a: i32, b: i32) -> Option<i32> { Some(a / b) }",
        "pub fn safe_div(a: i32, b: i32) -> Option<i32> { if b == 0 { None } else { Some(a / b) } }",
        "    #[test]\n    fn t_basic() { assert_eq!(safe_div(6, 3), Some(2)); }\n"
        "    #[test]\n    fn t_zero() { assert_eq!(safe_div(1, 0), None); }",
        ["t_zero"], ["t_basic"]))
    out.append(rust_instance(
        "lite__rust-clamp-upper",
        "clamp_100 clamps the lower bound but forgets to cap values above 100.",
        "pub fn clamp_100(x: i32) -> i32 { if x < 0 { 0 } else { x } }",
        "pub fn clamp_100(x: i32) -> i32 { if x < 0 { 0 } else if x > 100 { 100 } else { x } }",
        "    #[test]\n    fn t_mid() { assert_eq!(clamp_100(50), 50); }\n"
        "    #[test]\n    fn t_high() { assert_eq!(clamp_100(150), 100); }",
        ["t_high"], ["t_mid"]))
    out.append(rust_instance(
        "lite__rust-count-vowels",
        "count_vowels omits the vowel 'u'.",
        "pub fn count_vowels(s: &str) -> usize { s.chars().filter(|c| \"aeio\".contains(*c)).count() }",
        "pub fn count_vowels(s: &str) -> usize { s.chars().filter(|c| \"aeiou\".contains(*c)).count() }",
        "    #[test]\n    fn t_no_u() { assert_eq!(count_vowels(\"abc\"), 1); }\n"
        "    #[test]\n    fn t_u() { assert_eq!(count_vowels(\"uuu\"), 3); }",
        ["t_u"], ["t_no_u"]))
    out.append(rust_instance(
        "lite__rust-abs-diff",
        "abs_diff returns a signed difference; it should return the absolute value.",
        "pub fn abs_diff(a: i32, b: i32) -> i32 { a - b }",
        "pub fn abs_diff(a: i32, b: i32) -> i32 { (a - b).abs() }",
        "    #[test]\n    fn t_pos() { assert_eq!(abs_diff(5, 3), 2); }\n"
        "    #[test]\n    fn t_neg() { assert_eq!(abs_diff(3, 5), 2); }",
        ["t_neg"], ["t_pos"]))
    out.append(rust_instance(
        "lite__rust-parse-or-zero",
        "parse_or_zero panics on non-numeric input; it should default to 0.",
        "pub fn parse_or_zero(s: &str) -> i32 { s.parse::<i32>().unwrap() }",
        "pub fn parse_or_zero(s: &str) -> i32 { s.parse::<i32>().unwrap_or(0) }",
        "    #[test]\n    fn t_num() { assert_eq!(parse_or_zero(\"42\"), 42); }\n"
        "    #[test]\n    fn t_bad() { assert_eq!(parse_or_zero(\"x\"), 0); }",
        ["t_bad"], ["t_num"]))
    out.append(rust_instance(
        "lite__rust-palindrome",
        "is_palindrome compares the string to itself and is always true.",
        "pub fn is_palindrome(s: &str) -> bool { s == s }",
        "pub fn is_palindrome(s: &str) -> bool { s.chars().eq(s.chars().rev()) }",
        "    #[test]\n    fn t_yes() { assert!(is_palindrome(\"aba\")); }\n"
        "    #[test]\n    fn t_no() { assert!(!is_palindrome(\"abc\")); }",
        ["t_no"], ["t_yes"]))
    out.append(rust_instance(
        "lite__rust-last-of",
        "last_of returns the first element instead of the last.",
        "pub fn last_of(v: &[i32]) -> Option<i32> { v.first().copied() }",
        "pub fn last_of(v: &[i32]) -> Option<i32> { v.last().copied() }",
        "    #[test]\n    fn t_single() { assert_eq!(last_of(&[9]), Some(9)); }\n"
        "    #[test]\n    fn t_multi() { assert_eq!(last_of(&[1, 2, 3]), Some(3)); }",
        ["t_multi"], ["t_single"]))
    out.append(rust_instance(
        "lite__rust-first-word",
        "first_word returns the whole string instead of the first whitespace token.",
        "pub fn first_word(s: &str) -> &str { s }",
        "pub fn first_word(s: &str) -> &str { s.split_whitespace().next().unwrap_or(\"\") }",
        "    #[test]\n    fn t_one() { assert_eq!(first_word(\"hi\"), \"hi\"); }\n"
        "    #[test]\n    fn t_two() { assert_eq!(first_word(\"hello world\"), \"hello\"); }",
        ["t_two"], ["t_one"]))
    out.append(rust_instance(
        "lite__rust-gcd",
        "gcd returns the first argument instead of the greatest common divisor.",
        "pub fn gcd(a: u32, b: u32) -> u32 { a }",
        "pub fn gcd(a: u32, b: u32) -> u32 { if b == 0 { a } else { gcd(b, a % b) } }",
        "    #[test]\n    fn t_zero() { assert_eq!(gcd(5, 0), 5); }\n"
        "    #[test]\n    fn t_pair() { assert_eq!(gcd(12, 8), 4); }",
        ["t_pair"], ["t_zero"]))
    out.append(rust_instance(
        "lite__rust-capitalize",
        "capitalize returns the input unchanged; it should upper-case the first char.",
        "pub fn capitalize(s: &str) -> String { s.to_string() }",
        "pub fn capitalize(s: &str) -> String { let mut c = s.chars(); "
        "match c.next() { Some(f) => f.to_uppercase().collect::<String>() + c.as_str(), "
        "None => String::new() } }",
        "    #[test]\n    fn t_empty() { assert_eq!(capitalize(\"\"), \"\"); }\n"
        "    #[test]\n    fn t_word() { assert_eq!(capitalize(\"abc\"), \"Abc\"); }",
        ["t_word"], ["t_empty"]))
    out.append(rust_instance(
        "lite__rust-is-sorted",
        "is_sorted always returns true; it should check the slice is non-decreasing.",
        "pub fn is_sorted(v: &[i32]) -> bool { let _ = v; true }",
        "pub fn is_sorted(v: &[i32]) -> bool { v.windows(2).all(|w| w[0] <= w[1]) }",
        "    #[test]\n    fn t_sorted() { assert!(is_sorted(&[1, 2, 3])); }\n"
        "    #[test]\n    fn t_unsorted() { assert!(!is_sorted(&[3, 1, 2])); }",
        ["t_unsorted"], ["t_sorted"]))
    return out


def python_instances() -> list:
    """6 Python instances across distinct bug categories."""
    out = []
    out.append(py_instance(
        "lite__py-div-guard",
        "safe_div raises on division by zero; it should return None.",
        "def safe_div(a, b):\n    return a / b\n",
        "def safe_div(a, b):\n    if b == 0:\n        return None\n    return a / b\n",
        "from solution import safe_div",
        {"t_basic": "assert safe_div(6, 3) == 2",
         "t_zero": "assert safe_div(1, 0) is None"},
        ["t_zero"], ["t_basic"]))
    out.append(py_instance(
        "lite__py-mutable-default",
        "add_item uses a mutable default list shared across calls.",
        "def add_item(x, acc=[]):\n    acc.append(x)\n    return acc\n",
        "def add_item(x, acc=None):\n    if acc is None:\n        acc = []\n    acc.append(x)\n    return acc\n",
        "from solution import add_item",
        {"t_single": "assert add_item(1) == [1]",
         "t_isolation": "add_item(1)\nassert add_item(2) == [2]"},
        ["t_isolation"], ["t_single"]))
    out.append(py_instance(
        "lite__py-slice-off-by-one",
        "first_n returns n+1 elements instead of n.",
        "def first_n(lst, n):\n    return lst[:n + 1]\n",
        "def first_n(lst, n):\n    return lst[:n]\n",
        "from solution import first_n",
        {"t_full": "assert first_n([1, 2, 3, 4], 4) == [1, 2, 3, 4]",
         "t_part": "assert first_n([1, 2, 3, 4], 2) == [1, 2]"},
        ["t_part"], ["t_full"]))
    out.append(py_instance(
        "lite__py-dict-get",
        "lookup raises KeyError on a missing key; it should return None.",
        "def lookup(d, k):\n    return d[k]\n",
        "def lookup(d, k):\n    return d.get(k)\n",
        "from solution import lookup",
        {"t_hit": "assert lookup({'a': 1}, 'a') == 1",
         "t_miss": "assert lookup({}, 'x') is None"},
        ["t_miss"], ["t_hit"]))
    out.append(py_instance(
        "lite__py-average-intdiv",
        "average uses integer division and drops the fractional part.",
        "def average(nums):\n    return sum(nums) // len(nums)\n",
        "def average(nums):\n    return sum(nums) / len(nums)\n",
        "from solution import average",
        {"t_whole": "assert average([2, 2]) == 2",
         "t_frac": "assert average([1, 2]) == 1.5"},
        ["t_frac"], ["t_whole"]))
    out.append(py_instance(
        "lite__py-capitalize",
        "cap_first lower-cases the string instead of upper-casing the first char.",
        "def cap_first(s):\n    return s.lower()\n",
        "def cap_first(s):\n    return s[:1].upper() + s[1:]\n",
        "from solution import cap_first",
        {"t_empty": "assert cap_first('') == ''",
         "t_word": "assert cap_first('abc') == 'Abc'"},
        ["t_word"], ["t_empty"]))
    return out


def main(argv: Optional[list] = None) -> int:
    ap = argparse.ArgumentParser(prog="build-lite-bench",
                                 description="Emit the touring-eval lite dataset.")
    ap.add_argument("--out", type=Path,
                    default=Path(__file__).resolve().parent / "touring-lite-v1.jsonl")
    args = ap.parse_args(argv)
    instances = rust_instances() + python_instances()
    args.out.parent.mkdir(parents=True, exist_ok=True)
    with args.out.open("w") as fh:
        for inst in instances:
            fh.write(json.dumps(inst) + "\n")
    rust_n = sum(1 for i in instances if i["test_cmd"].startswith("cargo"))
    print(f"wrote {args.out} ({len(instances)} instances: {rust_n} rust, {len(instances) - rust_n} python)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
