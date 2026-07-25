# Touring CLI Commands — Quick Reference for Planning

## Discovery Commands (Phase 1: Scout)

```bash
touring doctor -j                      # System health pre-flight
touring status -j                      # Unified dashboard
touring e2e -j                         # Quick E2E health
touring e2e --depth standard -j        # Standard depth (30 files)
touring e2e --depth deep -j            # Deep (all files + temporal)
touring index find <symbol> -j         # Find symbol definition
touring index status -j                # Index health
touring index files "<pattern>" -j     # List indexed files
touring ast overview <file> -j         # Symbol map of file
touring ast blast <file> -j            # Blast radius analysis
touring ast find <symbol> -j           # AST symbol lookup
touring wiring status -j               # Wiring health summary
touring wiring orphans -j              # Orphaned pub symbols
touring wiring modules -j              # Module integration scores
touring wiring score <file> -j         # File integration score
touring wiring audit -j                # Full wiring audit
touring memory recall "<query>" -j     # Search knowledge graph
touring memory list --limit 20 -j      # List memory entries
touring memory stats -j                # Memory DB statistics
touring gotcha match <file> -j         # Pitfalls for a file
touring gotcha stats -j                # Gotcha statistics
touring evolution drift -j             # Detect drift
touring evolution insights -j          # Strategic insights
touring evolution tools -j             # Tool effectiveness
touring cognitive metrics -j           # Cognitive engine metrics
touring cognitive engines -j           # Engine health
touring incremental status -j          # Parser cache health
touring flywheel status -j             # Component health
```

## Planning Commands (Phase 2-4: Architect + Decompose)

```bash
touring session start <id> <type> "<objective>"   # Start session
touring session list -j                            # Active sessions
touring session assess <id> -j                     # Session quality
touring session checkpoint <id> "<notes>" -j       # Save checkpoint
touring decompose create <type> "<description>"    # Create task DAG
touring decompose add <task_id> <sub_id> "<desc>" [deps]  # Add subtask
touring decompose validate <task_id> -j            # Validate DAG (cycles)
touring decompose get <task_id> -j                 # Get DAG status
touring decompose status -j                        # Overall status
touring mcts search <root_state> -j                # Multi-path decision
touring suggest next "<query>" -j                  # RL-guided next action
touring suggest skill "<query>" -j                 # Skill recommendation
touring classify-intent                            # CILA classification
touring learning status -j                         # RL engine status
```

## Validation Commands (Phase 5-6: Engineer + Audit)

```bash
touring shadow validate -j             # Speculative validation
touring gotcha add "<pattern>" "<desc>" --severity <low|medium|high>
touring memory store "<key>" "<value>" --tier semantic --type pattern
touring learning reward <tool> <val> "<context>"   # RL reward
```

## MCP Tools for Planning

```
mcp__touring__touring_classify_intent      # CILA L0-L6 routing
mcp__touring__touring_session              # Session lifecycle
mcp__touring__touring_decompose            # DAG management
mcp__touring__touring_mcts_search          # Monte Carlo planning
mcp__touring__touring_suggest              # RL suggestions
mcp__touring__touring_memory_recall        # Knowledge search
mcp__touring__touring_memory_store         # Persist lessons
mcp__touring__touring_ast_find             # Symbol lookup (VGP)
mcp__touring__touring_ast_overview         # File symbol map
mcp__touring__touring_graph                # Dependencies/blast
mcp__touring__touring_speculate            # Shadow validation
mcp__touring__touring_wiring_audit         # Wiring health
mcp__touring__touring_wiring_orphans       # Orphan detection
mcp__touring__touring_evolution_status     # Evolution health
mcp__touring__touring_evolution_drift      # Drift detection
mcp__touring__touring_insights             # Tool effectiveness
mcp__touring__touring_gotcha              # Pitfall lookup
mcp__touring__touring_scan_pii            # PII safety check
mcp__touring__touring_checkpoint          # Durable checkpoint
mcp__touring__touring_cluster_skills      # Skill similarity
mcp__sequential-thinking__sequentialthinking  # Structured reasoning
```
