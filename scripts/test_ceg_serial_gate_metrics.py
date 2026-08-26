#!/usr/bin/env python3
"""Guard estrutural: teste que toca o pipeline do gateway DEVE ser serial(gate_metrics).

Origem (medida em 2026-08-25): os testes de `gateway/metrics.rs` medem DELTAS
exatos dos contadores globais de processo (`after - before == 1`). O grupo
`#[serial_test::serial(gate_metrics)]` os serializa ENTRE SI — mas
`run_gateway_speculative` chama `run_gateway` por candidato, e
`run_gateway` incrementa `record_ceg_captured()` incondicionalmente
(pre_exec.rs:224). Seis testes speculative não carregavam o atributo: sob
paralelismo do cargo, um deles intercalava com a medição de delta e a suíte
flakava — a mesma família do flake `observe_increments_captured` (21/08/2026)
que o comentário do módulo já documentava.

O remédio (marcar os 6) foi aplicado. Este guard existe porque corrigir os
sítios conhecidos não impede o sétimo: é a família
`definer-module-cinco-sitios` — a correção pontual mascara o defeito, o guard
estrutural o mantém corrigido.

Regras verificadas (tabela RULES): todo `#[test]` cujo corpo toca um
conjunto de contadores globais medidos por delta — diretamente ou via helper
do próprio módulo de testes — precisa do grupo serial correspondente:
`crates/touring-ceg/src/**` → `serial(gate_metrics)` para o pipeline do
gateway; `crates/touring-integration-tests/tests/**` → `serial(health_delta)`
para `record_pre_signals`/`compute_signals_delta`/contadores `health_delta_*`.
Origem da segunda regra: `axis8_identity_does_not_bump_directional_counters`
(igualdade exata) corrompido por axis5/axis6 em paralelo — o gate de
convergência do plano afordância falhou nesse teste em 25/08.

Uso:
    python3 scripts/test_ceg_serial_gate_metrics.py          # exit 1 se violado
    python3 scripts/test_ceg_serial_gate_metrics.py --json
    python3 -m pytest scripts/test_ceg_serial_gate_metrics.py -q
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent

# Regras nomeadas: (escopo, chamadas que tocam o contador global, grupo serial
# exigido). Cobre as duas famílias medidas em 25/08: o pipeline do gateway
# (7 sítios no ceg) e os contadores health_delta (axis8 em integration-tests
# — igualdade exata `reg_before == reg_after` corrompida por axis5/axis6
# paralelos, o MESMO modo de falha).
RULES = (
    {
        "name": "ceg-gateway",
        "glob": "crates/touring-ceg/src/**/*.rs",
        "calls": ("run_gateway(", "run_gateway_speculative", "record_ceg_", "record_enrichment_"),
        "serial": "serial(gate_metrics)",
    },
    {
        "name": "health-delta",
        "glob": "crates/touring-integration-tests/tests/**/*.rs",
        "calls": ("record_pre_signals", "compute_signals_delta", "health_delta_record_count",
                  "health_delta_regression_count", "health_delta_improvement_count"),
        "serial": "serial(health_delta)",
    },
    # Terceira família (26/08/2026): o gate de turno T3-B lê a env global
    # TOURING_T3_FUSE_DISABLED, e `turn_gate_kill_switch_desliga_a_fusao` a
    # ESCREVE. Uma env var é global ao processo: enquanto ela está posta, todo
    # teste concorrente que atravesse o gate observa o oposto do que afirma.
    #
    # Medido antes do remédio: a suíte falhava 1 em 6 execuções paralelas
    # (0 em 6 seriais); isolando só os `turn_gate_*` a taxa ia a 4/10, e o
    # alvo sozinho dava 0/10 — assinatura de estado global, não de lógica.
    # Havia 22 testes atravessando o gate e apenas o autor do kill switch
    # sabia disso; a vítima que aparecia era acidental. É a mesma família das
    # duas regras acima: corrigir a vítima do dia não impede a 23ª.
    {
        "name": "t3-env",
        "glob": "crates/touring-cli/src/cli_suggester_tests.rs",
        "calls": ("code_mode_gates(", "turn_gate_pre_bash("),
        "serial": "serial(t3_env)",
    },
    # Quarta família (26/08/2026): `bump_arm` passou a ser o escritor ÚNICO da
    # contagem do braço — grava a vista durável (arquivo) E a volátil (os
    # átomos code_mode_arm_*). A unificação matou a divergência silenciosa
    # entre as duas vistas, mas trouxe os testes de `bump_arm` para a família
    # "contador global medido por delta": o teste de sincronia mediu delta 4
    # contra 2 no disco porque um irmão paralelo também bumpou.
    #
    # A regra é a consequência estrutural da correção, não um detalhe: quem
    # mexe em estado global do processo pertence a um grupo serial, e é o
    # guard — não a memória de quem escreve o próximo teste — que garante.
    {
        "name": "arm-counters",
        "glob": "crates/touring-cli/src/cli_suggester_tests.rs",
        "calls": ("bump_arm(",),
        "serial": "serial(gate_metrics)",
    },
)

SERIAL_ATTR = "serial("
TEST_RE = re.compile(r"^\s*#\[test\]")
ATTR_RE = re.compile(r"^\s*#\[")
FN_RE = re.compile(r"^\s*(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?fn\s+(\w+)")


def _blocks(text: str) -> list[dict[str, object]]:
    """Extrai blocos `#[test]` → (atributos, nome, corpo) por balanceamento de chaves."""
    lines = text.splitlines()
    out: list[dict[str, object]] = []
    i = 0
    while i < len(lines):
        if not TEST_RE.match(lines[i]):
            i += 1
            continue
        j = i + 1
        attrs: list[str] = []
        while j < len(lines) and (ATTR_RE.match(lines[j]) or not lines[j].strip()):
            attrs.append(lines[j].strip())
            j += 1
        m = FN_RE.match(lines[j]) if j < len(lines) else None
        if not m:
            i += 1
            continue
        name = m.group(1)
        # corpo: da linha do fn até o fechamento balanceado
        body: list[str] = [lines[j]]
        depth = lines[j].count("{") - lines[j].count("}")
        k = j + 1
        while k < len(lines) and depth > 0:
            body.append(lines[k])
            depth += lines[k].count("{") - lines[k].count("}")
            k += 1
        out.append({"line": i + 1, "attrs": attrs, "name": name, "body": "\n".join(body)})
        i = k
    return out


def _helpers_touching_pipeline(text: str, pipeline_calls: tuple[str, ...]) -> set[str]:
    """Fns NÃO-teste do arquivo cujo corpo chama o pipeline (expansão de 1 nível)."""
    helpers: set[str] = set()
    for m in FN_RE.finditer(text):
        pass  # nomes coletados abaixo, corpo por janela de chaves
    lines = text.splitlines()
    for idx, line in enumerate(lines):
        m = FN_RE.match(line)
        if not m:
            continue
        name = m.group(1)
        depth = line.count("{") - line.count("}")
        body = [line]
        k = idx + 1
        while k < len(lines) and depth > 0:
            body.append(lines[k])
            depth += lines[k].count("{") - lines[k].count("}")
            k += 1
        joined = "\n".join(body)
        if any(c in joined for c in pipeline_calls):
            helpers.add(name)
    return helpers


def offenders() -> list[dict[str, object]]:
    """Testes que tocam contadores globais medidos por delta, sem o grupo serial."""
    found: list[dict[str, object]] = []
    for rule in RULES:
        for path in sorted(REPO.glob(rule["glob"])):
            text = path.read_text(encoding="utf-8", errors="ignore")
            if "#[test]" not in text:
                continue
            helpers = _helpers_touching_pipeline(text, rule["calls"])
            for blk in _blocks(text):
                if any(rule["serial"] in a for a in blk["attrs"]):  # type: ignore[union-attr]
                    continue
                body = str(blk["body"])
                direct = any(c in body for c in rule["calls"])
                via_helper = any(re.search(rf"\b{re.escape(h)}\s*\(", body) for h in helpers)
                if direct or via_helper:
                    found.append(
                        {
                            "rule": rule["name"],
                            "file": str(path.relative_to(REPO)),
                            "line": blk["line"],
                            "test": blk["name"],
                            "via": "direct" if direct else "helper",
                            "remedy": (
                                f"adicionar #[serial_test::{rule['serial']}] entre o "
                                "#[test] e o fn — o teste toca contadores globais que "
                                "irmãos medem por delta sob paralelismo do cargo"
                            ),
                        }
                    )
    return found


def main() -> int:
    """Executa o guard; exit 0 limpo, 1 com violação."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--json", action="store_true", help="saída JSON")
    args = parser.parse_args()

    bad = offenders()
    if args.json:
        json.dump({"ok": not bad, "offenders": bad}, sys.stdout, indent=2)
        print()
    elif bad:
        # A mensagem deriva da REGRA violada. Antes era fixa no texto da
        # primeira regra, então uma violação de `health-delta` ou `t3-env`
        # era anunciada como se fosse do pipeline do gateway — mandando quem
        # lê o CI investigar o subsistema errado.
        by_rule: dict[str, list[dict[str, object]]] = {}
        for entry in bad:
            by_rule.setdefault(str(entry["rule"]), []).append(entry)
        for name, entries in by_rule.items():
            serial = next(r["serial"] for r in RULES if r["name"] == name)
            print(f"VIOLAÇÃO [{name}] — teste toca estado global sem {serial}:")
            for entry in entries:
                print(f"  {entry['file']}:{entry['line']} {entry['test']} ({entry['via']})")
                print(f"    remédio: {entry['remedy']}")
        print(
            "\nSem o grupo serial, o teste intercala com irmãos que medem esse "
            "estado por delta (ou o sobrescrevem) sob o paralelismo do cargo, "
            "e a suíte flaka com vítima rotativa."
        )
    else:
        names = ", ".join(str(r["name"]) for r in RULES)
        print(f"OK — todo teste sob as regras [{names}] carrega seu grupo serial")
    return 1 if bad else 0


# ── pytest ────────────────────────────────────────────────────────────────────


def test_no_unmarked_pipeline_test():
    assert offenders() == [], f"testes sem serial(gate_metrics): {offenders()}"


def test_parser_flags_synthetic_violation():
    """Prova por construção: um teste sintético violando DEVE ser detectado."""
    synthetic = '''
    #[test]
    fn touches_the_pipeline() {
        let outcome = run_gateway("Bash", "ls", None, &deps).expect("ok");
        assert!(outcome.id.as_str().starts_with("exec-"));
    }

    #[test]
    #[serial_test::serial(gate_metrics)]
    fn touches_the_pipeline_marked() {
        let _ = run_gateway("Bash", "ls", None, &deps);
    }

    #[test]
    fn does_not_touch_the_pipeline() {
        assert_eq!(1 + 1, 2);
    }
    '''
    blocks = _blocks(synthetic)
    names = {b["name"]: b for b in blocks}
    assert len(blocks) == 3
    # o marcado carrega o atributo; o não-marcado não; o puro não chama o pipeline
    assert any(SERIAL_ATTR in a for a in names["touches_the_pipeline_marked"]["attrs"])
    assert not any(SERIAL_ATTR in a for a in names["touches_the_pipeline"]["attrs"])
    assert "run_gateway(" in str(names["touches_the_pipeline"]["body"])
    assert not any(c in str(names["does_not_touch_the_pipeline"]["body"]) for c in RULES[0]["calls"])


def test_helper_expansion_catches_indirect_call():
    """Teste que chama um helper local que toca o pipeline também é ofensor."""
    synthetic = '''
    fn drive_the_gateway(deps: &GatewayDeps) {
        let _ = run_gateway("Bash", "ls", None, deps);
    }

    #[test]
    fn indirect_via_helper() {
        drive_the_gateway(&deps);
    }
    '''
    helpers = _helpers_touching_pipeline(synthetic, RULES[0]["calls"])
    assert "drive_the_gateway" in helpers
    blocks = _blocks(synthetic)
    body = str(blocks[0]["body"])
    assert re.search(r"\bdrive_the_gateway\s*\(", body)


if __name__ == "__main__":
    sys.exit(main())
