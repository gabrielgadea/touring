#!/usr/bin/env python3
"""Guard N5 — o classificador do KPI espelha o resolved_tokens do Rust (S1)
e as classes são HONESTAS (S2): bash_native / native_tool / code_route +
funil pós-deny. Se o espelho divergir do executor, o KPI mede uma classe
que o gate não vê; se a semântica divergir, o KPI mede a coisa errada."""
import json
import os
import sys

sys.path.insert(0, os.path.dirname(__file__))
from n5_injection_kpi import (  # noqa: E402
    classify_tool_call,
    code_route_share,
    deny_outcome,
    deny_route_rate,
    effective_tokens,
    is_read_bash,
    is_search_bash,
    iter_events,
    mine,
    resolved_tokens,
)

# Paridade com os casos S1 (cli_suggester_tests.rs::s1_wrappers_*): o verbo
# real decide — wrappers são transparentes.


def test_resolved_tokens_wrappers():
    assert resolved_tokens("time grep -rn foo src/")[0] == "grep"
    assert resolved_tokens("nice -n 5 find . -name x.rs")[0] == "find"
    assert resolved_tokens("sudo cat README.md")[0] == "cat"
    assert resolved_tokens("env FOO=1 rg pattern")[0] == "rg"
    assert resolved_tokens("FOO=1 time grep x")[0] == "grep"
    assert resolved_tokens("timeout 30 ls -la")[0] == "ls"
    assert resolved_tokens("sudo -u root env A=1 wc -l f")[0] == "wc"
    assert resolved_tokens("chrt -f 10 nice -n 5 grep x f")[0] == "grep"
    assert resolved_tokens("env") == []
    assert resolved_tokens("env FOO=1") == []
    assert resolved_tokens("time") == []


def test_effective_tokens_cd_prefix():
    # paridade com s1_prefixos_cd_e_sequencia_sao_transparentes (Rust)
    assert effective_tokens("cd /tmp && grep foo f")[0] == "grep"
    assert effective_tokens("cd /x; cat f")[0] == "cat"
    assert effective_tokens("cd /x\ncat f")[0] == "cat"
    assert effective_tokens("cd /x && cd /y && find . -name z")[0] == "find"
    assert effective_tokens("cd /x") == []
    assert effective_tokens("cd /x && cargo test")[0] == "cargo"
    assert is_search_bash("cd /tmp && grep p f")
    assert not is_search_bash("cd /tmp && cargo test")


def test_search_bash():
    assert is_search_bash("grep -rn foo src/")
    assert is_search_bash("time grep foo f")
    assert is_search_bash("find . -name '*.rs'")
    assert is_search_bash("sudo find . -name x")
    assert not is_search_bash("grep")
    assert not is_search_bash("find . -type f")
    assert not is_search_bash("ls -la")


def test_read_bash():
    assert is_read_bash("cat README.md")
    assert is_read_bash("head -5 f.rs")
    assert is_read_bash("sed -n '5,10p' f.rs")
    assert not is_read_bash("cat > out.txt <<'EOF'")  # escrita, não leitura
    assert not is_read_bash("sed -i 's/a/b/' f")  # escrita cega (G9), não leitura
    assert not is_read_bash("cat")


def test_classify_tool_call_classes_honestas():
    # S2: os rótulos dizem o que a classe É (não um veredito disfarçado)
    assert classify_tool_call("Grep", {"pattern": "x"}) == "native_tool"
    assert classify_tool_call("Read", {"file_path": "f"}) == "native_tool"
    assert classify_tool_call("Glob", {"pattern": "*.rs"}) == "native_tool"
    assert classify_tool_call("Bash", {"command": "grep -n foo f"}) == "bash_native"
    assert classify_tool_call("Bash", {"command": "cat f.rs"}) == "bash_native"
    assert classify_tool_call("Bash", {"command": "touring run --lang bash --code 'grep x f'"}) == "code_route"
    assert classify_tool_call("Bash", {"command": "cargo test"}) is None
    assert classify_tool_call("Edit", {"file_path": "f"}) is None


def test_code_route_share_e_deny_route_rate():
    assert code_route_share({"bash_native": 1, "native_tool": 1, "code_route": 2}) == 0.5
    assert code_route_share({"bash_native": 0, "native_tool": 0, "code_route": 0}) is None
    assert deny_route_rate({"deny_then_route": 3, "deny_then_native": 1}) == 0.75
    assert deny_route_rate({}) is None


def test_bypass_conta_a_parte(tmp_path):
    lines = [
        {"type": "assistant", "timestamp": "2026-08-26T10:00:00", "isSidechain": False,
         "message": {"content": [{"type": "tool_use", "name": "Bash", "input": {"command": "TOURING_GATE_OK=1 grep x f"}}]}},
        {"type": "assistant", "timestamp": "2026-08-26T10:01:00", "isSidechain": False,
         "message": {"content": [{"type": "tool_use", "name": "Bash", "input": {"command": "grep y f"}}]}},
    ]
    p = tmp_path / "s.jsonl"
    p.write_text("\n".join(json.dumps(l) for l in lines))
    per_session, per_day = mine([str(p)], None)
    counts = list(per_session.values())[0]
    assert counts.get("bypass") == 1
    # o bypass TAMBÉM classifica no eixo (o grep subjacente é bash_native)
    assert counts.get("bash_native") == 2
    assert per_day["2026-08-26"]["bypass"] == 1


def _call(cmd=None, name="Bash", ts="2026-08-26T10:00:00"):
    inp = {"command": cmd} if name == "Bash" else {"file_path": "f"}
    return {"type": "assistant", "timestamp": ts, "isSidechain": False,
            "message": {"content": [{"type": "tool_use", "name": name, "input": inp}]}}


def _deny(ts="2026-08-26T10:00:30", as_blocks=False):
    text = "[CODE MODE] inspeção `cat` é modelo-direta — a rota é o programa"
    content = ([{"type": "text", "text": text}] if as_blocks else text)
    return {"type": "user", "timestamp": ts,
            "message": {"content": [{"type": "tool_result", "content": content}]}}


def test_deny_funnel_todos_os_outcomes(tmp_path):
    lines = [
        _deny(ts="2026-08-26T10:00:30"),                       # deny 1
        _call("touring run --lang bash --code 'cat f'", ts="2026-08-26T10:01:00"),  # → route
        _deny(ts="2026-08-26T10:02:00"),                       # deny 2
        _call(name="Grep", ts="2026-08-26T10:02:30"),          # → native
        _deny(ts="2026-08-26T10:03:00"),                       # deny 3
        _call("cat outro.txt", ts="2026-08-26T10:03:30"),      # → reissue
        _deny(ts="2026-08-26T10:04:00"),                       # deny 4
        _call("TOURING_GATE_OK=1 cat f", ts="2026-08-26T10:04:30"),  # → bypass
        _deny(ts="2026-08-26T10:05:00"),                       # deny 5
        _call(name="Edit", ts="2026-08-26T10:05:30"),          # → other
        _deny(ts="2026-08-26T10:06:00"),                       # deny 6
        _deny(ts="2026-08-26T10:06:30"),                       # deny 7 (6→reissue)
        _call("cargo test", ts="2026-08-26T10:07:00"),         # 7 → other
    ]
    p = tmp_path / "s.jsonl"
    p.write_text("\n".join(json.dumps(l) for l in lines))
    per_session, _ = mine([str(p)], None)
    counts = list(per_session.values())[0]
    assert counts.get("deny_total") == 7
    assert counts.get("deny_then_route") == 1
    assert counts.get("deny_then_native") == 1
    assert counts.get("deny_then_reissue") == 2  # a re-emissão + deny→deny
    assert counts.get("deny_then_bypass") == 1
    assert counts.get("deny_then_other") == 2
    # outcomes cobrem todos os denies
    total_out = sum(counts.get(k, 0) for k in (
        "deny_then_route", "deny_then_native", "deny_then_reissue",
        "deny_then_bypass", "deny_then_other"))
    assert total_out == counts["deny_total"]


def test_deny_marker_em_blocos_e_deny_outcome_direto(tmp_path):
    lines = [
        _deny(as_blocks=True),
        _call("TOURING_CODE_MODE=native grep x f", ts="2026-08-26T10:01:00"),
    ]
    p = tmp_path / "s.jsonl"
    p.write_text("\n".join(json.dumps(l) for l in lines))
    events = list(iter_events(str(p)))
    assert [e[0] for e in events] == ["deny", "call"]
    per_session, _ = mine([str(p)], None)
    counts = list(per_session.values())[0]
    assert counts.get("deny_then_bypass") == 1
    # deny_outcome: bypass tem precedência sobre a classe do comando
    assert deny_outcome("Bash", {"command": "TOURING_GATE_OK=1 grep x"}) == "deny_then_bypass"
    assert deny_outcome("Bash", {"command": "touring run --lang python --code 'x'"}) == "deny_then_route"
    assert deny_outcome("Read", {"file_path": "f"}) == "deny_then_native"
    assert deny_outcome("Bash", {"command": "grep y f"}) == "deny_then_reissue"
    assert deny_outcome("Bash", {"command": "cargo build"}) == "deny_then_other"


def test_iter_events_filtra_sidechain_e_nao_assistant(tmp_path):
    lines = [
        _call("grep x f"),
        {"type": "assistant", "timestamp": "2026-08-26T10:01:00", "isSidechain": True,
         "message": {"content": [{"type": "tool_use", "name": "Bash", "input": {"command": "cat f"}}]}},
        {"type": "user", "timestamp": "2026-08-26T10:02:00",
         "message": {"content": [{"type": "tool_result", "content": "sem marker aqui"}]}},
    ]
    p = tmp_path / "s.jsonl"
    p.write_text("\n".join(json.dumps(l) for l in lines))
    events = list(iter_events(str(p)))
    assert len(events) == 1
    assert events[0][0] == "call"
