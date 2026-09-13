---
id: 1qikt1av
date: 2026-09-13
kind: adr
status: accepted
tags: [adr, gpu, shell, portability]
---
# Use explicit GPU permission with renderer-neutral Sophia presentation

## Context

Lom preserves ironbar-inspired appearance, modules and configuration concepts
through KDL and a data-oriented TEA application. Xilem owns reactive view
reconciliation, Masonry owns widgets/layout, Parley/Fontique owns text, and Vello
through wgpu renders our pixels. None of those choices should become a mandatory
dependency or drawing API for Sophia or another shell developer.

The former Sophia GPU-domain design required a hard GPU-memory quota and a
custom-kernel route on the inspected machine. The operator requires ordinary
Linux distribution support, GPU rendering and no Waybridge. The accepted
[Sophia successor](https://github.com/sophia-org/sophia-stack/blob/master/docs/notes/decisions/mn4mzcnf-separate-shell-presentation-from-gpu-execution-permission.md)
is the authority for the replacement execution contract; this record adopts
that choice on the client side rather than duplicating a server specification.

## Decision

Keep Lom's current UI stack and non-Winit Sophia driver. Obtain GPU access only
through an explicit default-denied startup grant, separate from content and
input negotiation. Select and verify the actual admitted GPU; no ambient
DISPLAY, Wayland socket, host service bus, arbitrary render-node enumeration or
software fallback may substitute for a missing grant. Ordinary upstream Linux
device/driver interfaces are the target; no custom kernel or particular GPU
memory controller is required.

Direct permission accepts GPU-driver and availability risk. It is not a hard
aggregate VRAM quota or a guarantee against desktop-wide device loss. Lom must
bound its known allocations, outstanding work, queues and waits without
labelling those application limits as control over every driver allocation.
The operator may keep direct access denied when that trust level is unsuitable.

The first presentation path stays GPU render → bounded readback → immutable CPU
resources → complete content/target candidate → Engine native presentation.
Use negotiated formats, exact acknowledged allocation dimensions, pacing,
outcomes and final release. A local GPU completion never activates a target.
No protocol revision, new shader language or buffer-handle ABI follows from
this adoption. Another Sophia shell can render with a different library or CPU
renderer under the same presentation contract.

Keep the model data-oriented and deterministic. The protocol owner handles
bounded observations, actions and revocations while one renderer worker runs
GPU work; it must not wait synchronously for rendering to finish. Exchange
owned messages and generational identities, not shared mutable toolkit state.
Coalesce dirty work per output, reuse unchanged resources and schedule from
Engine permits. Preserve the existing one-job limit and 2000 ms readback
recovery deadline until measured evidence supports another choice. A deadline
starts recovery, not unsafe reuse or proof that a stuck kernel call returned.

One admitted connection owns content, indicators and supported descriptor
workflows. Lom must service the existing launcher/switcher exchange if it
replaces Narthex in the desktop. Narthex remains the independent reference and
rollback client in a separate session; it cannot occupy a second native slot.
The Minimal milestone does not grant a custom raster launcher or application
execution authority merely because Lom draws a panel.

Measure readback, copies, IPC upload, Engine upload/composition, actual native
retirement, action latency, idle wakeups, retained memory and queue depth. Set
workload-specific acceptance budgets before the run and retain source/device
identity and percentile results. Existing resource/placement reuse comes before
new delta vocabulary. DMA-BUF or sealed-memory transport requires measured need
and a separately admitted immutable ownership/synchronization contract.

An optional GPU bridge is a candidate for a named isolation or performance
problem. It is neither a Lom prerequisite nor a mandatory Vello server inside
Sophia. A helper running opaque jobs has not thereby acquired an enforceable
GPU-memory budget. Its trust, allocation owner, failure boundary and actual
performance require their own decision and evidence.

## Alternatives

- Retaining mandatory kernel GPU quotas conflicts with the generic-Linux target.
  Optional verified enforcement can be a distinct future execution mode.
- A mandatory bridge adds an execution protocol before the panel can ship;
  no measured requirement currently promotes it onto the critical path.
- GTK/Waybridge changes the rejected display-authority choice. Lom keeps its
  current toolkit adapter and does not create a private display.
- CPU raster remains useful for deterministic fixtures and other shell clients;
  it does not silently replace Lom's selected production GPU path.

## Consequences

The former Lom t003 hard-quota task becomes the client side of explicit GPU
admission and responsive scheduling. Sophia owns launch permission and
compositor resource enforcement. The
[paired critical path](../plans/pf4er77j-lom-daily-driver-critical-path.md)
names each side's task and acceptance exit. GPU bridge and image-transport work
are candidates; neither delays the first working desktop.

At this decision the service still has a synchronous renderer and Sophia's
production admission remains closed. This document does not enable a grant,
change an installed profile, establish native acceptance or validate old
artifacts against the new policy. The supported KDL panel/theme configuration
and completed-task history are unchanged.

## Acceptance and connections

Accepted on 2026-09-13 by the operator's instruction to update both architecture
documents and synchronize the critical path around this choice. This authorizes
the design and plan; installation and hardware/native acceptance remain separate.

[Architecture](../../../ARCHITECTURE.md) carries the client constraints;
[Sophia's ADR](https://github.com/sophia-org/sophia-stack/blob/master/docs/notes/decisions/mn4mzcnf-separate-shell-presentation-from-gpu-execution-permission.md)
owns server policy and explicitly supersedes its earlier hard-quota decision.
[Minimal evidence](../../minimal-port.md) preserves the existing results and
their limits. No claim of source compatibility with ironbar or upstream GTK is
introduced by this decision.
