---
type: Brief
title: Fan-out dos adaptadores da camada 1 — VTS/VTSX/JS · VHTML · VCDN+VCSS (transversais)
plan_id: 2026-10-08-vgp-motor-base
timestamp: 2026-10-10T00:05:00-03:00
autor: touring-09 (coordenadora)
tags: ["#kind:plan", "#status:active", "#domain:harness"]
---

# Fan-out dos adaptadores (depois da VL0)

Ordem do Gabriel (/goal 09/10): «prossiga com a implementação completa de proposta.md e VL0-multilinguagem-spec.md e
ao final, me prove, na prática, que tudo funciona de forma completa e perfeita». B =
`/home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base`.

## Leia antes de qualquer comando (todos)

`B/VL0-multilinguagem-spec.md` inteira (§2 linguagens, §3 o adaptador, §4 classes, §5 política, §7 corpus/juiz) ·
`B/RELATORIO-VL0-nucleo.md` §2 e §6 (o contrato REAL: `Verificacao { achados, nao_medido }`, `Alvo { Externo, Projeto,
Incerto }`, `politica::aplicar` sobrescreve a severidade — o adaptador NÃO decide deny; `lang::parse`/`lang::gramatica`;
`Transversal`; delta e veredito no `check`) · o código em `<seu worktree>/crates/touring-code/src/vgp/` (`mod.rs`,
`politica.rs`, `lang/mod.rs`, `lang/python.rs` como exemplo de adaptador completo).

## Regras comuns

- **Worktree próprio**, já criado com a VL0 aplicada por cima de `88c7cc8b` (sem commit — a VL0 ainda não foi integrada;
  a árvore `vgp/` é idêntica à do `touring-wt-vl0`, hash `df609f887ca1`). **Edite SÓ o(s) seu(s) arquivo(s)** em
  `crates/touring-code/src/vgp/lang/` (+ testes no mesmo diretório). Nenhuma linha de `mod.rs`, `politica.rs`, hooks ou
  CLI muda: se o contrato não cobrir algo, PARE e me escreva — não estenda o núcleo por conta própria.
- **Camada 1 é pura** (spec §3): sem daemon, sem rede, sem processo. Sem conseguir medir → `nao_medido` com o motivo
  (nunca vazio = limpo). Parse por `lang::parse("<lang>", src)` (passa pelo `parse_bounded`; TS e TSX são gramáticas
  DISTINTAS, nunca `ast::Lang::TypeScript`).
- **Build**: `CARGO_TARGET_DIR=/home/gabrielgadea/projects/touring/target/wt-<nome>`; TODO cargo por
  `/home/gabrielgadea/projects/touring/scripts/hw-slot --class rust --wait 3600 -- …` (máquina a 90–97 °C; rc 75 = sem
  vaga → repita; nunca contorne a espera). Binário do juiz: `cargo build --release -p touring-server --bin touring` no
  seu target, congelado em `~/.claude/touring/vgp/binarios/<sha8>/touring` com `.sha256` ao lado.
- **Context7** para a gramática tree-sitter da sua linguagem (nós e campos), a semântica de resolução (ESM/TS module
  resolution, `exports`, `paths`; HTML ids/`for`/`aria-*`; CSS seletores) — cite o que consultou.
- **Corpus de desenvolvimento** (pode ler e usar à vontade): `~/.claude/touring/vgp/corpus_semeado/<lang>/` (positivos
  por classe + negativos, gerados pela touring-4f; o `classe_erro` do juiz já classifica 296/296). **PROIBIDO ler**
  `~/.claude/touring/vgp/corpus_holdout_*` e `~/.claude/touring/vgp/oss/` (held-out selado — é o seu juiz final, não seu
  material).
- **Juiz**: `B/scripts/juiz_motor_vgp.py` (sha `6c8a31b02075da1e`, só leitura) com `--touring <bin congelado> --corpus
  <corpus_semeado/lang> --exigir-limite-superior`. Ele passa `--disk-file` quando a entrada tem `snap_antes`.

## Aceite (escrito antes; vale para cada agente, por linguagem/classe sob sua posse)

1. TDD por classe: teste VERMELHO no stub atual (responde `unknown`) → VERDE; controles que passam limpos (o símbolo
   existe; reexportado; de pacote externo instalado; atrás de construção dinâmica → `nao_medido`).
2. Mutantes mortos e listados (na CÓPIA, restauração por conteúdo com mtime = AGORA — `os.utime(p)` sem tempos; NUNCA o mtime original (`copy2`, `cp -p`, `utime` antigo), senão o cargo reusa o artefato do último mutante): pelo menos um por regra de resolução e
   um «abstenção vira allow».
3. Juiz no corpus semeado da(s) sua(s) linguagem(ns): recall de símbolo por classe ≥ 0,70 onde n ≥ 20 (n < 20 reportado
   com denominador); falso deny 0 nos negativos (o limite CP95 é reportado; o teto ≤ 1% é provado depois, por mim, no
   held-out selado); p95 ≤ 150 ms.
4. Gates: `cargo fmt --all -- --check`; `cargo clippy -p touring-code --all-targets -- -D warnings`; `cargo test -p
   touring-code --no-fail-fast` 0 falhas; `RUSTDOCFLAGS="-D warnings" cargo doc -p touring-code --no-deps`.
5. O teste do despacho do núcleo continua verde (sua linguagem deixa de responder «ainda não implementado»).

## As três frentes

### A — `vts` (worktree `~/projects/touring-wt-vts`; arquivos: `lang/typescript.rs`, `lang/tsx.rs`, `lang/javascript.rs`)
Classes: `import` (`import {X} from './m'` — `X` não exportado pelo módulo do projeto; módulo relativo inexistente, com
resolução de extensão/`index`/`paths` do tsconfig quando presente), `nome` (identificador do projeto inexistente),
`atributo` (membro de tipo DO projeto, só quando o tipo é inferível localmente — senão `nao_medido`), `kwarg` (chave de
objeto literal contra interface do projeto declarada localmente), e no TSX `componente` (`<Comp>` não importado nem
definido) e `prop` (prop que o `Props` do componente do projeto não tem). Pacote externo de `node_modules` → `Alvo::Externo`
só quando a declaração (`.d.ts`) é lida e o membro comprovadamente não existe; sem `.d.ts` → `nao_medido`. Corpus:
`corpus_semeado/{typescript,tsx,javascript}`.

### B — `vhtml` (worktree `~/projects/touring-wt-vhtml`; arquivo: `lang/html.rs`)
Classes: `recurso` (`src`/`href` LOCAL inexistente no disco, relativo ao arquivo e à raiz), `ancora` (`#id`, `for=`,
`aria-labelledby/describedby`, `getElementById`/`querySelector('#x')` no `<script>` inline, sem o id no documento; id
duplicado). URL externa → fora do seu escopo (é do VCDN). Corpus: `corpus_semeado/html` (ancora/recurso).

### C — `vtransv` (worktree `~/projects/touring-wt-vtransv`; arquivos: `lang/cdn.rs`, `lang/classe_css.rs`)
- `cdn`/`cdn_pin`: a camada 1 é PURA, então o 404 vem de um **cache local** de versões/arquivos/integrity
  (proponha o formato e o caminho, p.ex. `~/.claude/touring/vgp/cdn_cache.json`, e leia o que a touring-4f já gerou em
  `corpus_semeado/cdn_cache.json`); não está no cache → `nao_medido` com o motivo que NOMEIA o resolvedor. O resolvedor
  com rede (jsdelivr/unpkg/cdnjs; timeout curto; nunca na camada 1) é uma função separada no seu arquivo, exercida por
  teste com servidor HTTP falso local — a ligação dela a um comando/daemon fica para a integração (minha).
  `cdn_pin`: range/`latest`/sem versão → aviso com a versão exata resolvida (do cache).
- `classe_css`: classe usada (`class=` no HTML, `className=` no TSX/JSX) que nenhuma folha CARREGADA define, com «você
  quis dizer»; folha não resolvida → `nao_medido`; projeto com Tailwind/CSS-in-JS/utilitário gerado detectado →
  `nao_medido` para as classes utilitárias (nunca chute). Severidade é sempre aviso (spec §5.2), mas o recall é medido.
Corpus: `corpus_semeado/html` (cdn, cdn_pin, classe_css) e `corpus_semeado/tsx` quando houver classe_css.

## Proibido (todos)

commit/push; `update-touring`/`propagate`/`touring update`; `kill`/`pkill` em processo touring; `git stash`/`reset`/
`checkout --`; `rm -rf`; `cargo clean`; `~/.claude/settings.json`; editar fora dos seus arquivos e do seu relatório;
ler o held-out selado; rede na camada 1.

## Relatório (um por frente)

`B/RELATORIO-<VTS|VHTML|VTRANSV>.md`, roteiro de 8 seções (identidade · feito com evidência · estado por item do aceite
· processos · dados com sha · decisões/riscos · git · o que NÃO foi verificado). A entrega é hipótese até o juiz da
coordenadora no held-out selado.
