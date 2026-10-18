Sonda do falso deny de `kwarg` em classe comum sem `__init__` próprio (achado de 10/10, não corrigido):

    cd scripts/sonda_init
    <binario>/touring vgp check --path /tmp/vgp-t/a.py --content-file uso.py --project-root . --lang python -j

Esperado correto: `Filha(a=1)` NÃO é achado (herda `Base.__init__(self, a, b=2)`); `Sozinha(x=1)` é achado.
Observado em `e935aaf1` e `70e0051a`: `deny kwarg a` e `deny kwarg x`.
