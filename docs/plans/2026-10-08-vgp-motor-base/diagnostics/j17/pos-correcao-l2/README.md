# Anexo informativo do J17 — V3 fria depois da correção da camada 2 (08/11/2026)

**Não é julgamento.** O J17 está fechado (INCONCLUSIVO); este run só mede o efeito do commit `90d491ec`
(camada 2: módulo do projeto visto dos ancestrais do arquivo + PEP 420) no mesmo híbrido do J17
(`/tmp/vgp-j17/j17_hibrido_20261101c`, sha_tree em `../hibrido_sha_tree.txt`), com o binário de debug
`target/debug/touring` (build de `90d491ec`), o mesmo juiz e os mesmos argumentos da V3 do `../rodar_j17.sh`.

| | 30.4.104 (J17) | com `90d491ec` |
|---|---|---|
| falso deny arquivo / inline | 1/600 · 0/371 (`b81d9fa3c3e5`) | **0/600 · 0/371** |
| recall de símbolo arquivo / inline | 11/13 · 23/27 | 11/13 · 23/27 |
| timeouts camada 2 | 170/1035 | 143/1035 |
| p95 frio (informativo) | 1051 ms | 1037,5 ms |

Pelo critério do J15 a única pendência restante (p95 frio) é filtrada; falta a fase quente com instrumento válido,
que só um novo julgamento (J18, pré-registro novo) decide.
