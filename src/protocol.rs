//! Toolkit-independent Sophia shell transport and lifecycle boundary.
//!
//! This domain will own negotiated connection state, pacing, resource and
//! candidate records, and typed outcomes. Wire identities remain distinct from
//! local widget identities. New GPU handoff semantics require a separately
//! admitted Sophia design; this scaffold defines no wire records.
//!
//! The diagnostic content proof negotiates, transfers one immutable resource and
//! raises one frame demand and submits a complete candidate only under the
//! Engine-issued permit. Its headless
//! host returns the real renderer-failure outcome; it never reports native
//! presentation. Production allocation, demand pacing, input, and presentation
//! remain closed until their Sophia owners are implemented and admitted.

mod pixels;
pub use pixels::{ContentPixels, PixelChunk, PixelChunks};
