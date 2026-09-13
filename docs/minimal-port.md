# Minimal port: implementation and evidence

This tranche ports presentation concepts, configuration semantics and local
module behavior from ironbar's bundled Minimal preset. It establishes a real
Xilem/Masonry view path and a persistent Sophia content lifecycle, but production
GPU admission, semantic input and native acceptance remain open.

## Pinned sources

- Ironbar product reference: `e2910c7fded664dd1f4560217a92ba2030051746`,
  `examples/minimal/`, bar configuration, workspace and clock module behavior.
- Xilem workspace: `b81d8d7a631849def6eeab282561439b963862e5`.
  `xilem_masonry` owns view reconciliation. `MasonryRoot`/`RenderRoot` provide
  windowless construction and rebuild. `masonry_imaging::vello` bridges retained
  scenes to the compatible Vello 0.8 / wgpu 28 stack. Cargo.lock records all
  resolved dependencies. No GTK, Winit runner or private display server is used.
- Explicit DejaVu Sans Mono regular and bold assets: hashes and license in
  `THIRD_PARTY.md`. Regular previews use 13 logical pixels; ironbar Minimal
  itself specifies a monospace family but no universal pixel size.

## Tested boundaries

The standard `tools/check.sh` gate remains display-free and GPU-free. It checks
KDL errors and availability, deterministic reducer replay, stale module owners,
publication ordering, disconnect invalidation, calendar behavior, actual widget
click-to-message routing, existing-tree rebuild/teardown, image snapshots,
CLI refusals, the normal dependency graph and source length. Test bodies stay
outside `src/`. An actual manifest mutation promoting the software renderer into
normal dependencies was rejected by the dependency audit and then restored.

`tests/snapshots/` contains reviewed **CPU test images of production scenes**:
normal/narrow/fractional panels, an empty focused output and calendar variants.
This makes visual changes inspectable without opening a GPU or display. The
software renderer is a dev dependency only; the dependency gate rejects it in
the normal runtime graph. Snapshots use exact pixel comparisons with pinned fonts
and dependencies. To deliberately regenerate after reviewing a visual change:

```sh
LOM_UPDATE_SNAPSHOTS=1 cargo test --locked --test ui
```

The normal gate unsets this variable and cannot bless snapshots. Failure writes
actual images under `.artifacts/snapshots/` for inspection. These tests exercise
widget simulation and local scenes, not Sophia target routing or click-through.

## GPU diagnostic and the readback prototype

`lom preview` takes explicit configuration, theme and fixture inputs, validates
them before device creation and renders sequential images to a new directory.
Only Vulkan is requested, with no compatible surface. A reported CPU adapter is
refused. Masonry's Vello renderer draws and reads back the scenes; PNG output is
a diagnostic cost, not native presentation. The operator subsequently ran the
preview successfully on the RADV RAPHAEL_MENDOCINO integrated GPU, producing a
1280x24 panel and calendar in `/tmp/lom-minimal-preview`. That run establishes
offscreen GPU rendering, not presentation or a complete performance acceptance.
It predates the bounded readback change below; no GPU rerun of that change has
been performed. The command remains compiled by the standard offline gate.

The readback tranche adds six Rust tests: straight-alpha RGBA to
premultiplied BGRA conversion, canonical whole-row chunks, joint size limits,
and the production map-callback deadline. Conversion consumes the readback
vector in place and exports immutable bytes. Each content resource is at most
4 MiB, independently of the larger diagnostic preview limit.

The renderer now uses Vello's texture path with a caller-owned readback. A
two-second prototype deadline covers GPU polling and callback delivery. Failure
retains the submitted job and prevents a second job; it never treats timeout as
GPU completion. Callback tests are GPU-free and do not prove driver behavior.
Vello's internal GPU allocation budget is still unresolved. A protected
display-independent conformance client now negotiates content, uploads those
canonical bytes and submits one complete panel candidate under an Engine-issued
permit. Its host verifies the resource and candidate tables, then deliberately
reports renderer failure because it owns no native output. No native permission,
presentation or input path follows from that proof.

Local preview limits are 8192×4096, at most 8M pixels per image and scale 0.5–4;
there is one synchronous render/readback at a time. The widget-message queue is
bounded to 64 and surfaces saturation explicitly. These do not establish the
separate negotiated GPU memory/fence/retirement budgets required for a shell.

## Persistent content service

`lom --serve` is the protected production entry point. Its single KDL input
contains the panel and theme. It negotiates shell revision 6 with descriptor,
content and view-indicator capabilities, receives limits and output facts, and
requests one panel allocation per output. Each acknowledged allocation drives
the Xilem/Masonry scene at the Engine's exact physical dimensions and scale.
Vello readback becomes a new immutable content resource only after rendering
completes.

The client waits for transfer admission before sending whole-row chunks, demands
a frame, consumes its one-use permit, and submits one complete candidate. It
waits for a nonzero native `Presented` epoch; `Prepared` alone is insufficient.
Two resource slots per output allow the successor to present before the previous
resource is retired, and a slot generation advances only after `Released`.
Observations and retained response records are bounded to 64 per turn/queue.
Changed output facts terminate the process so its supervisor can reconnect under
a fresh epoch and obtain new allocations.

A real Unix-socket integration test drives two candidate generations. It sends a
revision-6 indicator change while the first candidate lifecycle is waiting and
proves the second render observes it. The fixture verifies allocation, admitted
resource transfer, permit pacing, `Prepared` then `Presented`, distinct resource
identity, and retirement/release only after the successor presents. It does not
prove X socket routing, a GPU driver, Engine scanout or operator-visible pixels.

Sophia's production content grant is still fail-closed. The current kernel has
no enforceable cgroup GPU-memory controller, so binding a render node or relying
on one-job concurrency is not accepted as aggregate GPU residency enforcement.
No native Lom session has been attempted through this path.

## Remaining acceptance

- Establish enforceable aggregate GPU-memory admission for Lom's confined domain;
  keep production content denied until it exists.
- Extend the observed GPU diagnostic with source/binary/device identity, timing
  and memory evidence, and test the changed bounded-readback path explicitly.
- Implement authorized live sources and scheduling. Focused title, battery,
  system information and tray are text fixtures, not functioning integrations.
- Replace restart-on-topology-change with a tested fresh-allocation/reconnect
  policy if supervisor restart proves insufficient.
- Implement and verify exact *presented-candidate* action bindings. Current
  semantic messages validate module and observation identity conservatively;
  they do not implement a candidate-retention ledger, event deduplication,
  native acknowledgements or replay across presentation epochs.
- Verify native placement, reservation, input, anchored popouts, outside-dismiss
  consumption and release with an independent protocol client and attended run.

Configuration replacement advances the local module generation and clears old
popout intent. Allocation refusal clears matching intent. A local popout,
rendered image or returned effect is never recorded as native visibility.
