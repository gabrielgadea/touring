"""Regression test: an undated api-request-blob must not create a corpus candidate."""

import sys
from pathlib import Path

SP = Path(__file__).resolve().parent
B = SP.parent.parent
sys.path.insert(0, str(B / "scripts"))
import corpus_v0d as c0  # noqa: E402

ARQ = "/home/gabrielgadea/projects/touring/client/hooks/tests/test_injection_policy_gate.py"
TS = "2026-10-27T14:22:17.466Z"


def evento_write(timestamp: str | None, tipo: str) -> dict:
    registro = {
        "type": tipo,
        "message": {
            "role": "assistant",
            "content": [
                {
                    "type": "tool_use",
                    "name": "Write",
                    "input": {"file_path": ARQ},
                }
            ],
        },
    }
    if timestamp is not None:
        registro["timestamp"] = timestamp
    return registro


def test_arquivo_preserva_evento_datado_e_exclui_blob_sem_ts():
    indice = {
        ARQ: [
            evento_write(None, "api-request-blob"),
            evento_write(TS, "assistant"),
        ]
    }
    filtrados, contagens = c0.universo_negativos(indice)
    assert filtrados == [f"{ARQ}@{TS}"]
    assert contagens["ts_ausente"] == 1
