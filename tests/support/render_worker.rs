use super::*;
use crate::config::parse_shell_config;
use chrono::{DateTime, Utc};

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
            pixels: crate::protocol::ContentPixels::from_rgba8(8, 48, vec![0; 1536]).unwrap(),
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
        requests,
        completions,
        submitted_at: Some(Instant::now()),
        timeout: Duration::ZERO,
    };
    assert!(worker.poll().unwrap_err().contains("remain quarantined"));
    assert!(worker.submit(identity(), model(), 8, 48, 2.0).is_err());
}

fn identity() -> RenderIdentity {
    RenderIdentity {
        grant: sophia_protocol::ContentGrant {
            connection_epoch: 1,
            content_grant_epoch: 2,
        },
        output: sophia_protocol::ContentOutputId {
            id: 1,
            generation: 1,
        },
        allocation: sophia_protocol::ContentAllocationId {
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
