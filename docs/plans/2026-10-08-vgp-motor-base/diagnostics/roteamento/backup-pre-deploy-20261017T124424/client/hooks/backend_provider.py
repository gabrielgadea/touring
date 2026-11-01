"""backend_provider.py — quem está atrás desta sessão do Claude Code: Anthropic ou uma irmã.

Fonte ÚNICA do predicado para os hooks do cliente (`agent_model_gate.py` isenta as irmãs;
`rate_limits_snapshot.py` guarda a cota de cada provedor em separado). As sessões irmãs são
Claude Code com `ANTHROPIC_BASE_URL` trocado (`kimi-cc`/`kimi1m`/`minimax` no ~/.bashrc), e todo
hook herda o env da sessão. A ferramenta `~/.claude/tools/sessoes_irmas.py` usa a mesma tabela;
`test_provider_predicate_matches_sessoes_irmas` reprova se as duas divergirem.
"""

from __future__ import annotations

PROVIDERS = (("anthropic.com", "anthropic"), ("kimi.com", "kimi"), ("moonshot", "kimi"), ("minimax", "minimax"))
#: Only the named sister backends are exempt from Anthropic-only policy; an unknown gateway may front Opus.
SISTER_PROVIDERS = frozenset({"kimi", "minimax"})


def provider_of(base_url: str | None) -> str:
    """Classify the backend from ANTHROPIC_BASE_URL (unset = Anthropic itself)."""
    if not base_url:
        return "anthropic"
    low = base_url.lower()
    for needle, name in PROVIDERS:
        if needle in low:
            return name
    return f"outro:{low.split('//')[-1].split('/')[0]}"
