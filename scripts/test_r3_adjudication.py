"""S-5 adjudication tooling: blind packs, Cohen's κ, gold, tie-break queue."""
import json
import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parent))
import r3_adjudication as adj  # noqa: E402


def _row(n):
    """The keys the REAL extractor + freeze emit (r3_extract_a2_offers.extract_session + item_id/split).
    The first version of this fixture invented `b0`/`nudge_head`/`mode` — a shape no real row ever had — so
    every test passed while pack() crashed with KeyError on the real sample."""
    return {"item_id": f"a2-{n:05d}", "session": "4d95646f", "ts": "2026-09-10T10:00:00.000Z", "lang": "bash",
            "rota_proposta": "grep -n foo src/a; grep -n bar src/b", "janela_k": 3,
            "cmds": [f"grep -r foo src/{n}", "cat README.md", "touring run --lang bash --code 'ls'"], "n_cmds": 3,
            "cortada_em_nova_oferta": False, "b0_decisao": "followed" if n % 2 == 0 else "not_followed",
            "b0_valor": 1.0 if n % 2 == 0 else None, "split": "audit-test"}


SAMPLE = [_row(n) for n in range(20)]

# Built at runtime: no credential-shaped literal lives in this file (GitHub push protection reads the source).
PLANTED = {
    "api_key_sk": "sk-" + "a1B2" * 8,
    "aws_access_key": "AK" + "IA" + "A1B2C3D4E5F6G7H8",
    "github_token": "gh" + "p_" + "x9" * 18,
    "slack_token": "xo" + "xb-" + "1234567890-abcdef",
    "private_key_block": "-----BEGIN " + "RSA PRIVATE KEY-----",
    "bearer_token": "Authorization: Bearer " + "q" * 24,
    "jwt": "ey" + "J" + "a" * 12 + "." + "b" * 12 + "." + "c" * 12,
    "assignment_secret": "export API_KEY=" + "z8" * 8,
    "env_var_secret": "MYSERVICE_TOKEN=" + "z8" * 8,           # `_` is a word char: a leading \b never matches here
    "json_quoted_secret": '{"api_token": "' + "z8" * 8 + '"}',  # a quote sits between the key and the colon
}


def test_pack_never_leaks_the_b0_label(tmp_path):
    adj.pack(SAMPLE, tmp_path, seed=20261002)
    blind = (tmp_path / "rater_view" / "pack_blind.jsonl").read_text()
    for hidden in set(SAMPLE[0]) - set(adj.BLIND_FIELDS):  # every real key except the three the rater may see
        assert f'"{hidden}"' not in blind, hidden
    assert "not_followed" not in blind
    rows = [json.loads(line) for line in blind.splitlines()]
    assert {r["item_id"] for r in rows} == {s["item_id"] for s in SAMPLE}
    assert all(set(r) == {"item_id", "rota_proposta", "cmds"} for r in rows)
    # the hidden fields live ONLY in the key file, which raters never receive
    key = [json.loads(line) for line in (tmp_path / "key.jsonl").read_text().splitlines()]
    assert all("b0_decisao" in k for k in key)


def test_the_rater_view_directory_structurally_excludes_the_key(tmp_path):
    """Blindness by structure, not by instruction: a rater is pointed at `rater_view/` and nothing in it (nor
    next to a file in it) names the B0 decision. The key lives one level up."""
    adj.pack(SAMPLE, tmp_path, seed=20261002)
    assert sorted(p.name for p in (tmp_path / "rater_view").iterdir()) == ["INSTRUCOES.md", "pack_blind.jsonl"]
    assert (tmp_path / "key.jsonl").exists() and not (tmp_path / "rater_view" / "key.jsonl").exists()
    view = "".join(p.read_text() for p in (tmp_path / "rater_view").iterdir())
    assert "b0_decisao" not in view and "key.jsonl" not in view


def test_pack_accepts_what_the_real_extractor_emits(tmp_path):
    """Contract across components: the rows come from extract_session on a transcript, not from a hand-written dict."""
    import r3_extract_a2_offers as ex  # noqa: PLC0415

    deny = ("PreToolUse:Bash hook error: [CODE MODE · rajada] 2ª inspeção `grep` em 300s\n"
            "  touring run --lang bash --code 'grep -n a f; grep -n b g'")
    ev = lambda ts, content: json.dumps({"timestamp": ts, "message": {"content": content}})  # noqa: E731
    lines = [ev("2026-09-10T10:00:00Z", [{"type": "tool_result", "is_error": True, "content": deny}])]
    lines += [ev(f"2026-09-10T10:00:0{i}Z", [{"type": "tool_use", "name": "Bash", "input": {"command": c}}])
              for i, c in enumerate(["grep -n a f", "ls", "touring run --lang bash --code 'ls'"], 1)]
    (tmp_path / "sess.jsonl").write_text("\n".join(lines) + "\n")
    rows = [{**r, "item_id": f"a2-{i:05d}", "split": "audit-test"} for i, r in enumerate(ex.extract_session(tmp_path / "sess.jsonl"))]
    assert len(rows) == 1
    adj.pack(rows, tmp_path / "pk")
    blind = [json.loads(x) for x in (tmp_path / "pk" / "rater_view" / "pack_blind.jsonl").read_text().splitlines()]
    assert [set(b) for b in blind] == [{"item_id", "rota_proposta", "cmds"}]
    assert blind[0]["rota_proposta"] == "grep -n a f; grep -n b g" and len(blind[0]["cmds"]) == 3


def test_pack_refuses_a_row_without_a_route(tmp_path):
    """No recommended action ⇒ nothing to judge the commands against. Fail LOUD, never ship a blind item."""
    for bad in (None, "", "   "):
        with pytest.raises(ValueError, match="a2-00000"):
            adj.pack([dict(SAMPLE[0], rota_proposta=bad)], tmp_path / "x")


@pytest.mark.parametrize("name", sorted(PLANTED))
def test_secret_findings_detects_each_planted_credential(name):
    assert adj.secret_findings(f"curl -s https://x # {PLANTED[name]} end"), name  # positive control per pattern


def test_secret_findings_leaves_ordinary_commands_alone():
    for ok in ("grep -n token src/lib.rs", "touring run --lang bash --code 'ls'", "cat ~/.claude/settings.json | head",
               "git log --oneline -5", "export PATH=/usr/bin:$PATH"):
        assert adj.secret_findings(ok) == [], ok


def test_pack_withholds_an_item_with_a_credential_and_never_writes_it(tmp_path):
    leaky = dict(SAMPLE[3], cmds=SAMPLE[3]["cmds"][:2] + [f"curl -H 'X: {PLANTED['api_key_sk']}' https://x"])
    adj.pack([SAMPLE[0], leaky], tmp_path, seed=1)
    blind = (tmp_path / "rater_view" / "pack_blind.jsonl").read_text()
    assert "a2-00003" not in blind and PLANTED["api_key_sk"] not in blind and "a2-00000" in blind
    withheld = json.loads((tmp_path / "withheld.json").read_text())
    assert list(withheld) == ["a2-00003"] and withheld["a2-00003"]          # which item and WHICH pattern ...
    assert PLANTED["api_key_sk"] not in json.dumps(withheld)                 # ... never the credential itself
    assert PLANTED["api_key_sk"] not in (tmp_path / "key.jsonl").read_text()  # nor in the key file


def test_pack_scans_the_route_text_too_not_only_the_commands(tmp_path):
    """`rota_proposta` also goes to the external vendor: a credential quoted in the recommended program is withheld."""
    leaky = dict(SAMPLE[7], rota_proposta=f"curl -H 'X: {PLANTED['api_key_sk']}' https://x")
    adj.pack([SAMPLE[0], leaky], tmp_path, seed=1)
    assert PLANTED["api_key_sk"] not in (tmp_path / "rater_view" / "pack_blind.jsonl").read_text()
    assert list(json.loads((tmp_path / "withheld.json").read_text())) == ["a2-00007"]


def test_pack_scans_the_raw_text_not_its_json_escaping(tmp_path):
    """json.dumps escapes the quotes of an embedded JSON credential and hides it from the scan."""
    leaky = dict(SAMPLE[5], cmds=[f"echo '{PLANTED['json_quoted_secret']}' > /tmp/x"])
    adj.pack([SAMPLE[0], leaky], tmp_path, seed=1)
    assert "a2-00005" not in (tmp_path / "rater_view" / "pack_blind.jsonl").read_text()
    assert list(json.loads((tmp_path / "withheld.json").read_text())) == ["a2-00005"]


def test_pack_order_is_a_seeded_shuffle(tmp_path):
    a, b, c = tmp_path / "a", tmp_path / "b", tmp_path / "c"
    adj.pack(SAMPLE, a, seed=20261002)
    adj.pack(SAMPLE, b, seed=20261002)
    adj.pack(SAMPLE, c, seed=7)
    order = lambda d: [json.loads(x)["item_id"] for x in (d / "rater_view" / "pack_blind.jsonl").read_text().splitlines()]  # noqa: E731
    assert order(a) == order(b) and order(a) != order(c)
    assert order(a) != [s["item_id"] for s in SAMPLE]  # not the original order (blinds position effects)


def test_kappa_matches_a_hand_computed_case():
    # n=10 binary; p_o = 8/10, p_e = .5*.5 + .5*.5 = .5  =>  kappa = (0.8-0.5)/(1-0.5) = 0.6
    a = [1, 1, 1, 1, 1, 0, 0, 0, 0, 0]
    b = [1, 1, 1, 0, 1, 0, 0, 0, 1, 0]
    assert round(adj.cohen_kappa(a, b), 9) == 0.6
    assert adj.cohen_kappa([1, 0, 1, 0], [1, 0, 1, 0]) == 1.0
    assert adj.cohen_kappa([1, 1, 0, 0], [0, 0, 1, 1]) == -1.0


def _labels(pairs):
    """pairs: list of (rater_a, rater_b) labels -> (adj_a, adj_b) record lists"""
    ra = [{"item_id": f"i{n:03d}", "label": x} for n, (x, _) in enumerate(pairs)]
    rb = [{"item_id": f"i{n:03d}", "label": y} for n, (_, y) in enumerate(pairs)]
    return ra, rb


def test_low_kappa_flags_gold_insufficient():
    pairs = [("followed", "ignored"), ("ignored", "followed")] * 5 + [("followed", "followed"), ("ignored", "ignored")]
    ra, rb = _labels(pairs)
    res = adj.merge(ra, rb)
    assert res["kappa"] < 0.60 and res["gold_insufficient"] is True


def test_boundary_kappa_exactly_at_the_floor_is_sufficient():
    a = ["followed"] * 5 + ["ignored"] * 5
    b = ["followed", "followed", "followed", "ignored", "followed", "ignored", "ignored", "ignored", "followed", "ignored"]
    ra, rb = _labels(list(zip(a, b)))
    res = adj.merge(ra, rb)
    assert round(res["kappa"], 9) == 0.6 and res["gold_insufficient"] is False


def test_indeterminate_is_excluded_from_kappa_gold_and_counted():
    pairs = [("followed", "followed"), ("ignored", "ignored"), ("indeterminate", "followed"), ("followed", "indeterminate")]
    res = adj.merge(*_labels(pairs))
    assert res["n_indeterminate"] == 2 and res["n_scored"] == 2
    assert {g["item_id"] for g in res["gold"]} == {"i000", "i001"}


def test_bypass_and_ignored_both_collapse_to_not_followed():
    pairs = [("bypass", "ignored"), ("followed", "followed"), ("ignored", "bypass"), ("ignored", "ignored")]
    res = adj.merge(*_labels(pairs))
    assert res["tiebreak"] == []                      # bypass vs ignored is NOT a disagreement on the primary metric
    assert {g["item_id"]: g["followed"] for g in res["gold"]}["i000"] is False


def test_disagreements_go_to_the_tiebreak_queue_and_finalize_closes_them():
    pairs = [("followed", "followed")] * 4 + [("followed", "ignored")] + [("ignored", "ignored")] * 4
    res = adj.merge(*_labels(pairs))
    assert [t["item_id"] for t in res["tiebreak"]] == ["i004"]
    assert all(g["provenance"] == "agree" for g in res["gold"])
    final = adj.finalize(res, [{"item_id": "i004", "label": "followed"}])
    by = {g["item_id"]: g for g in final}
    assert by["i004"]["provenance"] == "tiebreak" and by["i004"]["followed"] is True and len(final) == 9
