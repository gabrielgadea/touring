"""Estado no instante COMPLETO (HO13, 10/10/2026) — TDD com fixture SINTETICA (nenhum dado de held-out).

O caso real que motivou: `from scripts.eleitoral import publicar_mapa as pm` — o manifesto sobrepunha so o
`scripts/eleitoral/__init__.py`, e o motor julgou `pm._publicar` contra o `publicar_mapa.py` de HOJE (a funcao hoje se
chama outra coisa). Regra pre-registrada: o manifesto sobrepoe TODO modulo do projeto que a entrada importa — `import a.b`,
`from a import b` (b submodulo ou subpacote), relativos — e uma entrada cujo modulo nao reconstroi no instante e INCOMPLETA.

O modulo sob teste vem de `$ESTADO_MODULO` (o mutante roda sobre uma COPIA); sem a variavel, o original ao lado.
"""

import importlib.util
import json
import os
import sys
from pathlib import Path

import pytest

SCRIPTS = Path(__file__).resolve().parent
sys.path.insert(0, str(SCRIPTS))  # a copia mutada importa `corpus_v0d` daqui
import corpus_v0d  # noqa: E402

CAMINHO = Path(os.environ.get("ESTADO_MODULO", SCRIPTS / "estado_no_instante_v0d.py"))
_spec = importlib.util.spec_from_file_location("estado_sob_teste", CAMINHO)
eni = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(eni)

HOJE = "def _publicar_e_conferir():\n    return 'hoje'\n"
INSTANTE = "def _publicar():\n    return 'instante'\n"
ARQUIVOS = {
    "scripts/__init__.py": "",
    "scripts/eleitoral/__init__.py": "from .publicar_mapa import _publicar_e_conferir\n",
    "scripts/eleitoral/publicar_mapa.py": HOJE,
    "scripts/eleitoral/outro/__init__.py": "",
    "scripts/eleitoral/outro/folha.py": "x = 1\n",
    "scripts/eleitoral/vizinho.py": "from . import publicar_mapa\nfrom .outro import folha\n",
    "util.py": "",
    "solto/semi.py": "import irmao\n",
    "solto/irmao.py": "y = 2\n",
}


@pytest.fixture
def raiz(tmp_path: Path) -> Path:
    r = tmp_path / "proj"
    for rel, texto in ARQUIVOS.items():
        (r / rel).parent.mkdir(parents=True, exist_ok=True)
        (r / rel).write_text(texto)
    return r


def inline(raiz: Path) -> dict:
    return {"project_root": str(raiz), "path": "<stdin>", "ts": "2026-10-01T10:00:00.000Z", "id": "inl"}


def de_arquivo(raiz: Path, rel: str) -> dict:
    return {"project_root": str(raiz), "path": str(raiz / rel), "ts": "2026-10-01T10:00:00.000Z", "id": "arq"}


def rels(raiz: Path, achados) -> set[str]:
    return {str(Path(p).resolve().relative_to(raiz.resolve())) for p in achados}


def modulos(raiz: Path, e: dict, texto: str) -> set[str]:
    return rels(raiz, eni.modulos_do_projeto(e, texto))


INITS = {"scripts/__init__.py", "scripts/eleitoral/__init__.py"}


# --------------------------------------------------------------------------- a resolucao dos imports


def test_from_pacote_import_submodulo_o_caso_real(raiz):
    """`from scripts.eleitoral import publicar_mapa as pm`: o submodulo entra, nao so o __init__."""
    achados = modulos(raiz, inline(raiz), "from scripts.eleitoral import publicar_mapa as pm\nprint(pm._publicar)\n")
    assert achados == INITS | {"scripts/eleitoral/publicar_mapa.py"}


def test_import_pontilhado_traz_os_pacotes_ancestrais(raiz):
    achados = modulos(raiz, inline(raiz), "import scripts.eleitoral.publicar_mapa\n")
    assert achados == INITS | {"scripts/eleitoral/publicar_mapa.py"}


def test_import_pontilhado_com_alias(raiz):
    achados = modulos(raiz, inline(raiz), "import scripts.eleitoral.publicar_mapa as pm\n")
    assert achados == INITS | {"scripts/eleitoral/publicar_mapa.py"}


def test_from_pacote_import_subpacote(raiz):
    achados = modulos(raiz, inline(raiz), "from scripts.eleitoral import outro\n")
    assert achados == INITS | {"scripts/eleitoral/outro/__init__.py"}


def test_from_pontilhado_import_submodulo(raiz):
    achados = modulos(raiz, inline(raiz), "from scripts.eleitoral.outro import folha\n")
    assert achados == INITS | {"scripts/eleitoral/outro/__init__.py", "scripts/eleitoral/outro/folha.py"}


def test_varios_nomes_no_mesmo_from(raiz):
    achados = modulos(raiz, inline(raiz), "from scripts.eleitoral import publicar_mapa, outro, _inexistente\n")
    assert achados == INITS | {"scripts/eleitoral/publicar_mapa.py", "scripts/eleitoral/outro/__init__.py"}


def test_nome_que_nao_e_modulo_nao_inventa_arquivo(raiz):
    """`publicar` e uma funcao, nao um arquivo: so os __init__ entram."""
    achados = modulos(raiz, inline(raiz), "from scripts.eleitoral import publicar\n")
    assert achados == INITS


def test_import_estrela_so_o_pacote(raiz):
    assert modulos(raiz, inline(raiz), "from scripts.eleitoral import *\n") == INITS


def test_relativos(raiz):
    e = de_arquivo(raiz, "scripts/eleitoral/vizinho.py")
    texto = ARQUIVOS["scripts/eleitoral/vizinho.py"]
    assert modulos(raiz, e, texto) == {
        "scripts/eleitoral/__init__.py",  # o pacote corrente roda o próprio __init__
        "scripts/eleitoral/publicar_mapa.py",
        "scripts/eleitoral/outro/__init__.py",
        "scripts/eleitoral/outro/folha.py",
    }


def test_irmao_do_script_segue_resolvendo(raiz):
    """A segunda base (o diretorio do script) — o que a regra antiga ja fazia — continua valendo."""
    e = de_arquivo(raiz, "solto/semi.py")
    assert modulos(raiz, e, ARQUIVOS["solto/semi.py"]) == {"solto/irmao.py"}


def test_externo_nao_entra(raiz):
    assert modulos(raiz, inline(raiz), "import os, json\nfrom collections import abc\n") == set()


def test_modulo_fora_da_raiz_do_projeto_nao_entra(raiz, tmp_path):
    """Um irmão do script que mora FORA da raiz do projeto (ex.: em /tmp) não é módulo do projeto."""
    fora = tmp_path / "fora"
    fora.mkdir()
    (fora / "a.py").write_text("import b\n")
    (fora / "b.py").write_text("")
    e = {"project_root": str(raiz), "path": str(fora / "a.py"), "ts": "2026-10-01T10:00:00.000Z", "id": "f"}
    assert modulos(raiz, e, "import b\n") == set()


def test_o_proprio_arquivo_nao_e_overlay_de_si(raiz):
    e = de_arquivo(raiz, "util.py")
    assert modulos(raiz, e, "import util\n") == set()


def test_a_regra_antiga_so_via_o_init_o_defeito_documentado(raiz):
    """O legado devolve SO o __init__ no caso real — e o defeito que o teste 1 fecha."""
    achados = rels(raiz, eni.modulos_do_projeto_legado(inline(raiz), "from scripts.eleitoral import publicar_mapa as pm\n"))
    assert achados == {"scripts/eleitoral/__init__.py"}


def test_tipos_dos_modulos(raiz):
    tipos = {str(Path(p).resolve().relative_to(raiz.resolve())): t for p, t in eni.modulos_tipados(inline(raiz), "from scripts.eleitoral import publicar_mapa\n").items()}
    assert tipos == {"scripts/__init__.py": "ancestral", "scripts/eleitoral/__init__.py": "direto", "scripts/eleitoral/publicar_mapa.py": "submodulo"}


# --------------------------------------------------------------------------- completude e overlay do INSTANTE


def conteudo_de(faltando: set[str] | None = None):
    faltando = faltando or set()

    def fake(arq: str, ts: str, indice: dict) -> str | None:
        return None if Path(arq).name in faltando else (INSTANTE if Path(arq).name == "publicar_mapa.py" else "")

    return fake


def test_estado_completo_quando_tudo_reconstroi(raiz, monkeypatch):
    monkeypatch.setattr(corpus_v0d, "_conteudo_depois", conteudo_de())
    assert eni.estado_completo(inline(raiz), "from scripts.eleitoral import publicar_mapa as pm\n", {}) == (True, "ok")


def test_incompleta_quando_o_submodulo_nao_reconstroi(raiz, monkeypatch):
    monkeypatch.setattr(corpus_v0d, "_conteudo_depois", conteudo_de({"publicar_mapa.py"}))
    ok, motivo = eni.estado_completo(inline(raiz), "from scripts.eleitoral import publicar_mapa as pm\n", {})
    assert ok is False and motivo == "estado incompleto: submódulo importado não reconstrói no instante"


def test_incompleta_quando_o_init_nao_reconstroi(raiz, monkeypatch):
    monkeypatch.setattr(corpus_v0d, "_conteudo_depois", conteudo_de({"__init__.py"}))
    ok, motivo = eni.estado_completo(inline(raiz), "import scripts.eleitoral.publicar_mapa\n", {})
    assert ok is False and motivo.startswith("estado incompleto: ")


def test_sem_imports_do_projeto_e_completa(raiz, monkeypatch):
    monkeypatch.setattr(corpus_v0d, "_conteudo_depois", conteudo_de({"publicar_mapa.py"}))
    assert eni.estado_completo(inline(raiz), "import os\nprint(1)\n", {}) == (True, "ok")


def test_overlay_carrega_o_instante_nao_o_hoje(raiz, monkeypatch, tmp_path):
    monkeypatch.setattr(corpus_v0d, "_conteudo_depois", conteudo_de())
    destino = tmp_path / "corpus" / "estado_no_instante"
    destino.mkdir(parents=True)
    over, faltas = eni.overlays_e_faltas(inline(raiz), "from scripts.eleitoral import publicar_mapa as pm\n", {}, destino)
    assert faltas == []
    por_caminho = {Path(o["path"]).name: o for o in over}
    assert len(over) == 3
    arq = destino.parent / por_caminho["publicar_mapa.py"]["file"]
    assert arq.read_text() == INSTANTE and arq.read_text() != (raiz / "scripts/eleitoral/publicar_mapa.py").read_text()
    assert por_caminho["publicar_mapa.py"]["sha256"] == eni.sha(INSTANTE)


def test_overlays_e_faltas_nomeia_o_que_faltou(raiz, monkeypatch, tmp_path):
    monkeypatch.setattr(corpus_v0d, "_conteudo_depois", conteudo_de({"publicar_mapa.py"}))
    destino = tmp_path / "corpus" / "estado_no_instante"
    destino.mkdir(parents=True)
    over, faltas = eni.overlays_e_faltas(inline(raiz), "from scripts.eleitoral import publicar_mapa as pm\n", {}, destino)
    assert len(over) == 2 and [Path(f).name for f in faltas] == ["publicar_mapa.py"]


def test_o_fecho_cobre_o_que_a_profundidade_2_so_media(raiz, monkeypatch, tmp_path):
    """HO13f: `publicar_mapa.py` (do INSTANTE) importa `util` e `.outro.folha` — antes só MEDIDOS fora do manifesto (91 das
    1.153 entradas do 20261013 tinham lacuna); o fecho transitivo os sobrepõe e a lacuna some."""
    instante = "import util\nfrom .outro import folha\nfrom . import publicar_mapa\n"
    monkeypatch.setattr(corpus_v0d, "_conteudo_depois", lambda arq, ts, indice: instante if Path(arq).name == "publicar_mapa.py" else "")
    destino = tmp_path / "corpus" / "estado_no_instante"
    destino.mkdir(parents=True)
    e = inline(raiz)
    over, faltas = eni.overlays_e_faltas(e, "from scripts.eleitoral import publicar_mapa as pm\n", {}, destino)
    assert faltas == []
    assert {"util.py", "scripts/eleitoral/outro/__init__.py", "scripts/eleitoral/outro/folha.py"} <= rels(raiz, [o["path"] for o in over])
    assert eni.importacoes_nao_cobertas(e, over, destino) == []


def test_o_oraculo_de_cobertura_enxerga_a_lacuna_quando_ela_existe(raiz, monkeypatch, tmp_path):
    """Controle positivo do verificador independente (`importacoes_nao_cobertas`): tirar um overlay do fecho TEM de acusar."""
    instante = "import util\n"
    monkeypatch.setattr(corpus_v0d, "_conteudo_depois", lambda arq, ts, indice: instante if Path(arq).name == "publicar_mapa.py" else "")
    destino = tmp_path / "corpus" / "estado_no_instante"
    destino.mkdir(parents=True)
    e = inline(raiz)
    over, _ = eni.overlays_e_faltas(e, "from scripts.eleitoral import publicar_mapa as pm\n", {}, destino)
    sem_util = [o for o in over if Path(o["path"]).name != "util.py"]
    assert len(sem_util) == len(over) - 1
    assert [Path(p).name for p in eni.importacoes_nao_cobertas(e, sem_util, destino)] == ["util.py"]


def test_profundidade_2_nao_conta_o_que_ja_esta_coberto_nem_a_propria_entrada(raiz, monkeypatch, tmp_path):
    monkeypatch.setattr(corpus_v0d, "_conteudo_depois", lambda arq, ts, indice: "from . import vizinho\n")
    destino = tmp_path / "corpus" / "estado_no_instante"
    destino.mkdir(parents=True)
    e = de_arquivo(raiz, "scripts/eleitoral/vizinho.py")
    over, _ = eni.overlays_e_faltas(e, ARQUIVOS["scripts/eleitoral/vizinho.py"], {}, destino)
    # os módulos sobrepostos importam `vizinho` (a própria entrada) e irmãos já cobertos: nada a mais
    assert eni.importacoes_nao_cobertas(e, over, destino) == []


def test_overlays_legado_continua_pulando_em_silencio(raiz, monkeypatch, tmp_path):
    """`overlays()` mantem a assinatura e o comportamento (quem nao exige completude nao muda)."""
    monkeypatch.setattr(corpus_v0d, "_conteudo_depois", conteudo_de({"publicar_mapa.py"}))
    destino = tmp_path / "corpus" / "estado_no_instante"
    destino.mkdir(parents=True)
    assert len(eni.overlays(inline(raiz), "from scripts.eleitoral import publicar_mapa as pm\n", {}, destino)) == 2


# --------------------------------------------------------------------------- HO13f: o FECHO TRANSITIVO
#
# O que o P0M ensinou e o 20261013 nao curou: a regra de profundidade 1 sobrepoe o que a ENTRADA importa, mas o motor segue os
# imports DENTRO dos modulos sobrepostos — um `pkg/__init__.py` que reexporta `from .nucleo import f` faz o motor ler o
# `nucleo.py` DE HOJE. Regra nova (pre-registrada antes de reconstruir): todo modulo do projeto ALCANCAVEL pelos imports, em
# qualquer profundidade (lendo o conteudo do INSTANTE de cada um), e sobreposto; um irreconstrutivel deixa a entrada INCOMPLETA.

FECHO = {
    "pkg/__init__.py": "from .nucleo import f\n",  # nivel 1: reexporta de `nucleo`
    "pkg/nucleo.py": "from .base import g as f\n",  # nivel 2: reexporta de `base`
    "pkg/base.py": "def g():\n    return 'hoje'\n",  # nivel 3
    "util.py": "w = 1\n",
    "ciclo/__init__.py": "",
    "ciclo/a.py": "from . import b\n",
    "ciclo/b.py": "from . import a\n",
    "app/__init__.py": "",
    "app/main.py": "from app import helper\n",
    "app/helper.py": "from app import main\n",
}
BASE_DO_INSTANTE = "def g():\n    return 'instante'\n"


@pytest.fixture
def raiz_fecho(tmp_path: Path) -> Path:
    r = tmp_path / "projfecho"
    for rel, texto in FECHO.items():
        (r / rel).parent.mkdir(parents=True, exist_ok=True)
        (r / rel).write_text(texto)
    return r


def conteudo_fecho(raiz_f: Path, faltando: set[str] | None = None, instante: dict[str, str] | None = None):
    """O `_conteudo_depois` da fixture: o texto em disco, salvo o que `instante` reescreve; `faltando` = irreconstrutivel."""
    faltando, instante = faltando or set(), instante or {}

    def fake(arq: str, ts: str, indice: dict) -> str | None:
        rel = str(Path(arq).resolve().relative_to(raiz_f.resolve()))
        if rel in faltando:
            return None
        return instante[rel] if rel in instante else Path(arq).read_text()

    return fake


def prof(raiz_f: Path, fecho) -> dict[str, int]:
    return {str(Path(m["path"]).resolve().relative_to(raiz_f.resolve())): m["profundidade"] for m in fecho}


ENTRADA_PKG = "from pkg import f\n"
MOTIVO_FECHO = "estado incompleto: módulo do fecho transitivo (profundidade ≥ 2) não reconstrói no instante"


def test_fecho_alcanca_o_reexporte_de_2_niveis(raiz_fecho, monkeypatch):
    monkeypatch.setattr(corpus_v0d, "_conteudo_depois", conteudo_fecho(raiz_fecho))
    fecho = eni.fecho_transitivo(inline(raiz_fecho), ENTRADA_PKG, {})
    assert prof(raiz_fecho, fecho) == {"pkg/__init__.py": 1, "pkg/nucleo.py": 2, "pkg/base.py": 3}


def test_estado_completo_exige_o_modulo_de_profundidade_2(raiz_fecho, monkeypatch):
    monkeypatch.setattr(corpus_v0d, "_conteudo_depois", conteudo_fecho(raiz_fecho, {"pkg/nucleo.py"}))
    assert eni.estado_completo(inline(raiz_fecho), ENTRADA_PKG, {}) == (False, MOTIVO_FECHO)


def test_estado_completo_exige_o_modulo_de_profundidade_3(raiz_fecho, monkeypatch):
    monkeypatch.setattr(corpus_v0d, "_conteudo_depois", conteudo_fecho(raiz_fecho, {"pkg/base.py"}))
    assert eni.estado_completo(inline(raiz_fecho), ENTRADA_PKG, {}) == (False, MOTIVO_FECHO)


def test_estado_completo_quando_o_fecho_inteiro_reconstroi(raiz_fecho, monkeypatch):
    monkeypatch.setattr(corpus_v0d, "_conteudo_depois", conteudo_fecho(raiz_fecho))
    assert eni.estado_completo(inline(raiz_fecho), ENTRADA_PKG, {}) == (True, "ok")


def test_a_falta_de_profundidade_1_mantem_o_motivo_antigo(raiz_fecho, monkeypatch):
    """Com falta rasa E funda, vale o motivo da rasa — as contagens por motivo de antes e depois do fecho seguem comparáveis."""
    monkeypatch.setattr(corpus_v0d, "_conteudo_depois", conteudo_fecho(raiz_fecho, {"util.py", "pkg/nucleo.py"}))
    ok, motivo = eni.estado_completo(inline(raiz_fecho), "import util\nfrom pkg import f\n", {})
    assert ok is False and motivo == "estado incompleto: módulo importado não reconstrói no instante"


def test_avaliar_na_profundidade_1_reproduz_a_regra_antiga_e_o_fecho_a_nova(raiz_fecho, monkeypatch):
    """A medida do viés: a MESMA entrada passa na regra de profundidade 1 e reprova no fecho."""
    monkeypatch.setattr(corpus_v0d, "_conteudo_depois", conteudo_fecho(raiz_fecho, {"pkg/nucleo.py"}))
    fecho = eni.fecho_transitivo(inline(raiz_fecho), ENTRADA_PKG, {})
    assert eni.avaliar(fecho, 1) == (True, "ok")
    assert eni.avaliar(fecho) == (False, MOTIVO_FECHO)


def test_irreconstrutivel_nao_se_expande_e_nao_levanta(raiz_fecho, monkeypatch):
    monkeypatch.setattr(corpus_v0d, "_conteudo_depois", conteudo_fecho(raiz_fecho, {"pkg/nucleo.py"}))
    fecho = eni.fecho_transitivo(inline(raiz_fecho), ENTRADA_PKG, {})
    assert prof(raiz_fecho, fecho) == {"pkg/__init__.py": 1, "pkg/nucleo.py": 2}  # `base` so existe pelo conteudo do nucleo
    assert [m["conteudo"] is None for m in fecho if str(m["path"]).endswith("nucleo.py")] == [True]


def test_overlays_cobrem_todo_o_fecho_com_o_conteudo_do_instante(raiz_fecho, monkeypatch, tmp_path):
    monkeypatch.setattr(corpus_v0d, "_conteudo_depois", conteudo_fecho(raiz_fecho, instante={"pkg/base.py": BASE_DO_INSTANTE}))
    destino = tmp_path / "corpus" / "estado_no_instante"
    destino.mkdir(parents=True)
    over, faltas = eni.overlays_e_faltas(inline(raiz_fecho), ENTRADA_PKG, {}, destino)
    assert faltas == []
    por_rel = {str(Path(o["path"]).resolve().relative_to(raiz_fecho.resolve())): o for o in over}
    assert set(por_rel) == {"pkg/__init__.py", "pkg/nucleo.py", "pkg/base.py"}
    base = por_rel["pkg/base.py"]
    assert (destino.parent / base["file"]).read_text() == BASE_DO_INSTANTE and base["sha256"] == eni.sha(BASE_DO_INSTANTE)
    assert (raiz_fecho / "pkg/base.py").read_text() != BASE_DO_INSTANTE  # o disco de hoje e outro: o overlay NAO e o de hoje


def test_overlays_e_faltas_nomeia_o_irreconstrutivel_do_fecho(raiz_fecho, monkeypatch, tmp_path):
    monkeypatch.setattr(corpus_v0d, "_conteudo_depois", conteudo_fecho(raiz_fecho, {"pkg/nucleo.py"}))
    destino = tmp_path / "corpus" / "estado_no_instante"
    destino.mkdir(parents=True)
    over, faltas = eni.overlays_e_faltas(inline(raiz_fecho), ENTRADA_PKG, {}, destino)
    assert [Path(f).name for f in faltas] == ["nucleo.py"] and [Path(o["path"]).name for o in over] == ["__init__.py"]


def test_o_fecho_e_fechado_e_o_oraculo_independente_concorda(raiz_fecho, monkeypatch, tmp_path):
    """`importacoes_nao_cobertas` (leitura independente do BFS) acha 0 lacuna no fecho; sem o `base.py` ela acusa."""
    monkeypatch.setattr(corpus_v0d, "_conteudo_depois", conteudo_fecho(raiz_fecho))
    destino = tmp_path / "corpus" / "estado_no_instante"
    destino.mkdir(parents=True)
    e = inline(raiz_fecho)
    over, _ = eni.overlays_e_faltas(e, ENTRADA_PKG, {}, destino)
    assert eni.importacoes_nao_cobertas(e, over, destino) == []
    sem_base = [o for o in over if Path(o["path"]).name != "base.py"]
    assert [Path(p).name for p in eni.importacoes_nao_cobertas(e, sem_base, destino)] == ["base.py"]


def test_fecho_segue_os_imports_do_instante_nao_os_de_hoje(raiz_fecho, monkeypatch):
    """Hoje `pkg/__init__.py` importa `nucleo`; no INSTANTE importava `base` direto — o fecho segue o instante."""
    instante = {"pkg/__init__.py": "from .base import g as f\n"}
    monkeypatch.setattr(corpus_v0d, "_conteudo_depois", conteudo_fecho(raiz_fecho, instante=instante))
    fecho = eni.fecho_transitivo(inline(raiz_fecho), ENTRADA_PKG, {})
    assert prof(raiz_fecho, fecho) == {"pkg/__init__.py": 1, "pkg/base.py": 2}


def test_ciclo_termina_e_cada_modulo_aparece_uma_vez(raiz_fecho, monkeypatch):
    monkeypatch.setattr(corpus_v0d, "_conteudo_depois", conteudo_fecho(raiz_fecho))
    fecho = eni.fecho_transitivo(inline(raiz_fecho), "from ciclo import a\n", {})
    assert sorted(prof(raiz_fecho, fecho)) == ["ciclo/__init__.py", "ciclo/a.py", "ciclo/b.py"] and len(fecho) == 3


def test_o_proprio_arquivo_da_entrada_nao_entra_no_fecho(raiz_fecho, monkeypatch):
    """`helper` importa `main` de volta: `main.py` e a entrada (o `--content-file`), nunca um overlay dela mesma."""
    monkeypatch.setattr(corpus_v0d, "_conteudo_depois", conteudo_fecho(raiz_fecho))
    e = de_arquivo(raiz_fecho, "app/main.py")
    fecho = eni.fecho_transitivo(e, FECHO["app/main.py"], {})
    assert sorted(prof(raiz_fecho, fecho)) == ["app/__init__.py", "app/helper.py"]


def test_profundidade_registrada_e_a_menor_entre_os_caminhos(raiz_fecho, monkeypatch):
    """`pkg/base.py` e alcancavel por 2 caminhos (direto e via nucleo); vale o mais raso."""
    monkeypatch.setattr(corpus_v0d, "_conteudo_depois", conteudo_fecho(raiz_fecho))
    fecho = eni.fecho_transitivo(inline(raiz_fecho), "from pkg import f\nfrom pkg import base\n", {})
    assert prof(raiz_fecho, fecho)["pkg/base.py"] == 1


def test_distribuicao_do_tamanho_do_fecho():
    d = eni.distribuicao(list(range(1, 11)))
    assert d == {"n": 10, "soma": 55, "min": 1, "mediana": 5, "media": 5.5, "p90": 9, "p99": 10, "max": 10}
    assert eni.distribuicao([]) == {"n": 0, "soma": 0, "min": 0, "mediana": 0, "media": 0.0, "p90": 0, "p99": 0, "max": 0}
    assert eni.distribuicao([7])["p99"] == 7


def test_construir_grava_o_fecho_no_manifesto_e_o_oraculo_fecha(raiz_fecho, monkeypatch, tmp_path):
    """Ponta a ponta no manifesto: tamanho/profundidade do fecho, 0 lacuna pelo verificador independente, nenhuma incompleta."""
    corpus = tmp_path / "corpus"
    (corpus / "snap").mkdir(parents=True)
    (corpus / "snap" / "a.py").write_text(ENTRADA_PKG)
    entrada = {"id": "a", "rotulo": "positivo", "path": "<stdin>", "project_root": str(raiz_fecho), "ts": "2026-10-01T10:00:00.000Z", "snap": "snap/a.py"}
    (corpus / "entradas.jsonl").write_text(json.dumps(entrada) + "\n")
    monkeypatch.setattr(corpus_v0d, "_conteudo_depois", conteudo_fecho(raiz_fecho))
    monkeypatch.setattr(corpus_v0d, "varrer_transcripts", lambda: {})
    resumo = eni.construir(corpus)
    manifesto = json.loads((corpus / "estado_no_instante" / "manifesto.json").read_text())["a"]
    assert manifesto["completo"] is True and manifesto["faltantes"] == []
    assert manifesto["fecho"] == {"tamanho": 3, "profundidade_max": 3, "por_profundidade": {"1": 1, "2": 1, "3": 1}}
    assert manifesto["nao_cobertas_prof2"] == [] and len(manifesto["overlays"]) == 3
    assert resumo["entradas_incompletas"] == 0 and resumo["entradas_com_importacao_de_profundidade_2_nao_coberta"] == 0
    assert resumo["fecho"]["entradas_por_profundidade_maxima"] == {"3": 1} and resumo["fecho"]["tamanho_por_entrada"]["max"] == 3


def test_selftest_do_modulo_continua_passando():
    assert eni.selftest() == 0
