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
