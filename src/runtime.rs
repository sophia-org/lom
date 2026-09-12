//! Single-owner event coordination and explicit effect execution.
//!
//! Runtime adapters will manage bounded intake, timers, authorized services,
//! cancellation, and asynchronous results. They must not let workers mutate
//! application state or reinterpret old work under a new owner generation.
//!
//! No event loop, timer, service adapter, or effect executor is implemented yet.
