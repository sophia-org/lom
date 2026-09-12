//! Xilem/Masonry integration through a direct Sophia platform driver.
//!
//! Xilem will reconcile views derived from application state; Masonry will own
//! its widget tree and layout. The adapter must retain presentation-specific
//! target/action meaning rather than dispatching against the newest view tree.
//! No Winit runner, GTK frontend, or private Wayland bridge is planned.
//!
//! No toolkit dependency or driver is implemented yet.
