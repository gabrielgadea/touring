"""model_pool.py — the model pool of an Anthropic session: one source for the roster hook and the model gate.

Gabriel's directive of 13/11/2026: every Anthropic session is a Fable orchestrator with one Opus, three Sonnet
and five Haiku subagents, each family at its own default effort. The announcement (`model_pool_roster.py`,
SessionStart) and the executor (`agent_model_gate.py`, PreToolUse on `Agent`) both read `POOL`, so the text shown
to the model can never promise what the gate does not apply (D8).

Families are ALIASES on purpose: on the Anthropic API an alias resolves to the newest version of its family, so
the pool follows a release without an edit here. A pinned id (`claude-…`) would age silently
(docs/economia-de-creditos-e-contexto.md §6).
"""

from __future__ import annotations

import sys
from pathlib import Path
from typing import NamedTuple

sys.path.insert(0, str(Path(__file__).resolve().parent))
from backend_provider import model_family

ORCHESTRATOR = "orquestrador"
SUBAGENT = "subagente"


class PoolSlot(NamedTuple):
    """One line of the roster: `count` seats of `family`, run at `effort`, for the work named in `use`."""

    family: str
    role: str
    count: int
    effort: str
    use: str


#: The roster, in descending cost. The gate's fit rules decide which subagent types each family may take.
POOL = (
    PoolSlot("fable", ORCHESTRATOR, 1, "medium", "orquestrar, decidir, sintetizar, julgar a entrega de cada delegado"),
    PoolSlot("opus", SUBAGENT, 1, "medium", "decisão difícil delegada, arquitetura, segundo juiz independente"),
    PoolSlot("sonnet", SUBAGENT, 3, "high", "implementar com spec clara, testes, refactor delimitado, revisão de código"),
    PoolSlot("haiku", SUBAGENT, 5, "xhigh", "buscar, extrair, classificar, recuperar documentação, varredura mecânica"),
)
EFFORT_BY_FAMILY = {slot.family: slot.effort for slot in POOL}


def subagent_slots() -> tuple[PoolSlot, ...]:
    """The roster without the orchestrator."""
    return tuple(slot for slot in POOL if slot.role == SUBAGENT)


def slot_names() -> list[str]:
    """`opus-1`, `sonnet-1`…`sonnet-3`, `haiku-1`…`haiku-5`: the address of each subagent seat."""
    return [f"{slot.family}-{n}" for slot in subagent_slots() for n in range(1, slot.count + 1)]


def default_effort(model: object) -> str | None:
    """The pool's effort for the family of `model` (alias or full id); None when it names no family."""
    family = model_family(model)
    return EFFORT_BY_FAMILY.get(family) if family else None
