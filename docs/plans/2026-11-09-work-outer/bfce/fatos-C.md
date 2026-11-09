# C — módulo de ambiente Python: fatos (09/11/2026)

Levantamento só-leitura (subagente sonnet), sem execução. `C/` = `crates/touring-code/src/vgp/lang`,
`P/` = `crates/touring-pyright/src`. DAG `task_1794217886022321956`.

## L1 `C/python/modules.rs`
`site_packages` :331 só `.venv/lib/python*`, ordem do SO, sem `is_dir`; `venv_candidates` :345;
`resolve_module` :244 raiz → dir do importador → site-packages; só regular, nunca namespace;
`extension_exists` :358 nas raízes permitidas (:319), sem `lib-dynload`; `dir_has_extension` :371.
Comentários :303-304 e :316-317 afirmam `pyvenv.cfg` (nunca lido); cabeçalho :6 diz `python3*`.
Lê tudo pelo `caminho` (contido).

## L2 `P/pyright.rs`
`VENV_DIRS=[".venv","venv"]` :59; `locate_server` :223; `venv_ativo` :725; `raizes_do_interpretador`
:692 (lib+lib64 · pythonX.Y ordenado · site/dist-packages · `pyvenv.cfg` home/include-system :737;
ordem stdlib → venv → sistema); `modulo_do_projeto` :504; namespace `pacote_de_namespace` :621;
`extensao_c_em` :575 = cópia de `dir_has_extension`, em base e `base/lib-dynload` :565; 13 acessos
crus (228, 508, 576, 598, 614, 626, 638, 677, 734, 738, 758, 765, 1016); lexer à mão
`fonte_define_nome` :785 (~150 linhas); raízes recalculadas por achado (:524, :660).

## Divergências (derivadas)
`venv/` (L1 abstém, L2 nega Externo); `.venv` symlink externo (L1 recusa, L2 segue); `lib64` real;
duas `python3.x` (ordem do SO × lexical); namespace sem `__init__` (L1 pode negar import válido);
`.so` no projeto (só L1); `home`/include-system (só L2).

## Leitor contido e guarda
`C/caminho.rs` (`ler_contido` 327, `listar_contido` 360, `sondar` 375, `e_arquivo` 392,
`e_diretorio` 397, `existe` 402, `Recusa` 66); guarda `touring-code/tests/vgp_guarda_disco.rs:249`
varre só `src/vgp`.

## Crates
Dependem de touring-pyright: hook-handlers, hooks, lsp, server. touring-code: foundation, simd,
storage. Regra: `P/lib.rs:6-7` ("depends on no `touring-*` crate"), `Cargo.toml:14-16` (não lsp/
hooks/dispatch/hook-handlers). Opções: (a) folha nova só-std com trait `Disco` — recomendada;
(b) mover `caminho.rs` para a folha; (c) pyright→touring-code (+52 deps); (d) inverter; (e) foundation.

## Testes / corpus
L1 `python/tests.rs:85,1013`, `p0_*` (824), `p0b_a_*` (1622), `modules.rs:1012`; L2
`pyright.rs:1509-1615` (J17 :1537, caso `b81d9fa3c3e5` :1530), `p0b2_*` 2390-2670, `locate_server`
:1826. J18 sem diretório (`RETOMADA.md:30` exige pré-registro).

## Não muda veredito
unificar `extensao_c_em`/`dir_has_extension`; cachear o ambiente por sessão na L2; corrigir os
comentários de `pyvenv.cfg`; fixture compartilhada.

## Muda veredito (exige held-out pré-registrado — J18)
L1 ver `venv/`/`lib64`; ordenar `python*`; honrar home/include-system; aceitar namespace; L2 sob
contenção; escolha do venv.
