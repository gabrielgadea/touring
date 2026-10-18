"""Imprime linhas numeradas de FILE no intervalo [A, B]. Uso: touring run --file lerfaixa.py --args '[FILE, A, B]'"""

import sys

arq, a, b = sys.argv[1], int(sys.argv[2]), int(sys.argv[3])
with open(arq) as f:
    for n, linha in enumerate(f, 1):
        if a <= n <= b:
            print("%d\t%s" % (n, linha), end="")
