## What does this change?

<!-- One paragraph. What behaviour is different after this PR? -->

## Why?

<!-- The problem being solved. Link an issue with "Closes #123" if one exists. -->

## How was it verified?

<!--
Evidence, not intent. Paste the command and its output.
"Tests pass" is not evidence; the output is.
-->

```
$ cargo test -p <crate>
```

## Checklist

- [ ] `cargo check --workspace` passes
- [ ] `cargo clippy --workspace -- -D warnings` passes
- [ ] Tests added or updated for the changed behaviour
- [ ] No new orphan public symbols (`touring wiring orphans -j`)
- [ ] Documentation updated if behaviour or the CLI surface changed
- [ ] Commits explain *why*, not just *what*

## Blast radius

<!--
For changes touching shared code, paste `touring ast blast <file>` so
reviewers can see the reach. Delete this section for docs-only changes.
-->
