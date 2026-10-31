# Retained CPU regions: device-free feasibility

This is a t017 prerequisite, not a runtime implementation or native acceptance.
The production renderer still renders a whole panel. The next candidate may
replace only changed, non-overlapping regions while retaining the other accepted
resources. Keep the full-frame path as the fallback.

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

Before changing production, implement a bounded client resource ledger. Start with conservative
invalidation for proven fixed geometry; configuration, scale, allocation, layout
or unsupported paint changes fall back to full rendering. Check alternating
resources, delayed slot/resource release, failure during a multi-offer update,
and actions against the old Presented candidate. The retained CPU candidate
must leave GPU/readback and DMA behavior unchanged.

Eligibility must check actual output alpha, not infer opacity from the theme.
Initial full-frame eligibility and each replacement need checks; opacity alone
does not prove arbitrary scene equivalence. The supported scene/geometry rules
and fallback still need implementation. The current proof makes no production
admission decision.

The native gate needs a changing clock/workspace workload with byte qualification
and placement-cost measurement. Static cadence redirties identical content and
cannot qualify damage savings. No production code, release artifact or native
package is changed by this proof.
