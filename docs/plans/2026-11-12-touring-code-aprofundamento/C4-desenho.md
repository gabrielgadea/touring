# C4 — Vista de projeto: overlays sobre o disco (nota de desenho)

Linha de base (`cargo test -p touring-code --no-fail-fast`, antes de editar, 12/11): lib 1481 ok / 3 falhas (os 3 `c7_red_*` do C7, RED em curso do
outro engenheiro) / 2 ign; 25 binários de integração ok; `vgp::` na lib: 659 ok (490 nos 5 adaptadores). Final: lib 1505 ok; `vgp::` 668 (490 nos adaptadores,
+9 da vista); `vgp_vista_overlays` 11 ok, `vgp_contencao_replay` e `vgp_guarda_disco` byte a byte. Nenhum ADR de `docs/adr/` toca VGP/overlays; nenhum crate
consumidor muda de contrato (o mapa de overlays só é consultado dentro de `vgp/lang/`; `CheckInput.overlays` segue igual).

## Interface (`crates/touring-code/src/vgp/lang/vista.rs`, módulo `pub(crate)`, em cima de `caminho.rs`)
```rust
#[derive(Clone, Copy)] pub struct Vista<'a> { sobrepostos: &'a HashMap<String, String> }
impl<'a> Vista<'a> {
  pub fn nova(&'a HashMap<String,String>) -> Self;
  pub fn sobreposto(&self, p:&Path) -> Option<&'a str>;                    // a camada sozinha (sem disco)
  pub fn sondar<P>(&self, raizes:&[P], p:&Path) -> Sonda;                  // overlay exato => Arquivo; senão caminho::sondar
  pub fn e_arquivo<P>(..) -> bool;  pub fn e_diretorio<P>(..) -> bool;  pub fn existe<P>(..) -> bool;
  pub fn ler<P>(&self, raizes:&[P], p:&Path, limite:Option<u64>) -> Result<String, Recusa>;
  pub fn outros(&self, p:&Path) -> impl Iterator<Item=(&'a str,&'a str)>;  // todo overlay menos o de `p`
}
```
Convenção de chave, UMA função privada `chave(p)`: `caminho::normaliza(p).to_string_lossy()`. Chave do mapa = caminho absoluto
lexicamente normal; a consulta normaliza o que recebe. Só a CHAVE é normalizada: o caminho entregue ao disco segue como o chamador deu.

## Invariantes
1. Precedência única: overlay vence o disco; o disco só vale dentro das raízes (`caminho`), e o overlay NÃO depende das raízes (o chamador o declarou, não o conteúdo).
2. Diretório implícito: existe diretório em `p` se o disco diz, ou se algum overlay está sob `p/` (a regra do TS, agora de todos).
3. Mapa vazio (produção dos hooks: `HashMap::new()`) custa um `is_empty()`: nem normaliza, nem aloca.
4. A vista não toca o disco por conta própria: tudo passa por `caminho` (a guarda `so_o_caminho_toca_o_disco` cobre o arquivo novo).
5. `Recusa` e `Sonda` são os de `caminho`; nada de tipo paralelo. Overlay nunca tem teto de tamanho (o teto é do disco).

## O que sai / sítios que mudam (grep em TODO `crates/`: o mapa só é consultado em `vgp/lang/`)
- `rust.rs:712` (`overlays.get`) -> `vista.ler`. · `python/modules.rs:300` -> `vista.ler` (o memo `fs_cache` passa a guardar o resultado da vista).
- `typescript/projeto.rs:168,191,197,205` (`contains_key`, `keys().any(starts_with)`, `get`, `sobrepostos()`) -> `sondar/e_diretorio/ler`; `Projeto::sobrepostos()` sai.
- `typescript/globais.rs:180` (`sobrepostos().iter()` + `k != eu`) -> `vista().outros(arquivo)`.
- `classe_css/folhas.rs:253,260,275` -> `e_arquivo` / `ler(.., Some(MAX_BYTES_FOLHA))` / `sobreposto`. · `html/recurso.rs:325` -> `existe`.
- Resultado: 10 consultas diretas ao mapa -> 0 fora de `vista.rs` (guarda estrutural em `tests/vgp_vista_overlays.rs`); 9 grafias `p.to_string_lossy()` de chave -> 1.
- Os 5 adaptadores seguem com as MESMAS assinaturas de construtor (`Resolver::new`, `Leitor::novo/com_cache`, `Projeto::novo/para_arquivo`, `Disco::de`).

## Divergências encontradas (D) e destino
- D1 chave: TS/HTML/CSS normalizam no chamador, Rust/Python não (consulta crua). Defeito: `#[path = "./x.rs"]` com overlay em `x.rs` leu o disco e acusou "declarado sem arquivo". Unificado (RED->GREEN).
- D2 Rust: existência (`e_arquivo` de manifesto/`lib.rs`/`main.rs`) só olha o DISCO; a leitura olha o overlay primeiro. Lacuna de cobertura (crate só-overlay vira abstenção), não veredito errado. PRESERVADO (`caminho::e_arquivo` no wrapper do Rust, com a razão no comentário).
- D3 diretório implícito: só o TS contava o diretório que só um overlay cria; o atalho "primeiro segmento" do HTML (`/assets/a.png`) descartava a raiz e acusava recurso que o overlay entrega. Defeito, unificado em `existe` (RED->GREEN).
- D4 teto de leitura: TS 3 MiB e CSS `MAX_BYTES_FOLHA` limitam o disco; Rust e Python não. PRESERVADO como o parâmetro `limite` de `ler`.
- D5 espécie: HTML pergunta "existe qualquer coisa", CSS/TS/Rust "é arquivo". PRESERVADO (`existe` x `e_arquivo`).
- D6 caminho entregue ao disco: TS/HTML/CSS o normalizam lexicamente antes (`a/..` atravessa symlink diferente do SO), Rust/Python o dão cru. PRESERVADO (a vista não mexe no caminho do disco).
- D7 memo de leitura: só o Python guarda leitura por conjunto de raízes. PRESERVADO no adaptador (agora sobre `vista.ler`).
- Overlay-apaga: nenhum adaptador tem a noção (`HashMap<String,String>` não expressa remoção); fica fora, sem seam hipotético.
- `fora` do TS: o flag setado em `eh_diretorio` é redundante (a sonda de arquivo do mesmo caminho já o seta, e só `provar` o lê, depois de zerar); a replay `vgp_contencao_replay` prova.

## Testes
Caracterização ANTES de editar (`tests/vgp_vista_overlays.rs`, via `check_em`, por adaptador: só-disco, overlay-vence, só-overlay, ausente) + D1/D3 em RED, + guarda estrutural. Testes unitários da vista em `vista/tests.rs`.
