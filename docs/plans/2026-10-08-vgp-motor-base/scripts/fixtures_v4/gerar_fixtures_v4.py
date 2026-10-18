"""Gera as fixtures SINTETICAS do juiz da V4: um `sombra.jsonl` e os transcripts que o juiz le.

Por que fixtures e nao o dado real: o juiz e escrito CONTRA O CONTRATO, antes de o motor da Kimi
emitir. Um juiz ajustado ao dado real deixa de medir o motor e passa a medir a si mesmo.

Cada cenario monta uma sessao FICTICIA em ~/.claude/projects/-v4-fixtures-*/<sessao>.jsonl, com o
deny (recusa ativa `PreToolUse:<Tool> hook error: [VGP · deny] …`) e a chamada seguinte que decide o
veredito. Nada disso toca um projeto real.

Uso: python3 gerar_fixtures_v4.py [--dir <destino>]
"""

import json
import sys
from pathlib import Path

A = Path.home() / ".claude/projects"
DESTINO_PADRAO = (
    Path.home() / "projects/touring/docs/plans/2026-10-08-vgp-motor-base/fixtures_v4"
)
NEG = "PreToolUse:Edit hook error: [VGP · deny] `foo_inexistente` nao existe no indice"


def registro(
    sessao, ts, path, modo="ativo", verdict="deny", nomes=("foo_inexistente",), reais=()
):
    achados = [
        {
            "classe": "simbolo-inexistente",
            "nome": nome,
            "linha": 10,
            "severidade": "P0",
            "escopo": "local",
            "sugestao": "crie ou use um nome real",
            "reais": list(reais),
        }
        for nome in nomes
    ]
    return {
        "ts": ts,
        "session_id": sessao,
        "tool_use_id": f"toolu_{sessao[:4]}01",
        "hook": "pre-edit",
        "project_root": "/v4-fixtures",
        "path": path,
        "balde": "arquivo",
        "camadas": [1, 2],
        "modo": modo,
        "verdict": verdict,
        "elapsed_ms": 3,
        "findings": achados,
    }


def _linha(ts, tipo, conteudo):
    # A chave do transcript REAL e `timestamp` (verificada num transcript de verdade): uma fixture com
    # `ts` faz o juiz ler string vazia em todo filtro e classificar tudo como "nao classificado" —
    # um zero silencioso que parece dado.
    return json.dumps(
        {"timestamp": ts, "type": tipo, "message": {"content": conteudo}},
        ensure_ascii=False,
    )


def transcript(sessao, ts, chamada_seguinte, fala=None):
    """tool_use -> deny -> [chamada seguinte] -> [fala]. `chamada_seguinte` None = sem par."""
    linhas = [
        _linha(
            ts,
            "assistant",
            [
                {
                    "type": "tool_use",
                    "id": f"toolu_{sessao[:4]}01",
                    "name": "Edit",
                    "input": {"file_path": "/v4-fixtures/a.py"},
                }
            ],
        ),
        _linha(
            ts,
            "user",
            [
                {
                    "type": "tool_result",
                    "tool_use_id": f"toolu_{sessao[:4]}01",
                    "is_error": True,
                    "content": NEG,
                }
            ],
        ),
    ]
    # a chamada seguinte e um minuto depois: dentro da janela de par (10 min) e depois do deny
    ts_seguinte = ts.replace(
        ts[11:19],
        "%02d:%02d:%02d" % (int(ts[11:13]), (int(ts[14:16]) + 1) % 60, int(ts[17:19])),
    )
    if chamada_seguinte:
        linhas.append(
            _linha(
                ts_seguinte,
                "assistant",
                [
                    {
                        "type": "tool_use",
                        "id": "toolu_seguinte01",
                        "name": "Edit",
                        "input": chamada_seguinte,
                    }
                ],
            )
        )
    if fala:
        linhas.append(_linha(ts_seguinte, "user", [{"type": "text", "text": fala}]))
    return "\n".join(linhas)


def _sessao(pasta, nome, ts, seguinte, fala=None):
    p = A / pasta
    p.mkdir(parents=True, exist_ok=True)
    (p / f"{nome}.jsonl").write_text(transcript(nome, ts, seguinte, fala))


def main():
    d = (
        Path(sys.argv[sys.argv.index("--dir") + 1])
        if "--dir" in sys.argv
        else DESTINO_PADRAO
    )
    d.mkdir(parents=True, exist_ok=True)
    for p in A.glob("-v4-fixtures-*"):
        for f in p.glob("*.jsonl"):
            f.unlink()

    T = "2026-10-09T10:00:00Z"
    CERTO = {"file_path": "/v4-fixtures/a.py", "new_string": "foo_real"}
    cenarios = {}

    # 5 sessoes, cada uma corrige o nome -> 5 denies corretos -> exit 0.
    regs = []
    for i in range(5):
        s = f"s5corret{i}"
        _sessao("-v4-fixtures-5", s, T, CERTO)
        regs.append(registro(s, T, "/v4-fixtures/a.py"))
    cenarios["cinco_sessoes.jsonl"] = regs
    cenarios["quatro_sessoes.jsonl"] = [
        r for r in regs if not r["session_id"].endswith("4")
    ]

    _sessao(
        "-v4-fixtures-b",
        "sBypass",
        T,
        {
            "file_path": "/v4-fixtures/a.py",
            "new_string": "TOURING_GATE_OK=1 foo_inexistente",
        },
    )
    cenarios["um_bypass.jsonl"] = [registro("sBypass", T, "/v4-fixtures/a.py")]

    _sessao(
        "-v4-fixtures-r",
        "sReenvio",
        T,
        {"file_path": "/v4-fixtures/a.py", "new_string": "foo_inexistente"},
    )
    cenarios["reenvio.jsonl"] = [registro("sReenvio", T, "/v4-fixtures/a.py")]

    _sessao("-v4-fixtures-s", "sSombra", T, CERTO)
    cenarios["sombra_modo.jsonl"] = [
        registro("sSombra", T, "/v4-fixtures/a.py", modo="sombra")
    ]

    _sessao("-v4-fixtures-n", "sSemPar", T, None)
    cenarios["sem_par.jsonl"] = [registro("sSemPar", T, "/v4-fixtures/a.py")]

    _sessao(
        "-v4-fixtures-f",
        "sFala",
        T,
        CERTO,
        fala="isso e um falso positivo: o simbolo existe no indice",
    )
    cenarios["fala_falso.jsonl"] = [registro("sFala", T, "/v4-fixtures/a.py")]

    (d / "vazio.jsonl").write_text("")
    for nome, rs in cenarios.items():
        (d / nome).write_text(
            "".join(json.dumps(r, ensure_ascii=False) + "\n" for r in rs)
        )
    print(f"{len(cenarios) + 1} fixtures em {d}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
