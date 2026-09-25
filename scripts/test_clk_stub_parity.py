#!/usr/bin/env python3
"""Parity guard for `claude_learning_kernel.pyi` (24/09/2026).

The contract (touring-36): the stub's declared name set EQUALS the module's
runtime exports — so the stub can never drift from the `.so` it describes.
Pyright reads the stub for every consumer; a stub that drifts turns the
consumers' type errors from signal into fiction.

Two sets, compared exactly: the module's public top-level names, and the
`simd` submodule's public names. Runtime is read from the BUILT module; the
stub is parsed with `ast`. When the bindings grow a symbol, this test fails
until the stub grows with it.
"""

from __future__ import annotations

import ast
import sys
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parent.parent
STUB = (
    ROOT
    / "crates"
    / "touring-python"
    / "claude_learning_kernel"
    / "__init__.pyi"
)
INNER_STUB = (
    ROOT
    / "crates"
    / "touring-python"
    / "claude_learning_kernel"
    / "claude_learning_kernel.pyi"
)
SO = ROOT / "target" / "debug" / "libclaude_learning_kernel.so"


def _runtime_names() -> tuple[set[str], set[str]]:
    # Replicate the INSTALLED layout exactly (measured in the wheel):
    # `claude_learning_kernel/__init__.py` (maturin's star-import shim) +
    # `claude_learning_kernel/claude_learning_kernel.so` nested — that is
    # what makes the `claude_learning_kernel.claude_learning_kernel` bridge
    # form resolve and what consumers actually see.
    import shutil
    import tempfile

    tmp = Path(tempfile.mkdtemp(prefix="clk-parity-"))
    pkg = tmp / "claude_learning_kernel"
    pkg.mkdir()
    (pkg / "__init__.py").write_text(
        "from .claude_learning_kernel import *\n\n"
        "__doc__ = claude_learning_kernel.__doc__\n"
        "if hasattr(claude_learning_kernel, \"__all__\"):\n"
        "    __all__ = claude_learning_kernel.__all__\n",
        encoding="utf-8",
    )
    shutil.copy(SO, pkg / "claude_learning_kernel.so")
    sys.path.insert(0, str(tmp))
    for stale in ("claude_learning_kernel", "claude_learning_kernel.claude_learning_kernel"):
        sys.modules.pop(stale, None)
    import claude_learning_kernel as clk

    top = {n for n in dir(clk) if not n.startswith("_")}
    if hasattr(clk, "__version__"):
        top.add("__version__")
    simd_names = {n for n in dir(clk.simd) if not n.startswith("_")}
    return top, simd_names


def _stub_names(path: Path) -> tuple[set[str], set[str]]:
    tree = ast.parse(path.read_text(encoding="utf-8"))
    top: set[str] = set()
    simd_members: set[str] = set()
    for node in tree.body:
        if isinstance(node, (ast.ClassDef, ast.FunctionDef, ast.AsyncFunctionDef)):
            top.add(node.name)
        elif isinstance(node, ast.Assign):
            top.update(t.id for t in node.targets if isinstance(t, ast.Name))
        elif isinstance(node, ast.AnnAssign) and isinstance(node.target, ast.Name):
            top.add(node.target.id)
    for node in tree.body:
        if isinstance(node, ast.ClassDef) and node.name == "simd":
            for sub in node.body:
                if isinstance(sub, (ast.ClassDef, ast.FunctionDef, ast.AsyncFunctionDef)):
                    simd_members.add(sub.name)
    # `__all__` is a dunder — out of the name sets, compared separately.
    return {n for n in top if not n.startswith("_") or n == "__version__"}, simd_members


def _stub_all(path: Path) -> list[str]:
    """The literal `__all__` list declared in the stub, as a sorted set."""
    tree = ast.parse(path.read_text(encoding="utf-8"))
    for node in tree.body:
        if isinstance(node, ast.Assign):
            for t in node.targets:
                if isinstance(t, ast.Name) and t.id == "__all__" and isinstance(node.value, ast.List):
                    return sorted(
                        elt.value for elt in node.value.elts
                        if isinstance(elt, ast.Constant) and isinstance(elt.value, str)
                    )
    raise AssertionError(f"{path.name} declares no literal __all__ list")


def _runtime_all() -> list[str]:
    import claude_learning_kernel as clk

    return sorted(clk.__all__)


@pytest.mark.skipif(not SO.is_file(), reason="needs the built cdylib (cargo build -p touring-python)")
def test_the_stub_matches_the_runtime_surface_name_for_name():
    top_rt, simd_rt = _runtime_names()
    top_stub, simd_stub = _stub_names(STUB)
    assert top_stub == top_rt, (
        f"top-level drift — só no stub: {sorted(top_stub - top_rt)} | "
        f"só em runtime: {sorted(top_rt - top_stub)}"
    )
    assert simd_stub == simd_rt, (
        f"simd drift — só no stub: {sorted(simd_stub - simd_rt)} | "
        f"só em runtime: {sorted(simd_rt - simd_stub)}"
    )


@pytest.mark.skipif(not SO.is_file(), reason="needs the built cdylib (cargo build -p touring-python)")
def test_the_inner_stub_matches_the_inner_module_name_for_name():
    """The `claude_learning_kernel.claude_learning_kernel` import form (the
    aco bridge) must find the same names in the INNER stub as in the INNER
    `.so` at runtime."""
    import claude_learning_kernel as pkg

    inner = pkg.claude_learning_kernel
    top_rt = {n for n in dir(inner) if not n.startswith("_")}
    if hasattr(inner, "__version__"):
        top_rt.add("__version__")
    simd_rt = {n for n in dir(inner.simd) if not n.startswith("_")}
    top_stub, simd_stub = _stub_names(INNER_STUB)
    assert top_stub == top_rt, (
        f"inner drift — só no stub: {sorted(top_stub - top_rt)} | "
        f"só em runtime: {sorted(top_rt - top_stub)}"
    )
    assert simd_stub == simd_rt, (
        f"inner simd drift — só no stub: {sorted(simd_stub - simd_rt)} | "
        f"só em runtime: {sorted(simd_rt - simd_stub)}"
    )


@pytest.mark.skipif(not SO.is_file(), reason="needs the built cdylib (cargo build -p touring-python)")
def test_the_stubs_all_lists_equal_the_runtime_all():
    """`__all__` in both stubs equals the runtime's `__all__` — never the
    public set (which would drop `__version__` and add the submodule attr)."""
    runtime = _runtime_all()
    assert _stub_all(STUB) == runtime, "package stub's __all__ drifted from runtime"
    assert _stub_all(INNER_STUB) == runtime, "inner stub's __all__ drifted from runtime"


def test_the_wheel_init_py_is_identical_to_the_crates_own():
    """The hand-written `__init__.py` is a COPY of maturin's generated shim.
    If a future maturin changes its shim (or starts overriding ours), the
    copy diverges silently — measured by the coordinator 24/09/2026. The
    wheel must carry the crate's file byte-for-byte."""
    import glob
    import zipfile

    wheels = sorted(glob.glob(str(ROOT / "target" / "wheels" / "claude_learning_kernel-4.0.0-*.whl")))
    assert wheels, "no built wheel found (maturin build --release -m crates/touring-python/Cargo.toml)"
    mine = (
        ROOT / "crates" / "touring-python" / "claude_learning_kernel" / "__init__.py"
    ).read_bytes()
    for wheel in wheels:
        with zipfile.ZipFile(wheel) as z:
            shipped = z.read("claude_learning_kernel/__init__.py")
        assert shipped == mine, (
            f"{Path(wheel).name}: the wheel's __init__.py diverged from the "
            "crate's own — the shim changed hands silently"
        )


if __name__ == "__main__":
    sys.exit(pytest.main([__file__, "-q"]))
