---
id: 0wpbr93d
date: 2026-09-27
kind: plan
tags: [plan, milestone]
---
# Move Lom to the standalone desktop SDK and 9P shell transport

## Scope and exit

The operator selected this work while using the existing desktop. Move Lom's
production shell connection to the standalone Rust desktop SDK, using only
9P with no IPC dependency or fallback. Preserve rendering, protocol revision 6,
capabilities 0x783, and presentation-based action authority.

The SDK owns file transport, object fetching, submission custody and retry.
Lom observes every admitted unit's outcome within the SDK's bounded history;
submission acceptance does not imply presentation. Refused or ambiguous work
ends the service without replay.

Exit requires a signed SDK pin, the full offline Lom gate, file-wire lifecycle
and refusal tests, retained lifecycle assertions moved to 9P, and updated
external E2 consumers. Live promotion is separate: a new packaged profile
selects the 9P bar, then the
operator verifies presentation and actions. No current session is changed by
development gates.

## Task details

## t021

Use the public standalone protocol/client crates at signed revision
`0da10428ad2ef85ff9f1c35c11238fbfebe81d04`. Preserve the canonical git URL in
the lockfile. Provisioning may use an explicit local source override scoped to
the command; gates use private targets, offline dependencies, nice 19 and two
jobs with display variables unset and devices hidden.

The `--serve` entry requires `SOPHIA_SHELL_9P_SOCKET` and refuses the retired
`SOPHIA_SHELL_SOCKET` environment. It reports the chosen transport before GPU admission.
The existing `content-proof --socket` command uses the same 9P client.
Scripted file tests cover Lom and the real SDK over a Unix socket; they do not
claim Session policy, hardware rendering or physical presentation.

## Implementation and validation, 2026-09-27

The production and diagnostic paths use the pinned SDK's file connection with
no IPC feature or server crate. The dependency gate rejects those dependencies.
The service bounds outstanding ticket history, observes refusal and unknown
custody, and uses the SDK retry deadline for idle waits. Presentation and input
authority still come from lifecycle outcomes.

The service fixtures now speak 9P, fetch multi-read objects, exercise partial
slot writes, and require acknowledgement of Submitted before reopening the
transaction. They retain the allocation, presentation, retirement, workspace
action and interaction-refresh assertions. New executable controls cover the
9P diagnostic exchange and the transport record before GPU-grant validation.
The retired socket variable is refused even when empty or supplied alongside
the file endpoint.

Provisioning used a private copied Cargo cache and a command-scoped local
override for the canonical SDK URL. It downloaded zero crates; the registry
index was unchanged. Gates ran offline and locked, at nice 19 with two jobs,
inside a device-hidden, network-disabled sandbox with display variables absent.

The full gate passed: 59 Rust tests, 17 tooling tests, documentation tests,
formatting, dependency and layout checks, and Clippy with warnings denied.
Ignoring custody-observation errors made the refusal test fail its specific
errno assertion; the source was restored and the full gate passed again.
The earlier compile, fixture-teardown and test-lint failures remain in the logs.

Evidence: `~/.local/state/sophia/development-evidence/lom-9p-only/`, including
`provision-accounting.txt`, `full-final.log` and `custody-mutant.log`.

Task t021 remains open for the external E2 binding and desktop promotion. The
current desktop has not been modified; these tests make no GPU execution or
native presentation claim.

## Supervised stop (2026-09-27)

`--serve` now handles SIGTERM and SIGINT, including as sandbox PID 1. The
signal handler sets a flag; ordinary code stops service, drops the connection
and renderer, and reports `lom_shell_shutdown schema=1 reason=signal`. It
does not replay submissions or wait for server-owned retirement. The blocking
SDK handshake retains its five-second bound and GPU startup its two-second
bound. A worker still in a driver operation is reclaimed at process exit;
shutdown is not evidence of GPU completion.

The device-hidden offline gate passed 63 Rust tests, 17 tooling tests, doc
tests, layout/dependency checks, formatting and strict Clippy. Four signal
tests run the production serve owner with a GPU-free renderer as PID 1: a
stalled handshake, initial content facts, and both signals during rendering.
Removing cancellation from the initial-content wait makes the two-second
latency assertion fail; the original source was restored and retested.
Evidence is in `development-evidence/component-sigterm/lom-*.log`. Earlier
test-sandbox setup failures are retained. No live component was replaced.

## Connections

- [Architecture](../../../ARCHITECTURE.md)
- [Daily-driver critical path](pf4er77j-lom-daily-driver-critical-path.md)
