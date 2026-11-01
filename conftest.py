"""Repository-wide pytest configuration.

Bytecode is never written during a test run (01/11/2026). Tests import or run
modules that live in ``client/`` — the generated mirror of ``~/.claude`` — and
every such import left ``__pycache__/*.pyc`` inside the mirrored areas, which
``scripts/test_sync_client_skills.py::test_no_generated_artifact_sits_in_the_mirror``
then reported (12 files on 31/10, 6 more within an hour on 01/11). Deleting them
after the fact only moved the failure to the next run; not writing them is the
fix at the source, and it costs nothing a test can measure.
"""

import sys

sys.dont_write_bytecode = True
