"""Lição de 09/10/2026: o held-out disjunto do motor VGP achou o que o conjunto de ajuste (0/700) não via."""

from scripts.memoria.grafo_memoria import Aresta, No, gravar

D = "2026-10-09"
K_ANTERIOR = f"licao:holdout-fresco-o-zero-do-binario-novo-so-vale-com-o-velho-medindo-positivo-no-mesmo-corpus:{D}"
PROC = ("decomp:task_1791581548206685387",)

no = No(
    tipo="licao",
    slug="holdout-disjunto-achou-o-falso-deny-que-o-conjunto-de-ajuste-nao-via",
    corpo=(
        "Held-out DISJUNTO (semente 20261010, corte 2026-10-09T18:14:31.894Z): 600 negativos de arquivo (516 arquivos distintos, "
        "maior 5 = 0,83%) + 600 inline (141 sessões, maior 18 = 3,0%); sobreposição com o vivo e com o temporal: 0 em ids, "
        "tool_use_ids, shas e comandos (controle positivo vivo x vivo: 791/491/809/522). Binário e935 da Kimi: 5/600 arquivo "
        "(limite superior 95% Clopper-Pearson 1,74%) e 2/600 inline (1,05%); 7/1200 = 1,09% — o 0/700 do conjunto de ajuste não "
        "provava ≤ 1%. Binário velho c8b503c4 no mesmo corpus: 70/600 e 86/600 (taxas 11,7% e 14,3%, iguais às do vivo 10,75% e 14,0%) "
        "= o instrumento enxerga e a distribuição é a mesma. Dos 7: 3 são defeito REAL e GERAL do coletor de atributos de módulo "
        "(ligações dentro de bloco composto ou por setattr dinâmico não são vistas): sonda de 3 linhas com o binário congelado nega "
        "`np.array`/`np.where`, `Image.LANCZOS` (Pillow 11.3.0: setattr(module, item.name, item.value)) e "
        "`scripts.aco.rust_bridge.RUST_AVAILABLE` (atribuído em try/except); em ARQUIVO o numpy sai como warn (9 snaps do vivo, 4 do "
        "disjunto, todos warn) e em INLINE como deny — o vivo não tinha nenhum inline com np.array, por isso 0/300. Os outros 4 de "
        "arquivo são ruído de rótulo: 'limpo' = nenhum erro nomeou o arquivo antes da edição seguinte, não 'correto' (TSEClient nunca "
        "existiu: git log -S sem commit; trecho_pctx.py é fragmento sem Path/ComputeProcessContext; WIRING definido 7 s depois pela "
        "edição seguinte; WS não definido nem no estado seguinte pela regex). Regra: um zero medido no conjunto de ajuste não é taxa; "
        "o limite superior se declara por balde com n e método, e o falso deny geral se prova por sonda mínima, não por inferência."
    ),
    facetas={"kind": "lesson", "domain": "vgp", "process": "held-out", "status": "open"},
    procedencia=PROC,
    arestas=(Aresta(K_ANTERIOR, "relates-to"),),
)

if __name__ == "__main__":
    r = gravar(no)
    print(no.chave, "gravado=", r.gravado, "arestas", r.arestas_criadas, "/", r.arestas_pedidas)
