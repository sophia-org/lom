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
the ironbar-inspired Minimal panel on every admitted output. It shows workspace
state and active output, clock, and an anchored calendar popout. Workspace pills
activate the exact presented action. The panel survives output and session
lifecycle changes, coexists with Narthex, stays within advertised resource
budgets, and has a documented fallback.

This milestone preserves Sophia's architecture: no ambient X11 or Wayland
display, no Waybridge, no GTK dependency, no general pointer stream, and no
implicit render-node grant. Sophia owns placement, presented-target selection,
input authority, composition, and revocation. Lom owns its TEA model, Xilem and
Masonry views, Vello rendering, and the client side of admitted shell protocols.

Daily-driver status requires deterministic checks plus attended native evidence
from the exact installed release. Software snapshots, offscreen GPU output,
real-socket protocol fixtures, `Prepared`, and a locally completed render do not
substitute for native `Presented` retirement and operator-visible behavior.

The minimum milestone deliberately excludes focused title, battery and system
information, tray integration, broad ironbar compatibility, direct GPU-buffer
handoff, and content deltas. Those are parallel or candidate work and cannot
delay the first honest panel.

## Ordering and dependencies

GPU-domain admission and discrete input can be implemented independently, but
both must land before native acceptance. Popout work depends on discrete action
delivery. Lifecycle resilience follows the functioning panel path. Packaging
must retain a known fallback before the attended matrix. Soak and promotion use
one exact installed candidate and cannot be inferred from earlier artifacts.

## Task details

### t003

Add enforceable aggregate GPU-memory admission for the confined Lom protection
domain. The operator policy defaults denied; effective-profile evidence names
the granted device and budgets. Admission accounts for staging, resident,
renderer overlap, and retiring storage without treating a render-node bind,
one-job scheduling, or per-resource limits as aggregate enforcement. GPU waits
remain bounded. Deterministic tests prove denial, exhaustion, release, epoch
replacement, and disconnect while renderer references remain. The content grant
stays unavailable on kernels where the selected mechanism cannot enforce it.

Exit: Sophia can admit a protected Lom domain under explicit policy with a
closed resource ledger, or this task records a separately accepted replacement
design. No native presentation claim is required here.

### t004

Implement the first `content_discrete_input` workflow for a workspace pill.
The complete content candidate binds each target to exact presented semantic
meaning. Sophia dispatches from the applicable presented snapshot after clipping
and precedence; Lom never receives a general pointer stream. Unknown, stale,
duplicate, revoked, disconnected, and old-epoch actions settle visibly and
cannot activate replacement meaning.

Exit: protocol and reducer tests establish one successful activation plus every
refusal and teardown boundary, with bounded queues and explicit acknowledgements.

### t005

Implement the calendar popout as a separate allocation and complete content
candidate. Anchor it with acknowledged parent-allocation physical geometry and
the exact parent presentation epoch. Engine placement and coverage rules apply;
the client does not reconstruct fractional coordinates. Outside dismissal is
consumed by Engine without click-through or coordinate disclosure. Parent loss,
output loss, supersession, and revocation withdraw dependent work safely.

Exit: deterministic lifecycle tests cover open, replacement, dismissal,
rejection, parent loss, resource retirement, and exact action identity.

### t006

Replace the current restart-only topology response with bounded, tested recovery
for output addition/removal, scale or allocation-generation change, shell
reconnect, fresh grant epochs, configuration replacement, and normal supervisor
restart. Accepted obligations receive terminal outcomes; old resources and
actions remain invalid until their independent lifetimes settle. Recovery cannot
silently reuse identities or grow retained queues.

Exit: real-socket and reducer tests exercise each transition and demonstrate
fresh allocations and presentations without stale actions or resource reuse.

### t007

Produce an exact Lom artifact and a Sophia desktop-profile selection that starts
it as the protected content shell while retaining Narthex for its admitted
launcher and switcher role. Record source, dependency lock, binary hash, profile,
Sophia/Hagia/Narthex identities, output topology, and rollback command. Refuse a
profile whose host cannot enforce the grant established by t003.

Exit: an offline install verifier accepts the exact package and rejects identity,
policy, and configuration mutations. Installation remains a separate authorized
operator action.

### t008

Run an attended native matrix on the exact installed candidate and both outputs.
Verify panel pixels, top reservation, scale and placement, active-output styling
including an empty output, workspace activation, clock update, calendar open and
dismiss, correct target routing, no click-through, Narthex coexistence, clean
stop/relaunch, and recovery after an output or session transition. Require actual
native presentation and retirement evidence for the candidate under observation.

Exit: retained evidence ties every result to the installed binary and profile;
failures remain open rather than being converted into fixture success.

### t009

Use the accepted candidate for a daily workload. Exercise idle, VT suspend and
resume, normal logout/login, repeated popouts and workspace actions, config and
shell restart, and both outputs. Measure warmed memory, retained resource counts,
frame and action latency, and recovery. There must be no steady allocation
growth, unresolved accepted obligations, shell/session loss, or unbounded wait.

Exit: a retained soak record states duration, workload, exact identities,
measurements, incidents, and any rerun triggered by fixes.

### t010

Promote Lom to daily-driver status only after t003 through t009 meet their exits.
Update README and architecture statements to the observed behavior, record the
accepted configuration and rollback, and create a `zk milestone` linked from the
milestone index. Preserve remaining parallel and candidate work without implying
feature parity with ironbar.

Exit: product claims, task ledger, milestone record, installed identity, and
retained acceptance evidence agree.

### t011 through t015

These parallel tasks add authorized focused-title, battery/system, tray, atomic
KDL reload, and wider Minimal appearance/configuration parity. Each provider needs
its own authority and bounded lifecycle where applicable. They may proceed when
they do not change the daily-driver critical-path contract or delay its evidence.

### t016 through t018

Direct GPU-buffer handoff and tile/delta content remain measurement-triggered
candidates. Broad ironbar module and configuration compatibility is deferred.
Promoting any of them requires a named problem, accepted design, budgets, and a
measurable exit; perceived elegance or nominal bandwidth is not enough.

## Connections

[Architecture](../../../ARCHITECTURE.md) defines Lom's ownership and rendering
model. [Minimal port evidence](../../minimal-port.md) records the current local
UI, GPU-readback prototype, and persistent content lifecycle. [Configuration](../../configuration.md)
defines the supported KDL surface. Sophia's content-shell ADR and GPU-domain ADR
remain the cross-repository authority for server-side admission and wire behavior.
