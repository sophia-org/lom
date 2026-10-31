# Device-free CPU redraw attribution

Run the ordinary offline gate first. With builds stopped, explicitly run:

```sh
CARGO_BUILD_JOBS=2 cargo-slot lom cargo test --release --locked --lib \
  render::worker::tests::redraw_attribution::measure_redraw_phases \
  -- --exact --ignored --nocapture --test-threads=1
```

This diagnostic uses the production retained-host scene preparation and the
same Vello CPU raster, BGRA conversion and checksum functions at 1920x24 and
1920x512. The static config is byte-identical to cadence-native-02's h24 direct
panel; h512 changes only height. The nine admitted workspace projections are
reconstructed explicitly. Every result must match the saved produced FNV64
(`a7bbb8c840a7303c` / `9a603a023eb2ab7c`). The first warmup frame additionally
matches the unsplit production render byte for byte, before recorded samples.

Each size runs 120 warmup renders and 200 recorded renders in that fixed order.
Both slots are real SDK mappings. The owner acquires a loan, sends it to one
worker, receives it and commits with `RingProducer::commit_owned_slot`; the
previous slot is released only after the replacement commit. Accepted/release
events are fixtures. They validate local custody, not a Session or native
presentation. A busy acquisition while both slots are outstanding must refuse.

Raw records carry CLOCK_THREAD_CPUTIME_ID CPU and monotonic elapsed nanoseconds
for owner acquire/model clone/producer commit and worker scene/raster/conversion/
checksum/scene destruction. The raster phase includes the imaging adapter's
RGBA output, not just Vello scan conversion. Verify the pinned Vello feature
graph has no multithreading before interpreting worker CPU as the raster cost.
Timer-only records report overhead without subtraction. Logging happens after
each size, outside the measured operations.

This is phase attribution, not a speedup or an additive whole-process CPU model.
Scheduling, channel dispatch, validation, most destruction and fixture events
are outside these scopes. Commit ends at the producer's generated Offer bytes:
ShellConnection queuing, control-file writes, host acceptance and composition
are not included. The slots are warm and the loop has no native pacing. There
is no GPU/display access, copy-arm comparison or native performance claim.
Preserve the source tree, binary, feature graph, fixture and raw records with
any report; no choice of optimization is implied by a passing correctness test.

## First attribution, 2026-10-30

Signed source `8c31f95c405724257ba78396b7dde792e5ab6caa`, tree
`0d38804a83f5c904ffb4e7d8610c444aefc25f9a`, passed the full offline gate
(241 Rust tests, no failures; default and diagnostic-feature Clippy clean).
One release measurement then ran with no compiler processes observed at its
start. Evidence is in `~/.local/state/lom-cpu-redraw-attribution-01`:
`SOURCE.json`, `GATE.json`, `MEASUREMENT.json`, `measurement-1.log`,
`analysis-1.json` and `RECEIPT.json`. The test binary was consumed in place,
SHA256 `6c368c5c529c1a9516be145788bdf3981b60455283528d4dca67b1df2908264c`.
Its profile was opt-level 3 with debug assertions off. Resolved and compiled
Vello features were `png,std,text,u8_pipeline`, without multithreading.

Mean thread CPU, microseconds per measured operation (200 samples per cell):

| Phase | Thread | 1920×24 | 1920×512 |
|---|---|---:|---:|
| Acquire mapped loan | Owner | 0.54 | 0.73 |
| Clone model | Owner | 1.11 | 1.47 |
| Retained scene preparation | Worker | 35.44 | 32.60 |
| Raster and RGBA output | Worker | 63.96 | 552.31 |
| Premultiplied BGRA conversion | Worker | 75.04 | 1324.40 |
| FNV64 checksum | Worker | 156.25 | 2795.30 |
| Scene/targets destruction | Worker | 0.77 | 0.89 |
| Producer commit | Owner | 0.91 | 1.55 |

All 400 recorded renders matched their saved checksum. The first warmup at
each size also matched the unsplit renderer byte for byte. Raw elapsed times
and CPU distributions are retained. Owner timer-only CPU averaged 0.54 µs;
two scene-destruction reads returned zero CPU and remain in the data. Small
phase values near timer cost are not precise optimization estimates. No
overhead was subtracted, and owner timer calibration is not a worker baseline.

Checksum is the largest measured phase at both sizes; conversion is next.
They account for about 47%/23% of the summed measured worker phases at h24,
and 59%/28% at h512. Those shares are not whole-process or native CPU shares.
The warm offscreen loop is not comparable directly to the cadence CPU bounds.

The first follow-up is to examine checksum policy before adding tile machinery.
In this source, `RenderImage::checksum` supplies `RasterSummary` and the
`lom_panel_candidate` log; the FNV value is not a content admission or slot
release condition. Any proposed ordinary-path reduction must preserve exact
diagnostic pixel qualification and state its logging/schema change explicitly.
Conversion remains a separate evidenced target. Neither change is implemented
or credited with a speedup by this attribution.

## Diagnostic checksum policy candidate, 2026-10-31

Ordinary CPU fills omit the hash pass; the two diagnostic binaries explicitly
retain it. The existing schema-2 candidate record reports `checksum=unavailable`
for an ordinary CPU fill, including when the diagnostic feature is compiled.
This changes diagnostic observability, not pixels, protocol records or custody.
Consumers that require schema 1 must use a diagnostic entry; a zero or reused
checksum is never substituted. `RenderImage::RingFill.checksum` is now optional.
GPU readback still hashes on the owner, and DMA still reports unavailable.

The normal controls exercise actual entry dispatch, exact bytes at h24/h512,
and unchanged ring custody. For an explicitly requested release-only local
comparison, run `render::worker::tests::redraw_attribution::measure_checksum_policy`
with the same command form above. It calls the actual worker render function
on the test's calling thread, hash-on/off/off/on at each height, each block
with 120 warmups and 200 measured calls. Pixel/checksum validation and loan
abort occur outside the measured call, identically for both policies. Model
construction, dispatch, commit and host work are excluded; this is not an
asynchronous-worker or native measurement. Timer costs remain included.

The existing cadence binaries still hash every frame, so an unchanged cadence
comparison cannot qualify this ordinary-path saving. A fresh native design must
separate pixel qualification from the no-hash performance arm and report the
observability difference. No native performance benefit or production adoption
follows from this candidate's offline checks.

The signed candidate `91e792d930773c46838dd37bbad321bf44dc9297` passed
the full gate: 245 Rust tests, 18 tool tests, no failures or warnings, and both
feature configurations passed Clippy. Mutating either entry's policy caused
the entry-dispatch control to fail; the restored control passed.

One release sequence then measured the actual render call, with no compiler
processes observed at startup. All 2,560 renders, including warmups, passed
post-call pixel validation. Per-block mean calling-thread CPU in microseconds:

| Height | Hash on (first) | Hash off | Hash off | Hash on (last) |
|---|---:|---:|---:|---:|
| 24 | 330.39 | 172.50 | 150.31 | 273.60 |
| 512 | 4888.66 | 1894.51 | 1953.82 | 4733.92 |

The equal-weight means are 301.99 → 161.40 µs at h24 and
4811.29 → 1924.16 µs at h512. These are descriptive offscreen results;
variation between blocks is preserved, with no confidence interval or native
projection. They support removing the hash pass as a candidate, not deployment.

Evidence: `~/.local/state/lom-cpu-diagnostic-checksum-01/RECEIPT.json`,
`SOURCE-GATE.json`, `MEASUREMENT.json`, raw `measurement-1.log` and
`analysis-1.json`. The release test binary was consumed in place and hashed
before/after: `c7543b2951ee7a40d50a47e26d4d04b6687177cf0ea840856f6c132ad6e55c76`.
It used opt-level 3 without debug assertions; compiled Vello features were
`png,std,text,u8_pipeline`, without multithreading. No ordinary runtime
release, native launch or master promotion was performed for this candidate.
