# Region pixel qualification sequence

`lom-cpu-regions-full --serve` and `lom-cpu-regions --serve` are separately
named, default-off diagnostic entries. Both use the checksum-off CPU renderer
and the same six-slot resource ledger. The full entry forces full-frame updates;
the region entry uses the retained-region owner. Neither is a production entry
or a performance measurement. Both accept only `--serve`, the fixed three-group
CPU top-panel configuration, and one 1920-wide scale-one allocation of height
24 or 512. GPU, DMA, copy-reference and work-statistics environment selection
are refused. There is no runtime fixture path or phase-count override.

The nine embedded KDL snapshots in `diagnostics/regions/` use the existing
fixture parser. All records explicitly identify `source=fixture`. Their output
token is mapped to the admitted allocation, but their workspace generation
stays zero and they contain no workspace action authority. They share config,
theme and fixture values. The owner consumes live indicator observations without
applying them to the fixture model, stops observing the live clock, and refuses
content input. Protocol custody, revocation and output-fact checks still apply.

Each phase installs only after the previous candidate has been Presented and
held for ten seconds. The next render uses that immutable model. There must be
exactly one rendered candidate per phase; extra, reused or contaminated
candidates refuse. The final phase remains held until normal shutdown.

| Phase | Change | Region uploads | Uploaded panel fraction |
| --- | --- | ---: | ---: |
| 0 initial | First full frame and opacity inspection | 1 | 1 |
| 1 establish | First seconds tick, establish three tiles | 3 | 1 |
| 2 seconds | Seconds only | 1 | 1/3 |
| 3 workspace | Active workspace | 1 | 1/3 |
| 4 minute | Minute and seconds | 2 | 2/3 |
| 5 rename | Workspace label forces full fallback | 1 | 1 |
| 6 restore | Restore label, still full | 1 | 1 |
| 7 reestablish | Seconds tick, establish tiles again | 3 | 1 |
| 8 seconds-again | Seconds only | 1 | 1/3 |

The full entry always uploads one whole-panel resource. `lom_panel_candidate`
keeps its whole-panel byte meaning and reports `checksum=unavailable`.
`lom_region_phase` binds phase, candidate generation, placement count, upload
count and declared upload bytes. `lom_region_final` is complete only after all
nine phases with no pending candidate or render. An interrupted sequence reports
refused. No producer checksum is inferred from captured pixels.

The normal offline gate checks both heights through the real file/ring peer.
The peer reconstructs each placement set and compares its bytes with a fresh
full render of that phase, then compares the two entries. Every adjacent phase
must also differ. Live clock/workspace changes cannot alter the phase; input
refuses. These are owner and renderer proofs, not Session-composed pixel evidence.

The attended gate uses the existing private Session
output-capture delivery, capture after the exact phase Presented and before the
next installation, and refuse missing, extra or ambiguous phase candidates.
It must compare actual full/region strip bytes for every phase and height, plus
a known-different adjacent-phase pair as a negative control. It must require the
upload counts above, so silent full fallback cannot qualify. The ordinary
production entry is not captured by these sequence runs; common renderer and
owner code are the link, not an executable-identity equivalence claim. Session
still composes and uploads every placement. No end-to-end saving is established
by the reduced producer upload bytes.

## Native pixel qualification, 2026-10-31

The sealed `~/.local/state/t331/lom-region-pixels-native-01` completed all twelve
sessions with source `01e36024`: full and regions at heights 24 and 512, in three
capture groups per role and height. All nine fixture phases completed in each
session. Both audits found 18/18 corresponding full/region strips byte-equal,
4/4 same-model phase-4/phase-6 controls equal, and 24/24 adjacent-phase negative
controls different. Every captured strip was opaque; phase counters matched,
with no disabled-region fallback. Capture brackets, executable identities and
cleanup passed. The comparison concerns panel strips, not arbitrary content.

Manifest SHA256:
`af99fba225308aee33f91ba869394e5772b015505ebcf37376fecfae45b983bd`.
Root audit `~/.local/state/lom-region-pixels-audit-01/RECEIPT.json`:
`a3c697169bc42d1f75d6213181ce107e548cf4e8ddbb7eb0f0d6e36e37363059`.
Independent audit `~/.local/state/lom-region-pixels-claude-audit-01/AUDIT.json`:
`b23dfde634b61a366d0a3b0fa9e5bd1094eb062c1b882bd86cbe05d3d641cc0d`.

The separate [matched cadence result](region-cadence.md#audited-native-result-2026-10-31)
measures CPU accounting. Neither the ordinary entry nor those cadence binaries
were captured here; their shared renderer and owner source binds that evidence.
