//! Toolkit-independent Sophia shell transport and lifecycle boundary.
//!
//! This domain will own negotiated connection state, pacing, resource and
//! candidate records, and typed outcomes. Wire identities remain distinct from
//! local widget identities. New GPU handoff semantics require a separately
//! admitted Sophia design; this scaffold defines no wire records.
//!
//! The diagnostic content proof negotiates and transfers one immutable resource.
//! Production allocation, pacing, candidates, input, and presentation remain
//! closed until their Sophia owners are implemented and admitted.

mod pixels;
pub use pixels::{ContentPixels, PixelChunk, PixelChunks};
