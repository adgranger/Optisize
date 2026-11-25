use super::theme::*;
use super::ScanState;
use crate::fmt;
use eframe::egui::{self, RichText, Ui};
use std::time::Duration;

pub struct Info<'a> {
    pub state: ScanState,
    pub files: u64,
    pub dirs: u64,
    pub elapsed: Duration,
    pub errors: u64,
    pub mem: usize,
    pub toast: Option<&'a str>,
}

pub fn show(ui: &mut Ui, i: &Info) {
    ui.horizontal(|ui| {
        let (text, col) = match i.state {
            ScanState::Idle => ("Prêt — choisissez un dossier", MUTED),
            ScanState::Scanning => ("Analyse en cours…", ACCENT),
            ScanState::Paused => ("En pause", YELLOW),
            ScanState::Done => ("Scan terminé", GREEN),
            ScanState::Stopped => ("Scan arrêté", ORANGE),
        };
        ui.label(RichText::new(format!("● {text}")).color(col));
        ui.add_space(12.0);
        let secs = i.elapsed.as_secs_f64();
        let rate = if secs > 0.05 { (i.files as f64 / secs) as u64 } else { 0 };
        let mono_label = |ui: &mut Ui, s: String| ui.label(RichText::new(s).font(mono(12.0)).color(MUTED));
        mono_label(ui, format!("{} fichiers · {} dossiers", fmt::thousands(i.files), fmt::thousands(i.dirs)));
        ui.add_space(12.0);
        mono_label(ui, format!("{secs:.1} s · {} fichiers/s", fmt::thousands(rate)));
        if i.errors > 0 {
            ui.add_space(12.0);
            mono_label(ui, format!("{} dossiers ignorés (accès refusé)", fmt::thousands(i.errors)));
        }
        if let Some(t) = i.toast {
            ui.add_space(12.0);
            ui.label(RichText::new(t).color(YELLOW));
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(RichText::new(format!("Arbre en mémoire : {}", fmt::size(i.mem as u64))).font(mono(12.0)).color(MUTED));
        });
    });
}
