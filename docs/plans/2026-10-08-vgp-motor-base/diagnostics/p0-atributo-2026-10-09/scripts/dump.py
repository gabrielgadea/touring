import sys

sys.path.insert(0, "/home/gabrielgadea/projects/analise/.venv/lib/python3.12/site-packages")
import tree_sitter_python as tsp
from tree_sitter import Language, Parser

P = Parser(Language(tsp.language()))
SRC = {
    "try": "try:\n    import a\nexcept ImportError as e:\n    X = 1\nexcept (A, B):\n    Y = 2\nelse:\n    Z = 3\nfinally:\n    W = 4\n",
    "if": "if c:\n    A = 1\nelif d:\n    B = 2\nelse:\n    C = 3\n",
    "with": "with open('f') as fh, g() as (p, q):\n    D = 1\n",
    "for": "for ta in ['a', 'b']:\n    globals()[ta] = 1\nelse:\n    E = 1\n",
    "while": "while x:\n    F = 1\n",
    "match": "match v:\n    case 1:\n        G = 1\n    case _:\n        H = 2\n",
    "global": "def f():\n    global GG, HH\n    GG = 1\n",
    "setattr": "module = sys.modules[__name__]\nfor item in enum:\n    setattr(module, item.name, item.value)\nsys.modules[__name__].zz = 1\nglobals().update({'a': 1}, b=2)\nglobals()['k'] = 3\nsys.modules[__name__].__class__ = M\n",
    "getattr": "def __getattr__(attr):\n    if attr == 'a':\n        return 1\n    elif attr in ('b', 'c'):\n        return 2\n    if attr in X:\n        warn(f'x {attr}')\n    raise AttributeError(f'no {attr!r}')\n",
    "tuplegetattr": "__getattr__, __dir__, __all__ = lazy.attach(__name__)\n",
    "augall": "__all__ = ['a']\n__all__ += ['b']\n__all__.extend(['c'])\n",
    "walrus": "if (n := f()):\n    pass\n",
    "typed": "X: int = 1\nY: int\nclass K: pass\nasync def af(): pass\n",
    "async": "async def af(): pass\nasync for i in x:\n    Q = 1\n",
    "decor_in_if": "if c:\n    @dec\n    def g(): pass\n    class H: pass\n",
    "del": "del ta, tb\n",
    "import in if": "if c:\n    from x import *\n    from y import (a, b as c)\n    import p.q as r\n",
}


def show(n, src, d=0):
    if not n.is_named:
        return
    print("  " * d + n.type + "  " + repr(src[n.start_byte:n.end_byte].split(b"\n")[0][:50].decode()))
    for i, ch in enumerate(n.children):
        f = n.field_name_for_child(i)
        if ch.is_named:
            if f:
                print("  " * (d + 1) + f"<{f}>")
            show(ch, src, d + 1)


for k in SRC:
    print("=====", k)
    s = SRC[k].encode()
    show(P.parse(s).root_node, s)
