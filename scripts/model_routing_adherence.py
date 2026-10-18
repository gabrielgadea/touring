#!/usr/bin/env python3
"""model_routing_adherence.py — a aderência da matriz de roteamento de modelos (CLAUDE.md §17).

A matriz diz: busca/extração → subagente `haiku`; implementação com spec → `sonnet`;
decidir → a sessão Opus; determinístico → código. O gate `agent_model_gate.py` só exige que
`model` EXISTA na chamada `Agent`; nada media se a escolha seguia a matriz. Esta régua mede,
sobre os transcripts do Claude Code (`~/.claude/projects`), o que de fato foi pedido e gasto:

- `chamadas_agent`: chamadas `Agent` (e o nome legado `Task`) por `model` pedido, por
  `subagent_type`, por dia (UTC);
- `modelo_real_dos_subagentes`: arquivos de subagente por família do modelo que de fato rodou;
- `tokens_por_familia`: tokens (input + cache_creation + cache_read + output) por família,
  principal × subagente, com a parcela % do total ANTHROPIC (opus+sonnet+haiku+fable) e, à
  parte, o das sessões irmãs (kimi/minimax/glm/deepseek — cota fora da franquia Anthropic);
- `sessoes`: sessões principais Anthropic e quantas delegaram ao menos uma vez;
- `desajustes`: chamadas de tipo BUSCA (`Explore`, `touring-scouter`) com model caro
  (sonnet/opus/fable), e quantas chamadas pediram `fable` (consentimento não é decidível aqui);
- `aderencia_roteamento`: `1 - desajustes/elegiveis` — o escalar que o `touring kpi` consome;
- `gate`: o registro do `agent_model_gate.py` (`--gate-log`, `~/.claude/touring/
  agent_model_gate.jsonl`): chamadas por `why` e por dia, liberadas × negadas. Qualquer `why`
  vira chave — a lista cresce no hook (`fit:*`, `fable:*`) sem tocar aqui;
- `nudge`: o registro do `delegation_nudge.py` (`--nudge-log`, `~/.claude/touring/
  delegation_nudge.jsonl`): avisos por evento e por dia, e a TAXA DE CONVERSÃO — a fração dos
  avisos de `leitura` seguidos, na MESMA sessão, de uma chamada `Agent` com model haiku em até
  10 minutos (cruzada com as chamadas dos transcripts, já deduplicadas).

Os dois registros são opcionais: arquivo ausente = `{"presente": false}`, nunca erro.

O que o escalar NÃO vê: um `general-purpose` que faz busca sob `sonnet` é indistinguível de um
que implementa. A aderência é, portanto, um piso (só o desajuste DETECTÁVEL por tipo), nunca a
prova de que o roteamento está certo.

Deduplicação (medida em 17/10/2026 sobre 364 arquivos): o transcript grava UM registro por bloco
de conteúdo, todos com o mesmo `message.id` — 80.623 registros assistant para 35.767 mensagens
distintas, e 12.341 ids com usage diferente entre os registros (streaming parcial). Somar por
registro infla os tokens ~2,3x. Por isso: uma mensagem conta uma vez (por `message.id`; sem id,
por `uuid`), com o MÁXIMO por campo de usage; uma chamada conta uma vez por `tool_use.id`
(147 registros → 138 ids; sessões retomadas reproduzem o histórico em outro arquivo).

Uso:
  python3 scripts/model_routing_adherence.py [--root DIR] [--since YYYY-MM-DD] [--json]
                                             [--brief] [--emit PATH] [--min-n N]
                                             [--gate-log PATH] [--nudge-log PATH]

`--since` é meia-noite UTC (os timestamps dos transcripts são UTC); default: 7 dias atrás.
Sem `--json`, imprime o resumo curto (≤ 12 linhas). `--emit PATH` grava o contrato `external:`
do `touring kpi` (`{"value": <0..1>|null, ...}`; `null` + `detalhe` abaixo de `--min-n`
elegíveis — ausência de dado nunca vira zero). Exit 0 (analista); `--root` inexistente = exit 2,
porque «0 arquivos» de um diretório que não existe é exatamente o silêncio que mente.
"""

from __future__ import annotations

import argparse
import datetime as dt
import glob
import json
import os
import sys
import tempfile
from collections import Counter, defaultdict
from collections.abc import Callable, Iterable, Iterator
from pathlib import Path
from typing import Any, NamedTuple

FERRAMENTAS_DELEGACAO = frozenset({"Agent", "Task"})
FAMILIAS_ANTHROPIC = ("opus", "sonnet", "haiku", "fable")
#: Sessões irmãs e outros backends: cota fora da franquia Anthropic.
FAMILIAS_IRMAS = ("kimi", "minimax", "glm", "deepseek")
#: Substring do `message.model` → família. Ordem: Anthropic primeiro (nenhum nome contém outro).
_PADROES_FAMILIA = (
    ("opus", "opus"),
    ("sonnet", "sonnet"),
    ("haiku", "haiku"),
    ("fable", "fable"),
    ("kimi", "kimi"),
    ("k3", "kimi"),
    ("minimax", "minimax"),
    ("glm", "glm"),
    ("deepseek", "deepseek"),
)
#: Subagentes cujo ofício é busca/extração — a matriz manda `haiku`.
TIPOS_DE_BUSCA = frozenset({"Explore", "touring-scouter"})
FAMILIAS_CARAS = frozenset({"opus", "sonnet", "fable"})
#: Rótulos dos baldes sem valor (nunca confundir «ausente» com um modelo/tipo real).
TIPO_BLOB_DE_REQUISICAO = "api-request-blob"
SEM_MODEL = "sem_model"
SEM_TIPO = "(nenhum)"
ID_DO_KPI = "touring.routing.adherence"
MIN_ELEGIVEIS_PADRAO = 10
#: O aviso de leitura converte quando um `Agent` haiku da mesma sessão vem até este prazo depois.
JANELA_CONVERSAO_S = 600
EVENTO_LEITURA = "leitura"
SEM_WHY = "(sem why)"
#: `why` das chamadas decididas pelo env (`env:CLAUDE_CODE_SUBAGENT_MODEL=<v>`; o registro antigo trazia só `env:<v>`).
PREFIXO_WHY_ENV = "env:"
SEM_EVENTO = "(sem evento)"
SEM_REGISTRO: dict[str, Any] = {"presente": False}


# ── funções puras ───────────────────────────────────────────────────────────


def familia_do_modelo(modelo: object) -> str:
    """Família por substring de `message.model`; qualquer outro valor → «outro»."""
    if not isinstance(modelo, str):
        return "outro"
    minusculo = modelo.lower()
    for trecho, familia in _PADROES_FAMILIA:
        if trecho in minusculo:
            return familia
    return "outro"


def _inteiro_nao_negativo(valor: object) -> int:
    if isinstance(valor, bool) or not isinstance(valor, int):
        return 0
    return max(valor, 0)


def partes_de_usage(usage: object) -> tuple[int, int, int, int]:
    """(input, cache_creation, cache_read, output) de `message.usage`; ausente/inválido = 0."""
    if not isinstance(usage, dict):
        return (0, 0, 0, 0)
    return (
        _inteiro_nao_negativo(usage.get("input_tokens")),
        _inteiro_nao_negativo(usage.get("cache_creation_input_tokens")),
        _inteiro_nao_negativo(usage.get("cache_read_input_tokens")),
        _inteiro_nao_negativo(usage.get("output_tokens")),
    )


def tokens_de_usage(usage: object) -> int:
    return sum(partes_de_usage(usage))


def parse_instante(valor: object) -> dt.datetime | None:
    """ISO-8601 (com `Z` ou offset) → datetime com tz; qualquer outra coisa → None."""
    if not isinstance(valor, str):
        return None
    try:
        parsed = dt.datetime.fromisoformat(valor.replace("Z", "+00:00"))
    except ValueError:
        return None
    return parsed if parsed.tzinfo else parsed.replace(tzinfo=dt.timezone.utc)


def parse_since(texto: str) -> dt.datetime:
    """`YYYY-MM-DD` → meia-noite UTC. Formato inválido levanta ValueError."""
    dia = dt.date.fromisoformat(texto)
    return dt.datetime(dia.year, dia.month, dia.day, tzinfo=dt.timezone.utc)


def desde_padrao(agora: dt.datetime | None = None) -> dt.datetime:
    """7 dias antes de hoje (UTC), à meia-noite."""
    agora = agora or dt.datetime.now(dt.timezone.utc)
    return parse_since((agora - dt.timedelta(days=7)).date().isoformat())


def _texto_ou_none(valor: object) -> str | None:
    return valor.strip() or None if isinstance(valor, str) else None


def dominante(contagem: Counter[str]) -> str | None:
    """Família mais frequente; «outro» só vence se for a única. Empate → ordem alfabética."""
    if not contagem:
        return None
    reais = {f: n for f, n in contagem.items() if f != "outro"} or dict(contagem)
    return min(reais.items(), key=lambda par: (-par[1], par[0]))[0]


def fmt_int(n: int) -> str:
    return f"{n:,}".replace(",", ".")


def fmt_tokens(n: int) -> str:
    if n >= 1_000_000_000:
        return f"{n / 1e9:.2f}".replace(".", ",") + " G"
    if n >= 1_000_000:
        return f"{n / 1e6:.1f}".replace(".", ",") + " M"
    return fmt_int(n)


def fmt_pct(valor: float) -> str:
    return f"{valor:.1f}".replace(".", ",") + "%"


# ── agregação ───────────────────────────────────────────────────────────────


class Chamada(NamedTuple):
    """Uma chamada de delegação (`Agent`/`Task`), já deduplicada por `tool_use.id`."""

    dia: str
    modelo: str | None
    tipo: str | None
    origem: str
    instante: dt.datetime
    sessao: str | None


class Agregador:
    """Estado da varredura. Uma mensagem conta uma vez; uma chamada conta uma vez."""

    def __init__(self, desde: dt.datetime) -> None:
        self.desde = desde
        #: chave da mensagem → [origem, família, input, cache_creation, cache_read, output]
        self.mensagens: dict[str, list[Any]] = {}
        self.chamadas_vistas: set[str] = set()
        self.chamadas: list[Chamada] = []
        #: sessão do arquivo principal em leitura: a do registro, quando ele não a traz
        self._sessao_do_arquivo: str | None = None
        self.arquivos_subagente: Counter[str] = Counter()
        #: (família dominante, delegou) por sessão principal com mensagem nova na janela
        self.sessoes: list[tuple[str, bool]] = []
        self.puladas: Counter[str] = Counter()
        self.arquivos = Counter(principal=0, subagente=0, ignorados_por_mtime=0)
        self._sem_chave = 0

    def _chave_da_mensagem(self, rec: dict[str, Any], msg: dict[str, Any]) -> str:
        for candidato in (msg.get("id"), rec.get("uuid")):
            if isinstance(candidato, str) and candidato:
                return candidato
        self._sem_chave += (
            1  # sem identidade, não há como deduplicar: cada registro conta
        )
        return f"__sem_id_{self._sem_chave}"

    def _somar_mensagem(
        self, chave: str, origem: str, familia: str, usage: object
    ) -> bool:
        """True quando a mensagem é NOVA; a repetição só eleva os campos de usage (máximo)."""
        partes = partes_de_usage(usage)
        existente = self.mensagens.get(chave)
        if existente is None:
            self.mensagens[chave] = [origem, familia, *partes]
            return True
        for i, valor in enumerate(partes):
            existente[2 + i] = max(existente[2 + i], valor)
        return False

    def _registrar_chamadas(
        self, msg: dict[str, Any], quando: dt.datetime, origem: str, sessao: str | None
    ) -> bool:
        """Registra os `tool_use` de delegação ainda não vistos; True se houve algum novo."""
        conteudo = msg.get("content")
        if not isinstance(conteudo, list):
            return False
        novo = False
        for bloco in conteudo:
            if not isinstance(bloco, dict) or bloco.get("type") != "tool_use":
                continue
            if bloco.get("name") not in FERRAMENTAS_DELEGACAO:
                continue
            tid = bloco.get("id")
            if isinstance(tid, str) and tid:
                if tid in self.chamadas_vistas:
                    continue
                self.chamadas_vistas.add(tid)
            entrada = bloco.get("input")
            entrada = entrada if isinstance(entrada, dict) else {}
            self.chamadas.append(
                Chamada(
                    dia=quando.astimezone(dt.timezone.utc).date().isoformat(),
                    modelo=_texto_ou_none(entrada.get("model")),
                    tipo=_texto_ou_none(entrada.get("subagent_type")),
                    origem=origem,
                    instante=quando,
                    sessao=sessao,
                )
            )
            novo = True
        return novo

    def _processar_registro(
        self, rec: dict[str, Any], origem: str
    ) -> tuple[str | None, bool]:
        """(família da mensagem, se for NOVA; se o registro trouxe uma chamada nova)."""
        if rec.get("type") == TIPO_BLOB_DE_REQUISICAO:
            # O Claude Code grava o corpo da requisição como `message.role == assistant` sem
            # timestamp/id/usage, repetindo turnos antigos (709 de 709 sem timestamp, medido):
            # contá-lo contaria cada chamada passada de novo.
            self.puladas["blobs_de_requisicao"] += 1
            return None, False
        msg = rec.get("message")
        if not isinstance(msg, dict) or msg.get("role") != "assistant":
            return None, False
        quando = parse_instante(rec.get("timestamp"))
        if quando is None:
            self.puladas["sem_timestamp"] += 1
            return None, False
        if quando < self.desde:
            return None, False
        familia = familia_do_modelo(msg.get("model"))
        chave = self._chave_da_mensagem(rec, msg)
        nova = self._somar_mensagem(chave, origem, familia, msg.get("usage"))
        sessao = _texto_ou_none(rec.get("sessionId")) or self._sessao_do_arquivo
        chamou = self._registrar_chamadas(msg, quando, origem, sessao)
        return (familia if nova else None), chamou

    def processar_arquivo(self, caminho: Path, origem: str) -> None:
        familias_novas: Counter[str] = Counter()
        delegou = False
        self._sessao_do_arquivo = caminho.stem if origem == "principal" else None
        for rec in iter_registros(caminho):
            if rec is None:
                self.puladas["malformadas"] += 1
                continue
            familia_nova, chamou = self._processar_registro(rec, origem)
            if familia_nova is not None:
                familias_novas[familia_nova] += 1
            delegou = delegou or chamou
        escolhida = dominante(familias_novas)
        if escolhida is None:
            return
        if origem == "subagente":
            self.arquivos_subagente[escolhida] += 1
        else:
            self.sessoes.append((escolhida, delegou))


def iter_registros(caminho: Path) -> Iterator[dict[str, Any] | None]:
    """Um dict por linha JSON válida; None por linha malformada (o chamador conta e segue)."""
    try:
        with caminho.open(encoding="utf-8", errors="replace") as fh:
            for linha in fh:
                linha = linha.strip()
                if not linha:
                    continue
                try:
                    rec = json.loads(linha)
                except ValueError:
                    yield None
                    continue
                yield rec if isinstance(rec, dict) else None
    except OSError:
        yield None


def listar_arquivos(root: Path) -> list[tuple[Path, str]]:
    """(caminho, origem) — `*/*.jsonl` são sessões principais; `*/*/subagents/*.jsonl`, subagentes."""
    base = str(root)
    principais = sorted(glob.glob(os.path.join(base, "*", "*.jsonl")))
    subagentes = sorted(glob.glob(os.path.join(base, "*", "*", "subagents", "*.jsonl")))
    return [(Path(p), "principal") for p in principais] + [
        (Path(p), "subagente") for p in subagentes
    ]


def varrer(root: Path, desde: dt.datetime) -> Agregador:
    agg = Agregador(desde)
    limite = desde.timestamp()
    for caminho, origem in listar_arquivos(root):
        try:
            # Um arquivo que ninguém tocou desde `desde` não tem registro na janela.
            if caminho.stat().st_mtime < limite:
                agg.arquivos["ignorados_por_mtime"] += 1
                continue
        except OSError:
            continue
        agg.arquivos[origem] += 1
        agg.processar_arquivo(caminho, origem)
    return agg


# ── resumo ──────────────────────────────────────────────────────────────────


def _ordenado(contagem: Counter[str]) -> dict[str, int]:
    return dict(sorted(contagem.items(), key=lambda par: (-par[1], par[0])))


def _resumo_chamadas(chamadas: list[Chamada]) -> dict[str, Any]:
    """Conta as chamadas e separa as de BUSCA em elegíveis × aderentes × desajustes."""
    por_model: Counter[str] = Counter()
    por_tipo: Counter[str] = Counter()
    por_dia: Counter[str] = Counter()
    por_origem: Counter[str] = Counter()
    desajustes: Counter[str] = Counter()
    busca: Counter[str] = Counter()
    fable_pedido = 0
    for chamada in chamadas:
        dia, modelo, tipo, origem = chamada[:4]
        familia = familia_do_modelo(modelo) if modelo else None
        por_model[modelo if modelo else SEM_MODEL] += 1
        por_tipo[tipo if tipo else SEM_TIPO] += 1
        por_dia[dia] += 1
        por_origem[origem] += 1
        fable_pedido += familia == "fable"
        if tipo not in TIPOS_DE_BUSCA:
            continue
        if modelo is None:
            busca["sem_model"] += 1
        elif familia not in FAMILIAS_ANTHROPIC:
            busca["indecidivel"] += 1
        elif familia in FAMILIAS_CARAS:
            busca["elegiveis"] += 1
            desajustes[f"{tipo}/{familia}"] += 1
        else:
            busca["elegiveis"] += 1
            busca["aderentes"] += 1
    total_desajustes = sum(desajustes.values())
    return {
        "chamadas_agent": {
            "total": len(chamadas),
            "por_model_pedido": _ordenado(por_model),
            "por_subagent_type": _ordenado(por_tipo),
            "por_dia": dict(sorted(por_dia.items())),
            "por_origem": _ordenado(por_origem),
        },
        "desajustes": {
            "total": total_desajustes,
            "por_tipo_e_modelo": _ordenado(desajustes),
            "fable_pedido": fable_pedido,
            "busca_sem_model": busca["sem_model"],
            "busca_modelo_indecidivel": busca["indecidivel"],
        },
        "aderencia_roteamento": {
            "valor": round(busca["aderentes"] / busca["elegiveis"], 4)
            if busca["elegiveis"]
            else None,
            "elegiveis": busca["elegiveis"],
            "aderentes": busca["aderentes"],
            "desajustes": total_desajustes,
        },
    }


def _resumo_tokens(mensagens: dict[str, list[Any]]) -> dict[str, Any]:
    """Tokens por família, principal × subagente, com a parcela % do total ANTHROPIC."""
    tokens: dict[str, Counter[str]] = {"principal": Counter(), "subagente": Counter()}
    for origem, familia, *partes in mensagens.values():
        tokens[origem][familia] += sum(partes)
    por_familia = {
        familia: tokens["principal"][familia] + tokens["subagente"][familia]
        for familia in {*tokens["principal"], *tokens["subagente"]}
    }
    total = sum(por_familia.get(f, 0) for f in FAMILIAS_ANTHROPIC)
    em_subagentes = sum(tokens["subagente"].get(f, 0) for f in FAMILIAS_ANTHROPIC)
    irmas = {f: por_familia[f] for f in FAMILIAS_IRMAS if por_familia.get(f)}
    return {
        "principal": _ordenado(tokens["principal"]),
        "subagente": _ordenado(tokens["subagente"]),
        "anthropic": {
            "total": total,
            "subagente": em_subagentes,
            "subagente_pct": _pct(em_subagentes, total),
            "parcela_pct": {
                f: _pct(por_familia.get(f, 0), total) for f in FAMILIAS_ANTHROPIC
            },
        },
        "irmas": {"total": sum(irmas.values()), "por_familia": irmas},
        "outro": por_familia.get("outro", 0),
    }


def _resumo_sessoes(sessoes: list[tuple[str, bool]]) -> dict[str, Any]:
    anthropic = [
        delegou for familia, delegou in sessoes if familia in FAMILIAS_ANTHROPIC
    ]
    delegaram = sum(anthropic)
    return {
        "principais_anthropic": len(anthropic),
        "que_delegaram": delegaram,
        "taxa_delegacao": round(delegaram / len(anthropic), 4) if anthropic else None,
        "irmas": sum(1 for familia, _ in sessoes if familia in FAMILIAS_IRMAS),
    }


# ── registros de enforcement (opcionais: ausente = {"presente": false}) ─────


def _dia_utc(quando: dt.datetime) -> str:
    return quando.astimezone(dt.timezone.utc).date().isoformat()


def _registros_da_janela(
    caminho: Path, desde: dt.datetime, puladas: Counter[str]
) -> Iterator[tuple[dict[str, Any], dt.datetime]]:
    """Linhas do registro com `ts` na janela; malformadas e sem `ts` são contadas, nunca fatais."""
    for rec in iter_registros(caminho):
        if rec is None:
            puladas["malformadas"] += 1
            continue
        quando = parse_instante(rec.get("ts"))
        if quando is None:
            puladas["sem_ts"] += 1
        elif quando >= desde:
            yield rec, quando


def _balde(contagem: Counter[str]) -> dict[str, int]:
    return {
        "total": contagem["total"],
        "allowed": contagem["allowed"],
        "negadas": contagem["negadas"],
    }


def _valor_do_env(why: str) -> str:
    """`env:CLAUDE_CODE_SUBAGENT_MODEL=haiku` (formato de 17/10/2026) e o legado `env:haiku` → «haiku»."""
    return why.split("=", 1)[1] if "=" in why else why[len(PREFIXO_WHY_ENV) :]


def resumo_gate(caminho: Path, desde: dt.datetime) -> dict[str, Any]:
    """O registro do `agent_model_gate.py`: chamadas por `why` e por dia, liberadas × negadas.

    Todo `why` vira chave (o hook ganha motivos novos sem mexer aqui). `allowed` que não seja
    um booleano de verdade é «indeterminado» — nem liberada nem negada. `decididas_por_env` conta
    as chamadas que só passaram porque `CLAUDE_CODE_SUBAGENT_MODEL` estava definido: nelas as
    regras de adequação não rodaram, e sem este número a decisão do env era invisível.
    """
    if not os.path.isfile(caminho):
        return {"presente": False}
    puladas: Counter[str] = Counter()
    por_why: dict[str, Counter[str]] = defaultdict(Counter)
    por_dia: dict[str, Counter[str]] = defaultdict(Counter)
    total: Counter[str] = Counter()
    por_env: Counter[str] = Counter()
    for rec, quando in _registros_da_janela(caminho, desde, puladas):
        permitido = rec.get("allowed")
        estado = (
            "allowed"
            if permitido is True
            else "negadas"
            if permitido is False
            else "indeterminado"
        )
        why = _texto_ou_none(rec.get("why")) or SEM_WHY
        if why.startswith(PREFIXO_WHY_ENV):
            por_env[_valor_do_env(why)] += 1
        for balde in (total, por_why[why], por_dia[_dia_utc(quando)]):
            balde["total"] += 1
            balde[estado] += 1
    return {
        "presente": True,
        "arquivo": str(caminho),
        "total": total["total"],
        "allowed": {
            "true": total["allowed"],
            "false": total["negadas"],
            "indeterminado": total["indeterminado"],
        },
        "por_why": dict(
            sorted(
                ((w, _balde(c)) for w, c in por_why.items()),
                key=lambda par: (-par[1]["total"], par[0]),
            )
        ),
        "por_dia": {d: _balde(c) for d, c in sorted(por_dia.items())},
        "decididas_por_env": {
            "total": sum(por_env.values()),
            "por_valor": dict(sorted(por_env.items(), key=lambda par: (-par[1], par[0]))),
        },
        "linhas_puladas": {
            "malformadas": puladas["malformadas"],
            "sem_ts": puladas["sem_ts"],
        },
    }


def sessao_normalizada(valor: object) -> str | None:
    """O hook de aviso grava `safe_id(session_id)`; aplico a mesma limpeza aos DOIS lados do cruzamento
    (para um UUID é a identidade, então casar por aqui não depende do hook ter sanitizado)."""
    if not isinstance(valor, str) or not valor:
        return None
    return "".join(c if c.isalnum() or c in "-_" else "_" for c in valor)[:80]


def _haiku_por_sessao(chamadas: list[Chamada]) -> dict[str, list[dt.datetime]]:
    """Instantes das chamadas `Agent` haiku da sessão PRINCIPAL (um subagente não é a sessão avisada)."""
    haiku: dict[str, list[dt.datetime]] = defaultdict(list)
    for c in chamadas:
        sessao = sessao_normalizada(c.sessao)
        if (
            c.origem == "principal"
            and sessao
            and familia_do_modelo(c.modelo) == "haiku"
        ):
            haiku[sessao].append(c.instante)
    return haiku


def _seguido_por(
    aviso: dt.datetime, instantes: Iterable[dt.datetime], prazo: dt.timedelta
) -> bool:
    return any(dt.timedelta(0) <= t - aviso <= prazo for t in instantes)


def _conversao(
    leituras: list[tuple[str | None, dt.datetime]],
    chamadas: list[Chamada],
    agora: dt.datetime,
) -> dict[str, Any]:
    """Fração dos avisos de leitura seguidos, na MESMA sessão, de um `Agent` haiku em até 10 min.

    Só contam chamadas da sessão principal (um subagente não é a sessão avisada). Um aviso cuja
    janela de 10 min ainda não fechou em `agora` está «pendente» — não entra no denominador:
    sem o prazo vencido, a ausência de resposta não diz «não converteu».
    """
    haiku = _haiku_por_sessao(chamadas)
    prazo = dt.timedelta(seconds=JANELA_CONVERSAO_S)
    avaliados = convertidos = pendentes = sem_sessao = 0
    for sessao, quando in leituras:
        if sessao is None:
            sem_sessao += 1
        elif quando + prazo > agora:
            pendentes += 1
        else:
            avaliados += 1
            convertidos += _seguido_por(quando, haiku.get(sessao, ()), prazo)
    return {
        "avisos_de_leitura": avaliados,
        "convertidos": convertidos,
        "taxa": round(convertidos / avaliados, 4) if avaliados else None,
        "janela_s": JANELA_CONVERSAO_S,
        "pendentes": pendentes,
        "sem_sessao": sem_sessao,
    }


def resumo_nudge(
    caminho: Path,
    desde: dt.datetime,
    chamadas: list[Chamada],
    agora: dt.datetime | None = None,
) -> dict[str, Any]:
    """O registro do `delegation_nudge.py`: avisos por evento e por dia + taxa de conversão."""
    if not os.path.isfile(caminho):
        return {"presente": False}
    agora = agora or dt.datetime.now(dt.timezone.utc)
    puladas: Counter[str] = Counter()
    por_evento: Counter[str] = Counter()
    por_dia: dict[str, Counter[str]] = defaultdict(Counter)
    leituras: list[tuple[str | None, dt.datetime]] = []
    for rec, quando in _registros_da_janela(caminho, desde, puladas):
        evento = _texto_ou_none(rec.get("evento")) or SEM_EVENTO
        por_evento[evento] += 1
        por_dia[_dia_utc(quando)][evento] += 1
        if evento == EVENTO_LEITURA:
            leituras.append((sessao_normalizada(rec.get("session")), quando))
    return {
        "presente": True,
        "arquivo": str(caminho),
        "total": sum(por_evento.values()),
        "por_evento": _ordenado(por_evento),
        "por_dia": {d: _ordenado(c) for d, c in sorted(por_dia.items())},
        "conversao": _conversao(leituras, chamadas, agora),
        "linhas_puladas": {
            "malformadas": puladas["malformadas"],
            "sem_ts": puladas["sem_ts"],
        },
    }


def resumo(
    agg: Agregador,
    *,
    gate_log: Path | None = None,
    nudge_log: Path | None = None,
    agora: dt.datetime | None = None,
) -> dict[str, Any]:
    chamadas = _resumo_chamadas(agg.chamadas)
    return {
        "janela": {
            "desde": agg.desde.date().isoformat(),
            "arquivos_principais": agg.arquivos["principal"],
            "arquivos_subagente": agg.arquivos["subagente"],
            "arquivos_ignorados_por_mtime": agg.arquivos["ignorados_por_mtime"],
            "mensagens_distintas": len(agg.mensagens),
        },
        "chamadas_agent": chamadas["chamadas_agent"],
        "modelo_real_dos_subagentes": {
            "arquivos_por_familia": _ordenado(agg.arquivos_subagente),
            "total": sum(agg.arquivos_subagente.values()),
        },
        "tokens_por_familia": _resumo_tokens(agg.mensagens),
        "sessoes": _resumo_sessoes(agg.sessoes),
        "desajustes": chamadas["desajustes"],
        "aderencia_roteamento": chamadas["aderencia_roteamento"],
        "gate": resumo_gate(gate_log, agg.desde) if gate_log else SEM_REGISTRO,
        "nudge": (
            resumo_nudge(nudge_log, agg.desde, agg.chamadas, agora)
            if nudge_log
            else SEM_REGISTRO
        ),
        "linhas_puladas": {
            "malformadas": agg.puladas["malformadas"],
            "sem_timestamp": agg.puladas["sem_timestamp"],
            "blobs_de_requisicao": agg.puladas["blobs_de_requisicao"],
        },
    }


def _pct(parte: int, total: int) -> float | None:
    return round(100 * parte / total, 2) if total else None


def _top(contagem: dict[str, int], n: int, fmt: Callable[[int], str] = str) -> str:
    return ", ".join(f"{k} {fmt(v)}" for k, v in list(contagem.items())[:n]) or "—"


def _linha_tokens(t: dict[str, Any]) -> str:
    anth = t["anthropic"]
    parcelas = " ".join(
        f"{f} {fmt_pct(p or 0.0)}" for f, p in anth["parcela_pct"].items()
    )
    cabeca = f"tokens Anthropic: {fmt_tokens(anth['total'])}"
    return f"{cabeca} (subagentes {fmt_pct(anth['subagente_pct'] or 0.0)}) · {parcelas}"


def _linha_sessoes(s: dict[str, Any]) -> str:
    taxa = s["taxa_delegacao"]
    parte = f" ({fmt_pct(100 * taxa)})" if taxa is not None else ""
    cabeca = f"sessões Anthropic: {fmt_int(s['principais_anthropic'])}"
    return f"{cabeca}, delegaram {fmt_int(s['que_delegaram'])}{parte} · irmãs: {fmt_int(s['irmas'])}"


def _linha_desajustes(d: dict[str, Any], a: dict[str, Any]) -> str:
    if a["valor"] is None:
        veredito = "(aderência indefinida: nenhuma busca decidível)"
    else:
        veredito = f"(aderência {str(a['valor']).replace('.', ',')})"
    achado = f"{fmt_int(a['desajustes'])} de {fmt_int(a['elegiveis'])}"
    return f"desajustes busca→modelo caro: {achado} {veredito} · fable pedido: {d['fable_pedido']}"


def _linha_gate(g: dict[str, Any]) -> str:
    if not g["presente"]:
        return "gate de modelo: registro ausente"
    a = g["allowed"]
    motivos = {why: balde["total"] for why, balde in g["por_why"].items()}
    cabeca = f"gate de modelo: {fmt_int(g['total'])} chamadas"
    detalhe = f"{fmt_int(a['true'])} liberadas, {fmt_int(a['false'])} negadas"
    por_env = fmt_int(g["decididas_por_env"]["total"])
    return f"{cabeca} ({detalhe}) · {_top(motivos, 4)} · decididas por env: {por_env}"


def _linha_nudge(n: dict[str, Any]) -> str:
    if not n["presente"]:
        return "avisos de delegação: registro ausente"
    c = n["conversao"]
    if c["taxa"] is None:
        taxa = "indefinida (nenhum aviso de leitura com o prazo vencido)"
    else:
        taxa = (
            f"{c['convertidos']}/{c['avisos_de_leitura']} ({fmt_pct(100 * c['taxa'])})"
        )
    cabeca = f"avisos de delegação: {fmt_int(n['total'])} ({_top(n['por_evento'], 3)})"
    return f"{cabeca} · conversão em haiku ≤10 min: {taxa}"


def brief(r: dict[str, Any]) -> str:
    """O resumo curto (≤ 12 linhas), números no formato pt-BR."""
    j, c, t = r["janela"], r["chamadas_agent"], r["tokens_por_familia"]
    reais = r["modelo_real_dos_subagentes"]["arquivos_por_familia"]
    puladas = r["linhas_puladas"]
    abertura = f"roteamento de modelos · desde {j['desde']}"
    arquivos = f"{fmt_int(j['arquivos_principais'])} sessões principais"
    linhas = [
        f"{abertura} · {arquivos}, {fmt_int(j['arquivos_subagente'])} arquivos de subagente"
    ]
    if not (j["arquivos_principais"] or j["arquivos_subagente"]):
        linhas.append("AVISO: nenhum transcript tocado na janela — nada foi medido.")
    linhas.append(
        f"Agent/Task: {fmt_int(c['total'])} chamadas · model pedido: {_top(c['por_model_pedido'], 4)}"
    )
    linhas.append(f"por tipo: {_top(c['por_subagent_type'], 4)}")
    linhas.append(f"subagentes por modelo real (arquivos): {_top(reais, 4)}")
    linhas.append(_linha_tokens(t))
    linhas.append(
        f"irmãs (fora da franquia Anthropic): {fmt_tokens(t['irmas']['total'])} "
        f"({_top(t['irmas']['por_familia'], 3, fmt_tokens)})"
    )
    linhas.append(_linha_sessoes(r["sessoes"]))
    linhas.append(_linha_desajustes(r["desajustes"], r["aderencia_roteamento"]))
    linhas.append(_linha_gate(r["gate"]))
    linhas.append(_linha_nudge(r["nudge"]))
    linhas.append(
        f"linhas puladas: {fmt_int(puladas['malformadas'])} malformadas, "
        f"{fmt_int(puladas['sem_timestamp'])} sem timestamp · "
        f"{fmt_int(puladas['blobs_de_requisicao'])} blobs de requisição (ignorados por desenho)"
    )
    return "\n".join(linhas)


# ── contrato `external:` do touring kpi ─────────────────────────────────────


def payload_do_kpi(
    r: dict[str, Any], *, min_n: int, agora: dt.datetime, comando: str
) -> dict[str, Any]:
    """O JSON que `touring kpi` lê de `docs/kpi/external/<id>.json`.

    Abaixo de `min_n` buscas decidíveis o valor é `null` com `detalhe` — o resolvedor do kpi
    mostra a causa (`ExternalStub::Declared`) em vez de um zero que parece medição.
    """
    a = r["aderencia_roteamento"]
    payload: dict[str, Any] = {
        "value": a["valor"],
        "measured_at": agora.isoformat(),
        "command": comando,
        "n": a["elegiveis"],
        "id": ID_DO_KPI,
        "desde": r["janela"]["desde"],
    }
    if a["valor"] is None or a["elegiveis"] < min_n:
        payload["value"] = None
        payload["detalhe"] = (
            f"STUB — {a['elegiveis']} buscas decidíveis (Explore/touring-scouter com model) "
            f"abaixo do mínimo {min_n}. Ausência de dado não é zero."
        )
    return payload


def gravar_atomico(destino: Path, payload: dict[str, Any]) -> None:
    destino.parent.mkdir(parents=True, exist_ok=True)
    fd, tmp = tempfile.mkstemp(
        dir=destino.parent, prefix=f".{destino.name}.", suffix=".tmp"
    )
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as fh:
            json.dump(payload, fh, ensure_ascii=False, indent=2)
            fh.write("\n")
        os.replace(tmp, destino)
    except BaseException:
        Path(tmp).unlink(missing_ok=True)
        raise


# ── CLI ─────────────────────────────────────────────────────────────────────


def _argumentos(argv: list[str] | None) -> argparse.Namespace:
    ap = argparse.ArgumentParser(
        description="Aderência ao roteamento de modelos (CLAUDE.md §17)."
    )
    ap.add_argument("--root", type=Path, default=Path.home() / ".claude" / "projects")
    ap.add_argument("--since", help="YYYY-MM-DD (UTC); default: 7 dias atrás")
    ap.add_argument("--json", action="store_true", help="relatório completo em JSON")
    ap.add_argument(
        "--brief", action="store_true", help="resumo curto (default sem --json)"
    )
    ap.add_argument(
        "--emit", type=Path, help="grava o contrato external: do touring kpi"
    )
    ap.add_argument("--min-n", type=int, default=MIN_ELEGIVEIS_PADRAO)
    registros = Path.home() / ".claude" / "touring"
    ap.add_argument(
        "--gate-log",
        type=Path,
        default=registros / "agent_model_gate.jsonl",
        help="registro do agent_model_gate.py (opcional: ausente = {presente: false})",
    )
    ap.add_argument(
        "--nudge-log",
        type=Path,
        default=registros / "delegation_nudge.jsonl",
        help="registro do delegation_nudge.py (opcional: ausente = {presente: false})",
    )
    return ap.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    args = _argumentos(argv)
    try:
        desde = parse_since(args.since) if args.since else desde_padrao()
    except ValueError:
        print(
            f"erro: --since precisa ser YYYY-MM-DD, recebi {args.since!r}",
            file=sys.stderr,
        )
        return 2
    if not args.root.is_dir():
        print(f"erro: --root inexistente: {args.root}", file=sys.stderr)
        return 2
    r = resumo(
        varrer(args.root, desde), gate_log=args.gate_log, nudge_log=args.nudge_log
    )
    if args.emit:
        comando = "python3 scripts/model_routing_adherence.py " + " ".join(
            sys.argv[1:] if argv is None else argv
        )
        agora = dt.datetime.now(dt.timezone.utc)
        gravar_atomico(
            args.emit, payload_do_kpi(r, min_n=args.min_n, agora=agora, comando=comando)
        )
    if args.json and not args.brief:
        print(json.dumps(r, ensure_ascii=False, indent=2))
    else:
        print(brief(r))
    return 0


if __name__ == "__main__":
    sys.exit(main())
