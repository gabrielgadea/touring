# V3 «a frio» de 10/10 — CONTAMINADO, não vale como medida a frio

Início 12:43:03Z com 63 °C e load 2,63; fim 12:55:01Z com 95 °C e load 13,27. Os 8 agentes retomados depois da queda de
rede pegaram as vagas rust e pesada no meio da medição. Binário e935aaf1, vivo `corpus_v0d`, V3 (`--camadas 1,2`).

Resultado: p95 1.120,7 ms (rc 1). Recall 11/13 · 22/27, falso deny 0/400 · 0/274; os campos de veredito
batem com as medições de referência do e935.

Uso legítimo deste número: o p95 do V3 SOBE com a carga (1.120,7 ms com load ~13, contra ~1.012 ms com load ~6
em 09/10), compatível com a hipótese ambiental, mas NÃO a prova (não houve medição fria). A prova exige uma janela
EXCLUSIVA, sem build nem juiz concorrente: é o que o J faz com o binário integrado final.
