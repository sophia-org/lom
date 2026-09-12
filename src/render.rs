//! Client-side Vello GPU rendering through wgpu.
//!
//! This domain will own devices, scenes, textures, caches, and local rendering
//! completion. Local completion is not Sophia presentation or permission to
//! reuse storage. GPU access and content handoff remain separate design gates.
//!
//! No GPU initialization, content transfer, or CPU fallback is implemented yet.
