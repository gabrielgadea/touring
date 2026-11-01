"""Mutantes do P0: aplica cada mutação, roda os testes vgp::, registra rc + testes que falharam,
restaura o arquivo (cp do backup, nunca git). Uso: python3 mutantes.py <worktree> <saida.json> [M...]
Cada mutante e (arquivo, antigo, novo[, antigo2, novo2...]) sobre o texto do arquivo."""
import json
import os
import re
import shutil
import subprocess
import sys

WT = sys.argv[1]
SAIDA = sys.argv[2]
SO = {a for a in sys.argv[3:] if not a.startswith('--')}
V = f"{WT}/crates/touring-code/src/vgp"
ENV = dict(os.environ, CARGO_TARGET_DIR="/home/gabrielgadea/projects/touring/target/vgp-test")

MUTANTES = {
    # M0 = o código ORIGINAL (sem coletor, getattr de topo como antes): prova o RED dos testes novos
    "M0-ORIGINAL": [
        ("modules.rs",
         "    let ns = super::namespace::collect(source, root);\n    info.top_names.extend(ns.names);\n    info.has_star |= ns.has_star;\n    info.has_getattr |= ns.has_getattr;\n    info.has_dynamic |= ns.has_dynamic;\n",
         ""),
        ("modules.rs",
         "            let sig = func_sig(source, def);\n            info.functions.insert(nome, sig);",
         "            let sig = func_sig(source, def);\n            if nome == \"__getattr__\" {\n                info.has_getattr = true;\n            }\n            info.functions.insert(nome, sig);"),
    ],
    # remover a travessia de compostos: numpy/rust_bridge voltam a negar
    "M1-SEM-TRAVESSIA-DE-COMPOSTOS": [
        ("namespace.rs",
         "    fn bind_node(&mut self, node: Node) -> bool {\n        match node.kind() {",
         "    fn bind_node(&mut self, node: Node) -> bool {\n        if matches!(node.kind(), \"if_statement\" | \"try_statement\" | \"with_statement\" | \"while_statement\" | \"match_statement\" | \"for_statement\") {\n            return false;\n        }\n        match node.kind() {"),
    ],
    # remover o sinal dinâmico: Pillow volta a negar
    "M2-SEM-SINAL-DINAMICO": [
        ("namespace.rs",
         "        self.ns.has_dynamic = true;\n    }\n\n    // ------------------------------------------------------------------ literals",
         "    }\n\n    // ------------------------------------------------------------------ literals"),
    ],
    # severidade divergente arquivo x inline: o alvo incerto nega só nos baldes naturais
    "M3-SEVERIDADE-DIVERGENTE": [
        ("checks.rs",
         "        if modulo.has_getattr || modulo.has_star || modulo.has_dynamic {\n            return Tri::Unknown;\n        }",
         "        if modulo.has_getattr || modulo.has_star || modulo.has_dynamic {\n            return if self.deny_natural && modulo.has_dynamic {\n                Tri::No\n            } else {\n                Tri::Unknown\n            };\n        }"),
    ],
    # __getattr__ guardado nunca fecha: o controle np.arrai vira abstenção
    "M4-GETATTR-SEMPRE-ABERTO": [
        ("namespace.rs",
         "        let param = self.first_param(def)?;\n        let body = def.child_by_field_name(\"body\")?;",
         "        let _ = self.first_param(def)?;\n        return None;\n        #[allow(unreachable_code)]\n        let param = self.first_param(def)?;\n        let body = def.child_by_field_name(\"body\")?;"),
    ],
    # globals()[k] sobre lista literal deixa de fechar
    "M5-LACO-LITERAL-NAO-FECHA": [
        ("namespace.rs",
         "            if let Some((_, Some(lits))) = loops.iter().rev().find(|(n, _)| n == nome) {",
         "            if let Some((_, Some(lits))) = loops.iter().rev().find(|(n, _)| n == nome && false) {"),
    ],
    # sem a exigência do raise final (cair no fim devolve None)
    "M6-SEM-RAISE-FINAL": [
        ("namespace.rs",
         "        if last.kind() != \"raise_statement\" {\n            return None;\n        }",
         "        let _ = last;"),
    ],
    # else/elif herdam guarded=true: o return do else deixa de abrir
    "M7-ELSE-HERDA-GUARDA": [
        ("namespace.rs",
         "                    if !self.guarded(alt, param, guarded, nomes) {",
         "                    if !self.guarded(alt, param, true, nomes) {"),
    ],
    # o alias `module = sys.modules[__name__]` não conta
    "M8-SEM-ALIAS-DE-MODULO": [
        ("namespace.rs",
         "if !self.module_aliases.contains(nome) && self.is_module_expr(*rhs) {",
         "if !self.module_aliases.contains(nome) && self.is_module_expr(*rhs) && false {"),
    ],
    # setattr em qualquer objeto vira sinal (superabstenção: mata o teste do self/param)
    "M9-SETATTR-QUALQUER-OBJETO": [
        ("namespace.rs",
         "&& self.is_module_expr(target)",
         "&& (self.is_module_expr(target) || true)"),
    ],
    # global em função deixa de declarar nome
    "M10-SEM-GLOBAL": [
        ("namespace.rs",
         "    fn bind_global(&mut self, node: Node) {\n        let mut c = node.walk();\n        for ch in node.named_children(&mut c) {\n            let nome = self.t(ch);\n            self.add(&nome);",
         "    fn bind_global(&mut self, node: Node) {\n        let mut c = node.walk();\n        for ch in node.named_children(&mut c) {\n            let _ = ch;"),
    ],
    # o chamador que recebe globals()/o módulo pode ligar nomes: a entrega deixa de abster
    "M11-SEM-ENTREGA-DE-NAMESPACE": [
        ("namespace.rs",
         "        if hands {\n            self.ns.has_dynamic = true;\n        }",
         "        let _ = hands;"),
    ],
    # sys.modules[qualquer] = x passa a substituir ESTE módulo (superabstenção: pptx)
    "M12-SYS-MODULES-QUALQUER-CHAVE": [
        ("namespace.rs",
         "if matches!(text(self.source, k), \"__name__\" | \"__package__\") {",
         "if true {"),
    ],
    # módulo nomeado por literal passa a contar como este módulo
    "M13-MODULO-POR-LITERAL": [
        ("namespace.rs", "        by_name && !literal_first", "        by_name"),
    ],
    # from __future__ import annotations deixa de ligar
    "M14-SEM-FUTURE-IMPORT": [
        ("namespace.rs",
         "\"import_from_statement\" | \"future_import_statement\" => self.bind_import_from(node),",
         "\"import_from_statement\" => self.bind_import_from(node),"),
    ],
    # a lista de chamadores só-leitura vazia: print(globals()) vira ligação dinâmica
    "M15-SEM-CHAMADORES-SO-LEITURA": [
        ("namespace.rs",
         "n if READ_ONLY_CALLEES.contains(&n) => {}",
         "n if false && READ_ONLY_CALLEES.contains(&n) => {}"),
    ],
    "M16-SEM-TYPE-ALIAS": [
        ("namespace.rs",
         "\"type_alias_statement\" => self.bind_type_alias(node),", ""),
    ],
    "M17-EXEC-NUNCA-ABERTO": [
        ("namespace.rs", "\"exec\" if depth == 0 ||", "\"exec\" if false ||"),
    ],
    # `or` de duas guardas literais deixa de fixar os nomes
    "M18-OR-NAO-FECHA": [
        ("namespace.rs",
         "(\"and\" | \"or\", Some(mut a), Some(b)) => {",
         "(\"and\", Some(mut a), Some(b)) => {"),
    ],
    # a existência de extensão compilada deixa de valer: lxml.etree volta a ser negado
    "M19-SEM-EXTENSAO-COMPILADA": [
        ("modules.rs",
         "        bases\n            .iter()\n            .any(|b| dir_has_extension(&b.join(parent), last))",
         "        let _ = (bases, parent, last);\n        false"),
    ],
    # sem o ponto obrigatório depois do nome: `etree_extra.so` passa por `etree_`
    "M20-EXTENSAO-SEM-PONTO": [
        ("modules.rs", "rest.starts_with('.') && (", "(")
    ],
    # a profundidade nunca desce ao sair de um def/class: o resto do módulo parece estar "dentro"
    "M21-PROFUNDIDADE-NAO-DESCE": [
        ("namespace.rs",
         "            \"function_definition\" | \"class_definition\" => self.depth -= 1,",
         "            \"function_definition\" | \"class_definition\" => {}"),
    ],
    # o laço literal não é desempilhado ao sair do for
    "M22-LACO-NAO-DESEMPILHA": [
        ("namespace.rs",
         "                    self.loops.truncate(keep);",
         "                    let _ = keep;"),
    ],
}


def rodar():
    r = subprocess.run(
        ["cargo", "test", "-p", "touring-code", "--lib", "vgp::"],
        cwd=WT, env=ENV, capture_output=True, text=True)
    txt = r.stdout + r.stderr
    falhos = sorted(set(re.findall(r"^test (\S+) \.\.\. FAILED", txt, re.M)))
    resumo = re.findall(r"test result: .*", txt)
    return r.returncode, falhos, resumo, txt[-1500:] if r.returncode and not falhos else ""


def checar():
    """Valida que toda ancora existe no codigo atual (sem compilar)."""
    ruim = 0
    for nome, edits in MUTANTES.items():
        for arq, antigo, _ in edits:
            if antigo not in open(f"{V}/{arq}").read():
                print("ANCORA AUSENTE", nome, arq, repr(antigo[:70]))
                ruim += 1
    print("ancoras:", "ok" if not ruim else f"{ruim} ausentes")


def main():
    res = {}
    if os.path.exists(SAIDA):
        res = json.load(open(SAIDA))
    for nome, edits in MUTANTES.items():
        if SO and nome not in SO:
            continue
        arqs = {}
        try:
            for arq, antigo, novo in edits:
                p = f"{V}/{arq}"
                if p not in arqs:
                    arqs[p] = open(p).read()
                    shutil.copy(p, p + ".orig_mut")
                txt = open(p).read()
                if antigo not in txt:
                    raise RuntimeError(f"ancora ausente em {arq}: {antigo[:60]!r}")
                open(p, "w").write(txt.replace(antigo, novo, 1))
            rc, falhos, resumo, cauda = rodar()
            res[nome] = {"rc": rc, "morto": rc != 0, "falhos": falhos, "resumo": resumo, "cauda": cauda}
        except Exception as e:  # noqa: BLE001
            res[nome] = {"erro": str(e)}
        finally:
            for p, orig in arqs.items():
                open(p, "w").write(orig)
                os.remove(p + ".orig_mut")
        json.dump(res, open(SAIDA, "w"), indent=1, ensure_ascii=False)
        print(nome, "->", {k: v for k, v in res[nome].items() if k != "cauda"}, flush=True)


if __name__ == "__main__":
    (checar if "--checar" in sys.argv else main)()
