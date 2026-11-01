# Adendo ao P0B-d (17/10/2026, ~18:20Z) — gate de doc

O patch `p0bd.patch` (sha256 0dcb772a…) reprova `RUSTDOCFLAGS="-D warnings" cargo doc -p touring-code --no-deps`:

    error: public documentation for `fundir_findings` links to private item `corrobora_tipo_fechado`
       --> crates/touring-code/src/vgp/sombra.rs:369:15

Os gates do componente (code/pyright/hook-handlers/server + 15 mutantes) não rodavam o doc; o gate do integrado
(`int97-integrado/scripts/gates_int97i.sh`, passo «doc touring-code -D warnings») pegou.

Conserto aplicado no int97 (uma linha, sem mudança de comportamento): o link intra-doc vira código simples.

    -/// `deny` ([`corrobora_tipo_fechado`]). A different class, or one layer only, stays a warning.
    +/// `deny` (`corrobora_tipo_fechado`). A different class, or one layer only, stays a warning.

Verificado no int97: `cargo doc -p touring-code --no-deps` com `-D warnings` rc=0. Quem reaplicar o `p0bd.patch` em outra
árvore precisa aplicar esta linha junto.
