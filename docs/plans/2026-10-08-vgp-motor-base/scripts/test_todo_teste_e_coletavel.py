"""GUARDA ESTRUTURAL: todo `test_*.py` do bundle tem de ter funcao `test_` que o pytest coleta.

O DEFECTO (medido em 10/10,Coordinatora): tres arquivos de teste do bundle so tinham `main()`, que o
pytest nao coleta. `pytest` respondia `no tests ran` com **exit 5** — e exit 5 e o codigo que varios
CI tratam como "nada a fazer". Um CI que rodasse pytest aqui testaria UMA COISA (o
`test_cdn_pin`) e passaria VERDE, sem rodar os outros tres.

**Por que uma guarda, e nao so consertar os tres:** consertar os tres resolve HOJE. Sem uma guarda
estrutural, o proximo arquivo `test_*.py` escrito com so `main()` reintroduz o mesmo padrao, e o
sintomo continua invisivel — porque um teste que so roda por script nao tem veredito observavel por
quem nao o chama. O `rc` existe, mas so existe para quem abre o terminal; o CI ve "verde".

O QUE ESTA GUARDA FAZ: le cada `scripts/test_*.py` por `ast` (sem importar — importar executaria o
modulo, e um teste que importa outro teste roda o codigo do outro) e falha se o arquivo nao tiver
pelo menos uma funcao cujo nome comeca com `test_`. E o que o pytest coleta: uma funcao de nivel de
modulo chamada `test_*`. Um `def test_x()` dentro de uma classe tambem conta; o que nao conta e um
`test_` dentro de um `if` que o pytest nao visita, ou um nome que nao comeca com `test_`.

O CONTROLE POSITIVO esta em `test_guarda_reprova_um_so_main`: esta guarda, sozinha, tem de REPROVAR
contra uma copia com um arquivo so-`main()`. Uma guarda que nunca reprova e decoracao.
"""
import ast
import sys
from pathlib import Path

AQUI = Path(__file__).resolve().parent
PADRAO = "test_*.py"


def funcoes_de_test(path: Path) -> list[str]:
    """Os nomes `test_*` de nivel de modulo OU de classe — o que o pytest coleta.

    Por `ast`, sem importar: `import` de um modulo de teste EXECUTA o codigo dele, e um guarda que
    importa o que inspeciona pode ser influenciado pelo proprio alvo.
    """
    arvore = ast.parse(path.read_text(encoding="utf-8"), filename=str(path))
    achados = []
    for no in ast.walk(arvore):
        if isinstance(no, (ast.FunctionDef, ast.AsyncFunctionDef)):
            if no.name.startswith("test_"):
                achados.append(no.name)
    return achados


def arquivos_de_teste(raiz: Path | None = None) -> list[Path]:
    return sorted((raiz or AQUI).glob(PADRAO))


def confere(raiz: Path | None = None) -> list[tuple[Path, int]]:
    """(arquivo, n de funcoes `test_`) para cada arquivo que nao tem nenhuma. Vazio = tudo ok."""
    sem = []
    for p in arquivos_de_teste(raiz):
        n = len(funcoes_de_test(p))
        if n == 0:
            sem.append((p, n))
    return sem


def main() -> int:
    arquivos = arquivos_de_teste()
    print(f"=== {len(arquivos)} arquivos {PADRAO} em {AQUI} ===")
    sem = []
    for p in arquivos:
        nomes = funcoes_de_test(p)
        marca = "ok  " if nomes else "FALHA"
        if not nomes:
            sem.append(p.name)
        print(f"  {marca} {len(nomes):>2d} funcao(oes) test_  {p.name}")
    print()
    if sem:
        print(f"FALHOU: {len(sem)} arquivo(s) sem nenhuma funcao `test_` — o pytest nao coleta:")
        for n in sem:
            print(f"  - {n}")
        print()
        print("  Um arquivo assim da `no tests ran` com exit 5, que muitos CI leem como")
        print("  'nada a fazer': o pipeline fica VERDE sem rodar o teste. Acrescente")
        print("  `def test_<algo>(): assert main() == 0`.")
        return 1
    print(f"OK: os {len(arquivos)} arquivos tem ao menos uma funcao `test_` coletavel pelo pytest.")
    return 0


def test_todo_teste_e_coletavel():
    """A guarda, como o pytest a ve. `main()` faz o trabalho e devolve != 0 se algo falta."""
    assert main() == 0, "ha `test_*.py` sem funcao `test_`: o pytest nao os coleta"


def test_guarda_reprova_um_so_main():
    """O CONTROLE POSITIVO: a guarda tem que REPROVAR contra um arquivo so-`main()`.

    Sem este caso, a guarda e decoracao — um guard que nunca reprova nada e o que o proprio
    programa acabou de ensinar a nao escrever. O arquivo quebrado vai para `/tmp`, nunca para o
    bundle: o alvo da guarda e o bundle, e sujar o alvo para testar o alvo seria outro erro.
    """
    import tempfile

    with tempfile.TemporaryDirectory(prefix="vgp-guarda-") as tmp:
        raiz = Path(tmp)
        (raiz / "test_quebrado.py").write_text(
            '"""So main(): e o defeito, sem funcao test_."""\n'
            "\n"
            "def main() -> int:\n"
            "    return 0\n"
            "\n"
            '\nif __name__ == "__main__":\n'
            "    sys.exit(main())\n",
            encoding="utf-8",
        )
        achados = confere(raiz)
        assert achados, "a guarda NAO reprovou um arquivo so-`main()` — ela e decoracao"
        assert achados[0][0].name == "test_quebrado.py"

        # e o caminho feliz: um arquivo com `test_` passa
        (raiz / "test_quebrado.py").write_text(
            "def main() -> int:\n    return 0\n\n\n"
            "def test_algo():\n    assert main() == 0\n",
            encoding="utf-8",
        )
        assert not confere(raiz), "a guarda reprovou um arquivo COM funcao test_"


if __name__ == "__main__":
    sys.exit(main())