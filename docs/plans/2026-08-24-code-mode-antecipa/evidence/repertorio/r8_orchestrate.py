# #tags: kind:snippet purpose:round-trips-de-consulta domain:code-mode
"""R8 orchestrate: sub-chamadas ao daemon DENTRO do sandbox (1 call vs N).

As consultas morrem no sandbox; o contexto recebe o veredito. Exige
`touring run --orchestrate`. Contrato completo: `touring run --sdk-stub`.
"""
import sys
simbolo = sys.argv[1] if len(sys.argv) > 1 else "record_bash_call"
import touring  # injetado pelo runtime orchestrate
defs = touring.index_find(simbolo)
impacto = touring.wiring_impact(simbolo, 2)
print({"simbolo": simbolo,
       "definido": bool(defs),
       "consumidores_diretos": (impacto or {}).get("direct_consumers", "?")})
