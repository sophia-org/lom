# Minimal port: implementation and evidence

This tranche ports presentation concepts, configuration semantics and local
module behavior from ironbar's bundled Minimal preset. It establishes a real
Xilem/Masonry view path, not a complete admitted native shell.

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

The standard `tools/check.sh` gate passed locally: 16 Python gate tests and
29 Rust integration tests at the initial Minimal landing, formatting, doc checks, zero-warning Clippy and source
length checks. An actual manifest mutation promoting the software renderer into
normal dependencies was rejected by the dependency audit and then restored.
The gate remains display-free and GPU-free. It checks
KDL errors and availability, deterministic reducer replay, stale module owners,
publication ordering, disconnect invalidation, calendar behavior, actual widget
click-to-message routing, existing-tree rebuild/teardown, image snapshots,
CLI refusals and the normal dependency graph. Test bodies stay outside `src/`.

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

The readback tranche adds six Rust tests (35 total): straight-alpha RGBA to
premultiplied BGRA conversion, canonical whole-row chunks, joint size limits,
and the production map-callback deadline. Conversion consumes the readback
vector in place and exports immutable bytes. Each content resource is at most
4 MiB, independently of the larger diagnostic preview limit.

The renderer now uses Vello's texture path with a caller-owned readback. A
two-second prototype deadline covers GPU polling and callback delivery. Failure
retains the submitted job and prevents a second job; it never treats timeout as
GPU completion. Callback tests are GPU-free and do not prove driver behavior.
Vello's internal GPU allocation budget is still unresolved. No native permission,
connection, presentation or input path follows from these readback utilities.

Local preview limits are 8192×4096, at most 8M pixels per image and scale 0.5–4;
there is one synchronous render/readback at a time. The widget-message queue is
bounded to 64 and surfaces saturation explicitly. These do not establish the
separate negotiated GPU memory/fence/retirement budgets required for a shell.

## Remaining acceptance

- Extend the observed GPU diagnostic with source/binary/device identity, timing
  and memory evidence, and test the changed bounded-readback path explicitly.
- Implement authorized live sources and scheduling. Focused title, battery,
  system information and tray are text fixtures, not functioning integrations.
- Integrate Sophia content admission and the separately designed GPU access and
  handoff. No new wire records or grants were assigned here.
- Implement and verify exact *presented-candidate* action bindings. Current
  semantic messages validate module and observation identity conservatively;
  they do not implement a candidate-retention ledger, event deduplication,
  native acknowledgements or replay across presentation epochs.
- Verify native placement, reservation, input, anchored popouts, outside-dismiss
  consumption and release with an independent protocol client and attended run.

Configuration replacement advances the local module generation and clears old
popout intent. Allocation refusal clears matching intent. A local popout,
rendered image or returned effect is never recorded as native visibility.
