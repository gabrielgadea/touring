"""Model a CI runner: no skills root exists. Does the guard approve having measured nothing?"""
import sys
from pathlib import Path
from unittest import mock

sys.path.insert(0, sys.argv[1])
import test_skill_size_guard as guard

with mock.patch.object(Path, "is_dir", return_value=False):
    print("varrer() over absent roots ->", guard.varrer(guard.SKILLS_DIRS))
    rc = guard._rodar()
    print("script mode _rodar() rc =", rc)
