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
    worker.submit(model(), 8, 48, 2.0).unwrap();
    assert!(worker.submit(model(), 8, 48, 2.0).is_err());
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
    assert!(worker.submit(model(), 8, 48, 2.0).is_err());
}
