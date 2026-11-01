# Mutant specs for items 3 (BOM), 4 (env why), 5 (context sensor), 6 (skill size guard).
G = "client/hooks/agent_model_gate.py"
R = "scripts/model_routing_adherence.py"
CC = "client/hooks/check_context.sh"
SM = "client/hooks/status_monitor.sh"
GS = "scripts/test_skill_size_guard.py"

VIS = "client/hooks/tests/test_hook_sources_visible.py"
GT = "client/hooks/tests/test_agent_model_gate.py"
RT = "scripts/test_model_routing_adherence.py"
SENSOR = ["client/hooks/tests/test_s42_context_sensor.py", "client/hooks/tests/test_s41_s42_s43.py"]

COPY = [
    "scripts/model_routing_adherence.py",
    "scripts/test_model_routing_adherence.py",
    "scripts/test_skill_size_guard.py",
    "docs/economia-de-creditos-e-contexto.md",
]

BOM_REGEX = 'r"\\A\\ufeff?---'  # the source text: backslash-u escape, six characters

MUTANTS = [
    # item 3 — the BOM spelled as a visible escape
    ("BOM back to a literal character", G, BOM_REGEX, 'r"\\A\ufeff?---', [VIS, GT]),
    ("BOM no longer optional", G, BOM_REGEX, 'r"\\A\\ufeff---', [VIS, GT]),
    ("BOM no longer accepted", G, BOM_REGEX, 'r"\\A---', [VIS, GT]),
    # item 4 — the env decision named in the log and counted in the report
    ("env why loses the variable name", G, 'WHY_ENV_PREFIX = "env:CLAUDE_CODE_SUBAGENT_MODEL="', 'WHY_ENV_PREFIX = "env:"',
     [GT, "scripts/test_model_routing_adherence.py", "-k", "env or gate or brief"]),
    ("env why loses the value", G, '(f"{WHY_ENV_PREFIX}{env_model}" if env_model else None)',
     '(f"{WHY_ENV_PREFIX}" if env_model else None)', [GT]),
    ("report ignores env decisions", R, "        if why.startswith(PREFIXO_WHY_ENV):\n            por_env[_valor_do_env(why)] += 1",
     "        if False:\n            por_env[_valor_do_env(why)] += 1", [RT, "-k", "env or gate or brief"]),
    ("report keeps the whole why as the value", R,
     '    return why.split("=", 1)[1] if "=" in why else why[len(PREFIXO_WHY_ENV) :]', "    return why", [RT, "-k", "env or gate or brief"]),
    ("report prefix drifts from the hook", R, 'PREFIXO_WHY_ENV = "env:"', 'PREFIXO_WHY_ENV = "envx:"',
     [RT, "-k", "env or gate or brief"]),
    ("report counts values not calls", R, '"total": sum(por_env.values()),', '"total": len(por_env),',
     [RT, "-k", "env or gate or brief"]),
    ("brief drops the env count", R, '    return f"{cabeca} ({detalhe}) · {_top(motivos, 4)} · decididas por env: {por_env}"',
     '    return f"{cabeca} ({detalhe}) · {_top(motivos, 4)}"', [RT, "-k", "env or gate or brief"]),
    # item 5 — the sensor, told apart
    ("heartbeat never written", SM, "    printf '%s %s\\n' \"$NOW\" \"$CONTEXT_INT\" > \"${ALERT_DIR}/${SESSION}.seen\"", "    :", SENSOR),
    ("flag loses its timestamp", SM, "        printf '%s\\nts=%s\\n' \"$CONTEXT_INT\" \"$NOW\" > \"${ALERT_DIR}/${SESSION}.pct\"",
     "        printf '%s\\n' \"$CONTEXT_INT\" > \"${ALERT_DIR}/${SESSION}.pct\"", SENSOR),
    ("stale sensor never detected", CC, "    elif (( SENSOR_AGE > STALE_SECS )); then", "    elif false; then", SENSOR),
    ("missing sensor reads as abaixo", CC, '        journal_silent sem_sensor "$SESSION" "" ""', '        journal_silent abaixo "$SESSION" "" ""', SENSOR),
    ("journal not deduplicated", CC, '    if [[ "$last_reason" == "$reason" ]] && (( NOW - last_at < JOURNAL_EVERY )); then', "    if false; then", SENSOR),
    ("injected text changes", CC, "Grave agora o estado em disco", "Grave já o estado em disco", SENSOR),
    ("delivered reads as abaixo", CC, '    journal_silent entregue "$SESSION" "$(head -n1 "$FLAG" | tr -cd \'0-9\')" "$SENSOR_AGE"',
     '    journal_silent abaixo "$SESSION" "$(head -n1 "$FLAG" | tr -cd \'0-9\')" "$SENSOR_AGE"', SENSOR),
    ("speaking not journalled", CC, '\njournal true "" "$SESSION" "$PCT" "$SENSOR_AGE"\n', "\n:\n", SENSOR),
    ("two-line flag breaks the percentage", CC, "PCT=$(head -n1 \"$FLAG\" | tr -cd '0-9')\n: > \"$DELIVERED\"",
     "PCT=$(tr -cd '0-9' < \"$FLAG\")\n: > \"$DELIVERED\"", SENSOR),
    ("missing session not journalled", CC, '    journal_silent sem_sessao "_nosession" "" "" "$detail"', "    :", SENSOR),
    ("project monitor not journalled", CC, '            journal_silent monitor_projeto "$SESSION" "$SEEN_PCT" "$SENSOR_AGE"', "            :", SENSOR),
    ("unreadable heartbeat reads as missing", CC,
     '        journal_silent obsoleto "$SESSION" "$SEEN_PCT" "" "$SENSOR_DETAIL"  # a heartbeat that cannot be read proves nothing',
     '        journal_silent sem_sensor "$SESSION" "$SEEN_PCT" "" "$SENSOR_DETAIL"', SENSOR),
    # item 6 — the guard no longer measures empty
    ("script mode approves with no roots", GS, "    if not raizes_presentes(SKILLS_DIRS):\n        print(", "    if False:\n        print(", [GS]),
    ("release cannot require the roots", GS, '        if os.environ.get("SKILL_SIZE_GUARD_REQUIRE_ROOTS") == "1":\n            pytest.fail(motivo)',
     '        if False:\n            pytest.fail(motivo)', [GS]),
    ("absent roots no longer skip", GS, "        pytest.skip(motivo)", "        pass", [GS]),
    ("absent roots count as present", GS, "if raiz.is_dir()]", "if True]", [GS]),
]
