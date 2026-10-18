#!/usr/bin/env python3
"""Mutantes e VARIANTES do adaptador Rust, aplicados em `lang/rust.rs` do worktree vrs2.

Modo mutantes:   python3 mutantes.py mutantes <saida.json> [id ...]
  Aplica UM mutante por vez, roda os testes do adaptador e exige "test result: FAILED"; antes roda a
  BASE (verde) e o RED-stub (o stub do nucleo, contra o qual os testes por classe reprovam).
Modo variantes:  python3 mutantes.py variantes <dir-saida> <saida.json> [V0 V1 ...]
  Para cada variante V desfaz (REVERT) as correcoes que ela ainda nao tem, compila o binario
  `touring` (debug) e o copia para <dir-saida>/<V>/touring com o sha256. V0 = paridade estrita com a
  a6 (sem nenhuma correcao); V1 = +(a); V2 = +(b); V3 = +(c); V4 = +(d) = o fonte final.

Em ambos: marcador `<wt>/.TROCA-EM-CURSO` na janela, restauracao do arquivo POR CONTEUDO (sha
verificado) e mtime devolvido com os.utime. Roda DENTRO de uma aquisicao do hw-slot.
"""

import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import time
from pathlib import Path

WT = Path("/home/gabrielgadea/projects/touring-wt-vrs2")
ALVO = WT / "crates/touring-code/src/vgp/lang/rust.rs"
MARCADOR = WT / ".TROCA-EM-CURSO"
TARGET = "/home/gabrielgadea/projects/touring/target/wt-vrs2"
CMD_TESTE = ["cargo", "test", "-p", "touring-code", "--lib", "vgp::lang::rust", "--no-fail-fast"]
CMD_BIN = ["cargo", "build", "-p", "touring-server", "--bin", "touring"]

# --- as correcoes, como pares (trecho do fonte final -> trecho da a6): desfazer = aplicar os pares
REVERT = {
    "a": [  # use <pacote>:: em tests/benches/examples resolve a LIB do pacote
        (
            "Some(lib) => (lib.clone(), Vec::new(), 1),",
            "Some(_lib) => (k.root_file.clone(), Vec::new(), 1),",
        ),
    ],
    "b": [  # Tipo::item_associado nao e variante ausente: CamelCase; impl visivel decide; o resto nao medido
        (
            "if e.variantes.contains(variante) || e.assoc.contains(variante) {",
            "if e.variantes.contains(variante) {",
        ),
        ("if !parece_variante(variante) {", "if false {"),
    ],
    "c": [  # preludio completo + itens locais de fn
        (
            '"function_item" | "const_item" | "static_item" | "struct_item" | "enum_item"',
            '"zz_a" | "zz_b" | "zz_c" | "zz_d" | "zz_e"',
        ),
        (
            '| "union_item" | "type_item" | "trait_item" | "mod_item" | "macro_definition" =>',
            '| "zz_f" | "zz_g" | "zz_h" | "zz_i" | "zz_j" =>',
        ),
        (
            '        "drop",\n        "size_of",\n        "size_of_val",\n        "align_of",\n'
            '        "align_of_val",\n        "AsyncFn",\n        "AsyncFnMut",\n'
            '        "AsyncFnOnce",\n        "Future",\n        "IntoFuture",\n',
            "",
        ),
    ],
    "d": [  # atributos externos sao IRMAOS do item (o bug da a6: lidos como filhos)
        ("let mut atual = item.prev_sibling();", "let mut atual = None::<Node>;"),
    ],
    "e": [  # glob de modulo do PROJETO: self/super/crate sem item = o modulo; usos do alvo; cadeia; macro; #[path]
        ("        if resto.is_empty() {", "        if false && resto.is_empty() {"),
        ("        self.mesclar_usos_do_alvo(file, trail, prof, vistos);", "        let _ = (prof, &vistos);"),
        ("        if alvo.macro_item {\n            self.vinculos.glob_externo = true; // uma macro de item pode declarar nomes\n        }", "        if false && alvo.macro_item {\n            self.vinculos.glob_externo = true;\n        }"),
        ("self.modpath_declarado(path, &layout, prof)\n                .unwrap_or(layout)", "{\n                let _ = prof;\n                layout\n            }"),
        ("self.ler(c).is_some() || self.e_o_proposto(c)", "self.ler(c).is_some()"),
    ],
    "f": [  # #[path] fora de inline e relativo ao DIRETORIO do arquivo (nao a foo/)
        ("let relativo_a = if trail.is_empty() {", "let relativo_a = if false {"),
    ],
    "g": [  # Unit/Tupla/trait/alias::associada nao e import inexistente
        ("if model.structs.contains_key(seg) || model.itens.contains(seg) {", "if model.structs.contains_key(seg) {"),
    ],
    "h": [  # `pub use dep;` liga o crate externo
        ("\n                            && !(r.segs.len() == 1 && r.segs[0] == head)", ""),
    ],
    "i": [  # const/static: o nome e declaracao e o tipo nao e uso de valor
        ('            "const_item" | "static_item" => {\n                // `pub const ALL', '            "zz_const" | "zz_static" => {\n                // `pub const ALL'),
    ],
    "j": [  # modulos inline ANINHADOS enxergam os itens do proprio bloco
        ("let frame = self.itens_do_modulo_inline();", "let frame = self\n                    .modelo\n                    .inline\n                    .get(&nome)\n                    .map(|m| m.itens.clone())\n                    .unwrap_or_default();"),
    ],
    "l": [  # globs reexportados: o resto do caminho e procurado DENTRO do alvo (reexports e globs em cadeia, Enum::Variante)
        ("match self.descer(gf, gt, &resto[idx..], prof + 1, visitados) {", "match Resolucao::Inexistente(seg.clone()) {"),
    ],
    "q": [  # o segmento intermediario alcancado por reexport (`pub use a::b as c;`) segue DENTRO dele (o bug da a6: devolvia o modulo)
        ("Resolucao::Ok(Encontrado::Modulo { file: f, trail: t }) if !ultimo => {", "Resolucao::Ok(Encontrado::Modulo { file: f, trail: t }) if false => {"),
        ("Resolucao::Ok(Encontrado::Enum(e)) if !ultimo => {", "Resolucao::Ok(Encontrado::Enum(e)) if false => {"),
        ("Resolucao::Ok(_) if !ultimo => {", "Resolucao::Ok(_) if false => {"),
    ],
    "r": [  # reexports e globs de modulo INLINE: do modelo do arquivo, filtrados pela posicao (o submodelo so guarda itens)
        (".filter(|r| r.trail == trail);", ".filter(|_| trail.is_empty());"),
        (".filter(|(gt, _)| *gt == trail)", ".filter(|_| trail.is_empty())"),
        ("                        r.trail == local.inline", "                        local.inline.is_empty()"),
    ],
    "e3": [  # so a posicao de arquivo declarado por #[path] (o par e[3] de "e"), para a alternativa B
        ("self.modpath_declarado(path, &layout, prof)\n                .unwrap_or(layout)", "{\n                let _ = prof;\n                layout\n            }"),
    ],
    "p": [  # nome possivelmente dinamico (macro de item / glob externo) vira nao_medido, nao um achado Incerto
        (
            '            self.abstem(\n                Abst::Falha(\n                    "nome possivelmente dinâmico (macro em nível de item / `use *` externo)",\n                ),\n                &[nome.to_string()],\n            );',
            '            empurrar(\n                &mut self.out,\n                "nome",\n                nome,\n                linha,\n                Alvo::Incerto,\n                "nome possivelmente dinâmico (macro em nível de item / `use *` externo)",\n                Vec::new(),\n            );',
        ),
    ],
    "o": [  # `gera_itens!();` (com `;`) no nivel de item e macro de item, nao uma instrucao qualquer
        ('"expression_statement" if tem_macro_invocation(ch) => m.macro_item = true,', '"expression_statement" if false => m.macro_item = true,'),
    ],
    "m": [  # globs primeiro + `use Cor::{..}` com a cabeca ligada por glob
        ("uso.segs.first().is_some_and(|c| c == seg) && self.vinculos.nomes.contains(seg)", "false && uso.segs.first().is_some_and(|c| c == seg) && self.vinculos.nomes.contains(seg)"),
        ("usos.sort_by_key(|u| !u.glob);", "let _ = &mut usos;"),
    ],
    "s": [  # comentarios dentro da lista de argumentos nao sao argumentos
        ('"attribute_item" | "line_comment" | "block_comment"', '"attribute_item"'),
    ],
    "t": [  # fn/use/binding de mesmo nome num mod inline ou na fn sombreia a fn do arquivo (aridade)
        ("let sig = if self.nome_sombreado(&nome) {", "let sig = if false {"),
    ],
    "v": [  # import PRIVADO e visivel aos descendentes do modulo (`super::Nome`)
        ("const IMPORT_PRIVADO_VALE: bool = true;", "const IMPORT_PRIVADO_VALE: bool = false;"),
    ],
    "w": [  # `use rstest::rstest;` (caminho que comeca pelo proprio nome introduzido) liga o crate, nao um item local
        ("&& r.segs.first().is_some_and(|s| s != head)", "&& !(r.segs.len() == 1 && r.segs[0] == head)"),
    ],
    "x": [  # a ausencia so se afirma com o modulo enumeravel: macro de item e reexport do nome nao fechado abstem
        ("            let mut reexport_aberto: Option<Abst> = None;\n", ""),
        ("                    Resolucao::Abstencao(a) => {\n                        reexport_aberto.get_or_insert(a);\n                    }", "                    Resolucao::Abstencao(_) => {}"),
        ('            if model.macro_item {\n                return Resolucao::Abstencao(Abst::Falha(\n                    "módulo com macro em nível de item pode declarar o nome",\n                ));\n            }\n', ""),
        ("            if let Some(a) = reexport_aberto {\n                return Resolucao::Abstencao(a);\n            }\n", ""),
    ],
    "y": [  # D: um `mod x { .. }` inline so ve os itens do arquivo que o `use super::..` traz (e a macro DELE abre so ele)
        ("        self.modelo.macro_item || self.vinculos.glob_externo || self.macro_no_modulo_atual()", "        self.modelo.macro_item || self.vinculos.glob_externo"),
        ("        if self.destino.is_empty() {\n            self.vinculos.nomes.insert(nome);\n        } else {\n            self.nomes_do_modulo\n                .entry(self.destino.clone())\n                .or_default()\n                .insert(nome);\n        }", "        self.vinculos.nomes.insert(nome);"),
        ("self.ligado_em(&uso.trail, seg)", "self.vinculos.nomes.contains(seg)"),
        ("        self.ligado_em(&self.mod_stack, nome)\n            || self.bindings", "        self.vinculos.nomes.contains(nome)\n            || self.bindings"),
        ("                    let ligado = self.ligado(&nome);\n                    if let Some((min, max)) = sig.filter(|_| ligado) {\n                        self.checar_aridade(&nome, min, max, node);\n                    } else if !ligado {", "                    if let Some((min, max)) = sig {\n                        self.checar_aridade(&nome, min, max, node);\n                    } else if !self.ligado(&nome) {"),
        ("                    .filter(|_| self.ligado(cabeca))\n", ""),
    ],
    "z": [  # `unsafe extern "C" { fn flock(..); }`: o que o bloco extern declara e item do modulo
        ('            "foreign_mod_item" => {', '            "zz_foreign_mod_item" => {'),
    ],
}
# ordem cronologica das correcoes: a variante Vn tem as n primeiras (desfaz ORDEM[n:])
ORDEM = ["a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "l", "m", "o", "p", "q", "r", "s", "t", "v", "w", "x", "y", "z"]
VARIANTES = {f"V{n}": ORDEM[n:] for n in range(len(ORDEM) + 1)}


def pares_de(corrs):
    """Pares de reversao na ordem de aplicacao: (w) refina a linha de (h), entao e desfeita ANTES dela."""
    return [p for c in sorted(corrs, key=lambda c: c not in ("y", "x", "w")) for p in REVERT[c]]

# alternativa B: o fonte final SEM a posicao de arquivo declarado por #[path] (so o layout)
VARIANTES["VB"] = ["e3"]

# id, descricao, [(trecho exato, substituto), ...]
MUTANTES: list[tuple[str, str, list[tuple[str, str]]]] = [
    (
        "R1-crate-vira-arquivo-atual",
        "`crate::` resolve a partir do arquivo corrente em vez da raiz do crate",
        [
            (
                "Some(k) => (k.root_file.clone(), Vec::new(), 1),\n                    None => return Resolucao::Abstencao(SEM_CRATE),\n                },",
                "Some(_k) => (local.arquivo.to_path_buf(), local.inline.clone(), 1),\n                    None => return Resolucao::Abstencao(SEM_CRATE),\n                },",
            )
        ],
    ),
    (
        "R2-self-vira-nome-de-modulo",
        "`self::` nao pula o segmento `self` (vira nome de modulo)",
        [('"self" => (local.arquivo.to_path_buf(), local.inline.clone(), 1),', '"self" => (local.arquivo.to_path_buf(), local.inline.clone(), 0),')],
    ),
    ("R3-super-como-crate", "`super::` resolvido como `crate::`", [('"crate" => match &krate {', '"crate" | "super" => match &krate {')]),
    ("R4-ignora-mod-rs", "`mod x;` so procura `x.rs`, nunca `x/mod.rs`", [('base.join(seg).join("mod.rs"),', 'base.join(seg).join("mod-ignorado.rs"),')]),
    ("R5-ignora-pub-use", "reexportacoes `pub use` explicitas nunca sao seguidas", [("for r in reexports {", "for r in reexports.filter(|_| false) {")]),
    ("R6-ignora-pub-use-glob", "reexportacoes `pub use x::*` nunca sao seguidas", [("for (gtrail, gsegs) in globs {", "for (gtrail, gsegs) in Vec::<(Vec<String>, Vec<String>)>::new() {")]),
    ("R7-ignora-workspace", "membros do workspace nao sao consultados (cross-crate)", [("} else if let Some(crate_dir) = self.workspace().get(cabeca).cloned() {", "} else if let Some(crate_dir) = None::<PathBuf> {")]),
    ("R8-ignora-dependencias", "tabelas de dependencias do Cargo.toml ignoradas", [("} else if self.deps_de(&self.dir_da_crate(k)).contains(cabeca) {", "} else if false {")]),
    (
        "R9-ignora-path-attr",
        "`#[path = \"x.rs\"]` ignorado",
        [("if let Some(p) = path_attr {\n            let relativo_a", "if let Some(p) = path_attr.filter(|_| false) {\n            let relativo_a")],
    ),
    ("R10-macro-de-item-como-inexistente", "macro em nivel de item tratada como inexistente", [('"macro_invocation" => m.macro_item = true,', '"macro_invocation" => {}')]),
    ("R11a-glob-externo-sem-dinamismo", "`use crate_declarado::*` nao marca o escopo como dinamico", [("Resolucao::Ok(Encontrado::Externo) => self.vinculos.glob_externo = true,", "Resolucao::Ok(Encontrado::Externo) => {}")]),
    (
        "R11b-glob-nao-resolvido-sem-dinamismo",
        "glob que nao fecha nao marca o escopo como dinamico",
        [("Resolucao::Abstencao(abst) => {\n                        self.vinculos.glob_externo = true;\n                        self.abstem(abst, &uso.segs);", "Resolucao::Abstencao(abst) => {\n                        self.abstem(abst, &uso.segs);")],
    ),
    (
        "R12-variante-invertida",
        "o teste de variante de enum do projeto invertido",
        [("if e.variantes.contains(variante) || e.assoc.contains(variante) {\n            return;\n        }", "if !e.variantes.contains(variante) || e.assoc.contains(variante) {\n            return;\n        }")],
    ),
    ("R13-ignora-non-exhaustive", "`#[non_exhaustive]` deixa de abster o literal de struct", [("let pular = enum_ne || campos.as_ref().is_some_and(|s| s.non_exhaustive);", "let pular = false;")]),
    ("R14-parse-com-erro-vira-allow", "abstencao vira allow: sintaxe quebrada devolve `medido(vazio)`", [("if tree.root_node().has_error() {\n            return Verificacao::nao_medido(", "if tree.root_node().has_error() {\n            return Verificacao::medido(Vec::new());\n        }\n        if false {\n            return Verificacao::nao_medido(")]),
    ("R15-resolucao-nao-fechada-vira-allow", "abstencao vira allow: a resolucao tentada e nao fechada nao e registrada", [("if !self.abstencoes.contains(&item) {", "if false && !self.abstencoes.contains(&item) {")]),
    ("R16-motivo-nunca-sai", "abstencao vira allow: o motivo calculado nunca chega ao `Verificacao`", [("if abstencoes.is_empty() {\n        return None;\n    }", "if abstencoes.is_empty() || true {\n        return None;\n    }")]),
    ("R17-projeto-sob-tmp-vira-scratch", "projeto que mora em /tmp deixa de ser `Incerto`", [('if path.starts_with("/tmp/") && em_crate {\n        Alvo::Incerto\n    }', 'if path.starts_with("/tmp/") && em_crate {\n        Alvo::Projeto\n    }')]),
    ("R18-nome-dinamico-some-calado", "abstencao vira allow: o nome possivelmente dinamico nao e registrado em `nao_medido`", [('            self.abstem(\n                Abst::Falha(\n                    "nome possivelmente dinâmico (macro em nível de item / `use *` externo)",\n                ),\n                &[nome.to_string()],\n            );', "            let _ = (nome, linha);")]),
    ("R19-aridade-nunca-estoura", "aridade de funcao do projeto nunca acusa", [("let estourou = n < min || max.is_some_and(|m| n > m);", "let estourou = false;")]),
    ("R20-mod-sem-arquivo-nao-acusa", "`mod x;` sem arquivo deixa de ser `import`", [("                if !existe {", "                if false && !existe {")]),
    ("R21-self-fora-de-impl-nao-acusa", "`Self` fora de `impl` deixa de ser `nome`", [('None => self.achou_nome("Self", linha),', "None => {}")]),
    # --- as correcoes: cada mutante e o REVERT da correcao (o teste dela precisa matar)
    ("Ma-pacote-em-teste-resolve-o-proprio-arquivo", "(a) desfeita: `use <pacote>::` em tests/benches/examples resolve o arquivo checado", REVERT["a"]),
    ("Mb1-item-associado-vira-variante", "(b) desfeita: item associado de enum tomado por variante ausente", REVERT["b"]),
    ("Mb2-impl-visivel-ignorado", "(b): o `impl` visivel ao lado do enum deixa de decidir", [("if e.variantes.contains(variante) || e.assoc.contains(variante) {", "if e.variantes.contains(variante) {")]),
    ("Mb3-item-associado-vira-allow", "(b): abstencao do item associado nao registrada (vira allow)", [("if !parece_variante(variante) {\n            // item associado de um `impl` que o adaptador não vê: tentou e não decide\n            self.abstem(", "if !parece_variante(variante) {\n            // item associado de um `impl` que o adaptador não vê: tentou e não decide\n            let _ = (")]),
    ("Mc1-prelude-incompleto", "(c) desfeita: `drop`, `size_of`… fora do preludio", [REVERT["c"][2]]),
    ("Mc2-itens-locais-nao-coletados", "(c) desfeita: itens declarados dentro do corpo da fn nao entram no escopo", REVERT["c"][:2]),
    ("Md-atributos-nao-sao-irmaos", "(d) desfeita: atributos externos nunca encontrados (o bug original)", REVERT["d"]),
    # --- (e) glob de modulo do projeto: um mutante por parte
    ("Me1-modulo-sem-item-nao-e-o-modulo", "(e): `use super::*` / `self::*` / `crate::*` voltam a nao resolver ao modulo", [REVERT["e"][0]]),
    ("Me2-usos-do-alvo-nao-entram", "(e): o `use` privado do modulo-alvo do glob nao entra no escopo", [REVERT["e"][1]]),
    ("Me3-glob-sem-guarda-de-ciclo", "(e): a cadeia de globs perde a guarda de ciclo", [("if !vistos.insert((file.to_path_buf(), trail.to_vec())) {", "if false && !vistos.insert((file.to_path_buf(), trail.to_vec())) {")]),
    ("Me4-glob-externo-do-pai-ignorado", "(e): `use crate_externo::*` no modulo-alvo deixa de abrir o escopo", [("                _ => self.vinculos.glob_externo = true,", "                _ => {}")]),
    ("Me5-macro-de-item-do-alvo-ignorada", "(e): macro de item no modulo-alvo deixa de abrir o escopo", [REVERT["e"][2]]),
    ("Me6-path-attr-nao-acha-o-declarante", "(e): arquivo declarado por `#[path]` volta ao caminho do layout", [REVERT["e"][3]]),
    ("Me7-proposto-nao-conta-como-existente", "(e): o arquivo proposto (ainda nao em disco) nao e filho existente", [REVERT["e"][4]]),
    ("Mf-path-attr-relativo-a-foo", "(f) desfeita: `#[path]` de arquivo nao-mod-rs relativo a `foo/`", REVERT["f"]),
    ("Mg-unit-struct-vira-import-inexistente", "(g) desfeita: `Tupla::nova` vira import inexistente", REVERT["g"]),
    ("Mh-pub-use-dep-vira-item-local", "(h) desfeita: `pub use dep;` resolve contra si mesmo", REVERT["w"] + REVERT["h"]),
    ("Mi-const-assoc-e-uso", "(i) desfeita: o nome de uma `const`/`static` e o tipo dela sao lidos como uso", REVERT["i"]),
    ("Mj-inline-aninhado-sem-itens", "(j) desfeita: modulo inline aninhado nao enxerga os proprios itens", REVERT["j"]),
    ("Ml-glob-reexport-nao-segue-o-alvo", "(l): o resto do caminho nao e procurado dentro do alvo do glob reexportado", REVERT["l"]),
    ("Ml2-glob-reexport-ciclo-sem-guarda", "(l): a busca pelos alvos dos globs perde a guarda de ciclo", [("if !visitados.insert(chave) {", "if false && !visitados.insert(chave) {")]),
    ("Ml3-glob-externo-no-reexport-ignorado", "(l): glob reexportado de crate externo deixa de abrir o nome", [("                    _ => algum_aberto = true,\n                }\n            }\n            if algum_aberto {", "                    _ => {}\n                }\n            }\n            if algum_aberto {")]),
    ("Mm1-cabeca-ligada-por-glob-ignorada", "(m): `use Cor::{..}` com `Cor` trazido por glob volta a import inexistente", [REVERT["y"][2], REVERT["m"][0]]),
    ("Mm2-globs-nao-vem-primeiro", "(m): os globs deixam de ser processados antes das folhas", [REVERT["m"][1]]),
    ("Ms-comentario-conta-como-argumento", "(s) desfeita: comentario dentro da lista de argumentos conta como argumento", REVERT["s"]),
    ("Mt-fn-sombreada-usa-a-aridade-do-arquivo", "(t) desfeita: fn de mesmo nome em mod inline/fn local usa a aridade da fn do arquivo", REVERT["t"]),
    ("Mv-import-privado-invisivel-ao-filho", "(v) desfeita: `use` privado do pai nao e visto por `super::Nome`", REVERT["v"]),
    ("Mq1-modulo-por-reexport-vira-o-item", "(q) desfeita: o modulo alcancado por reexport e devolvido no lugar do item final", REVERT["q"]),
    ("Mq2-reexport-de-modulo-nao-segue-dentro", "(q): so o salto para dentro do modulo reexportado desfeito", [REVERT["q"][0]]),
    ("Mr1-reexport-inline-nao-visto", "(r) desfeita: o reexport dentro de um `mod x {..}` inline nao e visto", REVERT["r"]),
    ("Mr2-glob-reexport-inline-nao-visto", "(r): o glob reexportado dentro de um `mod x {..}` inline nao e visto", [REVERT["r"][1]]),
    ("Mpath-posicionar-pelo-layout", "(e)/#[path]: o arquivo declarado por `#[path]` volta a ser posicionado pelo layout (`super::super` sobe para a raiz do crate)", REVERT["e3"]),
    ("Mo-macro-de-item-com-ponto-e-virgula", "(o) desfeita: `gera_itens!();` no nivel de item deixa de ser macro de item", REVERT["o"]),
    ("Mw-use-autorreferente-vira-item-local", "(w) desfeita: `use rstest::rstest;` resolve contra si mesmo e vira «crate fora das dependencias»", REVERT["w"]),
    ("Mx1-macro-de-item-nao-abstem-a-ausencia", "(x): modulo com macro em nivel de item volta a provar a ausencia do nome", [REVERT["x"][2]]),
    ("Mx2-reexport-aberto-vira-ausencia", "(x): reexport do nome que nao fechou volta a ser engolido (ausencia afirmada)", [REVERT["x"][3]]),
    ("Md1-arquivo-inteiro-ligado-a-todo-mod-inline", "(D): os nomes da raiz do arquivo voltam a valer dentro de todo `mod` inline", [("        self.ligado_em(&self.mod_stack, nome)\n            || self.bindings", "        self.vinculos.nomes.contains(nome)\n            || self.ligado_em(&self.mod_stack, nome)\n            || self.bindings")]),
    ("Md2-use-do-modulo-vai-para-a-raiz", "(D): o `use` de dentro do `mod` inline liga o nome na raiz, nao no modulo", [REVERT["y"][1]]),
    ("Md3-macro-do-modulo-inline-ignorada", "(D): a macro de item de DENTRO do `mod` inline nao abre o escopo dele", [REVERT["y"][0]]),
    ("Md4-modulo-sem-modelo-prova-ausencia", "(D): modulo fundo demais para modelar passa a provar a ausencia", [("self.modelo_atual().is_none_or(|m| m.macro_item)", "self.modelo_atual().is_some_and(|m| m.macro_item)")]),
    ("Md5-cabeca-do-use-olha-so-a-raiz", "(D): `use Cor::..` num `mod` inline procura a cabeca so entre os nomes da raiz", [REVERT["y"][2]]),
    ("Md6-aridade-sem-escopo", "(D): a assinatura do arquivo vale num `mod` inline mesmo sem o `use super::..`", [REVERT["y"][4]]),
    ("Md7-cabeca-tipada-sem-escopo", "(D): o enum/modulo tipado do arquivo vale como cabeca num `mod` inline sem o `use`", [REVERT["y"][5]]),
    ("Md-escopo-por-modulo-desfeito", "(D) desfeita por inteiro", REVERT["y"]),
    ("Mz-bloco-extern-nao-declara-itens", "(z) desfeita: a funcao declarada num bloco `extern` volta a ser nome ausente", REVERT["z"]),
    ("Mx-ausencia-sempre-afirmada", "(x) desfeita por inteiro: macro de item e reexport aberto nao abstem", REVERT["x"]),
]

STUB = '''//! Rust adapter (VRS) — STUB: registered, answers `unknown` until its body is written.
use super::{Adaptador, Verificacao};
use crate::vgp::CheckInput;

pub struct Rust;

impl Adaptador for Rust {
    fn lang(&self) -> &'static str {
        "rust"
    }

    fn verificar(&self, _input: &CheckInput) -> Verificacao {
        Verificacao::nao_implementado("rust")
    }
}

#[cfg(test)]
mod tests;
'''

ULTIMA = [(-1, -1)]


def sha(b: bytes) -> str:
    return hashlib.sha256(b).hexdigest()


def contagem(txt: str) -> tuple[int, int]:
    m = re.findall(r"^test result: \w+\. (\d+) passed; (\d+) failed", txt, flags=re.M)
    return (int(m[-1][0]), int(m[-1][1])) if m else (-1, -1)


def roda_testes() -> tuple[str, list[str], str]:
    env = dict(os.environ, CARGO_TARGET_DIR=TARGET)
    r = subprocess.run(CMD_TESTE, cwd=WT, env=env, capture_output=True, text=True, check=False)
    txt = r.stdout + "\n" + r.stderr
    ULTIMA[0] = contagem(txt)
    if "error[E" in txt or "could not compile" in txt:
        return "ERRO_DE_COMPILACAO", [], txt[-1500:]
    falhos = re.findall(r"^test (\S+) \.\.\. FAILED$", txt, flags=re.M)
    if re.search(r"^test result: FAILED", txt, flags=re.M) or falhos:
        return "FAILED", sorted(set(falhos)), ""
    if re.search(r"^test result: ok\. (\d+) passed", txt, flags=re.M):
        return "OK", [], ""
    return "INCONCLUSIVO", [], txt[-800:]


class Troca:
    """Troca temporaria do fonte, com marcador e restauracao por conteudo; o mtime restaurado e o de AGORA."""

    def __init__(self) -> None:
        self.original = ALVO.read_bytes()
        st = ALVO.stat()
        self.tempos = (st.st_atime, st.st_mtime)
        self.sha = sha(self.original)

    def __enter__(self):
        MARCADOR.write_text("troca temporaria do VRS (vrs-porta) — restaura por conteudo + utime\n")
        return self

    def aplica(self, texto: str) -> None:
        ALVO.write_text(texto)

    def restaura(self) -> None:
        ALVO.write_bytes(self.original)
        os.utime(ALVO, None)  # mtime = AGORA, nunca o original: com o original o cargo reusa o artefato do ultimo mutante
        assert sha(ALVO.read_bytes()) == self.sha, "restauracao por conteudo falhou"

    def __exit__(self, *exc) -> None:
        self.restaura()
        if MARCADOR.exists():
            MARCADOR.unlink()


def aplica_pares(texto: str, pares: list[tuple[str, str]]) -> tuple[str | None, int]:
    for velho, novo in pares:
        n = texto.count(velho)
        if n != 1:
            return None, n
        texto = texto.replace(velho, novo, 1)
    return texto, 1


def modo_mutantes(saida: Path, ids: set[str]) -> int:
    resultados = []
    with Troca() as t:
        texto = t.original.decode()
        base = roda_testes()
        resultados.append({"id": "BASE", "resultado": base[0], "falhos": base[1], "contagem": ULTIMA[0]})
        try:
            t.aplica(STUB)
            red = roda_testes()
        finally:
            t.restaura()
        resultados.append({"id": "RED-stub", "descricao": "contra o stub (unknown) os testes por classe reprovam", "resultado": red[0], "falhos": red[1], "contagem": ULTIMA[0], "morto": red[0] == "FAILED"})
        print(f"BASE: {base[0]} · RED-stub: {red[0]} {ULTIMA[0]} (passaram, falharam)", flush=True)
        for mid, desc, pares in MUTANTES:
            if ids and mid not in ids:
                continue
            novo, n = aplica_pares(texto, pares)
            if novo is None:
                resultados.append({"id": mid, "descricao": desc, "resultado": "ERRO_DO_RUNNER", "ocorrencias": n})
                print(f"{mid}: ERRO_DO_RUNNER ({n} ocorrencias)", flush=True)
                continue
            t0 = time.time()
            try:
                t.aplica(novo)
                res, falhos, nota = roda_testes()
            finally:
                t.restaura()
            morto = res == "FAILED"
            resultados.append({"id": mid, "descricao": desc, "resultado": res, "morto": morto, "testes_que_mataram": falhos, "segundos": round(time.time() - t0, 1), "nota": nota})
            print(f"{mid}: {'MORTO' if morto else res} ({len(falhos)} teste(s))", flush=True)
    vivos = [r["id"] for r in resultados if r["id"] not in ("BASE",) and not r.get("morto")]
    saida.write_text(json.dumps({"sha_rust_rs": t.sha[:16], "resultados": resultados, "vivos": vivos}, ensure_ascii=False, indent=1))
    print(f"mutantes: vivos={vivos}")
    return 1 if vivos else 0


def modo_variantes(dest: Path, saida: Path, quais: list[str]) -> int:
    env = dict(os.environ, CARGO_TARGET_DIR=TARGET)
    out = []
    with Troca() as t:
        texto = t.original.decode()
        for v in quais or list(VARIANTES):
            pares = pares_de(VARIANTES[v])
            novo, n = aplica_pares(texto, pares) if pares else (texto, 1)
            if novo is None:
                out.append({"variante": v, "erro": f"trecho com {n} ocorrencias"})
                print(f"{v}: ERRO ({n} ocorrencias)", flush=True)
                continue
            t0 = time.time()
            try:
                t.aplica(novo)
                r = subprocess.run(CMD_BIN, cwd=WT, env=env, capture_output=True, text=True, check=False)
            finally:
                t.restaura()
            if r.returncode != 0:
                out.append({"variante": v, "erro": "build falhou", "cauda": (r.stdout + r.stderr)[-1500:]})
                print(f"{v}: BUILD FALHOU", flush=True)
                continue
            (dest / v).mkdir(parents=True, exist_ok=True)
            bin_v = dest / v / "touring"
            shutil.copy2(Path(TARGET) / "debug/touring", bin_v)
            out.append({"variante": v, "desfeitas": VARIANTES[v], "binario": str(bin_v), "sha256": sha(bin_v.read_bytes()), "segundos": round(time.time() - t0, 1)})
            print(f"{v}: ok {out[-1]['sha256'][:16]} ({out[-1]['segundos']} s)", flush=True)
    saida.write_text(json.dumps({"sha_rust_rs": t.sha[:16], "variantes": out}, ensure_ascii=False, indent=1))
    return 0 if all("erro" not in o for o in out) else 1


def main() -> int:
    modo = sys.argv[1]
    if modo == "mutantes":
        return modo_mutantes(Path(sys.argv[2]), set(sys.argv[3:]))
    if modo == "variantes":
        return modo_variantes(Path(sys.argv[2]), Path(sys.argv[3]), sys.argv[4:])
    print(__doc__)
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
