"""Keep this tree free of generated artifacts.

`scripts/test_sync_client_skills.py::test_no_generated_artifact_sits_in_the_mirror`
walks ALL of `client/` and fails on any `__pycache__` / `.pytest_cache` — the
2026-07-25 bulk copy shipped 134 such files to a public repo. Running this
suite must not recreate them: bytecode writing is switched off here (conftest
is imported before every test module), and `pytest.ini` moves the cache dir
out of the tree.
"""

from __future__ import annotations

import sys

sys.dont_write_bytecode = True
