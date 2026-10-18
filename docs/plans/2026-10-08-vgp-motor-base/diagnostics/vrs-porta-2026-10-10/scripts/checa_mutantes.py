import importlib.util
import sys

spec = importlib.util.spec_from_file_location(
    "mutantes",
    "/tmp/claude-1000/-home-gabrielgadea-projects-touring/751b3a8c-4a8b-49b3-885e-3956e002b458/scratchpad/vrs/mutantes.py",
)
m = importlib.util.module_from_spec(spec)
sys.modules["mutantes"] = m
spec.loader.exec_module(m)
texto = m.ALVO.read_text()
ruins = []
for mid, _d, pares in m.MUTANTES:
    novo, n = m.aplica_pares(texto, pares)
    if novo is None:
        ruins.append((mid, n))
for v, corr in m.VARIANTES.items():
    pares = m.pares_de(corr)
    if pares:
        novo, n = m.aplica_pares(texto, pares)
        if novo is None:
            ruins.append((v, n))
print(f"{len(m.MUTANTES)} mutantes, {len(m.VARIANTES)} variantes; trechos com ocorrencias != 1: {ruins}")
