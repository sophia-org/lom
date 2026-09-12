//! Deterministic application transitions using the Elm/TEA discipline.
//!
//! Updates will consume model state and typed messages, then produce state
//! changes and effect descriptions. Clock reads, I/O, and GPU execution belong
//! to runtime adapters; their correlated results return as messages.
//!
//! No reducer or message vocabulary is implemented yet.
