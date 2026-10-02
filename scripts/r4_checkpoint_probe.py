#!/usr/bin/env python3
"""Errata 12 (E12.1) — MEASURE which checkpoint `laya.Agent(...)` loads, instead of inferring it from the source.

Loads the repo-root checkpoint exactly as the r3 arms did (`Agent("convaiinnovations/laya", device="cpu")`, no `subfolder`) and the
`subfolder="multilingual"` one, and prints the encoder, `max_len` and `head_max_len` each agent reports (`Agent.cfg`, read from the
checkpoint's own `rl_agent_config.json`). CPU only; run with the Laya venv under `hw-slot`.
"""
import json
import sys


def probe(**kw) -> dict:
    import laya  # noqa: PLC0415
    cfg = laya.Agent(model_id_or_path="convaiinnovations/laya", device="cpu", **kw).cfg
    return {k: cfg.get(k) for k in ("encoder", "max_len", "head_max_len", "temperature")} | {"subfolder": kw.get("subfolder")}


if __name__ == "__main__":
    print(json.dumps({"as_r3_arms_loaded_it": probe(), "multilingual": probe(subfolder="multilingual")}, ensure_ascii=False, indent=1))
    sys.exit(0)
