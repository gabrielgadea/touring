# A slice of spec_d2.MUTANTS chosen by SHARD/NSHARDS (round-robin), so shards stay balanced.
import importlib.util
import os

_spec = importlib.util.spec_from_file_location("spec_d2", os.path.join(os.path.dirname(__file__), "spec_d2.py"))
_mod = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_mod)
_i, _n = int(os.environ["SHARD"]), int(os.environ["NSHARDS"])
MUTANTS = [m for k, m in enumerate(_mod.MUTANTS) if k % _n == _i]
COPY = _mod.COPY
