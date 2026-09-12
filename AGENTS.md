# Working on Lom

Read [ARCHITECTURE.md](ARCHITECTURE.md) and the
[style guide](docs/style-guide.md) before changing code. Their ownership,
presentation, permission, and source-layout rules apply to this repository.

- Keep the application model data-oriented and deterministic. Xilem owns view
  reconciliation; runtime adapters own side effects.
- Split files by domain ownership. Review at 800 lines; production sources
  over 1,000 lines fail the gate. Do not hide implementation in test directories.
- Keep Rust test bodies outside production `src/`. Do not widen APIs for tests.
- Run `sh tools/check.sh` for changes to source or the gate. Once Rust code
  exists, also run the Rust checks listed in the style guide.
- Keep compiler and Clippy output warning-free. Never weaken a check merely to
  make a change pass.
- GPU access, protocol extensions, installation, and native-session tests need
  their separately authorized designs or runs; an offline pass is not native
  acceptance.
