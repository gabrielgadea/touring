# Fontes de 27/10/2026 (plano v3)

Evidência das consultas citadas no `PLANO.md` v3 (§1.4) como `q.<id>`:

| arquivo | origem | pergunta |
|---|---|---|
| `cc1.json` | notebook "Claude Code " `65c3ecb1-a4ce-4901-9864-a2b28c04b469` (criado 25/09/2026; 94 fontes em 27/10) | `updatedToolOutput`: versão, ferramentas nativas, limite |
| `cc2.json` | idem | o que o modelo vê num deny; boas práticas de mensagem |
| `cc3.json` | idem | microcompactação / evicção de tool results; o que hooks podem na compactação |
| `cc4.json` | idem | limites do Read; saída grande de Bash (Output Diet) |
| `cc5.json` | idem | avaliar hooks/CLAUDE.md por desfecho; `claude plugin eval` |
| `gh1.json` | notebook "GitHub Repos" `0f7f9b3e-0376-4e96-9f76-502dfe20206c` (107 fontes) | compressão de saída (rtk, context-mode, LeanCTX…) |
| `gh2.json` | idem | medir mudança de harness por desfecho |
| `gh3.json` | idem | mensagens de deny que ensinam a rota; recuperação |
| `cc6.json` | notebook "Claude Code " | fast-jev-compaction: onde roda, o que envia, como mede, falhas |
| `triagem-cves.md` | `Cargo.lock` × `~/.cargo/advisory-db` + `cargo tree -i --target all -e all` | as CVEs por pacote |
| `trio-corpus-T.md` | `scripts/measure_tool_trio.py --sessions 200` | o trio corrigido |

Consulta feita via `nlm notebook query <id> "<pergunta>" --json --new-conversation`.
Repositórios lidos por `gh api` em 27/10: `google-research/harness2`
(`README`, `docs/BENCHMARKS.md`, `docs/EXPERIMENTS.md`), `rtk-ai/rtk` (README,
`engine/builtin/posthook.toml`), `PrimeIntellect-ai/prime-agent` (README).
Context7: `/anthropics/claude-code` (`mods/types/claude-code.d.ts`).
