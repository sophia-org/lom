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

The attended gate remains required. It must use the existing private Session
output-capture delivery, capture after the exact phase Presented and before the
next installation, and refuse missing, extra or ambiguous phase candidates.
It must compare actual full/region strip bytes for every phase and height, plus
a known-different adjacent-phase pair as a negative control. It must require the
upload counts above, so silent full fallback cannot qualify. The ordinary
production entry is not captured by these sequence runs; common renderer and
owner code are the link, not an executable-identity equivalence claim. Session
still composes and uploads every placement. No end-to-end saving is established
by the reduced producer upload bytes.
