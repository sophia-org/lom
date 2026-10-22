# Client-side shell GPU adapter

Shared by Lom and Provlita; this crate has no Sophia server, shell protocol,
application model or window-system dependency. It owns the existing exact DRM
grant selection, held render fd, Vello renderer and finite readback completion.
It is extracted from Lom's rendering implementation, with its original device-free
admission and completion controls. The applications retain their own Xilem hosts,
bounded worker/scheduler, content resources and presentation lifecycle.

GPU creation is explicit. A failed readback retains its job and refuses new work.
There is no CPU fallback. The separate diagnostic constructor grants no native
authority. Device-hidden tests cover selection and deadline behavior; they do
not prove driver compatibility or native presentation.

`GpuPreview::render_dma_buf` is the opt-in producer for Sophia's r10 DMA-BUF
source. It renders into a dedicated LINEAR (DRM modifier 0) B8G8R8A8 image,
which Sophia reads as opaque XRGB8888, with no CPU readback. After the bounded
draw completes, it releases the image to `VK_QUEUE_FAMILY_FOREIGN_EXT` in
`GENERAL` layout on wgpu's own queue under a fenced 2 s deadline. Only then does
it export the descriptor. Each `DmaFrame` owns its allocation. Callers retain
it, unchanged, until the matching SourceReleased. At most four frames may be
live. A draw or release whose completion is unknown keeps every object and
refuses all new work. Device creation adds `VK_EXT_queue_family_foreign` and
`VK_EXT_image_drm_format_modifier` when supported, because wgpu 28 enables
neither. The release layout relies on wgpu 28 leaving a drawn render target in
`COLOR_ATTACHMENT_OPTIMAL`. `examples/dma_smoke.rs` is the bounded render-node
smoke. It does not prove Sophia import, presentation or alpha.
