# Style Guide

This is Lom's adaptation of [Sophia's style guide](https://github.com/sophia-org/sophia-stack/blob/7ac6f1eac239b4dde29c66cbd55fc4219ae190f4/docs/style-guide.md),
inspected on 2026-09-12. It preserves Sophia's data-boundary, ownership, test,
logging, and warning discipline. Sophia-specific Engine, X11 authority, and
migration exceptions are not imported into this new client project.

[Lom Architecture](../ARCHITECTURE.md) defines the component boundaries.

## Languages and layout

Use Rust for the shell and reusable client libraries. Small Python or shell
tools may implement repository checks; other languages need a concrete boundary
reason. Do not mix languages inside a component without that reason.

Group source by ownership: passive types, state tables, updates, protocol,
runtime effects, UI adaptation, rendering, and modules. These are responsibilities,
not a requirement to create empty directories or a crate for every category.
Data records may provide pure validation, conversion, and formatting helpers;
they should not conceal I/O or authority-changing behavior.

Keep `lib.rs` and established public module paths as facades. Implementation
belongs with its owning domain. A file should have one owner and one reason to
change. Separate parsing, socket I/O, state transitions, rendering, and tests
when their ownership differs. Do not split a cohesive algorithm into arbitrary
fragments merely to lower a number, and do not widen visibility to enable a split.

## Source length gate

Lom adopts Sophia's thresholds:

- At **800 lines**, production and test files are reported for cohesion review.
- At **1,000 lines**, production files remain allowed but need that review.
- **Over 1,000 lines**, production files fail the gate.
- Test files at or above 800 lines are reported; their length alone does not
  fail the gate. Keep tests organized by behavior and fixtures by their owner.

There is no exception or accepted-debt ledger in Lom. Identify a real domain
boundary before production files exceed the limit. Changing the policy requires
an explicit reviewed policy change, not an entry that hides a new violation.

Run:

```sh
sh tools/check.sh
```

The command runs regression tests of the gate and then audits the real checkout.
GitHub Actions runs the same command on pushes and pull requests. A Rust
integration test also runs the real audit, so ordinary `cargo test` includes
the length check. The source-length portion requires only Python 3 and Git. The complete gate also uses the Rust
toolchain pinned in `rust-toolchain.toml`; it opens no GUI, GPU, or display
connection. Dependency and toolchain installation may require network access.

The audit scans Git-tracked and unignored untracked working-tree source files,
including root `src`, workspace crates, examples, build scripts, tooling, and
shaders. It counts physical lines, including blank/comment lines and a final
line without a newline. CRLF is one line ending. A tracked file remains checked
even if a new ignore pattern matches it.

Recognized suffixes are `.rs`, `.py`, `.sh`, `.c`, `.h`, `.cc`, `.cpp`, `.hpp`,
`.nim`, `.zig`, `.wgsl`, `.glsl`, `.vert`, `.frag`, and `.comp`. This extends the
Sophia Rust audit to Lom's own tooling and shader sources. Documentation and
data files are not source-length candidates. Top-level `vendor/`, `third_party/`,
`target/`, and `.artifacts/` are excluded as external or generated material;
Lom-owned implementation must not be placed there to evade review. Generated
files elsewhere are not automatically exempt. Source symlinks are rejected
rather than followed outside the audited checkout.

Files beneath an external `tests/` directory are test sources. A `tests.rs` or
`tests/` directory inside production `src/` does not escape the production cap.
Normal deletions of tracked files are ignored; Git or other read failures fail
the check. `--root PATH` on `tools/audit_source_layout.py` exists for isolated
fixtures and alternate checkout inspection, not threshold overrides.

The automatic gate enforces length, not every guideline below. Ownership,
cohesion, logging, and test placement also require code review.

## Test placement and evidence

Keep Rust test bodies outside production `src/`, normally in the crate's
`tests/`, with shared builders and fixtures in `tests/support/`. Do not add
inline `#[test]` functions or `#[cfg(test)] mod tests` blocks under `src/`.

Exercise public behavior and do not expose private helpers solely for tests.
If a private invariant genuinely requires an external test module mounted at a
private boundary, document the precise justification here before introducing
it. The readback deadline check in `tests/readback_completion.rs` compiles the
private, GPU-free `src/render/completion.rs` module directly. This exercises the
production poll/callback deadline function without exposing a test API or opening
a GPU. Test bodies remain outside `src`.

Repository-tool tests live in `tools/tests/` and exercise the actual command or
observable behavior. Gate regressions must prove both acceptance and rejection,
especially at threshold boundaries; a helper-only assertion is not sufficient.

Keep reducer, widget, renderer, protocol, and native tests distinct. Simulated
widget input is not proof of Sophia input routing. GPU completion is not proof
of desktop presentation. Record missing harnesses and unvalidated claims rather
than replacing evidence with a manual assertion. Do not run an attended/native
acceptance test implicitly from the offline gate.

## TEA and ownership

Application behavior follows `model + message -> update -> effects`. Updates
and view construction do not read clocks, access services, or render. Runtime
adapters execute effects and return correlated observations. One owner mutates
the model; other components receive records, snapshots, IDs, or explicit handles.

Prefer indexed tables, typed identities, generation validation, and immutable
candidate snapshots. Avoid mutable global registries, shared object graphs,
stringly typed hot-path IDs, and callbacks that mutate another owner's state.
Xilem owns reconciliation and Masonry owns its private widget tree; neither
justifies a second authoritative application model.

TEA does not govern every internal layout pass or GPU command. Keep execution
and resource lifetimes explicit, with bounded queues and predictable control
flow. Reuse buffers where appropriate, but measure allocations before hiding
ownership behind caches or pools. Stable scenes and repeated events should not
cause avoidable allocation churn. Preparing a new resource never authorizes
reusing one still referenced by a presented candidate.

## Naming and errors

Use ordinary Rust naming: `PascalCase` types/traits, `snake_case` functions,
variables and modules, and `SCREAMING_SNAKE_CASE` constants. IDs should name
their owner and purpose, such as `ModuleId`, `RequestId`, and `CandidateId`.
Wrap foreign protocol identifiers at their boundary; do not substitute local
widget identities for Sophia target identities.

Errors identify the failing boundary, for example `ProtocolError::StaleEpoch`
or `RenderError::DeviceLost`. Return typed failures where the caller can act.
Permission denial and unavailable capabilities are expected outcomes, not
internal crashes. Reconnection must not turn an ambiguous old action into an
automatic retry under new authority.

## Logging

Libraries use `tracing`; the application entrypoint installs subscribers.
Library code does not print directly to stdout/stderr. CLI output and repository
tools may print their documented results and diagnostics.

Default logs exclude titles, text input, clipboard/notification bodies, file or
URI payloads, pixels, credentials, and raw host or protocol-private identities.
Prefer opaque authorized IDs, generations, counts, enum outcomes, and durations.
Opaque does not mean that every handle is automatically appropriate to log.

Use `trace` for hot-path detail, `debug` for normal transitions, `info` for
coarse lifecycle summaries, and `warn` for security-relevant rejection, stale
work, timeout, or fallback. Use `error` when failure cannot be returned to its
caller; do not log the same returned failure at every layer.

Diagnostic fields form a schema. Preserve their meaning and name the measured
quantity, not the helper that emitted it. Count discarded work accurately; zero
must mean nothing was discarded, not that no counter exists. Aggregate pressure
diagnostics and use cumulative counts so saturation does not also flood logs.

## Formatting, warnings, and validation

Rust changes must pass formatting, compiler checks, tests, and Clippy without
warnings. `tools/check.sh` runs these commands with compiler and rustdoc
warnings denied:

```sh
cargo fmt --all -- --check
cargo fetch --locked
python3 -B tools/audit_dependencies.py
cargo test --workspace --all-targets --locked
cargo test --workspace --doc --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
```

Feature-specific checks must be added when those features exist. Rust tests
remain outside `src/`; the current CLI tests execute without a session
environment. Keep `rust-toolchain.toml`, the package minimum Rust version, and
the CI toolchain installation aligned when upgrading.

Fix warnings first. If a lint is inapplicable at one site, use
`#[expect(lint, reason = "...")]` where supported, so a stale suppression is
itself detected. A repeated deliberate threshold may be configured with a
documented rationale. Do not add blanket allowances or weaken the gate to make
a change pass.

For documentation-only changes, inspect the text, links, and architecture
consistency; a renderer build is unnecessary. Changes to this gate require its
regression tests and the repository audit to pass. Report exactly which checks
ran and which native or capability-dependent evidence remains open.
