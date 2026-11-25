use super::theme::*;
use crate::fmt;
use crate::scan::tree::{Tree, KINDS};
use eframe::egui::{self, Align2, Rect, Ui};

pub fn show(ui: &mut Ui, tree: &Tree) {
    ui.label(egui::RichText::new("Par type de fichier").strong());
    let mut items: Vec<_> = KINDS.iter().map(|&k| (k, tree.type_size(k))).filter(|t| t.1 > 0).collect();
    items.sort_by_key(|t| std::cmp::Reverse(t.1));
    let max = items.first().map_or(1, |t| t.1) as f32;
    for (k, s) in items.iter().take(8) {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 24.0), egui::Sense::hover());
        let p = ui.painter_at(rect);
        let cy = rect.center().y;
        let c = kind_color(*k);
        p.rect_filled(Rect::from_center_size(egui::pos2(rect.left() + 5.0, cy), egui::vec2(10.0, 10.0)), 2.0, c);
        p.text(egui::pos2(rect.left() + 20.0, cy), Align2::LEFT_CENTER, k.label(), egui::FontId::proportional(13.0), TEXT);
        let bar = Rect::from_min_max(egui::pos2(rect.left() + 120.0, cy - 3.0), egui::pos2(rect.right() - 82.0, cy + 3.0));
        p.rect_filled(bar, 3.0, TRACK);
        let w = bar.width() * (*s as f32 / max);
        p.rect_filled(Rect::from_min_size(bar.min, egui::vec2(w, 6.0)), 3.0, c);
        p.text(egui::pos2(rect.right(), cy), Align2::RIGHT_CENTER, fmt::size(*s), mono(12.0), MUTED);
    }
}
