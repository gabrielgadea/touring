# Inventário do ECC (rodada H, gerado por `scripts/inventario_ecc.py`)

601 entradas de catálogo. Lotes de leitura: H1-regras (122), H2a-skills-processo (148), H2b-skills-seguranca-contexto-linguagem (95), H3-agents-commands (156).

| tipo | classe | n |
|---|---|---|
| agent | contexto-custo-memoria | 5 |
| agent | dominio-nao-codigo | 6 |
| agent | outros | 4 |
| agent | processo | 35 |
| agent | seguranca | 21 |
| command | contexto-custo-memoria | 14 |
| command | dominio-nao-codigo | 6 |
| command | outros | 16 |
| command | processo | 50 |
| command | seguranca | 11 |
| rule | linguagem | 51 |
| rule | outros | 21 |
| rule | processo | 28 |
| rule | seguranca | 22 |
| skill | contexto-custo-memoria | 42 |
| skill | dominio-nao-codigo | 57 |
| skill | linguagem | 7 |
| skill | outros | 14 |
| skill | processo | 148 |
| skill | seguranca | 43 |

## Adaptadores de harness: gerados da fonte ou escritos à mão

O par é por NOME (homônimo, não par verificado); «ambíguos» tem mais de um candidato. O número robusto é o último: corpo comparado com todos os candidatos.

| diretório | arquivos | homônimo na fonte | ambíguos | iguais à fonte (≥ 0,9) |
|---|---|---|---|---|
| `.kiro` | 136 | 122 | 6 | 33 |
| `.cursor` | 53 | 19 | 1 | 7 |
| `.opencode` | 42 | 31 | 3 | 4 |
| `.agents` | 107 | 45 | 2 | 35 |
| `.codex` | 1 | 1 | 0 | 0 |
| `.codebuddy` | 2 | 1 | 0 | 0 |
| `.trae` | 2 | 1 | 0 | 0 |

## Outros diretórios

| diretório | arquivos | linhas | subdiretórios |
|---|---|---|---|
| `docker/context-profiles` | 204 | 9481 | Dockerfile, README.md, ai-corpus.json, ai-eval-lib.js, ai-eval.js, complex-corpus-v2.json, complex-corpus-v3.json, complex-corpus-v4.json, complex-corpus.json, complex-eval, example-task.json, legacy-source.json |
| `examples` | 67 | 5235 | CLAUDE.md, coordination-inventory, django-api-CLAUDE.md, eval-harness, evaluator-rag-prototype, gan-harness, go-microservice-CLAUDE.md, harmonyos-app-CLAUDE.md, hud-status-contract.json, laravel-api-CLAUDE.md, rails-app-CLAUDE.md, rust-api-CLAUDE.md |
| `src/llm` | 23 | 1993 | __init__.py, __main__.py, cli, core, prompt, providers, tools |
| `schemas` | 17 | 1837 | agent.schema.json, capsule-envelope.schema.json, context-carrier.schema.json, context-pack-registry.schema.json, context-profile.schema.json, ecc-install-config.schema.json, hooks-metadata.schema.json, hooks.schema.json, install-components.schema.json, install-modules.schema.json, install-profiles.schema.json, install-state.schema.json |
| `manifests` | 10 | 3155 | context-packs, context-profiles, install-assets, install-components.json, install-modules.json, install-profiles.json, pi-core.json |
| `ecc2` | 21 | 57996 | Cargo.lock, Cargo.toml, README.md, rust-toolchain.toml, src |
| `integrations` | 7 | 714 | aura |
| `plugins` | 3 | 185 | README.md, ecc |
| `hooks` | 9 | 1083 | README.md, codex-hooks.json, hermes, hooks.json, hooks.metadata.json, memory-persistence, qoder-hooks.json |

## Catálogo completo

| tipo | nome | classe | linguagens | linhas | caminho |
|---|---|---|---|---|---|
| skill | accessibility | processo |  | 147 | `skills/accessibility/SKILL.md` |
| skill | agent-architecture-audit | contexto-custo-memoria |  | 258 | `skills/agent-architecture-audit/SKILL.md` |
| skill | agent-eval | contexto-custo-memoria |  | 148 | `skills/agent-eval/SKILL.md` |
| skill | agent-harness-construction | processo |  | 75 | `skills/agent-harness-construction/SKILL.md` |
| skill | agent-introspection-debugging | processo |  | 155 | `skills/agent-introspection-debugging/SKILL.md` |
| skill | agent-payment-x402 | dominio-nao-codigo |  | 441 | `skills/agent-payment-x402/SKILL.md` |
| skill | agent-runtime-gateway-smoke-test | processo |  | 84 | `skills/agent-runtime-gateway-smoke-test/SKILL.md` |
| skill | agent-security-hardening | seguranca |  | 178 | `skills/agent-security-hardening/SKILL.md` |
| skill | agent-self-evaluation | processo |  | 183 | `skills/agent-self-evaluation/SKILL.md` |
| skill | agent-sort | processo |  | 217 | `skills/agent-sort/SKILL.md` |
| skill | agentic-engineering | contexto-custo-memoria |  | 65 | `skills/agentic-engineering/SKILL.md` |
| skill | agentic-os | contexto-custo-memoria |  | 389 | `skills/agentic-os/SKILL.md` |
| skill | ai-first-engineering | processo |  | 53 | `skills/ai-first-engineering/SKILL.md` |
| skill | ai-regression-testing | processo |  | 387 | `skills/ai-regression-testing/SKILL.md` |
| skill | android-clean-architecture | processo | kotlin | 341 | `skills/android-clean-architecture/SKILL.md` |
| skill | angular-developer | processo | angular | 156 | `skills/angular-developer/SKILL.md` |
| skill | api-connector-builder | processo |  | 122 | `skills/api-connector-builder/SKILL.md` |
| skill | api-design | processo |  | 525 | `skills/api-design/SKILL.md` |
| skill | architecture-decision-records | contexto-custo-memoria |  | 181 | `skills/architecture-decision-records/SKILL.md` |
| skill | article-writing | dominio-nao-codigo |  | 81 | `skills/article-writing/SKILL.md` |
| skill | automation-audit-ops | outros |  | 144 | `skills/automation-audit-ops/SKILL.md` |
| skill | autonomous-agent-harness | contexto-custo-memoria |  | 272 | `skills/autonomous-agent-harness/SKILL.md` |
| skill | autonomous-loops | processo |  | 612 | `skills/autonomous-loops/SKILL.md` |
| skill | backend-patterns | processo | node | 563 | `skills/backend-patterns/SKILL.md` |
| skill | benchmark | processo |  | 96 | `skills/benchmark/SKILL.md` |
| skill | benchmark-methodology | processo |  | 187 | `skills/benchmark-methodology/SKILL.md` |
| skill | benchmark-optimization-loop | contexto-custo-memoria |  | 72 | `skills/benchmark-optimization-loop/SKILL.md` |
| skill | bengali-nlp | dominio-nao-codigo |  | 463 | `skills/bengali-nlp/SKILL.md` |
| skill | blender-motion-state-inspection | processo |  | 166 | `skills/blender-motion-state-inspection/SKILL.md` |
| skill | blueprint | contexto-custo-memoria |  | 98 | `skills/blueprint/SKILL.md` |
| skill | brand-discovery | dominio-nao-codigo |  | 141 | `skills/brand-discovery/SKILL.md` |
| skill | brand-voice | dominio-nao-codigo |  | 99 | `skills/brand-voice/SKILL.md` |
| skill | brownfield-retrospective | processo |  | 178 | `skills/brownfield-retrospective/SKILL.md` |
| skill | browser-qa | processo |  | 106 | `skills/browser-qa/SKILL.md` |
| skill | bun-runtime | processo | node | 95 | `skills/bun-runtime/SKILL.md` |
| skill | canary-watch | processo |  | 109 | `skills/canary-watch/SKILL.md` |
| skill | carrier-relationship-management | dominio-nao-codigo |  | 207 | `skills/carrier-relationship-management/SKILL.md` |
| skill | change-impact | outros |  | 87 | `skills/change-impact/SKILL.md` |
| skill | cisco-ios-patterns | processo |  | 165 | `skills/cisco-ios-patterns/SKILL.md` |
| skill | ck | dominio-nao-codigo | node | 149 | `skills/ck/SKILL.md` |
| skill | claude-devfleet | processo |  | 113 | `skills/claude-devfleet/SKILL.md` |
| skill | click-path-audit | processo |  | 246 | `skills/click-path-audit/SKILL.md` |
| skill | clickhouse-io | processo |  | 446 | `skills/clickhouse-io/SKILL.md` |
| skill | code-tour | dominio-nao-codigo |  | 255 | `skills/code-tour/SKILL.md` |
| skill | codebase-onboarding | processo |  | 235 | `skills/codebase-onboarding/SKILL.md` |
| skill | codehealth-mcp | dominio-nao-codigo |  | 168 | `skills/codehealth-mcp/SKILL.md` |
| skill | coding-standards | processo |  | 552 | `skills/coding-standards/SKILL.md` |
| skill | competitive-platform-analysis | processo |  | 210 | `skills/competitive-platform-analysis/SKILL.md` |
| skill | competitive-report-structure | processo |  | 158 | `skills/competitive-report-structure/SKILL.md` |
| skill | compose-multiplatform-patterns | processo |  | 301 | `skills/compose-multiplatform-patterns/SKILL.md` |
| skill | config-gc | seguranca |  | 121 | `skills/config-gc/SKILL.md` |
| skill | configure-ecc | processo |  | 207 | `skills/configure-ecc/SKILL.md` |
| skill | connections-optimizer | dominio-nao-codigo |  | 191 | `skills/connections-optimizer/SKILL.md` |
| skill | content-engine | dominio-nao-codigo |  | 133 | `skills/content-engine/SKILL.md` |
| skill | content-hash-cache-pattern | dominio-nao-codigo |  | 163 | `skills/content-hash-cache-pattern/SKILL.md` |
| skill | context-and-decisions | contexto-custo-memoria |  | 73 | `skills/context-and-decisions/SKILL.md` |
| skill | context-budget | contexto-custo-memoria |  | 134 | `skills/context-budget/SKILL.md` |
| skill | continuous-agent-loop | processo |  | 47 | `skills/continuous-agent-loop/SKILL.md` |
| skill | continuous-learning | contexto-custo-memoria |  | 133 | `skills/continuous-learning/SKILL.md` |
| skill | continuous-learning-v2 | contexto-custo-memoria |  | 378 | `skills/continuous-learning-v2/SKILL.md` |
| skill | contract-first | processo |  | 288 | `skills/contract-first/SKILL.md` |
| skill | conventional-commit-review | processo |  | 107 | `skills/conventional-commit-review/SKILL.md` |
| skill | cost-aware-llm-pipeline | contexto-custo-memoria |  | 189 | `skills/cost-aware-llm-pipeline/SKILL.md` |
| skill | cost-tracking | contexto-custo-memoria |  | 108 | `skills/cost-tracking/SKILL.md` |
| skill | council | linguagem | go | 205 | `skills/council/SKILL.md` |
| skill | council-multi-model | contexto-custo-memoria |  | 168 | `skills/council-multi-model/SKILL.md` |
| skill | counterparty-channel-discipline | dominio-nao-codigo |  | 171 | `skills/counterparty-channel-discipline/SKILL.md` |
| skill | cpp-coding-standards | processo | cpp | 725 | `skills/cpp-coding-standards/SKILL.md` |
| skill | cpp-testing | seguranca | cpp | 326 | `skills/cpp-testing/SKILL.md` |
| skill | crosspost | dominio-nao-codigo |  | 124 | `skills/crosspost/SKILL.md` |
| skill | csharp-testing | processo | csharp | 323 | `skills/csharp-testing/SKILL.md` |
| skill | customer-billing-ops | processo |  | 142 | `skills/customer-billing-ops/SKILL.md` |
| skill | customs-trade-compliance | processo |  | 257 | `skills/customs-trade-compliance/SKILL.md` |
| skill | dart-flutter-patterns | seguranca | dart, flutter | 565 | `skills/dart-flutter-patterns/SKILL.md` |
| skill | dashboard-builder | processo |  | 110 | `skills/dashboard-builder/SKILL.md` |
| skill | data-scraper-agent | processo |  | 777 | `skills/data-scraper-agent/SKILL.md` |
| skill | data-throughput-accelerator | processo |  | 75 | `skills/data-throughput-accelerator/SKILL.md` |
| skill | database-migrations | processo | django, golang | 431 | `skills/database-migrations/SKILL.md` |
| skill | deep-research | processo |  | 171 | `skills/deep-research/SKILL.md` |
| skill | deepseek-harness-setup | contexto-custo-memoria |  | 90 | `skills/deepseek-harness-setup/SKILL.md` |
| skill | defi-amm-security | seguranca |  | 168 | `skills/defi-amm-security/SKILL.md` |
| skill | delivery-gate | contexto-custo-memoria |  | 130 | `skills/delivery-gate/SKILL.md` |
| skill | deployment-patterns | dominio-nao-codigo |  | 429 | `skills/deployment-patterns/SKILL.md` |
| skill | design-system | contexto-custo-memoria |  | 84 | `skills/design-system/SKILL.md` |
| skill | dev-team | dominio-nao-codigo |  | 204 | `skills/dev-team/SKILL.md` |
| skill | django-celery | processo | django | 459 | `skills/django-celery/SKILL.md` |
| skill | django-patterns | processo | django | 736 | `skills/django-patterns/SKILL.md` |
| skill | django-security | seguranca | django | 645 | `skills/django-security/SKILL.md` |
| skill | django-tdd | processo | django | 731 | `skills/django-tdd/SKILL.md` |
| skill | django-verification | seguranca | django | 471 | `skills/django-verification/SKILL.md` |
| skill | dmux-workflows | contexto-custo-memoria |  | 193 | `skills/dmux-workflows/SKILL.md` |
| skill | docker-patterns | seguranca |  | 560 | `skills/docker-patterns/SKILL.md` |
| skill | docs-governance | processo |  | 98 | `skills/docs-governance/SKILL.md` |
| skill | documentation-lookup | contexto-custo-memoria | react | 92 | `skills/documentation-lookup/SKILL.md` |
| skill | dotnet-patterns | processo | dotnet | 323 | `skills/dotnet-patterns/SKILL.md` |
| skill | dynamic-workflow-mode | processo |  | 125 | `skills/dynamic-workflow-mode/SKILL.md` |
| skill | e2e-testing | processo |  | 328 | `skills/e2e-testing/SKILL.md` |
| skill | ecc-guide | contexto-custo-memoria |  | 191 | `skills/ecc-guide/SKILL.md` |
| skill | ecc-recipes | processo |  | 151 | `skills/ecc-recipes/SKILL.md` |
| skill | ecc-tools-cost-audit | contexto-custo-memoria |  | 162 | `skills/ecc-tools-cost-audit/SKILL.md` |
| skill | email-ops | dominio-nao-codigo |  | 134 | `skills/email-ops/SKILL.md` |
| skill | energy-procurement | contexto-custo-memoria |  | 222 | `skills/energy-procurement/SKILL.md` |
| skill | enterprise-agent-ops | seguranca |  | 52 | `skills/enterprise-agent-ops/SKILL.md` |
| skill | error-handling | linguagem | go, python, typescript | 378 | `skills/error-handling/SKILL.md` |
| skill | esign-field-placement | contexto-custo-memoria |  | 200 | `skills/esign-field-placement/SKILL.md` |
| skill | eval-harness | contexto-custo-memoria |  | 306 | `skills/eval-harness/SKILL.md` |
| skill | evm-token-decimals | contexto-custo-memoria |  | 132 | `skills/evm-token-decimals/SKILL.md` |
| skill | exa-search | outros |  | 118 | `skills/exa-search/SKILL.md` |
| skill | fal-ai-media | dominio-nao-codigo |  | 295 | `skills/fal-ai-media/SKILL.md` |
| skill | fastapi-patterns | processo |  | 515 | `skills/fastapi-patterns/SKILL.md` |
| skill | finance-billing-ops | dominio-nao-codigo |  | 129 | `skills/finance-billing-ops/SKILL.md` |
| skill | flox-environments | processo |  | 498 | `skills/flox-environments/SKILL.md` |
| skill | flutter-dart-code-review | seguranca | dart, flutter | 437 | `skills/flutter-dart-code-review/SKILL.md` |
| skill | foundation-models-on-device | outros |  | 244 | `skills/foundation-models-on-device/SKILL.md` |
| skill | frontend-a11y | processo | react | 444 | `skills/frontend-a11y/SKILL.md` |
| skill | frontend-design-direction | processo |  | 173 | `skills/frontend-design-direction/SKILL.md` |
| skill | frontend-patterns | processo | react | 658 | `skills/frontend-patterns/SKILL.md` |
| skill | frontend-slides | dominio-nao-codigo |  | 186 | `skills/frontend-slides/SKILL.md` |
| skill | fsharp-testing | processo |  | 282 | `skills/fsharp-testing/SKILL.md` |
| skill | gan-style-harness | processo |  | 280 | `skills/gan-style-harness/SKILL.md` |
| skill | gateguard | processo |  | 712 | `skills/gateguard/SKILL.md` |
| skill | generating-python-installer | processo | python | 821 | `skills/generating-python-installer/SKILL.md` |
| skill | git-workflow | processo |  | 717 | `skills/git-workflow/SKILL.md` |
| skill | github-ops | seguranca |  | 163 | `skills/github-ops/SKILL.md` |
| skill | golang-patterns | processo | go, golang | 677 | `skills/golang-patterns/SKILL.md` |
| skill | golang-testing | processo | go, golang | 722 | `skills/golang-testing/SKILL.md` |
| skill | google-workspace-ops | processo |  | 97 | `skills/google-workspace-ops/SKILL.md` |
| skill | growth-log | contexto-custo-memoria |  | 129 | `skills/growth-log/SKILL.md` |
| skill | healthcare-cdss-patterns | dominio-nao-codigo |  | 247 | `skills/healthcare-cdss-patterns/SKILL.md` |
| skill | healthcare-emr-patterns | seguranca |  | 161 | `skills/healthcare-emr-patterns/SKILL.md` |
| skill | healthcare-eval-harness | seguranca |  | 209 | `skills/healthcare-eval-harness/SKILL.md` |
| skill | healthcare-phi-compliance | seguranca |  | 147 | `skills/healthcare-phi-compliance/SKILL.md` |
| skill | hermes-imports | seguranca |  | 90 | `skills/hermes-imports/SKILL.md` |
| skill | hexagonal-architecture | processo | go, java, kotlin, typescript | 278 | `skills/hexagonal-architecture/SKILL.md` |
| skill | hipaa-compliance | seguranca |  | 80 | `skills/hipaa-compliance/SKILL.md` |
| skill | homelab-network-readiness | processo |  | 171 | `skills/homelab-network-readiness/SKILL.md` |
| skill | homelab-network-setup | processo |  | 131 | `skills/homelab-network-setup/SKILL.md` |
| skill | homelab-pihole-dns | outros |  | 276 | `skills/homelab-pihole-dns/SKILL.md` |
| skill | homelab-vlan-segmentation | outros |  | 313 | `skills/homelab-vlan-segmentation/SKILL.md` |
| skill | homelab-wireguard-vpn | outros |  | 307 | `skills/homelab-wireguard-vpn/SKILL.md` |
| skill | hookify-rules | processo |  | 150 | `skills/hookify-rules/SKILL.md` |
| skill | i18n-sync | processo |  | 142 | `skills/i18n-sync/SKILL.md` |
| skill | inherit-legacy-style | processo |  | 158 | `skills/inherit-legacy-style/SKILL.md` |
| skill | intent-driven-development | seguranca |  | 361 | `skills/intent-driven-development/SKILL.md` |
| skill | inventory-demand-planning | seguranca |  | 241 | `skills/inventory-demand-planning/SKILL.md` |
| skill | investor-materials | dominio-nao-codigo |  | 98 | `skills/investor-materials/SKILL.md` |
| skill | investor-outreach | dominio-nao-codigo |  | 93 | `skills/investor-outreach/SKILL.md` |
| skill | ios-icon-gen | dominio-nao-codigo |  | 159 | `skills/ios-icon-gen/SKILL.md` |
| skill | iterative-retrieval | contexto-custo-memoria |  | 213 | `skills/iterative-retrieval/SKILL.md` |
| skill | ito-baskets | processo |  | 264 | `skills/ito-baskets/SKILL.md` |
| skill | ito-compute | processo | node | 166 | `skills/ito-compute/SKILL.md` |
| skill | ito-inference | processo |  | 120 | `skills/ito-inference/SKILL.md` |
| skill | ito-training | processo |  | 124 | `skills/ito-training/SKILL.md` |
| skill | java-coding-standards | processo | java, spring | 385 | `skills/java-coding-standards/SKILL.md` |
| skill | jira-integration | processo |  | 313 | `skills/jira-integration/SKILL.md` |
| skill | jpa-patterns | linguagem | spring | 153 | `skills/jpa-patterns/SKILL.md` |
| skill | knowledge-ops | contexto-custo-memoria |  | 156 | `skills/knowledge-ops/SKILL.md` |
| skill | kotlin-coroutines-flows | processo | kotlin | 286 | `skills/kotlin-coroutines-flows/SKILL.md` |
| skill | kotlin-exposed-patterns | processo | kotlin | 721 | `skills/kotlin-exposed-patterns/SKILL.md` |
| skill | kotlin-ktor-patterns | processo | kotlin | 691 | `skills/kotlin-ktor-patterns/SKILL.md` |
| skill | kotlin-patterns | seguranca | kotlin | 713 | `skills/kotlin-patterns/SKILL.md` |
| skill | kotlin-testing | processo | kotlin | 826 | `skills/kotlin-testing/SKILL.md` |
| skill | kubernetes-patterns | seguranca |  | 757 | `skills/kubernetes-patterns/SKILL.md` |
| skill | laravel-patterns | processo | laravel | 417 | `skills/laravel-patterns/SKILL.md` |
| skill | laravel-plugin-discovery | dominio-nao-codigo | laravel, php | 231 | `skills/laravel-plugin-discovery/SKILL.md` |
| skill | laravel-security | seguranca | laravel | 951 | `skills/laravel-security/SKILL.md` |
| skill | laravel-tdd | processo | laravel | 676 | `skills/laravel-tdd/SKILL.md` |
| skill | laravel-verification | seguranca | laravel | 181 | `skills/laravel-verification/SKILL.md` |
| skill | latency-critical-systems | processo |  | 76 | `skills/latency-critical-systems/SKILL.md` |
| skill | lead-intelligence | dominio-nao-codigo |  | 334 | `skills/lead-intelligence/SKILL.md` |
| skill | liquid-glass-design | processo |  | 280 | `skills/liquid-glass-design/SKILL.md` |
| skill | living-docs-governance | contexto-custo-memoria |  | 138 | `skills/living-docs-governance/SKILL.md` |
| skill | llm-trading-agent-security | seguranca |  | 148 | `skills/llm-trading-agent-security/SKILL.md` |
| skill | logistics-exception-management | dominio-nao-codigo |  | 217 | `skills/logistics-exception-management/SKILL.md` |
| skill | loop-design-check | processo |  | 144 | `skills/loop-design-check/SKILL.md` |
| skill | mailtrap-email-integration | dominio-nao-codigo |  | 78 | `skills/mailtrap-email-integration/SKILL.md` |
| skill | make-interfaces-feel-better | processo |  | 153 | `skills/make-interfaces-feel-better/SKILL.md` |
| skill | manim-video | dominio-nao-codigo |  | 91 | `skills/manim-video/SKILL.md` |
| skill | market-research | dominio-nao-codigo |  | 88 | `skills/market-research/SKILL.md` |
| skill | marketing-campaign | dominio-nao-codigo |  | 115 | `skills/marketing-campaign/SKILL.md` |
| skill | master-agreement-generator | processo |  | 231 | `skills/master-agreement-generator/SKILL.md` |
| skill | mcp-dependency-review | processo |  | 178 | `skills/mcp-dependency-review/SKILL.md` |
| skill | mcp-server-patterns | contexto-custo-memoria | node, typescript | 71 | `skills/mcp-server-patterns/SKILL.md` |
| skill | messages-ops | processo |  | 106 | `skills/messages-ops/SKILL.md` |
| skill | ml-adoption-playbook | contexto-custo-memoria |  | 58 | `skills/ml-adoption-playbook/SKILL.md` |
| skill | mle-workflow | contexto-custo-memoria |  | 349 | `skills/mle-workflow/SKILL.md` |
| skill | module-regression | outros |  | 100 | `skills/module-regression/SKILL.md` |
| skill | motion-advanced | processo | react | 598 | `skills/motion-advanced/SKILL.md` |
| skill | motion-foundations | seguranca | react, spring | 301 | `skills/motion-foundations/SKILL.md` |
| skill | motion-patterns | contexto-custo-memoria | react | 436 | `skills/motion-patterns/SKILL.md` |
| skill | mybatis-patterns | seguranca | java, spring | 415 | `skills/mybatis-patterns/SKILL.md` |
| skill | mysql-patterns | outros |  | 414 | `skills/mysql-patterns/SKILL.md` |
| skill | nanoclaw-repl | contexto-custo-memoria |  | 35 | `skills/nanoclaw-repl/SKILL.md` |
| skill | nasiko-control-plane | seguranca |  | 50 | `skills/nasiko-control-plane/SKILL.md` |
| skill | nestjs-patterns | processo | typescript | 232 | `skills/nestjs-patterns/SKILL.md` |
| skill | netmiko-ssh-automation | linguagem | python | 175 | `skills/netmiko-ssh-automation/SKILL.md` |
| skill | network-bgp-diagnostics | processo |  | 169 | `skills/network-bgp-diagnostics/SKILL.md` |
| skill | network-config-validation | seguranca |  | 212 | `skills/network-config-validation/SKILL.md` |
| skill | network-interface-health | dominio-nao-codigo |  | 154 | `skills/network-interface-health/SKILL.md` |
| skill | nextjs-turbopack | dominio-nao-codigo | nextjs, rust | 59 | `skills/nextjs-turbopack/SKILL.md` |
| skill | nodejs-keccak256 | linguagem | javascript, node, typescript | 104 | `skills/nodejs-keccak256/SKILL.md` |
| skill | nutrient-document-processing | dominio-nao-codigo |  | 169 | `skills/nutrient-document-processing/SKILL.md` |
| skill | nuxt4-patterns | seguranca |  | 102 | `skills/nuxt4-patterns/SKILL.md` |
| skill | openclaw-persona-forge | dominio-nao-codigo |  | 290 | `skills/openclaw-persona-forge/SKILL.md` |
| skill | opensource-pipeline | seguranca |  | 285 | `skills/opensource-pipeline/SKILL.md` |
| skill | operator-approval-loop | processo |  | 239 | `skills/operator-approval-loop/SKILL.md` |
| skill | orch-add-feature | dominio-nao-codigo |  | 46 | `skills/orch-add-feature/SKILL.md` |
| skill | orch-build-mvp | processo |  | 50 | `skills/orch-build-mvp/SKILL.md` |
| skill | orch-change-feature | processo |  | 44 | `skills/orch-change-feature/SKILL.md` |
| skill | orch-fix-defect | processo |  | 44 | `skills/orch-fix-defect/SKILL.md` |
| skill | orch-pipeline | processo |  | 122 | `skills/orch-pipeline/SKILL.md` |
| skill | orch-refine-code | processo |  | 45 | `skills/orch-refine-code/SKILL.md` |
| skill | osint-investigation | dominio-nao-codigo |  | 373 | `skills/osint-investigation/SKILL.md` |
| skill | parallel-execution-optimizer | processo |  | 75 | `skills/parallel-execution-optimizer/SKILL.md` |
| skill | perl-patterns | processo |  | 506 | `skills/perl-patterns/SKILL.md` |
| skill | perl-security | seguranca |  | 505 | `skills/perl-security/SKILL.md` |
| skill | perl-testing | processo |  | 477 | `skills/perl-testing/SKILL.md` |
| skill | plan-canvas | processo |  | 199 | `skills/plan-canvas/SKILL.md` |
| skill | plan-orchestrate | processo |  | 260 | `skills/plan-orchestrate/SKILL.md` |
| skill | plankton-code-quality | processo |  | 329 | `skills/plankton-code-quality/SKILL.md` |
| skill | postgres-patterns | seguranca |  | 149 | `skills/postgres-patterns/SKILL.md` |
| skill | prediction-market-oracle-research | processo |  | 65 | `skills/prediction-market-oracle-research/SKILL.md` |
| skill | prediction-market-risk-review | seguranca |  | 62 | `skills/prediction-market-risk-review/SKILL.md` |
| skill | prisma-patterns | processo | typescript | 402 | `skills/prisma-patterns/SKILL.md` |
| skill | product-capability | processo |  | 143 | `skills/product-capability/SKILL.md` |
| skill | product-lens | processo | go | 94 | `skills/product-lens/SKILL.md` |
| skill | production-audit | processo |  | 208 | `skills/production-audit/SKILL.md` |
| skill | production-scheduling | outros |  | 232 | `skills/production-scheduling/SKILL.md` |
| skill | project-flow-ops | processo |  | 113 | `skills/project-flow-ops/SKILL.md` |
| skill | prompt-caching-strategy | outros |  | 188 | `skills/prompt-caching-strategy/SKILL.md` |
| skill | prompt-optimizer | contexto-custo-memoria |  | 388 | `skills/prompt-optimizer/SKILL.md` |
| skill | python-patterns | processo | python | 752 | `skills/python-patterns/SKILL.md` |
| skill | python-testing | processo | python | 818 | `skills/python-testing/SKILL.md` |
| skill | pytorch-patterns | contexto-custo-memoria |  | 398 | `skills/pytorch-patterns/SKILL.md` |
| skill | quality-nonconformance | processo |  | 254 | `skills/quality-nonconformance/SKILL.md` |
| skill | quarkus-patterns | processo | java | 724 | `skills/quarkus-patterns/SKILL.md` |
| skill | quarkus-security | seguranca |  | 469 | `skills/quarkus-security/SKILL.md` |
| skill | quarkus-tdd | processo |  | 813 | `skills/quarkus-tdd/SKILL.md` |
| skill | quarkus-verification | seguranca |  | 482 | `skills/quarkus-verification/SKILL.md` |
| skill | rails-patterns | processo | rails, ruby | 476 | `skills/rails-patterns/SKILL.md` |
| skill | ralphinho-rfc-pipeline | processo |  | 69 | `skills/ralphinho-rfc-pipeline/SKILL.md` |
| skill | react-native-patterns | seguranca | react | 327 | `skills/react-native-patterns/SKILL.md` |
| skill | react-patterns | processo | react | 343 | `skills/react-patterns/SKILL.md` |
| skill | react-performance | processo | react | 576 | `skills/react-performance/SKILL.md` |
| skill | react-testing | processo | react | 425 | `skills/react-testing/SKILL.md` |
| skill | recsys-pipeline-architect | dominio-nao-codigo |  | 116 | `skills/recsys-pipeline-architect/SKILL.md` |
| skill | recursive-decision-ledger | processo |  | 82 | `skills/recursive-decision-ledger/SKILL.md` |
| skill | redis-patterns | processo |  | 405 | `skills/redis-patterns/SKILL.md` |
| skill | regex-vs-llm-structured-text | contexto-custo-memoria |  | 222 | `skills/regex-vs-llm-structured-text/SKILL.md` |
| skill | remotion-video-creation | dominio-nao-codigo | react | 44 | `skills/remotion-video-creation/SKILL.md` |
| skill | repo-scan | processo |  | 171 | `skills/repo-scan/SKILL.md` |
| skill | research-ops | contexto-custo-memoria |  | 114 | `skills/research-ops/SKILL.md` |
| skill | returns-reverse-logistics | processo |  | 234 | `skills/returns-reverse-logistics/SKILL.md` |
| skill | rules-distill | outros |  | 266 | `skills/rules-distill/SKILL.md` |
| skill | rust-patterns | processo | rust | 501 | `skills/rust-patterns/SKILL.md` |
| skill | rust-testing | processo | rust | 502 | `skills/rust-testing/SKILL.md` |
| skill | safety-guard | seguranca |  | 77 | `skills/safety-guard/SKILL.md` |
| skill | santa-method | dominio-nao-codigo |  | 308 | `skills/santa-method/SKILL.md` |
| skill | scientific-db-pubmed-database | dominio-nao-codigo |  | 177 | `skills/scientific-db-pubmed-database/SKILL.md` |
| skill | scientific-db-uspto-database | outros |  | 179 | `skills/scientific-db-uspto-database/SKILL.md` |
| skill | scientific-pkg-gget | linguagem | python | 168 | `skills/scientific-pkg-gget/SKILL.md` |
| skill | scientific-thinking-literature-review | dominio-nao-codigo |  | 194 | `skills/scientific-thinking-literature-review/SKILL.md` |
| skill | scientific-thinking-scholar-evaluation | processo |  | 162 | `skills/scientific-thinking-scholar-evaluation/SKILL.md` |
| skill | search-first | processo |  | 184 | `skills/search-first/SKILL.md` |
| skill | security-bounty-hunter | seguranca |  | 101 | `skills/security-bounty-hunter/SKILL.md` |
| skill | security-review | seguranca |  | 512 | `skills/security-review/SKILL.md` |
| skill | security-scan | seguranca |  | 167 | `skills/security-scan/SKILL.md` |
| skill | seo | dominio-nao-codigo |  | 156 | `skills/seo/SKILL.md` |
| skill | skill-comply | processo |  | 60 | `skills/skill-comply/SKILL.md` |
| skill | skill-host-compat | processo |  | 103 | `skills/skill-host-compat/SKILL.md` |
| skill | skill-scout | processo |  | 142 | `skills/skill-scout/SKILL.md` |
| skill | skill-stocktake | processo |  | 247 | `skills/skill-stocktake/SKILL.md` |
| skill | social-graph-ranker | dominio-nao-codigo |  | 156 | `skills/social-graph-ranker/SKILL.md` |
| skill | social-publisher | dominio-nao-codigo |  | 140 | `skills/social-publisher/SKILL.md` |
| skill | springboot-patterns | processo | java, spring | 316 | `skills/springboot-patterns/SKILL.md` |
| skill | springboot-security | seguranca | java, spring | 274 | `skills/springboot-security/SKILL.md` |
| skill | springboot-tdd | processo | spring | 160 | `skills/springboot-tdd/SKILL.md` |
| skill | springboot-verification | seguranca | spring | 233 | `skills/springboot-verification/SKILL.md` |
| skill | story-lifecycle | processo |  | 386 | `skills/story-lifecycle/SKILL.md` |
| skill | strategic-compact | contexto-custo-memoria |  | 157 | `skills/strategic-compact/SKILL.md` |
| skill | swift-actor-persistence | seguranca | swift | 145 | `skills/swift-actor-persistence/SKILL.md` |
| skill | swift-concurrency-6-2 | linguagem | swift | 217 | `skills/swift-concurrency-6-2/SKILL.md` |
| skill | swift-protocol-di-testing | processo | swift | 192 | `skills/swift-protocol-di-testing/SKILL.md` |
| skill | swiftui-patterns | processo |  | 260 | `skills/swiftui-patterns/SKILL.md` |
| skill | taste | dominio-nao-codigo |  | 265 | `skills/taste/SKILL.md` |
| skill | taste-application | dominio-nao-codigo |  | 353 | `skills/taste-application/SKILL.md` |
| skill | taste-distillation | dominio-nao-codigo |  | 201 | `skills/taste-distillation/SKILL.md` |
| skill | tasteforge-video | dominio-nao-codigo |  | 257 | `skills/tasteforge-video/SKILL.md` |
| skill | tdd-workflow | processo |  | 584 | `skills/tdd-workflow/SKILL.md` |
| skill | team-agent-orchestration | processo |  | 112 | `skills/team-agent-orchestration/SKILL.md` |
| skill | team-builder | dominio-nao-codigo |  | 170 | `skills/team-builder/SKILL.md` |
| skill | terminal-opener | contexto-custo-memoria |  | 56 | `skills/terminal-opener/SKILL.md` |
| skill | terminal-ops | processo |  | 111 | `skills/terminal-ops/SKILL.md` |
| skill | test-collaboration | processo |  | 171 | `skills/test-collaboration/SKILL.md` |
| skill | tinystruct-patterns | processo | java | 401 | `skills/tinystruct-patterns/SKILL.md` |
| skill | token-budget-advisor | dominio-nao-codigo |  | 123 | `skills/token-budget-advisor/SKILL.md` |
| skill | token-card | contexto-custo-memoria |  | 122 | `skills/token-card/SKILL.md` |
| skill | ui-demo | dominio-nao-codigo |  | 467 | `skills/ui-demo/SKILL.md` |
| skill | ui-to-vue | processo | vue | 136 | `skills/ui-to-vue/SKILL.md` |
| skill | uncloud | processo |  | 345 | `skills/uncloud/SKILL.md` |
| skill | unified-memory | dominio-nao-codigo |  | 200 | `skills/unified-memory/SKILL.md` |
| skill | unified-notifications-ops | outros |  | 189 | `skills/unified-notifications-ops/SKILL.md` |
| skill | verification-loop | seguranca |  | 136 | `skills/verification-loop/SKILL.md` |
| skill | video-editing | dominio-nao-codigo |  | 338 | `skills/video-editing/SKILL.md` |
| skill | videodb | dominio-nao-codigo | python | 376 | `skills/videodb/SKILL.md` |
| skill | visa-doc-translate | dominio-nao-codigo |  | 118 | `skills/visa-doc-translate/SKILL.md` |
| skill | vite-patterns | processo |  | 451 | `skills/vite-patterns/SKILL.md` |
| skill | vue-patterns | processo | vue | 472 | `skills/vue-patterns/SKILL.md` |
| skill | windows-desktop-e2e | processo |  | 889 | `skills/windows-desktop-e2e/SKILL.md` |
| skill | workspace-surface-audit | processo |  | 127 | `skills/workspace-surface-audit/SKILL.md` |
| skill | x-api | dominio-nao-codigo |  | 245 | `skills/x-api/SKILL.md` |
| agent | a11y-architect | processo |  | 150 | `agents/a11y-architect.md` |
| agent | agent-evaluator | processo |  | 216 | `agents/agent-evaluator.md` |
| agent | architect | processo |  | 221 | `agents/architect.md` |
| agent | bengali-reviewer | dominio-nao-codigo |  | 211 | `agents/bengali-reviewer.md` |
| agent | build-error-resolver | processo | typescript | 124 | `agents/build-error-resolver.md` |
| agent | chief-of-staff | dominio-nao-codigo |  | 161 | `agents/chief-of-staff.md` |
| agent | code-architect | processo |  | 81 | `agents/code-architect.md` |
| agent | code-explorer | processo |  | 79 | `agents/code-explorer.md` |
| agent | code-reviewer | seguranca |  | 324 | `agents/code-reviewer.md` |
| agent | code-simplifier | outros |  | 57 | `agents/code-simplifier.md` |
| agent | comment-analyzer | outros |  | 55 | `agents/comment-analyzer.md` |
| agent | commit-reviewer | processo |  | 118 | `agents/commit-reviewer.md` |
| agent | conversation-analyzer | processo |  | 62 | `agents/conversation-analyzer.md` |
| agent | cpp-build-resolver | processo | cpp | 100 | `agents/cpp-build-resolver.md` |
| agent | cpp-reviewer | seguranca | cpp | 82 | `agents/cpp-reviewer.md` |
| agent | csharp-reviewer | seguranca | csharp | 111 | `agents/csharp-reviewer.md` |
| agent | dart-build-resolver | processo | dart, flutter | 211 | `agents/dart-build-resolver.md` |
| agent | database-reviewer | seguranca |  | 101 | `agents/database-reviewer.md` |
| agent | django-build-resolver | dominio-nao-codigo | django, python | 253 | `agents/django-build-resolver.md` |
| agent | django-reviewer | seguranca | django | 170 | `agents/django-reviewer.md` |
| agent | doc-updater | processo |  | 117 | `agents/doc-updater.md` |
| agent | docs-lookup | contexto-custo-memoria |  | 78 | `agents/docs-lookup.md` |
| agent | e2e-runner | dominio-nao-codigo |  | 117 | `agents/e2e-runner.md` |
| agent | fastapi-reviewer | seguranca |  | 80 | `agents/fastapi-reviewer.md` |
| agent | flutter-reviewer | processo | dart, flutter | 253 | `agents/flutter-reviewer.md` |
| agent | fsharp-reviewer | seguranca |  | 110 | `agents/fsharp-reviewer.md` |
| agent | gan-evaluator | processo |  | 233 | `agents/gan-evaluator.md` |
| agent | gan-generator | processo |  | 141 | `agents/gan-generator.md` |
| agent | gan-planner | processo |  | 109 | `agents/gan-planner.md` |
| agent | go-build-resolver | processo | go | 104 | `agents/go-build-resolver.md` |
| agent | go-reviewer | processo | go | 86 | `agents/go-reviewer.md` |
| agent | harmonyos-app-resolver | processo | harmonyos | 183 | `agents/harmonyos-app-resolver.md` |
| agent | harness-optimizer | contexto-custo-memoria |  | 56 | `agents/harness-optimizer.md` |
| agent | healthcare-reviewer | seguranca |  | 93 | `agents/healthcare-reviewer.md` |
| agent | homelab-architect | processo |  | 108 | `agents/homelab-architect.md` |
| agent | java-build-resolver | processo | java, spring | 276 | `agents/java-build-resolver.md` |
| agent | java-reviewer | seguranca | java, spring | 191 | `agents/java-reviewer.md` |
| agent | kotlin-build-resolver | processo | kotlin | 128 | `agents/kotlin-build-resolver.md` |
| agent | kotlin-reviewer | seguranca | kotlin | 169 | `agents/kotlin-reviewer.md` |
| agent | loop-operator | processo |  | 46 | `agents/loop-operator.md` |
| agent | marketing-agent | dominio-nao-codigo |  | 160 | `agents/marketing-agent.md` |
| agent | mle-reviewer | contexto-custo-memoria |  | 163 | `agents/mle-reviewer.md` |
| agent | network-architect | processo |  | 107 | `agents/network-architect.md` |
| agent | network-config-reviewer | seguranca |  | 107 | `agents/network-config-reviewer.md` |
| agent | network-troubleshooter | outros |  | 129 | `agents/network-troubleshooter.md` |
| agent | opensource-forker | seguranca |  | 208 | `agents/opensource-forker.md` |
| agent | opensource-packager | seguranca |  | 259 | `agents/opensource-packager.md` |
| agent | opensource-sanitizer | seguranca |  | 401 | `agents/opensource-sanitizer.md` |
| agent | performance-optimizer | contexto-custo-memoria |  | 456 | `agents/performance-optimizer.md` |
| agent | php-reviewer | seguranca | php | 110 | `agents/php-reviewer.md` |
| agent | planner | processo |  | 222 | `agents/planner.md` |
| agent | pr-test-analyzer | processo |  | 55 | `agents/pr-test-analyzer.md` |
| agent | python-reviewer | seguranca | python | 108 | `agents/python-reviewer.md` |
| agent | pytorch-build-resolver | processo |  | 130 | `agents/pytorch-build-resolver.md` |
| agent | rag-pipeline-reviewer | processo |  | 68 | `agents/rag-pipeline-reviewer.md` |
| agent | react-build-resolver | processo | react | 216 | `agents/react-build-resolver.md` |
| agent | react-reviewer | seguranca | react | 168 | `agents/react-reviewer.md` |
| agent | refactor-cleaner | processo |  | 95 | `agents/refactor-cleaner.md` |
| agent | ruby-reviewer | seguranca | rails, ruby | 113 | `agents/ruby-reviewer.md` |
| agent | rust-build-resolver | processo | rust | 158 | `agents/rust-build-resolver.md` |
| agent | rust-reviewer | processo | rust | 104 | `agents/rust-reviewer.md` |
| agent | security-reviewer | seguranca |  | 118 | `agents/security-reviewer.md` |
| agent | seo-specialist | dominio-nao-codigo |  | 72 | `agents/seo-specialist.md` |
| agent | silent-failure-hunter | processo |  | 60 | `agents/silent-failure-hunter.md` |
| agent | spec-miner | processo |  | 218 | `agents/spec-miner.md` |
| agent | swift-build-resolver | processo | swift | 171 | `agents/swift-build-resolver.md` |
| agent | swift-reviewer | contexto-custo-memoria | swift | 117 | `agents/swift-reviewer.md` |
| agent | tdd-guide | processo |  | 101 | `agents/tdd-guide.md` |
| agent | type-design-analyzer | outros |  | 51 | `agents/type-design-analyzer.md` |
| agent | typescript-reviewer | seguranca | javascript, node, typescript | 125 | `agents/typescript-reviewer.md` |
| agent | vue-reviewer | seguranca | vue | 207 | `agents/vue-reviewer.md` |
| command | aside | dominio-nao-codigo |  | 165 | `commands/aside.md` |
| command | astra-review | processo |  | 125 | `commands/astra-review.md` |
| command | auto-update | processo |  | 29 | `commands/auto-update.md` |
| command | build-fix | processo |  | 67 | `commands/build-fix.md` |
| command | checkpoint | processo |  | 79 | `commands/checkpoint.md` |
| command | code-review | processo |  | 290 | `commands/code-review.md` |
| command | cost-report | contexto-custo-memoria |  | 82 | `commands/cost-report.md` |
| command | cpp-build | processo | cpp | 174 | `commands/cpp-build.md` |
| command | cpp-review | seguranca | cpp | 133 | `commands/cpp-review.md` |
| command | cpp-test | processo | cpp | 252 | `commands/cpp-test.md` |
| command | ecc-guide | processo |  | 94 | `commands/ecc-guide.md` |
| command | epic-claim | outros |  | 27 | `commands/epic-claim.md` |
| command | epic-decompose | outros |  | 24 | `commands/epic-decompose.md` |
| command | epic-publish | outros |  | 24 | `commands/epic-publish.md` |
| command | epic-review | processo |  | 24 | `commands/epic-review.md` |
| command | epic-sync | outros |  | 24 | `commands/epic-sync.md` |
| command | epic-unblock | outros |  | 23 | `commands/epic-unblock.md` |
| command | epic-validate | outros |  | 23 | `commands/epic-validate.md` |
| command | evolve | contexto-custo-memoria |  | 179 | `commands/evolve.md` |
| command | fastapi-review | seguranca |  | 40 | `commands/fastapi-review.md` |
| command | feature-dev | processo |  | 50 | `commands/feature-dev.md` |
| command | flutter-build | processo | dart, flutter | 165 | `commands/flutter-build.md` |
| command | flutter-review | seguranca | dart, flutter | 117 | `commands/flutter-review.md` |
| command | flutter-test | processo | dart, flutter | 145 | `commands/flutter-test.md` |
| command | gan-build | processo |  | 104 | `commands/gan-build.md` |
| command | gan-design | processo |  | 40 | `commands/gan-design.md` |
| command | go-build | processo | go | 184 | `commands/go-build.md` |
| command | go-review | seguranca | go | 149 | `commands/go-review.md` |
| command | go-test | processo | go | 269 | `commands/go-test.md` |
| command | gradle-build | processo |  | 71 | `commands/gradle-build.md` |
| command | harness-audit | seguranca |  | 85 | `commands/harness-audit.md` |
| command | hookify-configure | outros |  | 15 | `commands/hookify-configure.md` |
| command | hookify-help | outros |  | 60 | `commands/hookify-help.md` |
| command | hookify-list | outros |  | 22 | `commands/hookify-list.md` |
| command | hookify | outros |  | 53 | `commands/hookify.md` |
| command | instinct-export | contexto-custo-memoria |  | 67 | `commands/instinct-export.md` |
| command | instinct-import | contexto-custo-memoria |  | 115 | `commands/instinct-import.md` |
| command | instinct-status | contexto-custo-memoria |  | 60 | `commands/instinct-status.md` |
| command | jira | processo |  | 107 | `commands/jira.md` |
| command | kotlin-build | processo | kotlin | 175 | `commands/kotlin-build.md` |
| command | kotlin-review | seguranca | kotlin | 141 | `commands/kotlin-review.md` |
| command | kotlin-test | processo | kotlin | 313 | `commands/kotlin-test.md` |
| command | learn-eval | contexto-custo-memoria |  | 154 | `commands/learn-eval.md` |
| command | learn | contexto-custo-memoria |  | 108 | `commands/learn.md` |
| command | loop-start | seguranca |  | 37 | `commands/loop-start.md` |
| command | loop-status | processo |  | 78 | `commands/loop-status.md` |
| command | marketing-campaign | dominio-nao-codigo |  | 130 | `commands/marketing-campaign.md` |
| command | model-route | contexto-custo-memoria |  | 31 | `commands/model-route.md` |
| command | multi-backend | outros |  | 165 | `commands/multi-backend.md` |
| command | multi-execute | processo |  | 322 | `commands/multi-execute.md` |
| command | multi-frontend | outros |  | 165 | `commands/multi-frontend.md` |
| command | multi-plan | processo |  | 275 | `commands/multi-plan.md` |
| command | multi-workflow | processo |  | 198 | `commands/multi-workflow.md` |
| command | orch-add-feature | dominio-nao-codigo |  | 37 | `commands/orch-add-feature.md` |
| command | orch-build-mvp | processo |  | 37 | `commands/orch-build-mvp.md` |
| command | orch-change-feature | processo |  | 39 | `commands/orch-change-feature.md` |
| command | orch-fix-defect | processo |  | 39 | `commands/orch-fix-defect.md` |
| command | orch-refine-code | processo |  | 40 | `commands/orch-refine-code.md` |
| command | orch-review | processo |  | 120 | `commands/orch-review.md` |
| command | plan-canvas | processo |  | 46 | `commands/plan-canvas.md` |
| command | plan-prd | processo |  | 163 | `commands/plan-prd.md` |
| command | plan | processo |  | 207 | `commands/plan.md` |
| command | pm2 | outros |  | 277 | `commands/pm2.md` |
| command | pr | processo |  | 185 | `commands/pr.md` |
| command | project-init | processo |  | 87 | `commands/project-init.md` |
| command | projects | contexto-custo-memoria |  | 40 | `commands/projects.md` |
| command | promote | contexto-custo-memoria |  | 42 | `commands/promote.md` |
| command | prp-commit | processo |  | 113 | `commands/prp-commit.md` |
| command | prp-implement | processo |  | 386 | `commands/prp-implement.md` |
| command | prp-plan | processo |  | 503 | `commands/prp-plan.md` |
| command | prp-pr | processo |  | 185 | `commands/prp-pr.md` |
| command | prp-prd | processo |  | 448 | `commands/prp-prd.md` |
| command | prune | contexto-custo-memoria |  | 32 | `commands/prune.md` |
| command | python-review | seguranca | python | 298 | `commands/python-review.md` |
| command | quality-gate | outros |  | 53 | `commands/quality-gate.md` |
| command | react-build | processo | react | 188 | `commands/react-build.md` |
| command | react-review | seguranca | react, typescript | 171 | `commands/react-review.md` |
| command | react-test | processo | react | 266 | `commands/react-test.md` |
| command | refactor-clean | processo |  | 85 | `commands/refactor-clean.md` |
| command | resume-session | dominio-nao-codigo |  | 182 | `commands/resume-session.md` |
| command | review-pr | processo |  | 38 | `commands/review-pr.md` |
| command | rust-build | processo | rust | 188 | `commands/rust-build.md` |
| command | rust-review | processo | rust | 143 | `commands/rust-review.md` |
| command | rust-test | processo | rust | 309 | `commands/rust-test.md` |
| command | santa-loop | processo |  | 209 | `commands/santa-loop.md` |
| command | save-session | dominio-nao-codigo |  | 276 | `commands/save-session.md` |
| command | security-scan | seguranca |  | 93 | `commands/security-scan.md` |
| command | sessions | contexto-custo-memoria |  | 340 | `commands/sessions.md` |
| command | setup-pm | outros |  | 81 | `commands/setup-pm.md` |
| command | skill-create | outros |  | 179 | `commands/skill-create.md` |
| command | skill-health | dominio-nao-codigo |  | 59 | `commands/skill-health.md` |
| command | skill-host-compat | processo |  | 41 | `commands/skill-host-compat.md` |
| command | test-coverage | processo |  | 74 | `commands/test-coverage.md` |
| command | token-card | contexto-custo-memoria |  | 29 | `commands/token-card.md` |
| command | update-codemaps | contexto-custo-memoria |  | 77 | `commands/update-codemaps.md` |
| command | update-docs | processo |  | 89 | `commands/update-docs.md` |
| command | vue-review | seguranca | typescript, vue | 175 | `commands/vue-review.md` |
| rule | README.md | outros |  | 146 | `rules/README.md` |
| rule | angular/coding-style.md | linguagem | angular | 183 | `rules/angular/coding-style.md` |
| rule | angular/hooks.md | linguagem | angular | 26 | `rules/angular/hooks.md` |
| rule | angular/patterns.md | linguagem | angular | 250 | `rules/angular/patterns.md` |
| rule | angular/security.md | seguranca | angular | 88 | `rules/angular/security.md` |
| rule | angular/testing.md | processo | angular | 165 | `rules/angular/testing.md` |
| rule | arkts/coding-style.md | outros |  | 154 | `rules/arkts/coding-style.md` |
| rule | arkts/hooks.md | outros |  | 136 | `rules/arkts/hooks.md` |
| rule | arkts/patterns.md | outros |  | 237 | `rules/arkts/patterns.md` |
| rule | arkts/security.md | seguranca |  | 142 | `rules/arkts/security.md` |
| rule | arkts/testing.md | processo |  | 127 | `rules/arkts/testing.md` |
| rule | common/agents.md | processo |  | 69 | `rules/common/agents.md` |
| rule | common/code-review.md | processo |  | 125 | `rules/common/code-review.md` |
| rule | common/coding-style.md | outros |  | 98 | `rules/common/coding-style.md` |
| rule | common/development-workflow.md | outros |  | 45 | `rules/common/development-workflow.md` |
| rule | common/git-workflow.md | processo |  | 25 | `rules/common/git-workflow.md` |
| rule | common/hooks.md | outros |  | 31 | `rules/common/hooks.md` |
| rule | common/patterns.md | outros |  | 32 | `rules/common/patterns.md` |
| rule | common/performance.md | processo |  | 61 | `rules/common/performance.md` |
| rule | common/security.md | seguranca |  | 30 | `rules/common/security.md` |
| rule | common/testing.md | processo |  | 58 | `rules/common/testing.md` |
| rule | cpp/coding-style.md | linguagem | cpp | 45 | `rules/cpp/coding-style.md` |
| rule | cpp/hooks.md | linguagem | cpp | 40 | `rules/cpp/hooks.md` |
| rule | cpp/patterns.md | linguagem | cpp | 52 | `rules/cpp/patterns.md` |
| rule | cpp/security.md | seguranca | cpp | 52 | `rules/cpp/security.md` |
| rule | cpp/testing.md | processo | cpp | 45 | `rules/cpp/testing.md` |
| rule | csharp/coding-style.md | linguagem | csharp | 73 | `rules/csharp/coding-style.md` |
| rule | csharp/hooks.md | linguagem | csharp | 26 | `rules/csharp/hooks.md` |
| rule | csharp/patterns.md | linguagem | csharp | 51 | `rules/csharp/patterns.md` |
| rule | csharp/security.md | seguranca | csharp | 59 | `rules/csharp/security.md` |
| rule | csharp/testing.md | processo | csharp | 47 | `rules/csharp/testing.md` |
| rule | dart/coding-style.md | linguagem | dart | 160 | `rules/dart/coding-style.md` |
| rule | dart/hooks.md | linguagem | dart | 67 | `rules/dart/hooks.md` |
| rule | dart/patterns.md | linguagem | dart | 262 | `rules/dart/patterns.md` |
| rule | dart/security.md | seguranca | dart | 136 | `rules/dart/security.md` |
| rule | dart/testing.md | processo | dart | 216 | `rules/dart/testing.md` |
| rule | fsharp/coding-style.md | outros |  | 113 | `rules/fsharp/coding-style.md` |
| rule | fsharp/hooks.md | outros |  | 27 | `rules/fsharp/hooks.md` |
| rule | fsharp/patterns.md | outros |  | 112 | `rules/fsharp/patterns.md` |
| rule | fsharp/security.md | seguranca |  | 77 | `rules/fsharp/security.md` |
| rule | fsharp/testing.md | processo |  | 63 | `rules/fsharp/testing.md` |
| rule | golang/coding-style.md | linguagem | golang | 33 | `rules/golang/coding-style.md` |
| rule | golang/hooks.md | linguagem | golang | 18 | `rules/golang/hooks.md` |
| rule | golang/patterns.md | linguagem | golang | 46 | `rules/golang/patterns.md` |
| rule | golang/security.md | seguranca | golang | 35 | `rules/golang/security.md` |
| rule | golang/testing.md | processo | golang | 32 | `rules/golang/testing.md` |
| rule | java/coding-style.md | linguagem | java | 115 | `rules/java/coding-style.md` |
| rule | java/hooks.md | linguagem | java | 19 | `rules/java/hooks.md` |
| rule | java/patterns.md | linguagem | java | 148 | `rules/java/patterns.md` |
| rule | java/security.md | seguranca | java | 102 | `rules/java/security.md` |
| rule | java/testing.md | processo | java | 134 | `rules/java/testing.md` |
| rule | kotlin/coding-style.md | linguagem | kotlin | 87 | `rules/kotlin/coding-style.md` |
| rule | kotlin/hooks.md | linguagem | kotlin | 18 | `rules/kotlin/hooks.md` |
| rule | kotlin/patterns.md | linguagem | kotlin | 147 | `rules/kotlin/patterns.md` |
| rule | kotlin/security.md | seguranca | kotlin | 83 | `rules/kotlin/security.md` |
| rule | kotlin/testing.md | processo | kotlin | 129 | `rules/kotlin/testing.md` |
| rule | nuxt/coding-style.md | outros |  | 48 | `rules/nuxt/coding-style.md` |
| rule | nuxt/hooks.md | outros |  | 40 | `rules/nuxt/hooks.md` |
| rule | nuxt/patterns.md | outros |  | 55 | `rules/nuxt/patterns.md` |
| rule | nuxt/security.md | seguranca |  | 49 | `rules/nuxt/security.md` |
| rule | nuxt/testing.md | processo |  | 50 | `rules/nuxt/testing.md` |
| rule | perl/coding-style.md | outros |  | 47 | `rules/perl/coding-style.md` |
| rule | perl/hooks.md | outros |  | 23 | `rules/perl/hooks.md` |
| rule | perl/patterns.md | outros |  | 77 | `rules/perl/patterns.md` |
| rule | perl/security.md | seguranca |  | 70 | `rules/perl/security.md` |
| rule | perl/testing.md | processo |  | 55 | `rules/perl/testing.md` |
| rule | php/coding-style.md | linguagem | php | 41 | `rules/php/coding-style.md` |
| rule | php/hooks.md | linguagem | php | 25 | `rules/php/hooks.md` |
| rule | php/patterns.md | linguagem | php | 34 | `rules/php/patterns.md` |
| rule | php/security.md | seguranca | php | 38 | `rules/php/security.md` |
| rule | php/testing.md | processo | php | 40 | `rules/php/testing.md` |
| rule | python/coding-style.md | linguagem | python | 43 | `rules/python/coding-style.md` |
| rule | python/fastapi.md | linguagem | python | 59 | `rules/python/fastapi.md` |
| rule | python/hooks.md | linguagem | python | 20 | `rules/python/hooks.md` |
| rule | python/patterns.md | linguagem | python | 40 | `rules/python/patterns.md` |
| rule | python/security.md | seguranca | python | 31 | `rules/python/security.md` |
| rule | python/testing.md | processo | python | 39 | `rules/python/testing.md` |
| rule | react/coding-style.md | linguagem | react | 110 | `rules/react/coding-style.md` |
| rule | react/hooks.md | linguagem | react | 188 | `rules/react/hooks.md` |
| rule | react/patterns.md | linguagem | react | 195 | `rules/react/patterns.md` |
| rule | react/security.md | seguranca | react | 181 | `rules/react/security.md` |
| rule | react/testing.md | processo | react | 209 | `rules/react/testing.md` |
| rule | react-native/accessibility.md | linguagem | react | 56 | `rules/react-native/accessibility.md` |
| rule | react-native/coding-style.md | linguagem | react | 72 | `rules/react-native/coding-style.md` |
| rule | react-native/hooks.md | linguagem | react | 29 | `rules/react-native/hooks.md` |
| rule | react-native/patterns.md | linguagem | react | 89 | `rules/react-native/patterns.md` |
| rule | react-native/performance.md | processo | react | 46 | `rules/react-native/performance.md` |
| rule | react-native/production-readiness.md | linguagem | react | 52 | `rules/react-native/production-readiness.md` |
| rule | react-native/security.md | seguranca | react | 44 | `rules/react-native/security.md` |
| rule | react-native/testing.md | processo | react | 53 | `rules/react-native/testing.md` |
| rule | ruby/coding-style.md | linguagem | ruby | 47 | `rules/ruby/coding-style.md` |
| rule | ruby/hooks.md | linguagem | ruby | 38 | `rules/ruby/hooks.md` |
| rule | ruby/patterns.md | linguagem | ruby | 45 | `rules/ruby/patterns.md` |
| rule | ruby/security.md | seguranca | ruby | 52 | `rules/ruby/security.md` |
| rule | ruby/testing.md | processo | ruby | 52 | `rules/ruby/testing.md` |
| rule | rust/coding-style.md | linguagem | rust | 152 | `rules/rust/coding-style.md` |
| rule | rust/hooks.md | linguagem | rust | 17 | `rules/rust/hooks.md` |
| rule | rust/patterns.md | linguagem | rust | 169 | `rules/rust/patterns.md` |
| rule | rust/security.md | seguranca | rust | 142 | `rules/rust/security.md` |
| rule | rust/testing.md | processo | rust | 155 | `rules/rust/testing.md` |
| rule | swift/coding-style.md | linguagem | swift | 48 | `rules/swift/coding-style.md` |
| rule | swift/hooks.md | linguagem | swift | 21 | `rules/swift/hooks.md` |
| rule | swift/patterns.md | linguagem | swift | 67 | `rules/swift/patterns.md` |
| rule | swift/security.md | seguranca | swift | 34 | `rules/swift/security.md` |
| rule | swift/testing.md | processo | swift | 46 | `rules/swift/testing.md` |
| rule | typescript/coding-style.md | linguagem | typescript | 200 | `rules/typescript/coding-style.md` |
| rule | typescript/hooks.md | linguagem | typescript | 23 | `rules/typescript/hooks.md` |
| rule | typescript/patterns.md | linguagem | typescript | 53 | `rules/typescript/patterns.md` |
| rule | typescript/security.md | seguranca | typescript | 29 | `rules/typescript/security.md` |
| rule | typescript/testing.md | processo | typescript | 19 | `rules/typescript/testing.md` |
| rule | vue/coding-style.md | linguagem | vue | 55 | `rules/vue/coding-style.md` |
| rule | vue/hooks.md | linguagem | vue | 46 | `rules/vue/hooks.md` |
| rule | vue/patterns.md | linguagem | vue | 57 | `rules/vue/patterns.md` |
| rule | vue/security.md | seguranca | vue | 47 | `rules/vue/security.md` |
| rule | vue/testing.md | processo | vue | 54 | `rules/vue/testing.md` |
| rule | web/coding-style.md | outros |  | 109 | `rules/web/coding-style.md` |
| rule | web/design-quality.md | outros |  | 76 | `rules/web/design-quality.md` |
| rule | web/hooks.md | outros |  | 142 | `rules/web/hooks.md` |
| rule | web/patterns.md | outros |  | 92 | `rules/web/patterns.md` |
| rule | web/performance.md | processo |  | 77 | `rules/web/performance.md` |
| rule | web/security.md | seguranca |  | 70 | `rules/web/security.md` |
| rule | web/testing.md | processo |  | 68 | `rules/web/testing.md` |
