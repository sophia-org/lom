use super::*;
use crate::config::parse_shell_config;
use chrono::{DateTime, Utc};

#[path = "redraw_attribution.rs"]
mod redraw_attribution;

fn model() -> Model {
    let (config, theme) =
        parse_shell_config(include_str!("../../examples/minimal/shell.kdl")).unwrap();
    Model::new(config, theme, 1, DateTime::<Utc>::UNIX_EPOCH.fixed_offset())
}

#[test]
fn mailbox_allows_only_one_job_and_reports_one_completion() {
    let (requests, incoming) = mpsc::sync_channel(1);
    let (completed, completions) = mpsc::sync_channel(1);
    let mut worker = RendererWorker {
        wake: Arc::new(crate::wake::Wake::new().unwrap()),
        dma: false,
        cpu: false,
        #[cfg(feature = "diagnostic-cpu-reference")]
        copy: false,
        requests,
        completions,
        submitted_at: None,
        timeout: Duration::from_secs(1),
    };
    worker.submit(identity(), model(), 8, 48, 2.0).unwrap();
    assert!(worker.submit(identity(), model(), 8, 48, 2.0).is_err());
    let job = incoming.try_recv().unwrap();
    assert_eq!((job.width, job.height, job.scale), (8, 48, 2.0));
    completed
        .send(Ok(RenderedContent {
            pixels: (crate::protocol::ContentPixels::from_rgba8(8, 48, vec![0; 1536]).unwrap())
                .into(),
            targets: Vec::new(),
        }))
        .unwrap();
    assert!(worker.poll().unwrap().is_some());
    assert!(worker.poll().is_err());
}

#[test]
fn expired_worker_job_is_quarantined_instead_of_reused() {
    let (requests, _incoming) = mpsc::sync_channel(1);
    let (_completed, completions) = mpsc::sync_channel(1);
    let mut worker = RendererWorker {
        wake: Arc::new(crate::wake::Wake::new().unwrap()),
        dma: false,
        cpu: false,
        #[cfg(feature = "diagnostic-cpu-reference")]
        copy: false,
        requests,
        completions,
        submitted_at: Some(Instant::now()),
        timeout: Duration::ZERO,
    };
    assert!(worker.completion_fd().is_some());
    assert!(worker.completion_deadline().unwrap() <= Instant::now());
    assert!(worker.poll().unwrap_err().contains("remain quarantined"));
    assert!(worker.submit(identity(), model(), 8, 48, 2.0).is_err());
}

fn identity() -> RenderIdentity {
    RenderIdentity {
        grant: sophia_shell_protocol::ContentGrant {
            connection_epoch: 1,
            content_grant_epoch: 2,
        },
        output: sophia_shell_protocol::ContentOutputId {
            id: 1,
            generation: 1,
        },
        allocation: sophia_shell_protocol::ContentAllocationId {
            id: 1,
            generation: 1,
        },
        scale_generation: 1,
    }
}

#[test]
fn retained_hosts_are_bounded_by_exact_allocation_and_grant_identity() {
    let mut hosts = RetainedHosts::default();
    let mut first = identity();
    let mut second = first;
    second.output.id = 2;
    second.allocation.id = 2;
    let job = |identity| RenderJob {
        identity,
        fill: None,
        model: model(),
        width: 800,
        height: 48,
        scale: 1.0,
    };
    hosts.scene(job(first)).unwrap();
    hosts.scene(job(second)).unwrap();
    let unchanged = hosts.hosts.keys().find(|key| key.2 == 2).copied().unwrap();
    for _ in 0..10 {
        hosts.scene(job(first)).unwrap();
    }
    assert_eq!(hosts.hosts.len(), 2);
    assert!(hosts.hosts.contains_key(&unchanged));
    let mut wrong_size = job(first);
    wrong_size.width += 1;
    assert!(hosts.scene(wrong_size).is_err());
    first.allocation.generation += 1;
    hosts.scene(job(first)).unwrap();
    assert_eq!(hosts.hosts.len(), 2);
    assert!(hosts.hosts.contains_key(&unchanged));
    first.grant.content_grant_epoch += 1;
    hosts.scene(job(first)).unwrap();
    assert_eq!(hosts.hosts.len(), 1);
    assert!(!hosts.hosts.contains_key(&unchanged));
}

#[test]
fn an_idle_worker_clears_late_wakes_and_reports_disconnect() {
    let (requests, _incoming) = mpsc::sync_channel(1);
    let (completed, completions) = mpsc::sync_channel(1);
    let mut worker = RendererWorker {
        wake: Arc::new(crate::wake::Wake::new().unwrap()),
        dma: false,
        cpu: false,
        #[cfg(feature = "diagnostic-cpu-reference")]
        copy: false,
        requests,
        completions,
        submitted_at: None,
        timeout: Duration::from_secs(1),
    };
    worker.wake.notify().unwrap();
    worker.clear_completion_wake().unwrap();
    let mut fds = [crate::wake::interest(worker.wake.as_fd(), libc::POLLIN)];
    assert!(!crate::wake::wait(&mut fds, Some(Duration::ZERO)).unwrap());
    drop(completed);
    worker.wake.notify().unwrap();
    assert!(
        worker
            .clear_completion_wake()
            .unwrap_err()
            .contains("stopped")
    );
}

#[test]
fn a_late_cpu_completion_is_refused_and_left_with_the_quarantined_worker() {
    let (requests, _incoming) = mpsc::sync_channel(1);
    let (completed, completions) = mpsc::sync_channel(1);
    let mut worker = RendererWorker {
        wake: Arc::new(crate::wake::Wake::new().unwrap()),
        dma: false,
        cpu: true,
        #[cfg(feature = "diagnostic-cpu-reference")]
        copy: false,
        requests,
        completions,
        submitted_at: Some(Instant::now()),
        timeout: Duration::ZERO,
    };
    completed
        .send(Ok(RenderedContent {
            pixels: (crate::protocol::ContentPixels::from_rgba8(8, 48, vec![0; 1536]).unwrap())
                .into(),
            targets: Vec::new(),
        }))
        .unwrap();
    assert!(
        worker
            .poll()
            .unwrap_err()
            .starts_with("CPU worker deadline expired")
    );
    // The queued result was not taken for commit.
    assert!(worker.completions.try_recv().is_ok());
}

#[test]
fn cpu_and_gpu_workers_refuse_each_others_submissions() {
    let (requests, incoming) = mpsc::sync_channel(1);
    let (_completed, completions) = mpsc::sync_channel(1);
    let mut worker = RendererWorker {
        wake: Arc::new(crate::wake::Wake::new().unwrap()),
        dma: false,
        cpu: true,
        #[cfg(feature = "diagnostic-cpu-reference")]
        copy: false,
        requests,
        completions,
        submitted_at: None,
        timeout: Duration::from_secs(1),
    };
    assert!(worker.uses_ring_fill());
    assert!(worker.submit(identity(), model(), 8, 48, 2.0).is_err());
    assert!(
        incoming.try_recv().is_err(),
        "no CPU job without a ring loan"
    );
    worker.cpu = false;
    assert!(!worker.uses_ring_fill());
}

const RING: sophia_shell_protocol::shell_files::RingIdentity =
    sophia_shell_protocol::shell_files::RingIdentity {
        connection_epoch: 7,
        content_grant_epoch: 3,
        channel_generation: 9,
        ring: 1,
        generation: 1,
    };

fn ring_event(
    kind: sophia_shell_protocol::shell_files::RingEventKind,
) -> sophia_shell_protocol::shell_files::RingEvent {
    sophia_shell_protocol::shell_files::RingEvent {
        kind,
        identity: RING,
        sequence: 5,
        offer_id: 0,
        slot: 0,
        slot_generation: 0,
        resource: 0,
        resource_generation: 0,
        reason: 0,
        changed_generation: false,
    }
}

/// A real one-slot SDK ring producer; its loans own the actual mapping.
fn ready_ring() -> sophia_image_producer::RingProducer {
    use sophia_shell_protocol::shell_files::RingEventKind;
    let mut producer = sophia_image_producer::RingProducer::new(
        sophia_image_producer::RingProducerConfig {
            identity: RING,
            slots: 1,
            capacity: 0,
            max_resource_bytes: 8 * 48 * 4,
            transfer_timeout_ms: 2000,
        },
        0,
    )
    .unwrap();
    assert!(producer.registration().is_some());
    producer
        .ring_event(ring_event(RingEventKind::RingAdmitted), 1)
        .unwrap();
    assert!(producer.source_packet().is_some());
    producer.source_sent().unwrap();
    producer
        .ring_event(ring_event(RingEventKind::RingReady), 2)
        .unwrap();
    assert!(producer.ready());
    producer
}

fn ring_upload() -> sophia_image_producer::RingUpload {
    sophia_image_producer::RingUpload {
        resource: 11,
        resource_generation: 1,
        allocation: 21,
        allocation_generation: 1,
        output: 31,
        output_generation: 1,
        topology_generation: 41,
        width: 8,
        height: 48,
    }
}

/// Wait on the real completion eventfd, clearing before each poll as the
/// service does.
fn completion(worker: &mut RendererWorker) -> Result<RenderedContent, String> {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let mut fds = [crate::wake::interest(worker.wake.as_fd(), libc::POLLIN)];
        crate::wake::wait(
            &mut fds,
            Some(deadline.saturating_duration_since(Instant::now())),
        )?;
        worker.clear_completion_wake()?;
        if let Some(content) = worker.poll()? {
            return Ok(content);
        }
        assert!(Instant::now() < deadline, "CPU worker never completed");
    }
}

#[test]
fn a_real_cpu_worker_fills_the_owned_loan_with_the_same_renderer_bytes() {
    use masonry::imaging::render::ImageRenderer;
    let mut producer = ready_ring();
    let fill = producer.acquire_owned_slot(8 * 48 * 4, 3).unwrap();
    let mut worker = RendererWorker::start_cpu().unwrap();
    worker.timeout = Duration::from_secs(30);
    worker
        .submit_fill(identity(), model(), 8, 48, 2.0, fill)
        .map_err(|(error, _)| error)
        .unwrap();
    let content = completion(&mut worker).unwrap();
    let RenderImage::RingFill {
        mut fill,
        identity: rendered,
        width,
        height,
        checksum,
    } = content.pixels
    else {
        panic!("CPU worker must return its ring loan");
    };
    assert_eq!((rendered, width, height), (identity(), 8, 48));
    let mut driver = PreviewDriver::new(model(), 8, 48, 2.0, false).unwrap();
    let mut scene = driver.scene();
    let image = VelloCpuRenderer::new(1, 1)
        .render_source(&mut scene.scene, 8, 48)
        .unwrap();
    let reference = crate::protocol::ContentPixels::from_rgba8(8, 48, image.data).unwrap();
    assert_eq!(fill.pixels_mut().unwrap(), reference.bytes());
    assert_eq!(checksum, crate::service::pixel_checksum(reference.bytes()));
    let (offer, _) = producer
        .commit_owned_slot(&mut fill, ring_upload(), 4)
        .unwrap();
    assert_eq!(offer, 1, "the owner commits the worker's exact payload");
    assert!(fill.pixels_mut().is_err());
}

#[test]
fn a_failed_cpu_render_faults_its_loan_before_the_completion_wake() {
    let mut producer = ready_ring();
    // One pixel short of the job's geometry: the raster refuses it.
    let fill = producer.acquire_owned_slot(8 * 48 * 4 - 4, 3).unwrap();
    let mut worker = RendererWorker::start_cpu().unwrap();
    worker.timeout = Duration::from_secs(30);
    worker
        .submit_fill(identity(), model(), 8, 48, 2.0, fill)
        .map_err(|(error, _)| error)
        .unwrap();
    let error = completion(&mut worker).unwrap_err();
    assert!(error.contains("exactly width * height * 4"), "{error}");
    // The error was received after the wake; the loan dropped before both.
    assert!(producer.closed() && !producer.ready());
    assert!(producer.offer(1).is_none(), "nothing was published");
}

#[test]
fn a_late_queued_cpu_fill_is_refused_and_faults_only_when_quarantine_ends() {
    let mut producer = ready_ring();
    let fill = producer.acquire_owned_slot(8 * 48 * 4, 3).unwrap();
    let (requests, _incoming) = mpsc::sync_channel(1);
    let (completed, completions) = mpsc::sync_channel(1);
    let mut worker = RendererWorker {
        wake: Arc::new(crate::wake::Wake::new().unwrap()),
        dma: false,
        cpu: true,
        #[cfg(feature = "diagnostic-cpu-reference")]
        copy: false,
        requests,
        completions,
        submitted_at: Some(Instant::now()),
        timeout: Duration::ZERO,
    };
    completed
        .send(Ok(RenderedContent {
            pixels: RenderImage::RingFill {
                fill,
                identity: identity(),
                width: 8,
                height: 48,
                checksum: 0,
            },
            targets: Vec::new(),
        }))
        .unwrap();
    assert!(worker.poll().unwrap_err().contains("remain quarantined"));
    assert!(
        producer.ready(),
        "the queued loan is still live, not reclaimed"
    );
    drop(completed);
    drop(worker);
    assert!(
        producer.closed(),
        "ending quarantine drops and faults the loan"
    );
    assert!(producer.offer(1).is_none());
}

#[test]
fn refused_cpu_submissions_return_the_real_loan_unspent() {
    let mut producer = ready_ring();
    let (requests, incoming) = mpsc::sync_channel(1);
    let (_completed, completions) = mpsc::sync_channel(1);
    let mut worker = RendererWorker {
        wake: Arc::new(crate::wake::Wake::new().unwrap()),
        dma: false,
        cpu: true,
        #[cfg(feature = "diagnostic-cpu-reference")]
        copy: false,
        requests,
        completions,
        submitted_at: Some(Instant::now()),
        timeout: Duration::from_secs(1),
    };
    let fill = producer.acquire_owned_slot(8 * 48 * 4, 3).unwrap();
    let (error, mut fill) = worker
        .submit_fill(identity(), model(), 8, 48, 2.0, fill)
        .unwrap_err();
    assert!(error.contains("already owns"), "{error}");
    producer.abort_owned_slot(&mut fill).unwrap();
    worker.submitted_at = None;
    drop(incoming);
    let fill = producer.acquire_owned_slot(8 * 48 * 4, 4).unwrap();
    let (error, mut fill) = worker
        .submit_fill(identity(), model(), 8, 48, 2.0, fill)
        .unwrap_err();
    assert!(error.contains("CPU worker stopped"), "{error}");
    let (offer, _) = producer
        .commit_owned_slot(&mut fill, ring_upload(), 5)
        .unwrap();
    assert_eq!(offer, 1, "refusals spent no offer ID");
}

/// The diagnostic copy arm returns the direct fill's bytes for the same job
/// as owned pixels for the slice upload, and returns a refused loan unspent.
#[cfg(feature = "diagnostic-cpu-reference")]
#[test]
fn the_copy_reference_worker_returns_the_direct_fill_bytes_and_refuses_loans() {
    let mut producer = ready_ring();
    let fill = producer.acquire_owned_slot(8 * 48 * 4, 3).unwrap();
    let mut direct = RendererWorker::start_cpu().unwrap();
    direct.timeout = Duration::from_secs(30);
    direct
        .submit_fill(identity(), model(), 8, 48, 2.0, fill)
        .map_err(|(error, _)| error)
        .unwrap();
    let RenderImage::RingFill { mut fill, .. } = completion(&mut direct).unwrap().pixels else {
        panic!("the direct arm returns its ring loan");
    };
    let stop = AtomicBool::new(false);
    let (wake, _peer) = std::os::unix::net::UnixStream::pair().unwrap();
    let mut copy =
        RendererWorker::start_cpu_copy_reference_until_stopped(&stop, wake.as_fd()).unwrap();
    copy.timeout = Duration::from_secs(30);
    assert!(!copy.uses_ring_fill() && !copy.uses_dma_buf());
    copy.submit(identity(), model(), 8, 48, 2.0).unwrap();
    let RenderImage::Cpu(pixels) = completion(&mut copy).unwrap().pixels else {
        panic!("the copy arm returns owned pixels");
    };
    assert_eq!((pixels.width(), pixels.height()), (8, 48));
    assert_eq!(fill.pixels_mut().unwrap(), pixels.bytes());
    producer.abort_owned_slot(&mut fill).unwrap();

    let fill = producer.acquire_owned_slot(8 * 48 * 4, 4).unwrap();
    let (error, mut fill) = copy
        .submit_fill(identity(), model(), 8, 48, 2.0, fill)
        .unwrap_err();
    assert!(error.contains("copy reference does not fill"), "{error}");
    let (offer, _) = producer
        .commit_owned_slot(&mut fill, ring_upload(), 5)
        .unwrap();
    assert_eq!(offer, 1, "the refusal spent no offer ID");
}
