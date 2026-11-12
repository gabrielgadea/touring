# Rótulos de triagem

As skills falam em cinco papéis canônicos de triagem. Este arquivo mapeia cada papel
para a string usada neste repo. Os nomes são os padrão, sem renomear.

O tracker deste repo é `touring decompose` (ver `issue-tracker.md`). **O rótulo é um
`--label` da subtask no DAG**: essa é a fonte única, e `frontier`/`ready` filtram por
ela. Um ticket carrega um papel de triagem por vez: ao trocar, remova o anterior no
mesmo comando.

| Papel em mattpocock/skills | Rótulo no DAG | Grave junto | Significado |
| -------------------------- | ----------------- | ----------- | ----------- |
| `needs-triage`    | `needs-triage`    | — | O mantenedor precisa avaliar |
| `needs-info`      | `needs-info`      | `decompose update … --status blocked` | Aguardando informação de quem abriu |
| `ready-for-agent` | `ready-for-agent` | `decompose ticket … --autonomy afk --fog clear` | Totalmente especificado, pronto para agente AFK |
| `ready-for-human` | `ready-for-human` | `decompose ticket … --autonomy hitl` | Exige implementação humana |
| `wontfix`         | `wontfix`         | `decompose update … --status cancelled` | Não será feito |

Aplicar um papel (exemplo: de `needs-triage` para `ready-for-agent`):

```
touring decompose update <task_id> <SUBTASK_ID> --add-label ready-for-agent --remove-label needs-triage
touring decompose ticket <task_id> <SUBTASK_ID> --autonomy afk --fog clear
```

A coluna "Grave junto" mantém o estado do DAG coerente com o papel: é por ela que
`claim` e `frontier` deixam de oferecer um ticket `needs-info` ou `wontfix`.

Os rótulos ficam só no DAG; nada é criado no GitHub.
