# Issue tracker: `touring decompose` + arquivos markdown

Issues e specs deste repo **não** vivem no GitHub Issues. O rastreio tem duas metades
que andam juntas:

- **O DAG do `touring decompose` é o índice e o estado**: existência do ticket,
  dependências, quem o reivindicou, rótulos de triagem, o caminho do corpo e a fronteira
  (o que está desbloqueado).
- **Um arquivo markdown é o corpo**: o texto longo e os comentários. O DAG guarda o
  caminho dele (`--body-file`), nunca o conteúdo.

Use só este fluxo: o repositório é público e nenhum ticket sai para o GitHub por aqui.

## Layout

```
docs/plans/<AAAA-MM-DD>-<slug>/
├── spec.md                    ← a spec (uma por esforço)
└── tickets/
    └── <SUBTASK_ID>-<slug>.md ← um arquivo por ticket, nunca um arquivo combinado
```

Cabeçalho de todo arquivo de ticket:

```markdown
# <título>

Task: <task_id>
Subtask: <SUBTASK_ID>

<corpo>

## Comments
```

`<task_id>` é o id que `touring decompose create` devolve (`task_…`). `<SUBTASK_ID>` é
escolhido por quem cria (`S-01`, `S-02`, …) e é o mesmo no DAG e no nome do arquivo.

## Convenções

- **Criar o esforço (spec)**: escreva `spec.md`, depois
  `touring decompose create feature "<título> — docs/plans/<AAAA-MM-DD>-<slug>/spec.md"`.
  Anote o `task_id` devolvido no topo do `spec.md` (`Task: <task_id>`).
- **Criar um ticket**: escreva o arquivo em `tickets/` primeiro (o `add` recusa um
  `--body-file` que não existe), depois
  `touring decompose add <task_id> <SUBTASK_ID> "<título>" --body-file docs/plans/<AAAA-MM-DD>-<slug>/tickets/<SUBTASK_ID>-<slug>.md --label needs-triage`.
- **Ler um ticket**: `touring decompose get <task_id>` devolve, por subtask, o estado
  (status, dono do claim, dependências), `labels` e `body_file`; o corpo é o arquivo que
  `body_file` aponta.
- **Listar**: `touring decompose get <task_id>` (um esforço) ou
  `touring decompose status` (visão global). Para filtrar por rótulo:
  `touring decompose ready <task_id> --label ready-for-agent` ou
  `touring decompose frontier <task_id> --label ready-for-agent`.
- **Comentar**: acrescente ao fim do arquivo, sob `## Comments`, com data e autor.
- **Aplicar / trocar rótulo**:
  `touring decompose update <task_id> <SUBTASK_ID> --add-label <novo> --remove-label <antigo>`.
  O rótulo vive no DAG; o arquivo não repete. Papéis de triagem: `triage-labels.md`.
- **Dependência / bloqueio**: `--depends-on=S-01,S-02` no `add`, ou
  `touring decompose update <task_id> <SUBTASK_ID> --depends-on=S-01,S-02` depois.
- **Fechar**: `touring decompose update <task_id> <SUBTASK_ID> --status completed`,
  com um comentário final no arquivo. `cancelled` = não se aplica mais (e **não** libera
  quem depende dele); `skipped` = deliberadamente não executado (libera).
- **Encerrar o esforço**: `touring decompose finalize <task_id>` quando todas as
  subtasks estiverem em estado terminal.

## Quando uma skill diz "publish to the issue tracker"

Escreva o arquivo markdown e registre-o no DAG (`create` para spec, `add --body-file`
para ticket), nessa ordem. Um arquivo sem entrada no DAG fica fora da fronteira.

## Quando uma skill diz "fetch the relevant ticket"

O usuário passa o caminho do arquivo ou o par `<task_id> <SUBTASK_ID>`. Dado o par, o
caminho é o `body_file` da subtask em `touring decompose get <task_id>`.

## Operações de wayfinding

Usadas pelo `/wayfinder`. O **mapa** é um task DAG; cada **filho** é uma subtask anotada
como ticket.

- **Mapa**: `docs/plans/<AAAA-MM-DD>-<slug>/map.md` (corpo Notes / Decisions-so-far /
  Fog), registrado com `touring decompose create plan "<título> — …/map.md"`.
- **Ticket filho**: arquivo em `tickets/` + `touring decompose add … --body-file …`, e
  então `touring decompose ticket <task_id> <SUBTASK_ID> --kind decision|implementation
  --subtype research|prototype|grilling|task --autonomy hitl|afk --fog clear|hazy|unknown`.
  Trabalho que nasce de uma decisão leva `--origin-ticket <SUBTASK_ID da decisão>`.
- **Bloqueio**: `--depends-on`. Além disso, decisões abertas bloqueiam a fronteira de
  implementação por construção.
- **Fronteira**: `touring decompose frontier <task_id>`. Névoa não avaliada aparece como
  `unknown`, nunca como `clear`.
- **Claim**: `touring decompose claim <task_id> --owner <id> --subtask <SUBTASK_ID>`
  (sem `--subtask`, entrega a próxima pronta). É a primeira escrita da sessão. Escolha
  trabalho sempre por `claim`: o `ready` só lê, e duas sessões que o consultam recebem a
  mesma subtask. Trabalho longo renova o lease com `touring decompose renew`; desistência
  devolve com `touring decompose release --owner <id> <task_id> <SUBTASK_ID>`.
- **Resolver**: acrescente a resposta sob `## Answer` no arquivo do ticket,
  `touring decompose update <task_id> <SUBTASK_ID> --status completed`, e então
  acrescente um ponteiro de contexto (resumo + caminho) ao Decisions-so-far do `map.md`.
