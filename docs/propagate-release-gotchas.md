# `propagate-release.sh` — gotchas que o script encapsula

> Movido do `CLAUDE.md` da raiz (item 2.1) em 11/11/2026: o script já aplica cada um
> destes; este arquivo guarda o **porquê**, para quem for alterar o script. Operar o
> release não exige lê-lo. Teste do script: `scripts/test_propagate_release.py`.

- **(a)** `touring update --all-projects` SEM canal resolve pelo lock de cada projeto — mantém a versão velha e NÃO propaga a nova; COM canal (`touring update <v> --all-projects`) arrasta o workspace FONTE de `dev` para `<v>`, quebrando o dev iterativo. Por isso o script itera projeto a projeto pulando a fonte.

- **(b)** `touring --version` escreve em **stderr**, não stdout — `2>/dev/null` apaga a versão e qualquer gate de verificação passa sem verificar nada.

- **(c)** o gate 5.5 (prova comportamental, 35 asserções) tem **retry-once** com re-espera do doctor (28/08): a estreia da prova reprovou por transiente — project actor drenando índice — e o operador leu EXIT=0 porque `| tail` engoliu o exit (`${PIPESTATUS[0]}` é a leitura correta; o script agora falha só após 2 tentativas).

- **(d)** o gate 1/6 roda o S1a do shadow r3 (`scripts/r3-s1a-gate`, 03/10/2026): o benchmark `r3_emission_cost_is_bounded` é `#[ignore]` e sem chamador ninguém o rodava. O gate faz **uma** re-medição (ruído térmico) e exige a linha `1 passed` — `cargo test -- --ignored <filtro>` sai 0 quando o filtro não casa nada, e um exit não-zero do `hw-slot` (70 = machine check) reprova mesmo com `1 passed` no texto. O limite é o do teste (E5.2).

- **(e)** o gate 1/6 roda os **testes dos crates tocados** e o clippy leva `--all-targets` (09/10/2026): a 30.4.95 saiu com um e2e quebrado (`cli_suggester_e2e`) e dois testes de `handlers/index.rs` que nunca compilaram, porque o gate só fazia `check` + clippy sem código de teste. Pacotes = `git diff <último commit "release …">..HEAD` ∪ `git status`, cada arquivo no `manifest_path` mais longo; um `cargo test -p <pkg> --no-fail-fast` por vez sob o `hw-slot` (o `touring-hook-handlers` leva `--features pre-hooks,post-hooks`, §13b). rc 75 = «sem vaga» (repete 1×, não é falha de teste); sem commit de release ou git falho = TODOS os pacotes (sinal ausente nunca estreita); `Cargo.toml`/`Cargo.lock` da raiz não mapeiam a pacote e só avisam. `--dry-run` lista pacotes e comandos sem executar; `--skip-gates` pula tudo. Teste: `scripts/test_propagate_release.py` (probes + mutantes sobre cópia do script).

- **(f)** o gate 1/6 constrói o release (`cargo build --release --workspace`) **antes** dos testes (18/10/2026): os e2e executam `target/release/touring`, e o build só no passo 2 fazia o gate medir o binário do release ANTERIOR (a 30.4.97 reprovou em `version_matches_crate_version`). Todo job do gate espera a vaga rust por `PROPAGATE_SLOT_WAIT` s (default 7200), e não pelos 600 s do hw-slot, que reprovavam o gate «sem vaga» com um lote vizinho de 40 min.
