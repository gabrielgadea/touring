vulneraveis=38 informativas=19
| pacote@versão | advisories | compilado? | patch semver-compatível? | patched |
|---|---|---|---|---|
| ctutils@0.4.2 | RUSTSEC-2026-0385 | NÃO compilado | sim | >= 0.4.4 |
| quick-xml@0.26.0 | RUSTSEC-2026-0194, RUSTSEC-2026-0195 | compilado | não | >= 0.41.0 |
| quick-xml@0.39.4 | RUSTSEC-2026-0194, RUSTSEC-2026-0195 | compilado | não | >= 0.41.0 |
| quinn-proto@0.11.15 | RUSTSEC-2026-0338, RUSTSEC-2026-0339, RUSTSEC-2026-0340, RUSTSEC-2026-0341, RUSTSEC-2026-0342 | NÃO compilado | sim | >= 0.11.17; >= 0.11.18 |
| rustls@0.23.41 | RUSTSEC-2026-0285, RUSTSEC-2026-0346, RUSTSEC-2026-0348, RUSTSEC-2026-0349 | compilado | sim | >= 0.23.45; >= 0.23.46 |
| rustls-webpki@0.103.13 | RUSTSEC-2026-0382 | compilado | sim | >= 0.103.16, < 0.104.0-alpha.1; >= 0.104.0-alpha.8 |
| wasmtime@36.0.14 | RUSTSEC-2026-0379, RUSTSEC-2026-0380, RUSTSEC-2026-0316, RUSTSEC-2026-0363 | compilado | sim | >= 36.0.16, < 37.0.0; >= 36.0.18, < 37.0.0; >= 36.0.19, < 37 |
| wasmtime@46.0.3 | RUSTSEC-2026-0378, RUSTSEC-2026-0379, RUSTSEC-2026-0316, RUSTSEC-2026-0327, RUSTSEC-2026-0353, RUSTSEC-2026-0354, RUSTSEC-2026-0355, RUSTSEC-2026-0356, RUSTSEC-2026-0357, RUSTSEC-2026-0358, RUSTSEC-2026-0359, RUSTSEC-2026-0361, RUSTSEC-2026-0362, RUSTSEC-2026-0363 | compilado | não | >= 36.0.16, < 37.0.0; >= 36.0.18, < 37.0.0; >= 36.0.19, < 37 |
| webpki-roots@1.0.8 | RUSTSEC-2026-0381 | compilado | sim | >= 1.0.10 |
| z3@0.20.1 | RUSTSEC-2026-0295 | compilado | não | >= 0.21.0 |
| zip@1.1.4 | RUSTSEC-2026-0369, RUSTSEC-2026-0367, RUSTSEC-2026-0368 | compilado | não | >= 9.0.0 |

## Reconciliação com o F2.5 (27/10)
- 38 pares (pacote@versão, advisory) − 4 aceitos no `deny.toml` (quick-xml 0194/0195 × 0.26.0/0.39.4) = **34**, exatamente o F2.5. Mesmo db: `~/.cargo/advisory-db` HEAD 21d909e4, fetch 24/10 10:57.
- wasmtime 46.0.3: nenhuma advisory tem patch na linha 46 (patched: ≥ 48.0.7 <49 ou ≥ 49.0.4). Dependência direta do workspace (`Cargo.toml:339`).
- z3 0.20.1: direta (`Cargo.toml:564`). zip 1.1.4: via candle-core 0.8.4.
