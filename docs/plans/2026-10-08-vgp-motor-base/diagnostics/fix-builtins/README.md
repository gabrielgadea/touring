# Fix-builtins — evidência do conserto de `exit`

## Description / Descrição

Fixtures sintéticas e provas do falso deny de `exit` no motor VGP. Nenhum conteúdo do corpus gasto foi usado.

## Install / Instalação

Exige o workspace Touring, Python 3, Cargo e `scripts/hw-slot`. Não há pacote separado.

## Usage / Uso

```bash
python3 -I -B sonda_builtins.py
cargo test -p touring-code site_builtins_exit_e_quit_nao_geram_falso_deny
python3 -I -B mutante_exit.py
```

## Contributing / Contribuição

Mantenha `trap()` como controle de deny verdadeiro. Não adicione nomes à allowlist sem oráculo do runtime e teste de regressão.

## Tests / Testes

```bash
cargo test -p touring-code builtins
cargo test -p touring-code exec_e_return_tambem_nao_geram_falso_deny
cargo test -p touring-code trap_sem_definicao_continua_negado
cargo test -p touring-code --no-fail-fast
```

## License / Licença

Sem licença adicional própria; seguem as condições do repositório Touring.
