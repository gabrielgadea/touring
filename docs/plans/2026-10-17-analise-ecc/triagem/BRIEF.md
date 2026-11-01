# Triagem dos defeitos a confirmar (18/10/2026)

Entrada: `triagem/defeitos-a-confirmar.json` (30 itens; `defeito` + `evidencia` do leitor). Cada item é HIPÓTESE.
Para cada id do seu lote: reproduza OU refute **por comando** (grep, sed -n, `touring index find`, `touring scan`/`audit`
sobre arquivo-fixture que VOCÊ cria em `$SCR`), e registre a linha-chave da saída. Nunca afirme sem ter rodado.

Regras:
- `SCR=/tmp/claude-1000/-home-gabrielgadea-projects-touring/efe418c2-95f7-4a62-8bac-57080ed455b8/scratchpad/triagem` para fixtures.
- Só leitura no repositório `/home/gabrielgadea/projects/touring` e em `~/.claude`. Não edite nada fora de `$SCR` e do seu arquivo de saída.
- NÃO rode comando que grave recompensa/aprendizado (`touring ast quality` grava reward: leia o código em vez de rodar),
  nem `touring daemon-ctl restart|stop`, nem `update-touring`, nem git que não seja leitura. Nunca kill/pkill.
- Para «X não tem chamador»: `grep -rn` em `crates/ scripts/ client/ ~/.claude/settings.json ~/.claude/hooks` E `touring index find`.
- Para «o detector não pega Y»: crie a fixture mínima, rode a ferramenta, mostre a saída. Uma fixture POSITIVA de controle
  (algo que a ferramenta comprovadamente pega) antes da negativa — senão «não reportou» pode ser ferramenta quebrada.
- Se o item já foi corrigido, diga `corrigido` com a evidência.

Saída: `triagem/resultado-<lote>.json`, lista de
`{"id","estado": "confirmado|refutado|parcial|corrigido|inconclusivo","comando","saida_chave","nota"}` (nota ≤ 300 chars).
Resposta final: uma linha `{"arquivo": "...", "confirmados": n, "refutados": n, "parciais": n, "outros": n}`.
