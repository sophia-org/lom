# Lom development notes

Lom uses `zk` for linked investigations, decisions, plans, concepts, and
milestone records. Start with the [daily-driver plans](indexes/plans.md) and the
[active todo.txt queue](../../todo.md). The [tracking contract](../work-tracking.md)
defines which file owns task status and how work is completed.

## Record types

| Kind | Use |
| --- | --- |
| Investigation | An incident, experiment, diagnosis, or open question |
| ADR | An architectural choice, alternatives, and consequences |
| Plan | Scope, dependencies, measurable exits, and task criteria |
| Concept | A reusable idea with supporting evidence and limits |
| Milestone | Completion, retargeting, deferral, or evidence review |

Create notes from anywhere inside the checkout:

```sh
zk investigate --title "Describe the question"
zk adr --title "Describe the decision"
zk plan --title "Describe the scope and exit"
zk milestone --title "Describe the result"
zk recent
zk list docs/notes --match "GPU admission"
```

Templates allocate stable random IDs, dates, kinds, and initial statuses. Keep
the ID and filename stable when the title changes. Add a few useful tags without
using tags as authority or task state. Use ordinary relative Markdown links and
explain why connected records matter.

Record exact inputs and distinguish source analysis, deterministic fixtures,
offscreen GPU evidence, native presentation, and operator acceptance. Large raw
captures belong in retained artifacts; the note explains their identity and
meaning. Preserve disproved hypotheses instead of rewriting history.

Run `zk index`, inspect `zk list docs/notes --broken-links`, and run
`git diff --check` after changes. The SQLite index is local and rebuildable.
