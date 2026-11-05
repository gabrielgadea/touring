"""Contract of the single portfolio decision: the four veredito_gate/1 states, independent of the tau chosen."""

from __future__ import annotations

import json
import socket as _socket
import subprocess
import sys
import tempfile
import threading
import time
from pathlib import Path

import pytest

AQUI = Path(__file__).resolve().parent
sys.path.insert(0, str(AQUI))

import recuperar

PROGRAMA = "\n".join(f"linha_{i} = calcular(valor_{i}, base) + ajuste_{i}" for i in range(12))
EDITADO = PROGRAMA.replace("ajuste_3", "correcao_3") + "\n# comentario novo\n"
OUTRO = "\n".join(f"for item in lista_{i}:\n    print(item.nome, item.peso, item.custo)" for i in range(8))


def _c(cid: str, conteudo: str, nome: str = "x.py", proposito: str = "") -> recuperar.Candidato:
    return recuperar.Candidato(id=cid, nome=nome, conteudo=conteudo, proposito=proposito)


def test_copia_editada_e_reprovado() -> None:
    v = recuperar.decidir("novo.py", EDITADO, [_c("a", PROGRAMA), _c("b", OUTRO)])
    assert v.estado == "reprovado"
    assert v.candidatos[0][0] == "a"
    assert v.candidatos[0][1] == "corpo"
    assert v.exit_code == 1


def test_programa_distinto_e_aprovado() -> None:
    v = recuperar.decidir("novo.py", OUTRO, [_c("a", PROGRAMA)])
    assert v.estado == "aprovado"
    assert v.exit_code == 0


def test_fonte_fora_do_ar_e_inaferivel_nunca_aprovado() -> None:
    v = recuperar.decidir("novo.py", OUTRO, [_c("a", PROGRAMA)], fontes_falhas=["tantivy (TimeoutExpired)"])
    assert v.estado == "inaferivel"
    assert v.exit_code == 2
    assert "tantivy" in v.motivo


def test_evidencia_forte_vale_mesmo_com_fonte_falha() -> None:
    # Finding the equivalent in one source is evidence; the other source's failure does not erase it.
    v = recuperar.decidir("novo.py", EDITADO, [_c("a", PROGRAMA)], fontes_falhas=["tantivy (RuntimeError)"])
    assert v.estado == "reprovado"
    assert v.fontes_falhas == ("tantivy (RuntimeError)",)


def test_nao_script_nao_aplicavel() -> None:
    assert recuperar.decidir("notas.md", PROGRAMA, [_c("a", PROGRAMA)]).estado == "nao_aplicavel"


def test_corpo_curto_nunca_reprova() -> None:
    curto = "x = 1\nprint(x)\n"
    v = recuperar.decidir("novo.py", curto, [_c("a", curto)])
    assert v.estado == "aprovado"
    assert "curto" in v.motivo


def test_proposito_sozinho_aconselha_mas_nao_reprova() -> None:
    v = recuperar.decidir(
        "baixar_proveniencia.py",
        "# baixa a proveniencia das fontes oficiais\n" + OUTRO,
        [_c("a", PROGRAMA, "fetch_proveniencia.py", "baixa a proveniencia das fontes oficiais")],
    )
    assert v.estado == "aprovado"
    assert v.candidatos
    assert v.candidatos[0][1] == "proposito"


def test_fonte_manifest_ausente_e_vazia_e_ilegivel_levanta(tmp_path: Path) -> None:
    assert recuperar.fonte_manifest(tmp_path / "nao_existe.json") == []
    ruim = tmp_path / "MANIFEST.json"
    ruim.write_text("{nao é json")
    with pytest.raises(ValueError):
        recuperar.fonte_manifest(ruim)


def test_cli_exit_deriva_do_estado(tmp_path: Path) -> None:
    alvo = tmp_path / "notas.md"
    alvo.write_text("texto")
    r = subprocess.run(
        [sys.executable, str(AQUI / "recuperar.py"), "--arquivo", str(alvo)],
        capture_output=True,
        text=True,
        timeout=30,
        check=False,
    )
    assert r.returncode == 0
    assert json.loads(r.stdout)["estado"] == "nao_aplicavel"


def test_tantivy_com_cwd_removido_nao_vira_inaferivel(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    # Defect of 28/10/2026: a session directory removed before the hook ran made subprocess.run(cwd=…) raise
    # FileNotFoundError, and the whole tantivy source read as failed. The search needs no cwd.
    bin_dir = tmp_path / "bin"
    bin_dir.mkdir()
    falso = bin_dir / "touring"
    falso.write_text("#!/bin/sh\nprintf '[]'\n")
    falso.chmod(0o755)
    monkeypatch.setenv("PATH", f"{bin_dir}:/usr/bin:/bin")
    # This case is about the SUBPROCESS route: hide the ambient daemon socket, which answers first since T3.
    for var in ("TOURING_DAEMON_SOCKET", "TOURING_DAEMON_SOCK", "CLAUDE_PROJECT_DIR"):
        monkeypatch.delenv(var, raising=False)
    removido = tmp_path / "sessao_que_sumiu"
    assert not removido.exists()
    assert recuperar.fonte_tantivy("calcular ajuste base", cwd=str(removido)) == []


# --- tantivy through the daemon socket (T3, 04/11/2026): a persistent process instead of a fork+exec ---------------


HIT = {"symbol_name": "calcula ajuste", "file_path": "/abs/alvo_do_socket.py"}


class _Daemon:
    """A fake daemon on a UNIX socket: records each request line and answers with ``resposta`` (bytes) or hangs."""

    def __init__(self, sock_path: str, resposta: bytes | None) -> None:
        self.requests: list[dict] = []
        self._resposta = resposta
        self._srv = _socket.socket(_socket.AF_UNIX, _socket.SOCK_STREAM)
        self._srv.bind(sock_path)
        self._srv.listen(4)
        self._fim = threading.Event()
        self._t = threading.Thread(target=self._servir, daemon=True)
        self._t.start()

    def _servir(self) -> None:
        self._srv.settimeout(0.1)
        while not self._fim.is_set():
            try:
                conn, _ = self._srv.accept()
            except (TimeoutError, OSError):
                continue
            with conn:
                linha = b""
                while not linha.endswith(b"\n"):
                    pedaco = conn.recv(65536)
                    if not pedaco:
                        break
                    linha += pedaco
                try:
                    self.requests.append(json.loads(linha))
                except ValueError:
                    self.requests.append({"cru": linha.decode(errors="replace")})
                if self._resposta is None:  # hang: never answer, never close, until the test ends
                    self._fim.wait(5)
                else:
                    conn.sendall(self._resposta)

    def fechar(self) -> None:
        self._fim.set()
        self._t.join(timeout=2)
        self._srv.close()


def _resp(output: str, success: bool = True) -> bytes:
    return (json.dumps({"output": output, "success": success}) + "\n").encode()


@pytest.fixture
def curto() -> Path:
    """A short directory under /tmp: AF_UNIX paths are capped at ~108 bytes, and tmp_path can exceed that."""
    with tempfile.TemporaryDirectory(dir="/tmp", prefix="tsock") as d:
        yield Path(d)


@pytest.fixture
def ambiente(curto: Path, monkeypatch: pytest.MonkeyPatch) -> Path:
    """No ambient daemon: env cleared, PATH without a real touring; the test decides what exists."""
    for var in ("TOURING_DAEMON_SOCKET", "TOURING_DAEMON_SOCK", "CLAUDE_PROJECT_DIR"):
        monkeypatch.delenv(var, raising=False)
    (curto / "bin").mkdir()
    monkeypatch.setenv("PATH", f"{curto / 'bin'}:/usr/bin:/bin")
    return curto


def _touring_falso(curto: Path, stdout: str, rc: int = 0) -> None:
    falso = curto / "bin" / "touring"
    falso.write_text(f"#!/bin/sh\nprintf '%s' '{stdout}'\nexit {rc}\n")
    falso.chmod(0o755)


def _sem_subprocesso(monkeypatch: pytest.MonkeyPatch) -> list[list[str]]:
    chamadas: list[list[str]] = []

    def _proibido(argv, *a, **k):
        chamadas.append(list(argv))
        raise AssertionError("subprocess must not run when the daemon answers")

    monkeypatch.setattr(recuperar.subprocess, "run", _proibido)
    return chamadas


def test_socket_feliz_responde_sem_subprocesso(ambiente: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    sock = str(ambiente / "d.sock")
    daemon = _Daemon(sock, _resp(json.dumps([HIT])))
    monkeypatch.setenv("TOURING_DAEMON_SOCKET", sock)
    chamadas = _sem_subprocesso(monkeypatch)
    proj = ambiente / "proj"
    (proj / ".git").mkdir(parents=True)
    try:
        out = recuperar.fonte_tantivy("calcular ajuste base", cwd=str(proj))
    finally:
        daemon.fechar()
    assert [c.caminho for c in out] == ["/abs/alvo_do_socket.py"]
    assert out[0].origem == "tantivy"
    assert chamadas == []
    req = daemon.requests[0]
    assert req["hook"] == "cli-tantivy-search"
    assert req["payload"] == {"query": "calcular ajuste base", "top": 5}
    assert req["project_root"] == str(proj)  # normalized like the CLI: nearest dir with .git/.touring/[workspace]
    assert recuperar.ULTIMA_ROTA_TANTIVY == "socket"


def test_socket_achado_por_subida_de_diretorio(ambiente: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    proj = ambiente / "p"
    (proj / ".touring").mkdir(parents=True)
    (proj / "sub" / "fundo").mkdir(parents=True)
    daemon = _Daemon(str(proj / ".touring" / "daemon.sock"), _resp(json.dumps([HIT])))
    chamadas = _sem_subprocesso(monkeypatch)
    try:
        out = recuperar.fonte_tantivy("calcular ajuste base", cwd=str(proj / "sub" / "fundo"))
    finally:
        daemon.fechar()
    assert len(out) == 1
    assert chamadas == []
    assert daemon.requests[0]["project_root"] == str(proj)


def test_socket_do_ambiente_inexistente_cai_na_subida(ambiente: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    # A stale TOURING_DAEMON_SOCKET must not disable the walk-up daemon.
    proj = ambiente / "p"
    (proj / ".touring").mkdir(parents=True)
    daemon = _Daemon(str(proj / ".touring" / "daemon.sock"), _resp(json.dumps([HIT])))
    monkeypatch.setenv("TOURING_DAEMON_SOCKET", str(ambiente / "morto.sock"))
    chamadas = _sem_subprocesso(monkeypatch)
    try:
        out = recuperar.fonte_tantivy("calcular ajuste base", cwd=str(proj))
    finally:
        daemon.fechar()
    assert len(out) == 1
    assert chamadas == []


def test_socket_ausente_cai_no_subprocesso_e_nomeia_a_queda(ambiente: Path) -> None:
    _touring_falso(ambiente, json.dumps([HIT]))
    out = recuperar.fonte_tantivy("calcular ajuste base", cwd=str(ambiente))
    assert [c.caminho for c in out] == ["/abs/alvo_do_socket.py"]
    assert recuperar.ULTIMA_ROTA_TANTIVY.startswith("subprocesso")
    assert "socket" in recuperar.ULTIMA_ROTA_TANTIVY


@pytest.mark.parametrize(
    "resposta",
    [
        b"isto nao e json\n",
        b"",  # the daemon hangs up without answering
        _resp(json.dumps({"error": "boom"}), success=False),
        _resp(json.dumps({"error": "indice vazio", "hits": [], "total_docs": 0})),
        _resp("isto tambem nao e json"),
    ],
    ids=["lixo", "eof", "success_false", "envelope_de_erro", "output_invalido"],
)
def test_resposta_ruim_do_socket_cai_no_subprocesso(
    ambiente: Path, monkeypatch: pytest.MonkeyPatch, resposta: bytes
) -> None:
    sock = str(ambiente / "d.sock")
    daemon = _Daemon(sock, resposta)
    monkeypatch.setenv("TOURING_DAEMON_SOCKET", sock)
    _touring_falso(ambiente, json.dumps([HIT]))
    try:
        out = recuperar.fonte_tantivy("calcular ajuste base", cwd=str(ambiente))
    finally:
        daemon.fechar()
    assert [c.caminho for c in out] == ["/abs/alvo_do_socket.py"]  # the subprocess answered
    assert recuperar.ULTIMA_ROTA_TANTIVY.startswith("subprocesso (queda do socket")


def test_socket_que_nao_responde_estoura_no_prazo_sem_dobrar_o_pior_caso(
    ambiente: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    sock = str(ambiente / "d.sock")
    daemon = _Daemon(sock, None)  # accepts, never answers
    monkeypatch.setenv("TOURING_DAEMON_SOCKET", sock)
    chamadas = _sem_subprocesso(monkeypatch)
    t0 = time.perf_counter()
    try:
        with pytest.raises((OSError, subprocess.TimeoutExpired)):
            recuperar.fonte_tantivy("calcular ajuste base", cwd=str(ambiente), timeout=0.4)
    finally:
        daemon.fechar()
    assert time.perf_counter() - t0 < 1.5  # one budget for socket + fallback, not 2x
    assert chamadas == []  # the same daemon would be slow for the subprocess too: no second wait


def test_falha_do_socket_e_do_subprocesso_continua_nomeada_em_coletar(
    ambiente: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    sock = str(ambiente / "d.sock")
    daemon = _Daemon(sock, b"lixo\n")
    monkeypatch.setenv("TOURING_DAEMON_SOCKET", sock)
    _touring_falso(ambiente, "", rc=3)
    monkeypatch.setattr(recuperar, "fonte_manifest", lambda *a, **k: [])
    try:
        _, falhas = recuperar._coletar("novo.py", OUTRO, str(ambiente))
    finally:
        daemon.fechar()
    assert any(f.startswith("tantivy (RuntimeError)") for f in falhas)


def test_consulta_vazia_nao_toca_socket_nem_subprocesso(ambiente: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    chamadas = _sem_subprocesso(monkeypatch)
    assert recuperar.fonte_tantivy("   ", cwd=str(ambiente)) == []
    assert chamadas == []


# --- attempt 2 (04/11/2026): send first, collect later — no thread competing for the GIL with the index load -----


def _espera(cond, limite: float = 1.0) -> bool:
    fim = time.monotonic() + limite
    while time.monotonic() < fim:
        if cond():
            return True
        time.sleep(0.005)
    return cond()


def test_iniciar_envia_o_pedido_antes_de_coletar(ambiente: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    sock = str(ambiente / "d.sock")
    daemon = _Daemon(sock, _resp(json.dumps([HIT])))
    monkeypatch.setenv("TOURING_DAEMON_SOCKET", sock)
    chamadas = _sem_subprocesso(monkeypatch)
    try:
        coletar = recuperar.iniciar_tantivy("calcular ajuste base", cwd=str(ambiente))
        # the daemon already holds the request while the caller has not collected anything yet
        assert _espera(lambda: len(daemon.requests) == 1)
        out = coletar()
    finally:
        daemon.fechar()
    assert [c.caminho for c in out] == ["/abs/alvo_do_socket.py"]
    assert chamadas == []
    assert recuperar.ULTIMA_ROTA_TANTIVY == "socket"


def test_iniciar_sem_thread_de_consulta_quando_ha_socket(ambiente: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    sock = str(ambiente / "d.sock")
    daemon = _Daemon(sock, _resp(json.dumps([HIT])))
    monkeypatch.setenv("TOURING_DAEMON_SOCKET", sock)
    antes = threading.active_count()
    try:
        coletar = recuperar.iniciar_tantivy("calcular ajuste base", cwd=str(ambiente))
        # the pipelined path adds no worker thread: nothing to fight the caller's CPU-bound work for the GIL
        assert threading.active_count() == antes + 0
        coletar()
    finally:
        daemon.fechar()


def test_iniciar_resposta_ruim_cai_no_subprocesso_ao_coletar(ambiente: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    sock = str(ambiente / "d.sock")
    daemon = _Daemon(sock, b"lixo\n")
    monkeypatch.setenv("TOURING_DAEMON_SOCKET", sock)
    _touring_falso(ambiente, json.dumps([HIT]))
    try:
        out = recuperar.iniciar_tantivy("calcular ajuste base", cwd=str(ambiente))()
    finally:
        daemon.fechar()
    assert [c.caminho for c in out] == ["/abs/alvo_do_socket.py"]
    assert recuperar.ULTIMA_ROTA_TANTIVY.startswith("subprocesso (queda do socket")


def test_iniciar_sem_socket_usa_o_subprocesso_em_thread(ambiente: Path) -> None:
    _touring_falso(ambiente, json.dumps([HIT]))
    out = recuperar.iniciar_tantivy("calcular ajuste base", cwd=str(ambiente))()
    assert [c.caminho for c in out] == ["/abs/alvo_do_socket.py"]
    assert recuperar.ULTIMA_ROTA_TANTIVY.startswith("subprocesso")


def test_iniciar_daemon_mudo_estoura_no_prazo(ambiente: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    sock = str(ambiente / "d.sock")
    daemon = _Daemon(sock, None)
    monkeypatch.setenv("TOURING_DAEMON_SOCKET", sock)
    chamadas = _sem_subprocesso(monkeypatch)
    t0 = time.perf_counter()
    try:
        coletar = recuperar.iniciar_tantivy("calcular ajuste base", cwd=str(ambiente), timeout=0.4)
        with pytest.raises((OSError, subprocess.TimeoutExpired)):
            coletar()
    finally:
        daemon.fechar()
    assert time.perf_counter() - t0 < 1.5
    assert chamadas == []


def test_iniciar_consulta_vazia_e_vazia(ambiente: Path) -> None:
    assert recuperar.iniciar_tantivy("  ", cwd=str(ambiente))() == []


def test_coletar_usa_iniciar_e_nomeia_a_falha(ambiente: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    # _coletar asks the daemon FIRST and loads the other sources while it computes; the failure stays named.
    sock = str(ambiente / "d.sock")
    daemon = _Daemon(sock, _resp(json.dumps([HIT])))
    monkeypatch.setenv("TOURING_DAEMON_SOCKET", sock)
    monkeypatch.setattr(recuperar, "fonte_manifest", lambda *a, **k: [])
    chamadas = _sem_subprocesso(monkeypatch)
    try:
        cands, falhas = recuperar._coletar("novo.py", OUTRO, str(ambiente))
    finally:
        daemon.fechar()
    assert falhas == []
    assert [c.caminho for c in cands] == ["/abs/alvo_do_socket.py"]
    assert chamadas == []
