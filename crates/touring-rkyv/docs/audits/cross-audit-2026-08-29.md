---
type: AuditReport
title: "cross-audit crates/touring-rkyv"
date: 2026-08-29
flow: cross-audit (adw)
---

## PURPOSE AUDIT (FACT= por alvo)

Bench é fiel — mede exatamente o que documenta (IPC real, tráfego de hook). Tenho evidência suficiente para o veredito. Compilando as linhas FACT por alvo:

**PURPOSE AUDIT — `crates/touring-rkyv`**

FACT=crate:touring-rkyv(propósito geral)=desvio README:7-9 promete "instead of local types, import templates" mas 4 consumidores reais definem duplicatas homônimas — potencializar: migrar qtable/linucb/crdt_graph/esaa para os templates
FACT=src/ipc.rs::IpcRequest/IpcResponse/frame_request/frame_response/unframe=fiel touring-dispatch/daemon.rs:1036,1352 + touring-server/daemon_client.rs:157,390 consomem de fato
FACT=src/saga_ipc.rs::frame_saga=orfao daemon.rs:1444 comenta "frame_saga não tem cliente de produção" — potencializar: wire o lado emissor do coordinator ou remover
FACT=src/saga_ipc.rs::unframe_saga=orfao só citado em saga_ipc.rs testes; daemon.rs parseia body cru sem chamá-la — potencializar: usar unframe_saga no handler ou documentar o motivo
FACT=src/saga_ipc.rs::SagaMessage(uso de produção)=fiel daemon.rs:1409 importa e desserializa via `from_bytes::<SagaMessage>`
FACT=templates.rs::ArchivedHookEvent=orfao grep em touring-hooks/touring-server: zero import real, só tests/round_trip.rs — potencializar: wire em touring-hooks ou remover do README
FACT=templates.rs::ArchivedEventRecord=desvio esaa.rs:386 define `EventRecord` local homônimo, não importa o template — potencializar: refatorar esaa.rs para usar o template
FACT=templates.rs::ArchivedSymbol=orfao README diz "touring-intelligence::index" mas grep no diretório index/ não achou nenhum uso — potencializar: wire em touring-intelligence::index ou remover
FACT=templates.rs::ArchivedIndexSnapshot=fiel touring-hooks-core/dependency_cache.rs:36,320,343,353 importa e usa de fato
FACT=templates.rs::ArchivedLearningParamsSnapshot/ArchivedQTableSnapshot=desvio qtable.rs:695-697 define `QTableSnapshot` local com `#[derive(rkyv::Archive)]` próprio — potencializar: substituir pelo te

## MAP+HARMONY (harmony_map)

```json
python3: can't open file '/home/gabrielgadea/.claude/skills/TACO-cross-audit/scripts/harmony_map.py': [Errno 13] Permission denied

```

## DEBT (scan_debt)

```json
python3: can't open file '/home/gabrielgadea/.claude/skills/TACO-cross-audit/scripts/scan_debt.py': [Errno 13] Permission denied

```

## E2E PROOF (prove_invariants)

```json
usage: prove_invariants.py [-h] [--timeout TIMEOUT] [--json] directory
prove_invariants.py: error: argument --timeout: invalid float value: ''

```
