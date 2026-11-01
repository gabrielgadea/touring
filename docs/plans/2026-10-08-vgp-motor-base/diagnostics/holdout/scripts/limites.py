"""Limite superior UNILATERAL 95% de Clopper-Pearson (exato) para uma proporcao de falhas: k falhas em n.

  k = 0  ->  1 - 0,05^(1/n)   (a 'regra de tres' e a aproximacao 3/n deste valor)
  k > 0  ->  o p que resolve P(X <= k | n, p) = 0,05, por bissecao (sem scipy).
`python3 -I limites.py --selftest` confere contra valores conhecidos; `python3 -I limites.py k n [k n ...]` imprime.
"""

import math
import sys


def cdf_binomial(k: int, n: int, p: float) -> float:
    """P(X <= k) para X ~ Binomial(n, p), somando em escala log (estavel para n grande)."""
    if p <= 0.0:
        return 1.0
    if p >= 1.0:
        return 1.0 if k >= n else 0.0
    termos = [math.lgamma(n + 1) - math.lgamma(i + 1) - math.lgamma(n - i + 1) + i * math.log(p) + (n - i) * math.log1p(-p)
              for i in range(k + 1)]
    m = max(termos)
    return math.exp(m) * sum(math.exp(t - m) for t in termos)


def limite_superior(k: int, n: int, conf: float = 0.95) -> float:
    if n <= 0:
        raise ValueError("n deve ser positivo")
    if k >= n:
        return 1.0
    if k == 0:
        return 1.0 - (1.0 - conf) ** (1.0 / n)
    lo, hi = k / n, 1.0
    for _ in range(80):
        meio = (lo + hi) / 2
        if cdf_binomial(k, n, meio) > 1.0 - conf:
            lo = meio
        else:
            hi = meio
    return (lo + hi) / 2


def selftest() -> int:
    casos = {
        "k=0 n=600 ~ 0,498%": abs(limite_superior(0, 600) - 0.004982) < 1e-5,
        "k=0 n=77 ~ 3,8%": abs(limite_superior(0, 77) - 0.0382) < 1e-3,
        "k=0 n=189 ~ 1,6%": abs(limite_superior(0, 189) - 0.0157) < 5e-4,
        "regra de tres 3/n ~ exato (n=600)": abs(3 / 600 - limite_superior(0, 600)) < 2e-4,
        "k=1 n=100 ~ 4,66% (valor de tabela)": abs(limite_superior(1, 100) - 0.0466) < 1e-3,
        "k=11 n=77 > k/n": limite_superior(11, 77) > 11 / 77,
        "monotono em k": limite_superior(2, 600) > limite_superior(1, 600) > limite_superior(0, 600),
        "monotono em n": limite_superior(0, 1200) < limite_superior(0, 600),
        "cdf invertida: P(X<=k|limite) = 0,05": abs(cdf_binomial(3, 400, limite_superior(3, 400)) - 0.05) < 1e-6,
    }
    for nome, ok in casos.items():
        print(f"  {'ok ' if ok else 'FALHOU'} {nome}")
    return 0 if all(casos.values()) else 1


if __name__ == "__main__":
    if "--selftest" in sys.argv:
        raise SystemExit(selftest())
    args = [int(x) for x in sys.argv[1:]]
    for k, n in zip(args[::2], args[1::2], strict=True):
        print(f"k={k} n={n}: limite superior 95% = {100 * limite_superior(k, n):.2f}%")
