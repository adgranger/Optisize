use super::{Action, Flags, ScanState, View};
use eframe::egui::{self, Button, Ui};

/// Entrée de menu : `action = None` => fonctionnalité pas encore disponible (grisée).
fn item(ui: &mut Ui, out: &mut Option<Action>, label: &str, key: &str, action: Option<Action>, enabled: bool) {
    let b = Button::new(label).shortcut_text(key).min_size(egui::vec2(300.0, 28.0));
    if ui.add_enabled(enabled && action.is_some(), b).clicked() {
        *out = action;
        ui.close();
    }
}

/// Entrée de vue : toujours active, la vue courante est marquée.
fn view_item(ui: &mut Ui, out: &mut Option<Action>, f: Flags, v: View) {
    let b = Button::new(v.label()).shortcut_text(v.key_text()).selected(f.view == v).min_size(egui::vec2(300.0, 28.0));
    if ui.add(b).clicked() {
        *out = Some(Action::SetView(v));
        ui.close();
    }
}

pub fn show(ui: &mut Ui, f: Flags) -> Option<Action> {
    let mut out = None;
    let o = &mut out;
    let busy = f.busy();
    egui::MenuBar::new().ui(ui, |ui| {
        ui.label(egui::RichText::new("Optisize").strong().color(super::theme::ACCENT));
        ui.menu_button("Fichier", |ui| {
            item(ui, o, "Choisir un dossier…", "Ctrl+O", Some(Action::PickFolder), true);
            item(ui, o, "Choisir un lecteur", ">", None, true);
            item(ui, o, "Dossiers récents", ">", None, true);
            ui.separator();
            item(ui, o, "Exporter en CSV / Excel…", "Ctrl+E", None, true);
            item(ui, o, "Exporter en HTML / PDF…", "", None, true);
            item(ui, o, "Enregistrer le scan (.optisize)", "Ctrl+S", None, true);
            item(ui, o, "Ouvrir un scan enregistré…", "", None, true);
            ui.separator();
            item(ui, o, "Quitter", "Alt+F4", Some(Action::Quit), true);
        });
        ui.menu_button("Édition", |ui| {
            item(ui, o, "Ouvrir dans l’Explorateur", "Entrée", Some(Action::OpenExplorer), f.has_selection);
            item(ui, o, "Copier le chemin", "Ctrl+C", Some(Action::CopyPath), f.has_selection);
            item(ui, o, "Propriétés", "Alt+Entrée", None, true);
            ui.separator();
            item(ui, o, "Mettre à la corbeille", "Suppr", None, true);
            item(ui, o, "Supprimer définitivement", "Maj+Suppr", None, true);
            ui.separator();
            item(ui, o, "Rechercher", "Ctrl+F", None, true);
            item(ui, o, "Tout sélectionner", "Ctrl+A", None, true);
        });
        ui.menu_button("Affichage", |ui| {
            for v in View::ALL {
                view_item(ui, o, f, v);
            }
            item(ui, o, "Fichiers les plus anciens", "", None, true);
            item(ui, o, "Doublons", "", None, true);
            ui.separator();
            item(ui, o, "Tout développer (3 niveaux)", "Ctrl+Maj+Droite", Some(Action::ExpandAll), true);
            item(ui, o, "Tout réduire", "Ctrl+Maj+Gauche", Some(Action::CollapseAll), true);
            ui.separator();
            let t = if f.on_disk { "Afficher la taille logique" } else { "Afficher la taille sur disque" };
            item(ui, o, t, "Ctrl+T", Some(Action::ToggleOnDisk), true);
            item(ui, o, "Unités : Go / Gio", "", None, true);
            item(ui, o, "Thème clair / sombre", "", None, true);
        });
        ui.menu_button("Analyse", |ui| {
            item(ui, o, "Démarrer le scan", "F5", Some(Action::Scan), !busy || f.state == ScanState::Paused);
            let pause = if f.state == ScanState::Paused { "Reprendre" } else { "Pause" };
            item(ui, o, pause, "Espace", Some(Action::TogglePause), busy);
            item(ui, o, "Arrêter", "Échap", Some(Action::Stop), busy);
            ui.separator();
            item(ui, o, "Rafraîchir le dossier sélectionné", "Ctrl+R", Some(Action::Refresh), !busy);
            item(ui, o, "Tout rescanner", "Ctrl+Maj+R", Some(Action::Refresh), !busy);
            ui.separator();
            item(ui, o, "Dossier parent", "Alt+Haut", Some(Action::Parent), !busy);
            item(ui, o, "Surveiller les changements en direct", "", None, true);
            item(ui, o, "Exclusions…", "", None, true);
        });
        ui.menu_button("Outils", |ui| {
            item(ui, o, "Nettoyage…", "Ctrl+L", Some(Action::Clean), true);
            item(ui, o, "Rechercher les doublons", "", None, true);
            item(ui, o, "Fichiers temporaires et caches", "", None, true);
            item(ui, o, "Comparer deux scans", "", None, true);
            ui.separator();
            item(ui, o, "Planifier un scan", "", None, true);
            item(ui, o, "Options…", "Ctrl+,", None, true);
        });
        ui.menu_button("Aide", |ui| {
            item(ui, o, "Raccourcis clavier", "F1", None, true);
            item(ui, o, "Documentation", "", None, true);
            item(ui, o, "À propos d’Optisize", "", None, true);
        });
    });
    out
}
