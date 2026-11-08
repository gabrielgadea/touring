# BRIEF — touring-0c (MiniMax) · C8 (#8 exceções da guarda `caminho`) + C3a (#3 denominador do `code_mode_adherence`)

Coordenadora: touring-02 (Anthropic). Aprovação do Gabriel em 04/11/2026: "opção A, aprovo P0 a P3".
Contexto: `docs/plans/2026-11-01-aprofundamento-arquitetural/RETOMADA.md` (seções #8 e #3).

## Onde trabalhar (exclusivo seu)

- Worktree: `~/projects/touring-wt-c8` · branch `feat/2026-11-04-c8-c3` (base `4cc2d806`).
- Arquivos seus: `crates/touring-code/src/vgp/sombra.rs`, `crates/touring-code/src/vgp/lang/cdn.rs` (só doc-comment),
  o teste da guarda estrutural do #8 (procure em `crates/touring-code` a lista de exceções que cita `sombra.rs`/`cdn.rs`),
  `crates/touring-cli/src/cli/kpi/code_mode.rs` e `crates/touring-cli/src/cli/kpi.rs` (só o trecho do `code_mode_adherence`).
- NÃO tocar: qualquer outro arquivo de `touring-cli` (o `code_mode_gates/` é de outra sessão), `touring-ceg`,
  `touring-hooks*`, `touring-dispatch`.
- Proibido até 08/11 00:10 (AB34): hooks/settings vivos, daemon, `update-touring`, `cargo build --release` no `target/` principal.
- Todo cargo via `hw-slot --class rust -- cargo …` (rc 75 = sem vaga, repita depois).

## C8 — decisão aprovada

- `sombra.rs` (linhas 69 e 129 leem `.touring/touring.toml` com `std::fs::read_to_string`) passa a ler por
  `caminho::ler_contido(&[project_root], &project_root.join(".touring/touring.toml"))` — uma regra só de contenção.
  Arquivo ausente/recusado continua dando o mesmo resultado de hoje (o `.ok()` atual vira o tratamento do `Recusa`).
  As escritas do log de sombra (linha ~423) NÃO são leitura: ficam como estão.
- Sai da lista de exceções da guarda estrutural; o teste da guarda passa a falhar se alguém reintroduzir `std::fs` de leitura ali.
- `cdn.rs` (cache CDN) CONTINUA exceção: é rede, não disco de projeto. Documente no doc-comment do módulo e na lista
  de exceções o motivo em uma frase.

## C3a — decisão aprovada

O `code_mode_adherence` (`touring kpi -j`, fonte `~/.claude/touring/run_journal.jsonl`) conta hoje 58 eventos de RECUSA
e 3 linhas que não são objeto JSON como runs falhos (medido 0,92729; sem eles 0,92844). Recusa não é run:
- exclua do denominador (e do numerador) os eventos de recusa e as linhas não-objeto;
- exponha os dois contadores excluídos no JSON (`refused_events`, `non_object_lines`) — nada some em silêncio;
- use os predicados nomeados de `touring_foundation::run_outcome` (`is_success`/`has_failure`) em vez de reimplementar.
Não recalibre o piso 0,8 (é decisão posterior, com o número novo).

## Aceite (o meu juiz roda tudo de novo)

- C8: `cargo test -p touring-code` 0 falhas (base 1708/0); replay dos 86 casos adversariais do #8 idêntico ao de hoje;
  `cargo clippy -p touring-code --all-targets --all-features -- -D warnings` limpo.
- C3a: teste unitário com fixture que tenha 1 run ok, 1 run falho, 1 recusa e 1 linha não-objeto → ratio 0,5 e os dois
  contadores = 1; `cargo test -p touring-cli` 0 falhas; clippy `-p touring-cli --all-targets --all-features -D warnings` limpo;
  `cargo run -p touring-cli -- kpi -j` (debug, no seu worktree) mostra o ratio novo e os contadores sobre o journal real.
- Commits pequenos (um por item), mensagem em inglês, sem nome de usuário nem `/home/<user>` em arquivo.

## Relatório (um só, ao terminar)

SendMessage para `touring-02` com commits (hash + título), comandos com exit code real e contagens, o número medido
antes/depois do `code_mode_adherence`, e o que ficou de fora. Também `RELATORIO-C8-C3a.md` neste bundle, no SEU worktree.
