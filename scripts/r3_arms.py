#!/usr/bin/env python3
"""r3_arms — braços ONLINE de modelo para o avaliador de shadow (S-9/S-11).

O avaliador (`r3_shadow_evaluator.build_arms`, linha 58) importa este módulo
SE ele existir; sem ele, só o braço determinístico (B-D, A1) roda e os eventos
A2 ficam sem decisão de braço.

O CONTRATO de um braço (lido de `DedupArm` e `_decide`):
    .name .use_case .model .kind   e   .decide(event) -> dict | None
Para A2 a decisão é `{"followed": bool}` — o avaliador compara com
`event["baseline_followed"]` e grava `baseline_agrees`. `None` = "sem decisão"
(não é veredito); uma exceção sobe e o avaliador a isola e CONTA o braço que
falhou (linha 105-107) — por isso aqui não se engole exceção nenhuma.

## Um braço só: B-L (Laya base zero-shot, CPU)

* **Não há braço Jev online.** O orçamento de API (~486 chamadas) não se gasta sem
  decisão do Gabriel, e o replay do S-6 mediu que o B-J **não discrimina**
  (separação +0,010 sobre a decisão do B0). Um braço que consome cota e não
  separa evento de nada é custo sem medida.
* **Carga PREGUIÇOSA**: o `laya.Agent` leva ~3 s para construir e o chip está a
  93-95 °C. Importar este módulo NÃO pode carregar o modelo — só a 1ª decisão.
* **CPU com 2 threads**: a GPU é da analise-0e.
* **Sem duplicar**: as features, o prompt e a leitura de τ vêm do replay
  (`r3_replay_a2`), importado.
"""
from __future__ import annotations

import os
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import r3_replay_a2 as replay  # noqa: E402  (features/prompt/τ — importados, nao copiados)

# τ de DESFEÇO: o mesmo que o replay usa (a2_outcome), lido do modulo do replay
# para que mudar lá mudeie aqui — um τ divergente entre replay e online produziria
# dois números incompatíveis para o mesmo braço.
TAU_DESFECHO = float(os.environ.get("R3_TAU_DESFECHO", "0.5"))

# O prompt é o MESMO do replay (`braco_jev`), para que a decisão online seja a
# mesma decisão que o replay mediu. Não é redigido: o braço é `local` (loopback,
# sem rede) e o avaliador só redige para braços `api`.
INSTRUCOES_A2 = ("Did the agent follow the recommended route after the nudge?")


class LayaBaseArm:
    """B-L online: Laya base (`convaiinnovations/laya`, 322M), zero-shot, CPU.

    Zero-shot e sem τ calibrado — é o mesmo braço do S-6, e a comparação com o
    replay é o que dá sentido ao número. O `laya` avisa no load que as
    temperaturas do checkpoint são invalidas ("treat confidence as uncalibrated");
    o aviso é do próprio modelo e fica no log, não é escondido.
    """

    name = "B-L"
    use_case = "a2_outcome"
    model = "laya-base-zero-shot-cpu"
    kind = "local"

    def __init__(self, agent_factory=None, threads: int = 2):
        # `agent_factory` é o ponto de injecao dos testes: um Agent FALSO, para que
        # nenhum teste carregue o modelo de 842 MB.
        self._agent = None
        self._factory = agent_factory
        self._threads = threads
        self.decisoes = 0

    # ------------------------------------------------------------------ carga
    def _carregar(self):
        """CONSTRÓI o Agent na 1ª decisão — nunca no import, nunca antes."""
        if self._agent is not None:
            return self._agent
        # 2 threads: a máquina é compartilhada e o chip está a 93-95 °C.
        for var in ("OMP_NUM_THREADS", "MKL_NUM_THREADS", "OPENBLAS_NUM_THREADS"):
            os.environ.setdefault(var, str(self._threads))
        if self._factory is not None:
            self._agent = self._factory()
            return self._agent
        import torch  # noqa: PLC0415 — import tardio: o import do módulo não paga torch
        torch.set_num_threads(self._threads)
        import laya  # noqa: PLC0415
        self._agent = laya.Agent(model_id_or_path="convaiinnovations/laya", device="cpu")
        return self._agent

    # -------------------------------------------------------------- decisão
    def decide(self, event: dict) -> dict | None:
        """`{"followed": bool}` a partir de `p >= τ`; exceção sobe.

        O `event` chega CRU (o braço é `local`), e o A2 já traz `cmd_heads` com
        ≤80 chars por cabeça — o que o evento pode carregar é o que o schema
        permite, nada mais.
        """
        if event.get("use_case") != self.use_case:
            return None                      # braço de outro caso: sem decisão
        if event.get("kind") != "a2_offer_closed":
            return None
        agent = self._carregar()            # pode levantar: o avaliador conta
        r = agent.decide(
            replay.payload_of(event),
            questions={replay.QID: {"type": "noul", "instructions": INSTRUCOES_A2}},
            return_details=True,
        )
        p = replay._prob_de_resposta(r, replay.QID)  # o MESMO leitor dos dois formatos
        self.decisoes += 1
        return {"followed": bool(p >= TAU_DESFECHO)}


def build_online_arms():
    """Os braços de modelo do r3. Vazio é uma resposta VÁLIDA: o avaliador
    então roda só o B-D determinístico, que é o que acontece sem este módulo."""
    return [LayaBaseArm()]
