# Matched full-frame and retained-region cadence

The diagnostic-feature binaries `lom-cpu-region-cadence-full` and
`lom-cpu-region-cadence` accept exactly `--serve`. They use the same CPU worker,
opacity inspection, six-resource ledger and one staging slot. The full entry
forces one whole-panel resource. The regions entry permits retained thirds.
Neither entry enables a producer checksum or represents a live clock feed.

The driver alternates the embedded `diagnostics/regions/01.kdl` and `02.kdl`
models after each exact Presented, stopping at 2120 candidates. These are the
qualified held-sequence seconds fixtures. The 01→02 transition was captured;
02→01 was not captured. The real-socket test reconstructs both directions from
accepted resources and compares every composite with a full CPU render.
The rendering, conversion, region planning, candidate submission and resource
custody implementations are unchanged from the pixel-qualified source. Only
diagnostic driver selection and shared fixture admission are adapted.

Configuration and fixture admission reuse the held sequence's checks: one
1920-wide, scale-one, CPU top panel at height 24 or 512; Start workspaces, Center
`%H:%M`, End `%H:%M:%S`; no fixture actions, popouts or arbitrary values. The
fixture output token is remapped to the admitted output. Live indicators are
received but not applied, clock observations and wakes are suppressed, and
content input refuses. No environment knob selects the workload or its quota.

## Counters and timing

Candidate 1 establishes a full frame in both entries. Regions candidate 2
offers three tiles; later candidates replace only the End tile. Full always
offers one whole-panel resource. Cumulative render/complete/upload counts are
therefore `n` for full and `n+2` for regions from candidate 2 onward. They remain
actual job counts in `lom_cadence_sample`; they are never relabelled as candidate
counts. Establishment must lie inside warmup in any native measurement.

Each `lom_region_cadence_sample` reports actual placement count, upload and byte
counter deltas, retained resource-id/generation count, model index and disabled
state. The driver refuses a prediction mismatch or contaminated model. Steady
state requires one upload in both entries, 1920×height×4 bytes for full and
640×height×4 for regions, with two retained resources. A failed last prediction
cannot produce a complete final record.

The unchanged presentation emitter writes one extra `lom_panel_regions` row
per tiled candidate, only in the regions arm. This instrumentation asymmetry
adds work against regions and may matter at h24; it is not subtracted from the
accounting result.

The existing owner-thread monotonic timing stamps are reused. An establishment
candidate keeps its first render-attempt stamp through all three jobs; its Offer
stamp is the first queued Offer. This three-offer candidate is outside the
counted window. Every counted candidate has one job and one Offer. Neither the
stamps nor the quota identify a pure raster cost or exactly 2000 jobs inside a
kernel-bracketed accounting window.

## Native comparison contract

Use one fixed B/C/C/B block per height, h24 then h512, with full as baseline.
Use the established quota 2120, anchor generations 120 and 2120, and CPUwin's
2000 reported scanout-start intervals. Preserve process/executable custody,
zero-offset monotonic-clock proofs, signed straddle offsets, next-scanout
refusals, and all cadence outcomes. Require every counted sample's job deltas
to be 1/1/1 and its region prediction to hold. Require the median completion
interval to be within 5% of the reported refresh period; retain and report any
missed refreshes. Refuse mixed boots and duplicate run names. No replacement or
conditional repeat is planned.

Report unweighted full and regions run-mean accounting bounds and the interval
`[regions_lo - full_hi, regions_hi - full_lo]`, separately for Lom, Session, WM
and their combined total. These are tick-bracket accounting bounds, not
confidence intervals. A negative upper bound supports a combined reduction;
an interval touching zero is inconclusive. A Session increase is a finding even
if the combined interval is negative. Do not infer a saving from upload bytes.

This is a display-rate, seconds-only update workload with hot fixture glyph and
layout caches. It does not price ordinary once-per-second use, minute rollover,
workspace changes or idle cost. Scene/layout construction still runs per job.
Session still composites and uploads every placement: regions reduce producer
raster/conversion and ingest work while adding placements/draws. Any later
production-rate estimate must state its assumed update rate and remains an
estimate, not a native production measurement.

## Audited native result, 2026-10-31

`lom-region-cadence-native-01` completed all eight runs in the declared order on
one boot, with source `89c26c35` and the held Sophia `a56ace89` host. DP-1 was
1920×1080 at nominal 60 Hz. All counted median intervals were 16683 µs, with
zero missed refreshes. Both saved-data audits reproduced the raw tick bounds,
executable custody, counter gates and clean cleanup. Every counted regions
candidate uploaded one third of the panel and retained the other two resources.

All values below are µs per reported scanout interval. Differences are regions
minus full, with unweighted means of the two runs per role.

| Height | Combined full | Combined regions | Combined difference | Lom difference | Session difference |
| --- | ---: | ---: | ---: | ---: | ---: |
| 24 | 1450–1500 | 1415–1465 | −85…+15 | −65…−25 | −10…+30 |
| 512 | 3800–3855 | 2465–2515 | −1390…−1285 | −877.5…−835 | −502.5…−460 |

The h512 combined, Lom and Session intervals support reductions. At h24 Lom
improved, while combined and Session remain inconclusive; the combined midpoint
is −35 µs and its upper bound allows a 15 µs increase. Neither height has a
resolved Session increase. WM is −10…+10 µs at both heights.

Mason accepted promotion on this overall evidence, including the unresolved h24
combined interval. This decision does not relabel h24 as a measured combined
saving or establish ordinary-use savings. The matched full arm uses the same
six-resource ledger as regions, so this result does not isolate the ledger's
change from the older two-resource owner. The cadence executables themselves
were not captured; their rendering and owner paths are source-identical to the
[qualified sequence](region-sequence.md#native-pixel-qualification-2026-10-31).

Evidence (all under `~/.local/state/`):

- `t331/lom-region-cadence-native-01`, manifest
  `cb0eed0a6d4d840687aea045b4f42313eab19bf0ed62a85a6a7a5323e1769082`.
- `lom-region-cadence-audit-01/RECEIPT.json`, SHA256
  `9410ef56c4b6eed56a38672e4112e8482ab1a26de69b7be24c017db88835329d`.
- `lom-region-cadence-claude-audit-01/RECEIPT.json`, SHA256
  `e0c969c348328b3927397948e6913817402e9c407eac4383ad4588225ad3aa2c`.

The signed source's full offline gate passed 304 Rust tests. Keep both release
pools and their source worktrees held for the sealed evidence; promotion does
not rebuild, install or replace either artifact.
