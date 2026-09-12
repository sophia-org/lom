//! Toolkit-independent Sophia shell transport and lifecycle boundary.
//!
//! This domain will own negotiated connection state, pacing, resource and
//! candidate records, and typed outcomes. Wire identities remain distinct from
//! local widget identities. New GPU handoff semantics require a separately
//! admitted Sophia design; this scaffold defines no wire records.
//!
//! No protocol connection or negotiation is implemented yet.
