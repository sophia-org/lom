# lom

An original native desktop shell for [Sophia](https://github.com/sophia-org/sophia-stack).

**Лом** (*lom*) is Russian for a crowbar or heavy iron bar.

Lom aims for the configurable panels, useful modules, and polished popouts that
make ironbar appealing, with a Rust UI built for Sophia's native shell contract.
It is a new project, not an ironbar fork or a drop-in replacement. No ironbar
source has been imported. Configuration and theme compatibility are not yet
promised.

## Status

Project foundation only. There is no runnable shell, native acceptance result,
or build command yet.

The proposed UI stack is [Masonry](https://github.com/linebender/xilem/tree/main/masonry)
with Vello GPU rendering and a direct Sophia platform driver. Masonry belongs
to the Xilem repository; using its widgets does not require adopting Xilem's
reactive application layer. Exact dependency versions and integration points
must be established by a feasibility prototype.

GTK and a private Wayland bridge are outside this project's chosen direction.

## Architecture

- Lom owns widgets, styling, module state, and rendering.
- Sophia owns shell admission, authoritative placement, composition, physical
  input selection, and revocation.
- A direct driver connects Lom to `sophia_shell_v1`; toolkit objects and Vello
  scenes remain implementation details of the client.
- GPU rendering is a project requirement. Sophia's initial proposed content
  transport carries CPU pixel bytes. GPU resource sharing, synchronization,
  and release require a separately admitted design; they are not available
  simply because the client uses Vello. Readback would be a measured prototype
  technique, not an assumed final transport.
- Interactions must refer to the applicable presented targets and their action
  identities. A newer local widget tree cannot reinterpret an older activation.
- Content permission does not grant arbitrary host services, application
  control, clipboard access, execution, or a general input stream. Modules need
  separately authorized data and actions.

Sophia's [content-shell contract](https://github.com/sophia-org/sophia-stack/blob/master/docs/content-shell.md)
and [target-resolved input contract](https://github.com/sophia-org/sophia-stack/blob/master/docs/target-resolved-input.md)
govern integration. This README proposes a client direction; it does not amend
those contracts or claim the content runtime is implemented.

## First milestone

Prove one panel, label, button, and anchored popout through a direct Masonry
driver before expanding into a desktop shell:

1. Pin and inspect the Masonry platform-driver and Vello GPU interfaces.
2. Render offscreen without an X11 or Wayland connection in an explicitly
   authorized GPU test environment; retain versioned evidence.
3. Demonstrate target registration and action delivery across UI changes,
   including stale activation and teardown cases.
4. Specify and validate Sophia's GPU access and content handoff before native
   integration. Account for budgets, fences, rejection, and delayed retirement.
5. Complete the shared content lifecycle and run a real panel/popout acceptance
   test only when the required Sophia implementation is available.

Shell authors should eventually be able to reuse the driver rather than
reimplement protocol framing, resource ownership, pacing, and epoch handling.
The protocol must remain independently implementable and renderer-independent.

## License

BSD-3-Clause. See [LICENSE](LICENSE).
