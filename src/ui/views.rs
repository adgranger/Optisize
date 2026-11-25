use super::{Action, View};
use eframe::egui::{Key, Ui};

impl View {
    /// Ordre des onglets et du menu. F10 ajoutera sa variante ici.
    pub const ALL: [View; 4] = [View::Details, View::Map, View::Types, View::Top];

    pub fn label(self) -> &'static str {
        match self {
            View::Details => "Détails (arborescence)",
            View::Map => "Carte (treemap)",
            View::Types => "Types de fichiers",
            View::Top => "100 plus gros fichiers",
        }
    }

    pub fn tab(self) -> &'static str {
        match self {
            View::Details => "Détails",
            View::Map => "Carte",
            View::Types => "Types",
            View::Top => "Gros fichiers",
        }
    }

    pub fn key(self) -> Key {
        match self {
            View::Details => Key::F6,
            View::Map => Key::F7,
            View::Types => Key::F8,
            View::Top => Key::F9,
        }
    }

    pub fn key_text(self) -> &'static str {
        match self {
            View::Details => "F6",
            View::Map => "F7",
            View::Types => "F8",
            View::Top => "F9",
        }
    }

    /// Inverse de `key()` (utilisé par les tests ; `shortcuts` parcourt `ALL`).
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn for_key(k: Key) -> Option<View> {
        View::ALL.into_iter().find(|v| v.key() == k)
    }

    /// La colonne droite montre le treemap (sauf si la vue centrale est la carte).
    pub fn side_treemap(self) -> bool {
        self != View::Map
    }

    /// La colonne droite montre les barres Types (sauf si la vue centrale est Types).
    pub fn side_types(self) -> bool {
        self != View::Types
    }
}

/// Onglets au-dessus des puces de lecteurs.
pub fn tabs(ui: &mut Ui, current: View) -> Option<Action> {
    let mut out = None;
    ui.horizontal(|ui| {
        for v in View::ALL {
            let r = ui.selectable_label(current == v, v.tab()).on_hover_text(format!("{} ({})", v.label(), v.key_text()));
            if r.clicked() {
                out = Some(Action::SetView(v));
            }
        }
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui;

    #[test]
    fn view_for_key_maps_f6_to_f9() {
        assert_eq!(View::for_key(egui::Key::F6), Some(View::Details));
        assert_eq!(View::for_key(egui::Key::F7), Some(View::Map));
        assert_eq!(View::for_key(egui::Key::F8), Some(View::Types));
        assert_eq!(View::for_key(egui::Key::F9), Some(View::Top));
    }

    #[test]
    fn view_for_key_ignores_other_keys() {
        for k in [egui::Key::F5, egui::Key::F10, egui::Key::F1, egui::Key::A] {
            assert_eq!(View::for_key(k), None);
        }
    }

    #[test]
    fn view_keys_round_trip() {
        for v in View::ALL {
            assert_eq!(View::for_key(v.key()), Some(v));
        }
        for (i, a) in View::ALL.iter().enumerate() {
            for b in &View::ALL[i + 1..] {
                assert!(a.key() != b.key() && a.label() != b.label() && a.tab() != b.tab());
            }
        }
    }

    #[test]
    fn side_cards_hide_the_central_one() {
        assert!(View::Details.side_treemap() && View::Details.side_types());
        assert!(!View::Map.side_treemap() && View::Map.side_types());
        assert!(View::Types.side_treemap() && !View::Types.side_types());
        assert!(View::Top.side_treemap() && View::Top.side_types());
    }
}
