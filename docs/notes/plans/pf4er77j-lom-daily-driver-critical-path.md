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
adapter through `VK_EXT_physical_device_drm` render major/minor identity. PCI
facts are diagnostics and cannot authorize or rescue an adapter. Production rendering
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

The next protected result on Sophia `bb97c561` and Lom `6e274bd` reported one
enumerated adapter and zero non-CPU Vulkan adapters. Sophia had mounted the
render node but no `/sys`; inspected RADV/libdrm discovery requires the render
minor's DRM and PCI sysfs identity and therefore discarded the physical device
before Lom's selector ran. The repair consumes Sophia's immutable one-device
sysfs projection, preserves the actual render-minor basename and selects only an
exact DRM `dev_t` match. No CPU or PCI fallback remains. This explains the
observed enumeration failure but does not establish later RADV initialization,
rendering or native presentation; those remain hardware gates.

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

Implemented in the t004 source tranche: Lom requests the discrete-input and
indicator-activation capabilities, derives non-overlapping physical targets and
retained TEA messages from the same Masonry layout as each frame, and runs a
nonblocking resource/candidate presentation ledger. An exact content action is
acknowledged before the retained message is reduced and its authorized
indicator action is emitted. Real-socket tests cover that chain. Native pointer
selection and observation remain t008 evidence.

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

The September 15 lifecycle checkpoint retains one Xilem/Masonry host per exact
grant/output/allocation/scale generation. Reconciliation keeps the widget tree;
clock updates dirty a panel only when its configured visible text changes.
A capacity-one GPU worker feeds independent output presentation state machines.
Resource release, permits and candidate outcomes no longer impose a global
wait. Two lazy resource slots per output remain subject to advertised aggregate
byte/resource/candidate limits; upload service admits at most four chunks per
turn. Presented installs the originating model and targets through the generic
ordered client lifecycle, independently of retiring the preceding resource.

The client pins published Sophia `2e569301` directly, with no local Cargo patch.
Device-hidden `tools/check.sh` passes 50 Rust tests, 16 tooling tests, dependency
and layout checks and strict Clippy. A compiled global-retirement-wait mutant
fails the two-output private-socket control. Retained-tree controls inspect
actual Masonry widget identities; rendering in the socket fixture is simulated.
These checks do not establish GPU/driver latency, compositor owner-loop/policy
causality, or attended acceptance. Source and logs are retained in Sophia's
`.artifacts/lom-lifecycle-dev`; the failed private-cache setup precedes tests and
is not counted as a pass. Reconnect/topology, complete action latency evidence
and the paired native workload remain open.

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

The attended `20260913T221559Z` run on Sophia `826cff1b` and Lom `4340aa0`
rendered the panel and workspace pills on output 1, while output 2 stayed empty.
Sophia published both outputs, then each fresh grant prepared and presented
candidate generation 1 on output 1 before Lom exited and reconnected. The client
had incorrectly restarted candidate generations for every panel; r5 requires a
single grant-wide increasing sequence because CandidateChunk and CandidateEnd
omit output. Output 2 therefore repeated generation 1 and was rejected stale.
The repair moves generation ownership to ShellService and pins two distinct
outputs to generations 1 then 2 through a real-socket lifecycle test. This is
deterministic multi-output recovery evidence, not native acceptance; a later
attended run must still show stable panels on both outputs without reconnects.

The attended `20260914T011705Z` run on Sophia `56fc17a2` and Lom `d09e100`
removed the native content-ownership fatal and presented both outputs for the
full 20-second window. It did not establish a stable shell: 21 GPU grants and 20
`ResourceBegin` stale rejections show that each first dirty update restarted
Lom. Resource slots were interleaved per output (`[1,2]`, then `[3,4]`), so first
use followed `1,3,2`; after admitting 3, Sophia correctly refused the previously
unseen 2 below the grant-wide resource-ID high-water. Lom now assigns all
primary slots `1..N` and all alternates `N+1..2N`, making first use `1,2,3,4`
before later exact generation reuse. The real-socket two-output lifecycle pins
that order and the complete project gate passes. A new attended run must still
show one stable grant, repeated updates on both outputs, and an accepted
presented-content action; this source repair does not supply those observations.

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


The causal-evidence follow-up adds exact connection/grant and presentation epoch
to the existing `lom_panel_candidate` record. Its indicator generation remains
the render job's captured revision, not the latest snapshot at logging time.
Sophia's new workload verifier joins that origin to host-owned action, policy
and native completion records; client output cannot replace host evidence.
This diagnostic remains evidence preparation, not native acceptance. Sophia
`82342681` now wires the operator-only `lom-test` launcher to a 90-second normal
exit with a 110-second failure watchdog. Its checked-in candidate budget declares
10 seconds of warmup, 40 state-changing workspace clicks (20 per output) within
60 seconds, validated ACK p95/max 50/100 ms and exact native retirement p95/max
150/300 ms. Those are predeclared workload gates, not recovered historical
numerical approval or driver guarantees.

The launcher hashes the copied budget, profiles and exact binaries before GPU
preflight, refuses changed inputs and existing evidence directories, and runs
strict native-health/recovery plus causal workload verification after normal
exit. Host samples bracket the workload on the existing bounded five-second
cadence. Protocol/storage checks bound four reusable resource slots, bytes by
twice the two exact panel sizes, warmed resource-ID growth, candidate/allocation
and queue ownership. The final same-grant record must confirm current renderer
workers joined and every actual content/transport credit is zero. These are not
RSS or GPU residency measurements; the retained-cache 1000-cycle controls and
broader daily-driver matrix retain their separate scopes.

This next attended test covers stable two-output workspace/clock interaction
and normal cleanup, not calendar/descriptor/topology acceptance or all of t008
and t009. No native run, install or task closure is inferred from the launcher
and verifier controls. The existing `lom-test` command is the foreground entry
on tty4 after ending the graphical session; the operator must still confirm
appearance, placement and pointer behavior and retain any failure unchanged.

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

## 2026-09-16 workspace ownership repair

The DP-2 incident is being repaired in Sophia's explicit WM action target and
Hagia's configured workspace ownership. Lom remains a generic consumer of
published output/indicator/label/action records. Numbering belongs to the WM
profile: DP-1 1–3, DP-2 4–6; Super+number selects the current owning output.
No monitor-number table or special DP-2 selection behavior belongs in Lom.

The next attended t008/t009 run must check both disjoint labels and the exact
output that changes, steady per-output indicators, the reported DP-2 flashing,
keyboard selection, workload latency and clean shutdown. Headless policy/socket
regressions cannot close those observations. The Sophia launcher builds and
records the exact clean signed Hagia source alongside Lom/Sophia identities.
No native acceptance, installation or release claim follows from this repair.

## 2026-09-16: bar continuity and interaction-only updates

Operator evidence from Sophia capture `20260916T114128Z` confirms correct
per-output labels and mouse/keyboard switching. Both bars flashed on switching.
Sophia found a policy-cycle composition path omitting shell pixels, then
restoring them when Lom republished. That generic composition repair is separate
from this client's unnecessary rendering; reducing redraws alone could prolong
the missing bar.

Lom now separates visual invalidation from interaction revision changes. The
comparison uses the same filtered labels and colors as the view, along with
widget shape, styles, allocation-local model and formatted clocks. Unchanged
pixels reuse the resident resource in a new candidate with refreshed targets;
no GPU work/upload/retire occurs for that candidate. Geometry is reused only
when the resolved view agrees. Presented, not local rebinding or Prepared,
installs the new model/targets. In-flight render origins remain immutable and
new observations coalesce independently per output.

The private socket control verifies three renders/uploads for two initial bars
and one changed bar, followed by interaction-only candidates on both outputs.
An old resource release stays withheld while both outputs' exact actions
complete. An action before replacement Presented still uses the old targets.
CPU-raster controls compare actual Xilem/Masonry pixels, target geometry and
messages across revision-only updates and foreign-output changes. This evidence
uses supplied renderer/native outcomes at the socket boundary, not a GPU/VT run.
Candidate diagnostics distinguish `raster_source=rendered` from `reused`.

Native continuity, the full 40-action workload, latency and daily-driver
acceptance remain pending under t008/t009; no task is closed by offline checks.
