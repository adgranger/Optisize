use super::theme::*;
use super::{Action, Flags, ScanState, View};
use eframe::egui::{self, Button, RichText, Ui};

fn btn(ui: &mut Ui, out: &mut Option<Action>, label: &str, tip: &str, a: Action, enabled: bool) {
    let b = Button::new(label).min_size(egui::vec2(0.0, 32.0)).corner_radius(8.0);
    if ui.add_enabled(enabled, b).on_hover_text(tip).clicked() {
        *out = Some(a);
    }
}

pub fn show(ui: &mut Ui, f: Flags, path: &mut String, filter: &mut String) -> Option<Action> {
    let mut out = None;
    ui.horizontal_wrapped(|ui| {
        let busy = f.busy();
        btn(ui, &mut out, "Choisir un dossier…", "Choisir un dossier (Ctrl+O)", Action::PickFolder, true);
        let scan = Button::new(RichText::new("Scanner").color(egui::Color32::from_rgb(8, 18, 31)))
            .fill(ACCENT)
            .min_size(egui::vec2(0.0, 32.0))
            .corner_radius(8.0);
        if ui.add_enabled(!busy, scan).on_hover_text("Démarrer le scan (F5)").clicked() {
            out = Some(Action::Scan);
        }
        let pause = if f.state == ScanState::Paused { "Reprendre" } else { "Pause" };
        btn(ui, &mut out, pause, "Pause / Reprendre (Espace)", Action::TogglePause, busy);
        btn(ui, &mut out, "Arrêter", "Arrêter le scan (Échap)", Action::Stop, busy);
        btn(ui, &mut out, "Rafraîchir", "Rescanner le dossier (Ctrl+R)", Action::Refresh, !busy);
        btn(ui, &mut out, "Nettoyer", "Nettoyage (bientôt disponible)", Action::Clean, true);
        btn(ui, &mut out, "Dossier parent", "Dossier parent (Alt+Haut)", Action::Parent, !busy);
        ui.label(RichText::new("Dossier").color(MUTED));
        let r = ui.add(
            egui::TextEdit::singleline(path)
                .font(mono(13.0))
                .desired_width(ui.available_width().clamp(160.0, 520.0) - 230.0)
                .margin(egui::vec2(8.0, 7.0)),
        );
        if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && !busy {
            out = Some(Action::Scan);
        }
        let r = ui.add(
            egui::TextEdit::singleline(filter)
                .hint_text("Filtrer… (nom)")
                .desired_width(180.0)
                .margin(egui::vec2(8.0, 7.0)),
        );
        // Le filtre ne s'applique qu'à Détails : le modifier y ramène.
        if r.changed() && f.view != View::Details {
            out = Some(Action::SetView(View::Details));
        }
    });
    out
}
