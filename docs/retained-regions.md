# Retained CPU regions

The ordinary CPU direct-fill path can replace changed start/center/end regions
while retaining the other accepted resources. This implementation has device-free
pixel and custody controls; native acceptance and performance remain open.
GPU, readback, copy-reference and fixed cadence entries keep full-frame rendering.

## Pixel checks

`tests/retained_regions.rs` renders actual `PreviewDriver` scenes with the current
Vello CPU backend. Each smaller target receives the same scene translated by the
region origin. The oracle renders the original scene at full size; both paths
then use the production premultiplied-BGRA conversion. Comparison is byte-exact.

The checked cases are opaque panels at 1920×24 and 1920×512, a 1919×30 panel at
scale 1.25, and fractional translucent shapes over an opaque background. Regions
cut through text and shapes, including unaligned origins. A second test changes
the clock and workspace selection, replacing only the affected 640-pixel thirds
of a 1920-wide panel. Every reconstruction equals a fresh full render; every
declared changed region actually changes. Every real panel full-frame fixture
explicitly asserts alpha 255. The synthetic translucent shapes over an opaque
background have 204 pixels at alpha 254 with this renderer, despite matching
crops; they are a separate empirical case, not an opaque fixture.
This uses retained byte buffers, not the resource protocol.
The dirty lists are hand-written oracle knowledge, not a production predictor.
A variable-width clock, renamed workspace or reflow requires new bounds and
conservative invalidation or full-frame fallback. A renderer or Cargo.lock
update must pass these normal-gate comparisons before using region rendering.

There is a counterexample to arbitrary sub-target rendering: the same fractional
translucent shapes on a transparent background differ after translation and
clipping. The first failing crop at (17, 0), size 47×7, has channel values 81
versus 82. The explicit ignored `diagnose_translucent_boundaries` probe preserves
the case and reports differences at aligned origins too. No tolerance is used
to admit it. Transparent targets and unproven rendering cases must retain the
full-frame path; these fixtures are not a proof for all opaque scenes or effects.

## Semantic change classification

`modules::panel_changes` now supplies the existing whole-panel reuse predicate.
It distinguishes unchanged pixels, changed start/center/end groups, and full
invalidation. The boolean reuse result is unchanged. The owner uses the region
classification only after the separate geometry, opacity and budget checks below.

Fixed clipping portals contain each group's logical content. The numeric-clock
and unchanged-workspace-label restrictions are additional conservative policy,
not a layout proof: font fallback can change glyph advances. Sub-group geometry
and target reuse must use actual resolved layout. New model fields and module
kinds require an explicit review through exhaustive matching.

The classifier's pixel control renders 1920-wide horizontal panels at scale 1,
at heights 24 and 512. It compares full frames and requires the actual changed
groups to equal the prediction. This is a test-only oracle, not production
admission. The owner sequence now additionally checks vertical bars. Fractional
thirds and fractional scale stay full-frame; the earlier crop tests do not admit
those geometries for incremental updates.

Compare directly against the model of the raster or resident tile set referenced
by the next candidate. The current reuse path references Presented; a future
ledger could reference an in-flight resident set. Do not accumulate masks from
intermediate observations: a full-invalidating change that is reverted can be
unchanged relative to the referenced model.

## Raster screen

Run explicitly with:

```sh
CARGO_BUILD_JOBS=2 cargo-slot lom cargo test --locked --release \
  --test retained_regions compare_full_frame_with_changed_clock_region \
  -- --ignored --nocapture --test-threads=1
```

The screen alternates two real clock scenes. Its fixed full/region/region/full
blocks each discard 120 warmups and measure 200 iterations. All output bytes are
checked outside the clock against an independently rendered full-frame crop.
Calling-thread CPU includes source validation, replay, rasterization and straight
RGBA output into a preallocated target. It excludes model reconciliation, BGRA
conversion, output-buffer allocation, ring custody, commit, Session work and native display.
It includes dispatch and clock overhead and uses optimized test code.

The first screen on 2026-10-31, based on Lom `bcf5f50`, gave these block means:

| Target | Full A / B (µs) | Clock third A / B (µs) | Output bytes full → region |
| --- | ---: | ---: | ---: |
| 1920×24 | 70.04 / 67.22 | 50.83 / 50.70 | 184320 → 61440 |
| 1920×512 | 605.38 / 598.81 | 329.12 / 320.85 | 3932160 → 1310720 |

Evidence: `~/.local/state/lom-retained-regions-01/raster-screen-1.log`.
The final-byte rerun (`raster-screen-2.log`) retained the same direction:

| Target | Full A / B (µs) | Clock third A / B (µs) |
| --- | ---: | ---: |
| 1920×24 | 79.07 / 70.40 | 58.68 / 59.24 |
| 1920×512 | 617.53 / 597.64 | 321.04 / 323.70 |

Both runs are retained; the h24 variation precludes treating one block mean as
a fixed cost. Raster time is not proportional to the smaller output area.
The results justify further implementation work; they are not whole-redraw
savings or native CPU accounting bounds. No Wayland comparison was run.

## Contract and implementation boundary

The trace at Sophia `abe9b2fd`, Lom `bcf5f50`, SDK `99b05e34` distinguishes a
staging slot from an accepted resource. Session copies a slot into independently
owned immutable storage before `SlotReleased`; retaining a tile does not retain
the staging slot. The existing single slot can serialize changed-tile offers.
Do not add per-tile rings or parallel offers to obtain reuse.

Each candidate must contain a complete placement and target table. Keep the
unchanged resource id/generation in that table. Replace a changed resource only
after its exact acceptance; retire the old resource only after the candidate
which excludes it is Presented. `ResourceReleased` remains the authority for
reusing a resource identity/generation. A ring release is not that authority.
Budget the old, incoming and retiring resources, including simultaneous dirty
regions and deferred releases. Preserve the existing stale-epoch, cancellation,
coalescing, target-binding and one-presentation-in-flight rules.

Retention currently saves only client rendering/conversion and Session ingestion
for unchanged regions. Sophia creates a new compositor node for each placement
on each candidate generation. Native EGL uploads every CPU layer on every repaint
through one shared texture; it has no CPU resource-id/generation upload cache.
CPU composition also processes damaged layers. More placements can therefore add
composition work. Fixing damage identity or adding an upload cache is a separate
Sophia design, not a benefit of this client contract.

Sophia `3f5ecd1b` adds generic three-placement lease/projection controls: retained
storage survives replacement, old consumers delay release, generation+1 may
coexist with its predecessor, and a refused candidate leaks no lease. These are
runtime/projection tests, not native composition evidence.

## Owner implementation

Admission is limited to the current fixed clipping-portal view builder, no
popout, scale one and a physical long axis divisible by three. Region offsets
are integer pixels checked at the renderer boundary. The initial full render
must have alpha 255 everywhere. Every replacement reports actual opacity from
the existing conversion pass. A nonopaque result disables regions for this
connection; unsupported geometry and full invalidation use the full-frame path.
Opacity is necessary, not a general proof of translated rendering equivalence.
Renderer, scene-builder and dependency changes must retain the pixel controls.
The empirical byte-equality claim covers the tested fixture panels; replay of
other content can round antialiased geometry differently. A popout render is
not opacity-qualified, so closing it establishes another full frame before
regions can resume.

The owner has six bounded resource identities per panel, allocated lazily.
Free, Reserved, Staging, Accepted, Resident and Retiring are distinct states.
All replacement jobs reserve resident bytes, live-resource and identity credit
before the first Offer. One ring slot serializes their renders and transfers.
Each tile resolves the full scene and targets from the same immutable model;
only its clipped pixels are rasterized and converted into the loan. Observations
during the update are coalesced for the next update against the Presented model.
This saves output-pixel work, not full-scene resolution or layout. Establishing
three tiles resolves the scene three times. A region-capable incarnation keeps
all six slots even during full fallback, including after regions are disabled;
the legacy full-only paths retain their two-slot release backpressure. A matched
full-versus-region experiment must account for that difference.

Every candidate places a complete non-overlapping panel and carries its full
target table. Old replaced resources retire only after the excluding candidate
is Presented. Each successful retire enqueue advances the queue once; exact
ResourceReleased advances its generation. Retiring slots are never borrowed.
Interaction-only candidates re-place the complete current set without uploads.
Output-facts changes still terminate this incarnation; there is no hot allocation
replacement or cross-geometry reuse.

If a tile is nonopaque before Offer, its Reserved slot returns directly to Free
without spending an identity or generation. Earlier accepted but unpublished
tiles retire normally. The old displayed set stays intact until a full-frame
replacement presents. No partially assembled set becomes a candidate.

`tests/support/region_session.rs` uses the real file/ring transport and CPU worker.
Its peer composites the actual accepted bytes and compares each candidate against
a fresh full render after seconds ticks, workspace changes, minute rollover,
popout transitions, interaction retargeting and an injected nonopaque abort.
The controls cover h24/h512, both vertical edges, non-divisible geometry, delayed
slot/resource releases, mid-update observations and aggregate admission. A
seconds-only update uploads one third of the panel. These are simulated-peer
tests, not Sophia composition or scanout evidence.

The existing `lom_panel_candidate` dimensions and bytes still describe the whole
panel; producer checksum remains unavailable on this ordinary path. The appended
`lom_panel_regions` row records placement count, uploads and uploaded bytes for
a tiled candidate. Aborted unpublished work remains in cumulative work counters;
it is not attributed to a displayed tile.

The native gate needs a changing clock/workspace workload with byte qualification
and placement-cost measurement. Static cadence redirties identical content and
cannot qualify damage savings. No native CPU reduction or displayed-pixel claim
follows from the smaller upload counter. No release artifact or native package
is changed by the device-free controls.
