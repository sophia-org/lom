//! Fixed fixture observations for attended full-versus-region pixel capture.
//! These entries never represent a live workspace or clock feed.
use super::*;
use crate::config::{Region, Renderer};

const HOLD: Duration = Duration::from_secs(10);
const PHASES: [(&str, &str); 9] = [
    ("initial", include_str!("../../diagnostics/regions/00.kdl")),
    (
        "establish",
        include_str!("../../diagnostics/regions/01.kdl"),
    ),
    ("seconds", include_str!("../../diagnostics/regions/02.kdl")),
    (
        "workspace",
        include_str!("../../diagnostics/regions/03.kdl"),
    ),
    ("minute", include_str!("../../diagnostics/regions/04.kdl")),
    ("rename", include_str!("../../diagnostics/regions/05.kdl")),
    ("restore", include_str!("../../diagnostics/regions/06.kdl")),
    (
        "reestablish",
        include_str!("../../diagnostics/regions/07.kdl"),
    ),
    (
        "seconds-again",
        include_str!("../../diagnostics/regions/08.kdl"),
    ),
];

pub(super) struct Sequence {
    pub(super) full: bool,
    pub(super) next_at: Option<Instant>,
    models: Vec<Model>,
    phase: usize,
    presented: usize,
}

impl Sequence {
    fn role(&self) -> &'static str {
        if self.full { "full" } else { "regions" }
    }
}

/// Reject unrelated renderer/configuration choices before connecting.
pub(crate) fn validate_config(config: &PanelConfig) -> Result<(), String> {
    let shape = [
        (ModuleKind::Workspaces, Region::Start, None),
        (ModuleKind::Clock, Region::Center, Some("%H:%M")),
        (ModuleKind::Clock, Region::End, Some("%H:%M:%S")),
    ];
    if config.renderer != Renderer::Cpu
        || config.dma_buf
        || config.position != Position::Top
        || ![24, 512].contains(&config.height)
        || config.modules.len() != shape.len()
        || shape.iter().any(|(kind, region, format)| {
            config
                .modules
                .iter()
                .filter(|m| {
                    m.kind == *kind
                        && m.region == *region
                        && format.is_none_or(|f| m.format == f)
                        && m.names.is_empty()
                        && m.hidden.is_empty()
                        && !m.all_monitors
                })
                .count()
                != 1
        })
    {
        return Err(
            "region sequence requires the fixed CPU top-panel clock/workspace configuration".into(),
        );
    }
    Ok(())
}

impl<R: ContentRenderer> ShellService<R> {
    pub(crate) fn enable_region_sequence(&mut self, full: bool) -> Result<(), String> {
        validate_config(&self.config)?;
        if self.panels.len() != 1
            || self.statistics.is_some()
            || self.cadence.is_some()
            || self.sequence.is_some()
            || self.rendering_panel.is_some()
            || !self.presentations.is_empty()
            || self.panels[0].presented.is_some()
            || !self.renderer.supports_regions()
            || !self.renderer.uses_ring_fill()
            || self.renderer.uses_dma_buf()
        {
            return Err("region sequence requires a fresh single-panel CPU region owner".into());
        }
        let models = self.fixture_models(&PHASES.map(|(_, source)| source))?;
        self.statistics = Some(WorkStatistics::default());
        self.sequence = Some(Sequence {
            full,
            next_at: None,
            models,
            phase: 0,
            presented: 0,
        });
        println!(
            "lom_region_sequence schema=1 source=fixture role={} phases={} hold_ms={}",
            if full { "full" } else { "regions" },
            PHASES.len(),
            HOLD.as_millis()
        );
        self.install_sequence_phase()
    }

    /// Shared admission/remapping for fixture-owned diagnostics, not live feeds.
    pub(super) fn fixture_models(&self, sources: &[&str]) -> Result<Vec<Model>, String> {
        validate_config(&self.config)?;
        if self.panels.len() != 1 {
            return Err("fixture diagnostic requires one panel".into());
        }
        let panel = &self.panels[0];
        if panel.allocation.pixel.width != 1920
            || panel.allocation.pixel.height != self.config.height
            || panel.allocation.scale_numerator != panel.allocation.scale_denominator
            || panel.allocation.scale_numerator == 0
        {
            return Err("region sequence requires 1920-wide scale-one allocation".into());
        }
        let mut models = Vec::with_capacity(sources.len());
        for source in sources {
            let mut model =
                crate::runtime::parse_fixture(source, self.config.clone(), self.theme.clone())
                    .map_err(|e| format!("region fixture: {e}"))?;
            if !model.fixture
                || !model.fixture_values.is_empty()
                || model.popout.is_some()
                || model
                    .workspaces
                    .entries
                    .iter()
                    .any(|w| w.output != model.output || w.action.is_some())
            {
                return Err("region fixture contains unsupported data or action authority".into());
            }
            // Fixture output 1 is a token, not an externally granted identity.
            model.output = panel.output.output.id;
            model.epoch = self.connection.connection_epoch();
            model.workspaces.epoch = model.epoch;
            model.workspaces.generation = 0;
            model.workspaces.active_output = Some(model.output);
            for entry in &mut model.workspaces.entries {
                entry.output = model.output;
            }
            models.push(model);
        }
        Ok(models)
    }

    fn install_sequence_phase(&mut self) -> Result<(), String> {
        if self.rendering_panel.is_some() || !self.presentations.is_empty() {
            return Err("region sequence changed model with an update in flight".into());
        }
        let sequence = self.sequence.as_ref().ok_or("region sequence missing")?;
        self.panels[0].model = sequence.models[sequence.phase].clone();
        self.panels[0].dirty = true;
        self.panels[0].interaction_dirty = true;
        println!(
            "lom_region_phase schema=1 event=installed source=fixture role={} phase={} name={}",
            sequence.role(),
            sequence.phase,
            PHASES[sequence.phase].0
        );
        self.progressed = true;
        Ok(())
    }

    pub(super) fn advance_sequence(&mut self, now: Instant) -> Result<(), String> {
        let Some(sequence) = &mut self.sequence else {
            return Ok(());
        };
        if sequence.next_at.is_none_or(|at| now < at) {
            return Ok(());
        }
        if sequence.presented != sequence.phase + 1 || sequence.presented >= PHASES.len() {
            return Err("region sequence phase advance is out of order".into());
        }
        sequence.phase += 1;
        sequence.next_at = None;
        self.install_sequence_phase()
    }

    pub(super) fn sequence_presented(
        &mut self,
        panel: usize,
        candidate: u64,
        uploads: u32,
        upload_bytes: u64,
        reused: bool,
    ) -> Result<(), String> {
        let Some(sequence) = &mut self.sequence else {
            return Ok(());
        };
        let current = &self.panels[0];
        if panel != 0
            || reused
            || sequence.presented != sequence.phase
            || candidate != (sequence.phase + 1) as u64
            || current
                .presented
                .as_ref()
                .is_none_or(|p| p.model != sequence.models[sequence.phase])
        {
            return Err(
                "region sequence refused an extra, reused or contaminated candidate".into(),
            );
        }
        sequence.presented += 1;
        if sequence.presented < PHASES.len() {
            sequence.next_at = Some(Instant::now() + HOLD);
        }
        println!(
            "lom_region_phase schema=1 event=presented source=fixture role={} phase={} name={} candidate_generation={} placements={} uploads={} upload_bytes={} regions_disabled={}",
            sequence.role(),
            sequence.phase,
            PHASES[sequence.phase].0,
            candidate,
            current.current.placements().count(),
            uploads,
            upload_bytes,
            u8::from(current.regions_disabled)
        );
        Ok(())
    }

    pub(crate) fn region_sequence_final(&self) -> Option<String> {
        let sequence = self.sequence.as_ref()?;
        let complete = sequence.presented == PHASES.len()
            && sequence.next_at.is_none()
            && self.rendering_panel.is_none()
            && self.presentations.is_empty()
            && !self.panels[0].dirty;
        Some(format!(
            "lom_region_final schema=1 source=fixture role={} status={} phases={} presented={}",
            sequence.role(),
            if complete { "complete" } else { "refused" },
            PHASES.len(),
            sequence.presented
        ))
    }
}
