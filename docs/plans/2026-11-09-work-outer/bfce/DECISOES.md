# B · F · C · E — decisões de desenho (09/11/2026)

Pedido do Gabriel: «prossiga com B, F, C, E», decidindo pelas melhores opções (mesma delegação do
candidato A). Fatos: `fatos-{B,F,C,E}.md` nesta pasta. DAG `task_1794217886022321956`.

**Princípio comum (J18):** o J18 ainda exige pré-registro (`RETOMADA.md:30`). Toda refatoração do
VGP aqui PRESERVA veredito; uma unificação que muda veredito fica listada como pendente do
pré-registro, nunca entra calada. Mudança visível que não é veredito (texto, rótulo, log) entra só
com RED próprio e razão escrita.

## E — leitor tipado da resposta do hook
- Mora em `touring_foundation::hook_wire` (L0, todos os leitores alcançam): `HookWire::parse` +
  `Decision` na precedência do Claude Code + constantes dos nomes de campo.
- Migram: `hook_trace::classify_answer` (trace), `tool_trio::deny_gate_of`,
  `hook_response::exit_code_block_reason`; listas de campos (`DECISION_FIELDS`) usam as constantes.
- `merge_pretool_json_at` continua mesclando JSON (escreve, não só lê) com as constantes.
- Muda de propósito (RED): `allow`+`updatedInput` = `rewrite` no trace; negação do PermissionRequest
  lida; pré-filtro por substring sai (rejeitava `"deny"`).
- Fica: malformado nunca nega; `ask`/`defer` não são negação; os dois `gate_tag` (intencional).

## F — coletor de achados dos adapters
- `vgp/lang/coletor.rs`: dedup (classe,nome), sugestão com desempate total (distância, nome),
  frase única «. Você quis dizer `s`?», `reais_ordenados` (ordem total ANTES do corte).
- Entra (não é veredito; juízes excluem `sugestao`/`reais`): rust determinístico (hoje
  `HashSet.iter().take(8)`); classe_css pré-filtro em chars, não bytes.
- Pendente de decisão de produto (fora desta rodada): limiar único, igualdade ignorando caixa em
  py/ts, formato da frase do classe_css, teto/`nao_medido` nos demais, sugerir sobre o conjunto
  inteiro antes de cortar `reais`.

## C — ambiente Python
- Crate folha novo `touring-python-env` (só std; respeita a regra do `touring-pyright`): trait
  `Disco` + `AmbientePython` (raízes do interpretador, venv ativo, `pyvenv.cfg`, `lib-dynload`) +
  `tem_extensao_c`. Dois adapters = seam real: L1 pelo `caminho` (contido), L2 por `std::fs`.
- L2 passa a descobrir o ambiente pelo módulo (mesma ordem e regra de hoje).
- L1 usa `tem_extensao_c` do módulo; `site_packages` da L1 continua `.venv/lib` — adotar o
  `AmbientePython` na L1 muda veredito (vê `venv/`, `lib64`, home) → pendente do J18; comentário
  corrigido para dizer isso, em vez de afirmar `pyvenv.cfg`.
- Guarda: teste de zero `std::fs` no crate novo fora do adapter declarado.

## B — dono do veredito em duas camadas
- `touring-code` passa a depender de `touring-pyright` (folha leve: thiserror/serde; acíclico — a
  regra do pyright é sobre ELE não depender de touring-*). O comentário de `sombra.rs:262` muda.
- `vgp::camadas::compor(l1, Camada2, contexto) -> CheckOutput`: rebaixamento, conversão de
  severidade, fusão e `verdict_dos` num lugar; `Camada2 { Desligada(nota?), Indisponivel(r), Medida(L2Resultado) }`.
- CLI, sombra (`vgp_l2::mapear`) e o teste passam a chamar `compor`; saída JSON idêntica (testes
  da CLI, da sombra e dos handlers fixam).
- `escopo` da sombra deriva do `Alvo` do achado, não da severidade (log; sem leitor de máquina).
- `verdict == "deny"` dos handlers vira `CheckOutput::nega()`.
- Pendente do J18 (muda veredito): D1 `.cache` em Execução na L2, D2 raiz sob /tmp, D3 `<…>`/TMPDIR,
  D4 TS com contexto real.

## Ordem
E → F → C → B (B depende do C só no `Balde`; cada um com RED, testes dos crates, clippy, commit),
depois um release único com prova no binário instalado.
