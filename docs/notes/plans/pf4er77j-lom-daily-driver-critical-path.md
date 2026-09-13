---
id: pf4er77j
date: 2026-09-13
kind: plan
status: active
tags: [plan, daily-driver, shell]
---
# Lom daily-driver critical path

## Scope and exit

The first daily-driver milestone is one protected Lom shell process presenting
the ironbar-inspired Minimal panel on every admitted output: workspace state,
active output, clock and an anchored calendar popout. Workspace pills invoke
the exact presented action. Preserve launcher/switcher access on the same
admitted connection and retain Narthex as a separate-session rollback client.
The earlier wording promising simultaneous Narthex coexistence was incorrect:
Sophia has one native shell slot.

The [accepted GPU decision](../decisions/1qikt1av-use-explicit-gpu-permission-with-renderer-neutral-sophia-presentation.md)
keeps GPU rendering under explicit startup permission, without requiring a
custom kernel, hard GPU quota, Waybridge, GTK, private display or Vello inside
Sophia. Direct GPU access accepts driver/resource-availability risk. It grants
no foreign pixels or general pointer stream. Engine owns placement,
composition, target selection and revocation; Lom owns its TEA model,
Xilem/Masonry views and Vello renderer. KDL preserves supported ironbar concepts
without promising drop-in CSS or complete feature compatibility.

Daily-driver status requires deterministic checks and attended native evidence
from the exact installed release. Software snapshots, offscreen renders,
private-socket fixtures and Prepared are not native retirement or visible UI.
Retain historical evidence with its original identities; none validates the new
launch policy automatically. This plan does not authorize install or live tests.

Focused title, battery/system, tray, hot reload and wider ironbar parity remain
parallel work. New image transport, content deltas and a GPU bridge remain
candidates. The first useful panel must not wait for those architectures.

## Ordering and paired ownership

Sophia owns the [server critical path](https://github.com/sophia-org/sophia-stack/blob/master/docs/notes/plans/1m3z9q0j-lom-and-sophia-portable-gpu-shell-critical-path.md).
Task IDs are repository-local: `depends:` names local tasks and
`peer:sophia/tNNN` names paired work. A peer's completion does not close this
repository's task. Both sides may cite one exact integration evidence record
when it proves their distinct exits.

| Step | Lom task | Sophia task | Shared exit |
| --- | --- | --- | --- |
| 1 | t003: admitted device and asynchronous renderer | t097: direct launch grant and backing charges | A protected stock-Linux GPU render can prove or refuse the exact permission |
| 2 | t004: candidate-bound actions and acknowledgement | t098: presented target dispatch | One exact presented workspace action; stale and unpresented cases refused |
| 3 | t005: calendar popout | t099: placement, composition and dismissal | Coherent panel/popout lifecycle and consumed outside dismissal |
| 4 | t006: per-output and reconnect recovery | t100: topology, epoch and retirement owners | Fresh state and drained obligations through loss/replacement |
| 5 | t020: descriptor workflows; t007: package | t101: shared workflow support and verifier | One shell retains desktop controls and an exact rollback |
| 6 | t008: attended matrix; t009: soak; t010: promotion | t081: native-shell acceptance | Both ledgers and product claims match the exact installed evidence |

Steps 1 and 2 can be developed independently under contained tests. Popouts
need input. Recovery tests and descriptor integration can begin early but must
validate the combined path before packaging. Package identity includes both
accepted repository tips; keep Lom's Sophia dependency pin explicit. Coordinate
owners before shared library/transport edits and integrate clean, checked tips.
Do not import unfinished input work merely to obtain a documentation change.

## Task details

### t003

Consume the new explicit launch permission from Sophia t097; verify the actual
Vulkan/wgpu adapter against the admitted kernel device identity. Refuse missing,
ambiguous, unsupported, old-epoch or mismatched devices. Do not discover an
ambient display or select software rendering when permission is denied.
Sophia implements policy/device exposure; Lom does not provision cgroups or
claim aggregate driver-memory accounting.

Separate the synchronous GPU renderer from the protocol owner with bounded
messages and one outstanding job. Actions, revocations, permit expiry and
disconnect remain serviceable while render/readback is delayed. Keep the
existing 2000 ms readback recovery deadline, exact resource dimensions and
known CPU/readback allocation checks. Timeout retains unsafe-to-reuse work and
prevents new jobs; a worker/watchdog is not proof of cancellation of a kernel
operation. Retain the no-silent-CPU-fallback behavior.

Exit: deterministic adapter/queue/recovery tests pass, including delayed GPU
completion with ongoing control progress. A separately authorized isolated GPU
run proves the actual selected device inside the launch domain on a stock
kernel, records its capabilities/driver and image result, and verifies the
changed readback path. Mocked selection and the old preview cannot close that
hardware part; no ordinary Sophia desktop startup is needed for it.

Implementation checkpoint: Lom validates the connection-scoped grant, private
render-node character-device identity, and an exact unique non-CPU Vulkan
adapter, including PCI identity when Sophia publishes one. Production rendering
runs on a capacity-one worker with a 2000 ms recovery deadline while the service
continues protocol observation. Deterministic tests cover denied and stale
grants, ambiguous and CPU adapters, one-job ownership, quarantine after expiry,
and an indicator update arriving while rendering is outstanding. This does not
close t003: the paired contained GPU/device-exclusion proof and native retirement
evidence remain separately authorized and unrun.

The next candidate records the admitted Vulkan adapter and complete private
`/dev/dri` inventory, emits checksums for content the Engine reports Presented,
and provides a tracked Minimal live configuration with workspace pills and a
seconds clock. Sophia's paired isolated runner and tty4 gate are materialized,
but neither is hardware evidence until it is actually run and retained. t003
therefore remains active.

The first protected hardware preflight on Sophia `61c37741` and Lom `1225b12`
stopped before display takeover because no enumerated Vulkan adapter passed the
grant selector. The aggregate client error does not reveal whether enumeration
was empty or whether wgpu omitted or disagreed on the optional PCI-bus string.
The successor candidate validates Sophia's PCI vendor/device fallback within
the single-render-node domain and reports bounded adapter counts on failure.
This is a portable selection repair with deterministic coverage, not proof that
it caused the observed refusal; t003 still requires a retained hardware result.

### t004

Export a target table with each complete view candidate. Retain the exact
mapping from candidate/interaction/target identity to TEA meaning and the
authorized indicator action token. Resolve an activation against the presented
binding, not the latest widget position or module index. Do not widen metadata,
application actions or pointer disclosure to make toolkit input convenient.

Use Sophia t098's action/acknowledgement path with bounded deduplication and
terminal outcomes. Unknown, stale, duplicate, revoked, wrong-output and
old-epoch events cannot activate replacement meaning. Prepared does not make a
target active. Cancellation and timeout must settle without keeping a widget
pressed forever, and driver work must not block the response.

Exit: reducer, model/trace mappings where required, and real private-socket
tests prove one valid activation plus all refusal and teardown cases. Keep
backend-queued, socket-routed and client-observed outcomes distinct. Native
physical targeting remains t008.

### t005

Implement the calendar as a separate allocation and part of a complete coherent
panel/popout candidate using Sophia t099. Bind a parent-allocation-local
physical anchor to the acknowledged allocation and parent presentation epoch;
never reconstruct a logical anchor from rounded widget coordinates. Honor the
Engine's returned physical extents, scale, coverage and no-reservation rules.

Route calendar actions through exact candidate bindings. Engine consumes
outside dismissal without exposing the outside location or replaying a click
into an application. Parent loss, output loss, replacement and revocation
withdraw dependent intent and settle resources without confusing cancellation
with final GPU or Engine release.

Exit: deterministic tests cover open, navigation/action, replacement,
dismissal/timeout, placement rejection, parent/epoch loss and retirement,
including fractional scaling. No local popup intent is recorded as visibility.

### t006

Maintain per-output state, dirty work and generations through output addition,
removal, scale/allocation changes and content-grant replacement. Replace the
current exit-on-facts-change path with explicit bounded recovery; if the
transport/domain must restart, invalidate old work and reconstruct complete
state at the fresh epoch. Pair with Sophia t100.

Use Engine permits to schedule dirty outputs fairly and reuse unchanged
resources; a blocked render/output must not starve control traffic. Keep one
protocol owner and bounded queues. Supervisor/configuration replacement must
discard stale effects and bindings. This is restart recovery, not the optional
atomic hot-reload feature in t014.

Exit: reducer/private-socket tests exercise topology, every disconnect phase,
revocation, retained old storage and restart. Show fresh allocations and
presentations, exact terminal outcomes, no stale activation/reuse, and bounded
metadata/backlog. Add measurable idle wakeup and dirty-output scheduling checks.

### t020

Preserve the descriptor switcher/launcher and any other capabilities the client
negotiates within Lom's single native connection. Use existing sanitized
descriptors, complete candidates, Engine rendering/input and issuer-scoped
actions; this task does not authorize a custom raster application launcher.
Coordinate bounded demultiplexing APIs with Sophia t101 rather than leaving
unconsumed descriptor records to fill a queue. Keep their controller state out
of the GPU renderer.

Exit: real protocol fixtures show launcher/switcher operation while panels
update, including unknown/old records, acknowledgements, reconnect and content
failure. Required ordinary desktop controls remain accessible with one shell
slot. Narthex is an independently verified fallback, not a simultaneous client.

### t007

Package the paired, accepted Sophia/Lom commits after t003 through t006 and
t020. Supply protected panel/theme KDL and a desktop-profile selection with the
new explicit GPU permission, content/input grant and panel allowance. Record
dependency lock, hashes, policy-client identity, driver/device prerequisites
and the effective profile. Keep the known Narthex artifact/profile for rollback
under the existing supervisor; no second shell slot is assumed.

Exit: offline package verification with Sophia t101 accepts the exact inputs and
refuses old quota syntax, missing resources, changed identities and inadequate
permissions. Document the operator-authorized install, foreground acceptance,
stop and rollback procedure. Packaging does not install or accept the candidate.

### t008

Run the attended matrix on the exact installed package and every admitted
output (both outputs on the current acceptance machine). Verify actual pixels,
reservation, scale/placement, an empty focused output, workspace activation,
clock update, calendar open/action/dismissal, no click-through, descriptor
launcher/switcher, stop/relaunch and topology/reconnect recovery. Observe
rollback to Narthex in a separate session. Pair results with Sophia t081.

Exit: retained evidence binds each action to the correct candidate's actual
native retirement, source/binary/profile/grant/device identities and observed
behavior. A fixture-reported Presented is insufficient. Failed or unexecuted
cases remain open and cannot be replaced by older evidence.

### t009

Define a daily workload and its refresh-relative latency/resource acceptance
budgets before testing. Exercise idle, repeated workspace/popout/descriptor
actions, output transitions, VT suspend/resume, config/shell restart and normal
logout/login. Retain duration and exact identities. Measure render/readback,
IPC copies/bytes, Engine upload/composition and native retirement separately;
report action/frame p50/p95/p99, idle wakeups, queue depth, warmed storage and
retirement counts. Averages or local render time alone cannot prove responsiveness.

Exit: the declared workload meets its budgets, old generations drain, there is
no sustained storage growth or lost accepted obligation, and failures/recovery
have evidence. Device incidents remain visible; direct GPU mode promises no
hard driver VRAM isolation. Fixes require a new exact-candidate acceptance record.

### t010

Promote only after t003 through t009 and t020 meet their exits and Sophia t081
agrees on the shared native evidence. Update README/architecture product claims,
record accepted configuration and rollback, and create a zk milestone linked
from the milestone index. Preserve parallel/candidate work without implying
feature parity with ironbar or universal driver compatibility.

Exit: installed identity, product claims, milestone and both task ledgers agree.

### t011 through t015

Authorized focused-title, battery/system, tray, atomic KDL reload and wider
Minimal appearance/configuration parity remain parallel scopes. Each provider
needs its own authority and bounded lifecycle. They must not expand the
critical-path contract or turn fixture values into purported live data.

### t016 through t018

Pair t016 with Sophia t102: measure the CPU-byte path before admitting shared
memory or DMA-BUF. A new transport must prove immutable acceptance, finite
overlap accounting, synchronization, cross-device behavior and final release.
No zero-copy claim follows from an imported handle.

t017 first measures reuse of the existing immutable resource/placement tables
for static panels and small clock updates. Tiles already exist in the content
contract; Lom's current full-panel submission does not exploit them. A new delta
contract, if needed, must name its base and pass a separate design gate.
t018 broad ironbar parity remains deferred to a user-prioritized compatibility
list after daily-driver acceptance.

### t019

Pair with Sophia t103. A GPU bridge is a candidate only when a named isolation
or measured performance need justifies mediation. Evaluate trusted renderer
adapters, bounded job identity/queues, enforceable allocation ownership,
failure/restart behavior and a second independent implementation. A helper
executing opaque jobs is not a hard GPU quota. No new kernel, Vello requirement
in Sophia, private display or mandatory shell API is implied by this task.

## Connections

[Architecture](../../../ARCHITECTURE.md), the
[adoption ADR](../decisions/1qikt1av-use-explicit-gpu-permission-with-renderer-neutral-sophia-presentation.md)
and [configuration](../../configuration.md) own client constraints.
[Minimal evidence](../../minimal-port.md) preserves existing results.
[Sophia's paired plan](https://github.com/sophia-org/sophia-stack/blob/master/docs/notes/plans/1m3z9q0j-lom-and-sophia-portable-gpu-shell-critical-path.md)
owns server exits; [its decision](https://github.com/sophia-org/sophia-stack/blob/master/docs/notes/decisions/mn4mzcnf-separate-shell-presentation-from-gpu-execution-permission.md)
owns GPU/presentation policy. [Active tasks](../../../todo.md) own local status.
