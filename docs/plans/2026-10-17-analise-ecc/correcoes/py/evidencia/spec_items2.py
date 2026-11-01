GS = "scripts/test_skill_size_guard.py"
COPY = ["scripts/test_skill_size_guard.py"]
MUTANTS = [
    ("release cannot require the roots", GS, '        if os.environ.get("SKILL_SIZE_GUARD_REQUIRE_ROOTS") == "1":\n            pytest.fail(motivo)',
     '        if False:\n            pytest.fail(motivo)', [GS]),
    ("release requires roots but env var misread", GS, 'os.environ.get("SKILL_SIZE_GUARD_REQUIRE_ROOTS") == "1"',
     'os.environ.get("SKILL_SIZE_GUARD_REQUIRE_ROOTS") == "0"', [GS]),
]
