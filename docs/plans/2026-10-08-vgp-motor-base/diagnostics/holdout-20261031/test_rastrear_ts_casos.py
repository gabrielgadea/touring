"""End-to-end test for timestamp provenance extraction on a synthetic transcript."""

import json
import os
import subprocess
import sys
from pathlib import Path

ARQ = Path(__file__).resolve().with_name("rastrear_ts_casos.py")
ALVO = "/projeto/a.py"


def test_rastreador_distingue_evento_datado_de_blob(tmp_path):
    transcript = tmp_path / ".claude/projects/proj/sessao.jsonl"
    transcript.parent.mkdir(parents=True)
    registros = [
        {"type": "api-request-blob", "message": {"content": [{"type": "tool_use", "input": {"file_path": ALVO}}]}},
        {"type": "assistant", "timestamp": "2026-10-27T14:22:17.466Z", "message": {"content": [{"type": "tool_use", "input": {"file_path": ALVO}}]}},
    ]
    transcript.write_text("".join(json.dumps(r) + "\n" for r in registros))
    env = os.environ.copy() | {"HOME": str(tmp_path)}
    run = subprocess.run([sys.executable, "-I", str(ARQ), ALVO], capture_output=True, text=True, env=env, check=False)
    assert run.returncode == 0, run.stderr
    out = json.loads(run.stdout)
    assert out["total"] == 2 and out["sem_timestamp"] == 1
    assert out["amostra_com_ts"][0]["timestamp"] == "2026-10-27T14:22:17.466Z"
