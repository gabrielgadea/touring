//! TDD fixtures for the namespace collector. The shapes are distilled from the three measured
//! held-out defects (numpy's composite `else:` + literal `globals()[ta]` + guarded `__getattr__`,
//! Pillow's `setattr(module, item.name, …)`, rust_bridge's `try/except ImportError`) and from the
//! CPython `__getattr__` idioms (`_collections_abc`, `re`).

use super::*;
use crate::ast::languages::Lang;
use crate::ast::parser::parse_thread_local;

fn ns(src: &str) -> Namespace {
    let tree = parse_thread_local(src, Lang::Python).expect("parse");
    collect(src, tree.root_node())
}

fn has(n: &Namespace, nome: &str) -> bool {
    n.names.contains(nome)
}

// --------------------------------------------------------------------------- compound scope

#[test]
fn if_elif_else_unem_as_ligacoes_de_todos_os_ramos() {
    let n = ns("if c:\n    A = 1\nelif d:\n    B = 2\nelse:\n    C = 3\n");
    for nome in ["A", "B", "C"] {
        assert!(has(&n, nome), "faltou {nome}");
    }
}

#[test]
fn try_except_else_finally_unem_as_ligacoes() {
    let n = ns(
        "try:\n    import a\nexcept ImportError as e:\n    X = 1\nexcept (A, B):\n    Y = 2\nelse:\n    Z = 3\nfinally:\n    W = 4\n",
    );
    for nome in ["a", "e", "X", "Y", "Z", "W"] {
        assert!(has(&n, nome), "faltou {nome}");
    }
}

#[test]
fn with_for_while_match_ligam_alvos_e_corpos() {
    let n = ns(
        "with open('f') as fh, g() as (p, q):\n    D = 1\nfor i, j in z:\n    E = 1\nelse:\n    F = 1\nwhile x:\n    G = 1\nmatch v:\n    case 1:\n        H = 1\n    case _:\n        I = 2\n",
    );
    for nome in ["fh", "p", "q", "D", "i", "j", "E", "F", "G", "H", "I"] {
        assert!(has(&n, nome), "faltou {nome}");
    }
}

#[test]
fn imports_e_defs_dentro_de_compostos_ligam_o_nome() {
    let n = ns(
        "if c:\n    from y import (a, b as bb)\n    import p.q as r\n    import s.t\n    @dec\n    def g():\n        pass\n    class H:\n        pass\nasync def af():\n    pass\n",
    );
    for nome in ["a", "bb", "r", "s", "g", "H", "af"] {
        assert!(has(&n, nome), "faltou {nome}");
    }
    assert!(!has(&n, "b"), "o nome original sob alias não é ligação");
    assert!(!has(&n, "p"), "alias esconde o primeiro segmento");
}

#[test]
fn type_alias_liga_o_nome_simples_e_o_generico() {
    let n = ns("type X = int\ntype Y[T] = list[T]\n");
    assert!(has(&n, "X") && has(&n, "Y"));
    assert!(
        !has(&n, "T"),
        "o parâmetro de tipo não é atributo do módulo"
    );
}

#[test]
fn walrus_e_atribuicao_encadeada_ligam() {
    let n = ns("if (n := f()):\n    pass\nT = E = R = 0\n");
    for nome in ["n", "T", "E", "R"] {
        assert!(has(&n, nome), "faltou {nome}");
    }
}

#[test]
fn corpos_de_def_class_lambda_e_comprehension_nao_sao_escopo_de_modulo() {
    let n = ns(
        "def f():\n    local_f = 1\nclass K:\n    campo = 1\n    def m(self):\n        self.x = 1\nL = lambda q: (z := q)\nCmp = [w for w in range(3)]\n",
    );
    for nome in ["f", "K", "L", "Cmp"] {
        assert!(has(&n, nome), "faltou {nome}");
    }
    for nome in ["local_f", "campo", "m", "q", "z", "w"] {
        assert!(!has(&n, nome), "{nome} vazou para o módulo");
    }
}

#[test]
fn global_em_funcao_declara_atributo_de_modulo() {
    let n = ns("def init():\n    global CACHE, LOCK\n    CACHE = {}\n");
    assert!(has(&n, "CACHE") && has(&n, "LOCK"));
}

#[test]
fn star_import_em_qualquer_ramo_marca_namespace_nao_enumeravel() {
    assert!(ns("try:\n    from x import *\nexcept ImportError:\n    pass\n").has_star);
    assert!(!ns("from x import a\n").has_star);
}

#[test]
fn dunder_all_literal_em_composto_e_estendido_conta_como_nome() {
    let n = ns(
        "if c:\n    __all__ = ['a', 'b']\n__all__ += ['c']\n__all__.extend(['d'])\n__all__.append('e')\n",
    );
    for nome in ["a", "b", "c", "d", "e"] {
        assert!(has(&n, nome), "faltou {nome}");
    }
}

// --------------------------------------------------------------------------- forma do numpy

const NUMPY: &str = r#"
import os
import sys
try:
    __NUMPY_SETUP__
except NameError:
    __NUMPY_SETUP__ = False

if __NUMPY_SETUP__:
    sys.stderr.write('setup\n')
else:
    from . import _core
    from ._core import (
        array,
        where,
        zeros,
    )
    for ta in ["float96", "float128", "complex192", "complex256"]:
        try:
            globals()[ta] = getattr(_core, ta)
        except AttributeError:
            pass
    del ta

    __numpy_submodules__ = {"linalg", "fft"}
    __future_scalars__ = {"str", "bytes", "object"}

    def __getattr__(attr):
        import warnings

        if attr == "linalg":
            import numpy.linalg as linalg
            return linalg
        elif attr == "fft":
            import numpy.fft as fft
            return fft
        elif attr == "array_api":
            raise AttributeError("not available", name=None)
        elif attr == "distutils":
            if 'distutils' in __numpy_submodules__:
                import numpy.distutils as distutils
                return distutils
            else:
                raise AttributeError("gone", name=None)

        if attr in __future_scalars__:
            warnings.warn(f"In the future `np.{attr}` ...", FutureWarning, stacklevel=2)

        if attr in __expired_attributes__:
            raise AttributeError(f"`np.{attr}` was removed {__expired_attributes__[attr]}", name=None)

        if attr == "chararray":
            warnings.warn("deprecated", DeprecationWarning, stacklevel=2)
            import numpy.char as char
            return char.chararray

        raise AttributeError(f"module {__name__!r} has no attribute {attr!r}")
"#;

#[test]
fn numpy_composto_liga_array_where_e_fecha_o_namespace() {
    let n = ns(NUMPY);
    for nome in ["array", "where", "zeros"] {
        assert!(has(&n, nome), "o from-import dentro do else: perdeu {nome}");
    }
    // os dois mecanismos dinâmicos do numpy são FECHADOS: nomes literais, nenhuma abstenção
    for nome in ["float96", "complex256"] {
        assert!(
            has(&n, nome),
            "globals()[ta] sobre lista literal perdeu {nome}"
        );
    }
    for nome in ["linalg", "fft", "array_api", "distutils", "chararray"] {
        assert!(has(&n, nome), "o __getattr__ guardado serve {nome}");
    }
    assert!(
        !n.has_getattr,
        "__getattr__ guardado por literais e com raise final é fechado"
    );
    assert!(
        !n.has_dynamic,
        "globals()[ta] sobre lista literal é fechado"
    );
    assert!(!has(&n, "arrai"), "o typo não pode entrar no namespace");
}

// --------------------------------------------------------------------------- forma do Pillow

#[test]
fn pillow_setattr_sobre_o_modulo_com_nome_nao_literal_e_aberto() {
    let n = ns(
        "import sys\nfrom enum import IntEnum\nclass Resampling(IntEnum):\n    LANCZOS = 1\nmodule = sys.modules[__name__]\nfor enum in (Resampling,):\n    for item in enum:\n        setattr(module, item.name, item.value)\n",
    );
    assert!(
        n.has_dynamic,
        "setattr(module, item.name, …) liga nomes que a leitura estática não vê"
    );
}

#[test]
fn setattr_sobre_objeto_que_nao_e_o_modulo_nao_conta() {
    let n = ns(
        "class K:\n    def __init__(self, d):\n        for k, v in d.items():\n            setattr(self, k, v)\n\ndef cfg(obj, k, v):\n    setattr(obj, k, v)\n",
    );
    assert!(!n.has_dynamic, "setattr em self/param não escreve o módulo");
}

#[test]
fn setattr_e_globals_com_chave_literal_ou_laco_literal_sao_fechados() {
    let n = ns(
        "import sys\nm = sys.modules[__name__]\nsetattr(m, 'um', 1)\nglobals()['dois'] = 2\nfor k in ('tres', 'quatro'):\n    setattr(m, k, 0)\nglobals().update({'cinco': 5}, seis=6)\nsys.modules[__name__].sete = 7\nvars(m)['oito'] = 8\nm.__dict__['nove'] = 9\nns = globals()\nns['dez'] = 10\n",
    );
    for nome in [
        "um", "dois", "tres", "quatro", "cinco", "seis", "sete", "oito", "nove", "dez",
    ] {
        assert!(has(&n, nome), "faltou {nome}");
    }
    assert!(!n.has_dynamic, "todas as chaves são literais");
}

#[test]
fn globals_com_chave_nao_literal_e_aberto() {
    assert!(ns("for k in names():\n    globals()[k] = 1\n").has_dynamic);
    assert!(
        ns("for k in ('a', 'b'):\n    for k in other():\n        globals()[k] = 1\n").has_dynamic
    );
    assert!(ns("globals()[f'x_{n}'] = 1\n").has_dynamic);
    assert!(ns("globals().update(build())\n").has_dynamic);
    assert!(ns("globals().update(**extra)\n").has_dynamic);
}

#[test]
fn modulo_substituido_por_objeto_exec_no_modulo_e_class_sao_abertos() {
    assert!(ns("import sys\nsys.modules[__name__] = Proxy()\n").has_dynamic);
    assert!(ns("exec(CODE)\n").has_dynamic);
    assert!(ns("import sys\nsys.modules[__name__].__class__ = Mod\n").has_dynamic);
    // exec dentro de função sem entregar o namespace do módulo: não escreve o módulo
    assert!(!ns("def f(c):\n    exec(c, {})\n").has_dynamic);
    assert!(ns("def f(c):\n    exec(c, globals())\n").has_dynamic);
}

#[test]
fn future_import_liga_annotations() {
    // `from __future__ import annotations` é `future_import_statement`, não `import_from_statement`;
    // o runtime expõe `pkg.annotations` (censo contra dir() de 18 pacotes do venv)
    assert!(has(
        &ns("from __future__ import annotations\n"),
        "annotations"
    ));
}

#[test]
fn sys_modules_de_outra_entrada_nao_substitui_este_modulo() {
    // pptx: `sys.modules["pptx.exceptions"] = exceptions` registra um alias, não troca o módulo
    assert!(!ns("import sys\nsys.modules['pkg.alias'] = sub\n").has_dynamic);
    assert!(ns("import sys\nsys.modules[__name__] = Proxy()\n").has_dynamic);
    assert!(ns("import sys\nsys.modules[__package__] = Proxy()\n").has_dynamic);
}

#[test]
fn modulo_nomeado_por_literal_e_outro_modulo_e_variavel_pode_ser_este() {
    let outro = ns(
        "import importlib\ny = importlib.import_module('yaml')\nsetattr(y, k, 1)\nregister(y)\nz = __import__('json')\nsetattr(z, k, 1)\n",
    );
    assert!(
        !outro.has_dynamic,
        "módulos nomeados por literal não são este"
    );
    let este = ns("import importlib\nm = importlib.import_module(nome)\nsetattr(m, k, 1)\n");
    assert!(
        este.has_dynamic,
        "o nome em variável pode ser o próprio módulo"
    );
    let via_sys = ns("import sys\nm = sys.modules[nome]\nm.__dict__[k] = 1\n");
    assert!(via_sys.has_dynamic);
    let literal = ns("import sys\nm = sys.modules['yaml']\nm.__dict__[k] = 1\n");
    assert!(!literal.has_dynamic);
}

#[test]
fn namespace_entregue_a_um_chamador_que_pode_ligar_nomes_e_aberto() {
    // websockets: `lazy_import(globals(), aliases=…, deprecated_aliases=…)` instala `__getattr__`
    assert!(ns("lazy_import(globals(), aliases={})\n").has_dynamic);
    assert!(ns("mod.setup(globals())\n").has_dynamic);
    assert!(ns("f(ns=globals())\n").has_dynamic);
    assert!(ns("import sys\nregister(sys.modules[__name__])\n").has_dynamic);
    assert!(ns("ns = globals()\nhelper(ns)\n").has_dynamic);
    // só leitura: nada é ligado
    for src in [
        "print(globals())\n",
        "d = dict(globals())\n",
        "n = len(globals())\n",
        "names = sorted(globals())\n",
        "x = globals().get('k')\n",
        "for k, v in globals().items():\n    pass\n",
        "logging.getLogger(__name__)\n",
    ] {
        assert!(!ns(src).has_dynamic, "{src:?} só lê");
    }
}

#[test]
fn o_escopo_volta_ao_modulo_depois_de_um_def_ou_class() {
    // a varredura de cursor único mantém a profundidade no enter/exit: um `def` ou `class` que
    // terminou não pode deixar o resto do módulo "dentro" dele
    let g = ns("def helper():\n    pass\nclass K:\n    pass\ndef __getattr__(n):\n    return 1\n");
    assert!(
        g.has_getattr,
        "o __getattr__ depois de um def/class continua sendo do módulo"
    );
    assert!(
        ns("def helper():\n    pass\nexec(CODE)\n").has_dynamic,
        "exec depois de um def é de módulo"
    );
    // `vars()` sem argumento é o dict do módulo só na profundidade 0: depois do class, de volta a 0,
    // uma chave não literal é ligação dinâmica aberta (chave literal seria fechada)
    assert!(
        ns("class K:\n    def m(self):\n        pass\nvars()[chave] = 1\n").has_dynamic,
        "vars() no nível do módulo, depois de um class, ainda é o dict do módulo"
    );
    assert!(
        !ns("def f():\n    vars()[chave] = 1\n").has_dynamic,
        "dentro de função, vars() é o dict local, não o do módulo"
    );
}

#[test]
fn laco_literal_nao_vaza_para_depois_do_for() {
    // `k` só é literal DENTRO do for que o itera
    assert!(!ns("for k in ('a', 'b'):\n    globals()[k] = 1\n").has_dynamic);
    assert!(ns("for k in ('a', 'b'):\n    pass\nglobals()[k] = 1\n").has_dynamic);
    // laços aninhados: o interno (não literal) sombreia o externo e é desempilhado ao sair
    let n = ns("for k in ('a', 'b'):\n    for k in other():\n        pass\n    globals()[k] = 1\n");
    assert!(
        !n.has_dynamic,
        "ao sair do for interno, k volta a ser o literal do externo"
    );
    assert!(has(&n, "a") && has(&n, "b"));
}

// --------------------------------------------------------------------------- __getattr__ (PEP 562)

#[test]
fn getattr_idioma_do_cpython_e_fechado() {
    // `re.__version__`, `_collections_abc.ByteString`: literal + raise final
    let n = ns(
        "def __getattr__(name):\n    if name == '__version__':\n        return '2.2.1'\n    raise AttributeError(f'module {__name__!r} has no attribute {name!r}')\n",
    );
    assert!(!n.has_getattr);
    assert!(has(&n, "__version__"));
    let n = ns(
        "def __getattr__(attr):\n    if attr == 'ByteString':\n        globals()['ByteString'] = _x\n        return _x\n    raise AttributeError(attr)\n",
    );
    assert!(!n.has_getattr && !n.has_dynamic);
    assert!(has(&n, "ByteString"));
}

#[test]
fn getattr_so_com_raise_serve_nenhum_nome() {
    let n = ns("def __getattr__(name):\n    raise AttributeError(name)\n");
    assert!(!n.has_getattr);
}

#[test]
fn getattr_com_return_sem_guarda_e_aberto() {
    assert!(ns("def __getattr__(n):\n    return 1\nfrom x import *\n").has_getattr);
    // o idioma preguiçoso: devolve QUALQUER nome do mapa
    assert!(
        ns("def __getattr__(name):\n    if name in _LAZY:\n        return import_module(_LAZY[name])\n    raise AttributeError(name)\n")
            .has_getattr
    );
    assert!(ns("def __getattr__(name):\n    return getattr(_impl, name)\n").has_getattr);
}

#[test]
fn getattr_return_no_else_ou_depois_do_if_e_aberto() {
    assert!(
        ns("def __getattr__(name):\n    if name == 'a':\n        return 1\n    else:\n        return 2\n    raise AttributeError(name)\n")
            .has_getattr
    );
    assert!(
        ns("def __getattr__(name):\n    if name == 'a':\n        pass\n    return 3\n    raise AttributeError(name)\n")
            .has_getattr
    );
}

#[test]
fn getattr_sem_raise_final_devolve_none_e_e_aberto() {
    // cair no fim devolve None — PEP 562: isso é um VALOR válido, não AttributeError
    assert!(ns("def __getattr__(name):\n    if name == 'a':\n        return 1\n").has_getattr);
    assert!(
        ns("def __getattr__(name):\n    if name == 'a':\n        return 1\n    pass\n").has_getattr
    );
}

#[test]
fn getattr_guarda_in_literal_and_or_e_condicao_nao_literal() {
    let n = ns(
        "def __getattr__(name):\n    if name in ('a', 'b'):\n        return 1\n    elif name == 'c' and ok():\n        return 2\n    elif (name == 'd') or (name == 'e'):\n        return 3\n    raise AttributeError(name)\n",
    );
    assert!(!n.has_getattr);
    for nome in ["a", "b", "c", "d", "e"] {
        assert!(has(&n, nome), "faltou {nome}");
    }
    // `or` com um lado não literal NÃO fixa o nome; `not in`/`!=`/chamada sobre o parâmetro também não
    for guarda in [
        "name == 'a' or other()",
        "name not in ('a',)",
        "name != 'a'",
        "name.startswith('a')",
    ] {
        let src = format!(
            "def __getattr__(name):\n    if {guarda}:\n        return 1\n    raise AttributeError(name)\n"
        );
        assert!(ns(&src).has_getattr, "guarda {guarda} não fixa o nome");
    }
}

#[test]
fn getattr_por_atribuicao_ou_reexport_e_aberto() {
    assert!(ns("__getattr__, __dir__, __all__ = lazy.attach(__name__)\n").has_getattr);
    assert!(ns("__getattr__ = make_getattr()\n").has_getattr);
    assert!(ns("from ._lazy import __getattr__\n").has_getattr);
}

#[test]
fn getattr_de_classe_ou_de_funcao_aninhada_nao_e_do_modulo() {
    assert!(!ns("class K:\n    def __getattr__(self, n):\n        return 1\n").has_getattr);
    assert!(!ns("def f():\n    def __getattr__(n):\n        return 1\n").has_getattr);
}

#[test]
fn getattr_em_ramo_composto_do_modulo_e_analisado() {
    // numpy: o `def __getattr__` mora dentro de `else:` — invisível ao extrator de filhos diretos
    let aberto = ns("if True:\n    def __getattr__(n):\n        return 1\n");
    assert!(aberto.has_getattr);
}

// --------------------------------------------------------------------------- censo (instrumento)

/// Instrumento, não gate: imprime o namespace estático de cada arquivo listado em `VGP_CENSUS_FILES`
/// (um caminho por linha) para o censo contra o runtime — `censo.py comparar` cruza com `dir(módulo)`.
/// `cargo test -p touring-code --lib namespace::tests::censo -- --ignored --nocapture`.
#[test]
#[ignore = "instrumento de censo: precisa de VGP_CENSUS_FILES"]
fn censo_do_namespace_para_cruzar_com_o_runtime() {
    let Ok(lista) = std::env::var("VGP_CENSUS_FILES") else {
        return;
    };
    for path in std::fs::read_to_string(lista).expect("lista").lines() {
        let Ok(src) = std::fs::read_to_string(path) else {
            continue;
        };
        let n = ns(&src);
        let mut nomes: Vec<&str> = n.names.iter().map(String::as_str).collect();
        nomes.sort_unstable();
        println!(
            "CENSO\t{path}\t{}\t{}\t{}\t{}",
            u8::from(n.has_star),
            u8::from(n.has_getattr),
            u8::from(n.has_dynamic),
            nomes.join(",")
        );
    }
}
