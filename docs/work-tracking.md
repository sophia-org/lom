# Track work with todo.txt and zk

**Role:** repository work-tracking contract. Architecture and specifications
retain authority over product behavior.

`todo.md` uses the upstream todo.txt format: one task per line, with no Markdown
headings, checklists, or progress diary. Monthly `done-YYYY-MM.md` files use the
same format for completed tasks. `done.md` is a short history guide.

Linked `zk` notes carry scope, dependencies, evidence, investigations, and
decisions. The task files own status and order; notes must not copy the queue.

## Task format

```text
(A) Implement admitted content input. +critical @development id:t004 order:004 [details](docs/notes/plans/pf4er77j-lom-daily-driver-critical-path.md#t004)
```

- `(A)` is critical-path priority and `(B)` is admitted parallel work.
  Candidate and deferred work has no priority until promoted.
- Every open task has exactly one lane: `+critical`, `+parallel`, `+candidate`,
  or `+deferred`.
- Context is `@development`, `@physical`, or `@planning`.
- `id:tNNN` is stable and unique across open and completed task files.
- `order:NNN` is the reviewed execution order. Use the lowest critical order
  unless the operator selects another scope.
- The Markdown link names the note or evidence that owns the task details.

Priority, lane, context, identity, and order use fields permitted by todo.txt.
The todo.txt CLI preserves them but cannot decide whether an acceptance criterion
has been met. A later row does not authorize bypassing an earlier gate.

## Repository-local commands

Run these from the Lom checkout or a subdirectory:

```sh
zk queue
zk tasks ls +parallel
zk tasks ls +candidate
zk tasks ls id:t004
zk investigate --title "Describe the incident" --print-path --no-input
zk plan --title "Describe the scope and exit" --print-path --no-input
zk index
```

`zk tasks` invokes the unmodified upstream todo.txt CLI with `.todo/config` and
refreshes the notebook index after a successful command. Set `LOM_TODO_CLI` if
the CLI is installed somewhere other than `~/src/todo.txt-cli/todo.sh`.
Configuration derives all task paths from the current checkout; an isolated
checkout cannot accidentally mutate another queue.

For a new task, search first, create or reuse the right note, then add one short
line with a new stable ID and a resolvable link. The CLI dates newly added tasks.
Direct edits are allowed; run `zk index` afterward.

Before completion, update the linked note with the result, exact source and
artifact identity where relevant, validation, and remaining limits. Locate the
task by stable ID, re-read its current line number, then use `zk tasks do N`.
That command adds the completion date and moves the line to the current monthly
completion file. Never mark native acceptance complete from an offline fixture.

## Notes and evidence

Use an investigation for a coherent incident or unanswered question, an ADR for
an architectural choice, a plan for task criteria and ordering, and a milestone
for a completion or changed exit. Architecture and protocol contracts describe
current behavior; notes preserve reasoning and evidence without overriding them.

Keep exact source, binary, configuration, device, topology, and evidence identity
for promotion claims. GPU, protocol, installation, and physical results remain
separate. Retained software images and real-socket fixtures do not establish
operator-visible native presentation.

For tracking-only changes run:

```sh
zk index
zk list docs/notes --broken-links
git diff --check
```

Inspect every broken link. Task IDs must be unique across `todo.md` and all
`done-*.md` files, and every open task must have one lane, one order key, and a
resolvable detail link.
