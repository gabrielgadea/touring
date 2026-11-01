# Incidente 09/10/2026 ~21:46 — o 1º harness do teste TDD matou o daemon global (PID 3156)

> Registrado a pedido da coordenadora analise-6b (aceite pós-incidente).
> Executor: analise-8614d1f9. Estado final: contido, daemon global DOWN
> (pidfile "3156" stale, socket removido pelo shutdown graceful), daemon
> per-project do analise intacto (PID 2928009). O passo 3 refeito é o que
> restaura o global (com o binário 30.4.93).

## Sequência (medida no log do teste, pytest-732)

1. O 1º harness rodava o update-touring END-TO-END com HOME falso + stub do
   `touring` em `$HOME/.local/bin/touring`.
2. O `install_symlinks` **sobrescreveu o stub**: o caminho virou symlink para o
   binário fake vazio do workspace-isca (log L7-11).
3. O `daemon-ctl restart` seguinte executou o binário vazio → falhou → o script
   caiu no **fallback legacy** do `force_daemon_restart` (log L19).
4. O fallback resolveu o daemon global REAL — `DAEMON_SOCKET` é fixo no script
   (`/tmp/touring-daemon-$(id -u).sock`, L77) e `global_daemon_pid` lê o
   registry/lsof reais — e matou o **PID 3156** com kill -TERM
   ("daemon stopped gracefully", L21, 21:46:49).
5. `start_daemon` falhou (binário fake vazio: "daemon exited before its socket
   accepted a connection", L26) → o global ficou DOWN.

## Impacto

- Daemon global morto — era o binário `(deleted)` que o release 30.4.93
  precisava substituir, mas pelo caminho errado e sem o novo no lugar.
- Daemon per-project do analise intacto; sessões CC em projetos com pin não
  afetadas. Shells fora de CC sem daemon global até o auto-spawn ou o passo 3.
- Nada mais tocado — a 6b conferiu read-only: os 5 links
  (~/.local/bin/{touring,touring-hook,touring-daemon},
  ~/.claude/hooks/{touring,touring-daemon}), o shim, o toolchain 30.4.92.

## Causa-raiz (no TESTE, não no script)

Harness que executa o dispatch completo do update-touring toca o estado real
da máquina — o script existe para operar sobre a máquina real (install real,
daemon real, registry real). Stubs no HOME falso não isolam o que o script
resolve por caminho fixo (/tmp/touring-daemon-UID.sock, pidfile, registry).

## Conserto do teste

Harness = `script_prelude()` (tudo até a 1ª definição de função — onde o scrub
de env mora) + `shell_function("kill_daemon")` + stub que sai 0; um invariante
posicional (`unset` antes da 1ª chamada a `"${HOME}/.local/bin/touring"`)
cobre `daemon-ctl restart` e `doctor` por construção. Inocuidade MEDIDA (aceite
A da 6b): diff ANTES/DEPOIS da suíte sobre {5 readlinks, PIDs vivos de
touring-daemon, mtime dos 3 ELF de target/release} = idêntico, nas duas formas
(normal e UPDATE_TOURING_REQUIRE_SYMLINK=1).

## Lição

Teste de script-de-deploy nunca executa o dispatch completo num harness — o
deploy é o único lugar onde o dispatch roda. A cascata que a REGRA #19 descreve
foi disparada por um teste, não por um humano: `install` sobrescreveu o stub e
o fallback legacy fez o resto. RED/GREEN de harness novo se prova primeiro com
medição de inocuidade, não depois.
