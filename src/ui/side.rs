//! Cartes de la colonne droite : nettoyage suggéré (stub) et panneau de sélection.

use super::theme::*;
use super::Action;
use crate::fmt;
use eframe::egui::{self, Button, Color32, Frame, Margin, RichText, Ui};

pub struct Cleanup<'a> {
    pub scanned: bool,
    pub total: u64,
    /// (libellé, taille) des plus gros dossiers temp / cache détectés.
    pub top: &'a [(String, u64)],
}

pub fn cleanup_card(ui: &mut Ui, c: &Cleanup) -> Option<Action> {
    let green_bg = Color32::from_rgb(0x15, 0x1f, 0x1a);
    let green_border = Color32::from_rgb(0x2f, 0x4a, 0x3a);
    let mut out = None;
    Frame::new().fill(green_bg).stroke(egui::Stroke::new(1.0, green_border)).corner_radius(8.0).inner_margin(Margin::same(12)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.label(RichText::new("Nettoyage suggéré").strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if c.scanned {
                    ui.label(RichText::new(format!("{} récupérables", fmt::size(c.total))).font(mono(12.0)).color(GREEN));
                }
            });
        });
        let text = if !c.scanned {
            "Lancez un scan pour détecter les dossiers temporaires et caches.".to_string()
        } else if c.top.is_empty() {
            "Aucun dossier temporaire ou cache détecté.".to_string()
        } else {
            c.top.iter().map(|(n, s)| format!("{n} ({})", fmt::size(*s))).collect::<Vec<_>>().join(" · ")
        };
        ui.label(RichText::new(text).size(12.0).color(MUTED));
        let b = Button::new(RichText::new("Examiner et nettoyer").strong().color(Color32::from_rgb(0x0c, 0x1a, 0x12)))
            .fill(GREEN)
            .min_size(egui::vec2(0.0, 32.0))
            .corner_radius(8.0);
        if ui.add(b).on_hover_text("Bientôt disponible — rien n'est supprimé").clicked() {
            out = Some(Action::Clean);
        }
    });
    out
}

pub fn selection_panel(ui: &mut Ui, sel: Option<(&str, String)>) -> Option<Action> {
    let mut out = None;
    Frame::new().fill(PANEL).stroke(egui::Stroke::new(1.0, BORDER)).corner_radius(8.0).inner_margin(Margin::same(12)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        match &sel {
            Some((name, path)) => {
                ui.label(RichText::new(format!("Sélection : {name}")).strong());
                ui.label(RichText::new(path).font(mono(12.0)).color(MUTED));
            }
            None => {
                ui.label(RichText::new("Aucune sélection").strong());
            }
        }
        ui.horizontal_wrapped(|ui| {
            let open = Button::new("Ouvrir dans l'Explorateur").min_size(egui::vec2(0.0, 32.0)).corner_radius(8.0);
            if ui.add_enabled(sel.is_some(), open).clicked() {
                out = Some(Action::OpenExplorer);
            }
            let trash = Button::new(RichText::new("Corbeille").color(Color32::from_rgb(0xff, 0x9a, 0xa5)))
                .stroke(egui::Stroke::new(1.0, Color32::from_rgb(0x5a, 0x2f, 0x35)))
                .min_size(egui::vec2(0.0, 32.0))
                .corner_radius(8.0);
            if ui.add_enabled(sel.is_some(), trash).on_hover_text("Bientôt disponible — rien n'est supprimé").clicked() {
                out = Some(Action::Trash);
            }
        });
    });
    out
}
