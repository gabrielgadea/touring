"""Prova que o `test_contrato_cdn_pin` DISCRIMINA: um pytest que passa sempre nao e um guard.

Copio os dois arquivos para /tmp, MUTO o criterio (todo vira `cdn_pin`, o mutante classico que faz
o detector aceitar demais), e o pytest tem de REPROVAR. Se passar, o assert e decorativo.

Nao toco no original: as copias vao para /tmp e morrem la.
"""

from pathlib import Path
import shutil
import subprocess
import sys

SRC = Path("/home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base/scripts")
TMP = Path("/tmp/vgp-mut-cdn")
TMP.mkdir(parents=True, exist_ok=True)

shutil.copy(SRC / "cdn_pin.py", TMP / "mut_cdn.py")
texto = (TMP / "mut_cdn.py").read_text()
# o mutante: `VERSAO_EXATA` nunca casa, entao TODA URL de CDN vira `cdn_pin`
antes = texto
texto = texto.replace('VERSAO_EXATA = re.compile(r"^\\d+\\.\\d+\\.\\d+$")', 'VERSAO_EXATA = re.compile(r"^NAO_CASA_NUNCA$")')
assert texto != antes, "nao achei a linha de VERSAO_EXATA para mutar"
(TMP / "mut_cdn.py").write_text(texto)

shutil.copy(SRC / "test_cdn_pin.py", TMP / "mut_test.py")
t2 = (TMP / "mut_test.py").read_text()
assert "from cdn_pin import" in t2
(TMP / "mut_test.py").write_text(t2.replace("from cdn_pin import", "from mut_cdn import"))

print("=== pytest com o criterio MUTADO (tem de reprovar) ===")
r = subprocess.run(
    [sys.executable, "-m", "pytest", "mut_test.py", "-q"],
    capture_output=True,
    text=True,
    cwd=TMP,
    timeout=600,
)
print("\n".join((r.stdout or r.stderr).splitlines()[-6:]))
print(f"EXIT={r.returncode}")
if r.returncode == 0:
    print()
    print("RESULTADO: o guard NAO discrimina — o pytest passou com o criterio quebrado.")
    sys.exit(1)
print()
print("RESULTADO: o guard REPROVA o criterio quebrado — discrimina.")
sys.exit(0)