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
