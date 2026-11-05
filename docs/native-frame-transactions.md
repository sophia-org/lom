# Sophia Frame Transactions in Lom

Status: default-off source prototype and device-free producer tests. The
`experimental-frame-transactions` Cargo feature accepts an explicitly supplied
r13 connection only with a direct CPU ring renderer. The CLI continues to
request r10. Host negotiation, model trace validation and
native qualification remain separate gates. No CPU saving is claimed.

The pinned Rust SDK is `b483e1ee9d79730705d372ffcebd47c7b12c4514`;
its paired C initial-image API is published at `0df720b0b1af7dd793f55b2bceac867bc1261b6b`.

Lom keeps its existing retained region plan, resource owners and immutable
candidate-specific input bindings. Initial full-panel rendering and multi-upload
region establishment use ordinary image uploads and candidates before opening
a stream. An eligible update uses one whole FrameCommit: retained resources,
zero or one new owned-ring image, and at most one excluded predecessor. It
does not flatten a multi-layer scene or drop retained resources to fit SFT.

The SDK owns one credit per stream and two source slots. A successful local
enqueue spends that credit; Prepared does not return it. Presented activates
the exact new targets and permits the embedded predecessor retirement, while
source-slot reuse still waits for its own exact release. Predecessor resources
also remain unavailable until ResourceReleased. The service consumes frame
outcomes and input actions in the same journal order.

An update needing multiple uploads or predecessor retirements closes and drains
an existing stream before continuing with the ordinary image/demand/candidate
path on the same connection. This fallback stays selected for that binding.
An explicit unadmitted FrameRefused spends the attempted candidate identity,
keeps the displayed scene, settles source refusal, and renders the update again
after draining. Unknown custody or transport failure fails closed.

The real CPU worker and socket client tests compare peer-composed pixels with
the full renderer across establishment, copied/retained frames and fallback.
They withhold source and resource releases independently and send actions on
both sides of Presented. This peer is a scripted protocol owner, not Sophia's
runtime or native renderer; these tests do not establish host interoperation,
tear-free scanout or performance.

Evidence: `~/.local/state/sophia-lom-frame-producer-01`. The complete locked
repository gate passes with the published SDK. Five focused controls cover the
region path, a full-panel worker at heights 24/512, explicit unadmitted refusal,
independent releases and input ordering. Removing the source-release guard or
skipping the new Presented target installation makes the corresponding test
fail; the guards were restored before the final gate.
