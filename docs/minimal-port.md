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
29 Rust integration tests, formatting, doc checks, zero-warning Clippy and source
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

## Implemented GPU diagnostic, execution pending

`lom preview` takes explicit configuration, theme and fixture inputs, validates
them before device creation and renders sequential images to a new directory.
Only Vulkan is requested, with no compatible surface. A reported CPU adapter is
refused. Masonry's Vello renderer draws and reads back the scenes; PNG output is
a diagnostic cost, not the eventual transport design. No GPU/native run was
performed as part of this port. The command is compiled by the standard gate.

Local preview limits are 8192×4096, at most 8M pixels per image and scale 0.5–4;
there is one synchronous render/readback at a time. The widget-message queue is
bounded to 64 and surfaces saturation explicitly. These do not establish the
separate negotiated GPU memory/fence/retirement budgets required for a shell.

## Remaining acceptance

- Execute the GPU diagnostic in an explicitly authorized environment, retaining
  source/binary/device identity, images, timing and memory observations.
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
