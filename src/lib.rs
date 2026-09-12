//! Lom's native Sophia shell foundation.
//!
//! These modules reserve the ownership boundaries described in
//! [the architecture](https://github.com/sophia-org/lom/blob/main/ARCHITECTURE.md).
//! They do not implement a GUI, a protocol client, or a renderer yet.
//! Xilem/Masonry integration and GPU content handoff remain feasibility gates.

pub mod model;
pub mod modules;
pub mod protocol;
pub mod render;
pub mod runtime;
pub mod ui;
pub mod update;
