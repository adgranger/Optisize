use super::theme::*;
use crate::fmt;
use crate::scan::tree::{Kind, Tree, KINDS};
use eframe::egui::{self, Align2, Rect, Ui};

/// Types de taille > 0, du plus gros au plus petit (égalité : ordre de `KINDS`).
pub fn kind_rows(tree: &Tree) -> Vec<(Kind, u64)> {
    let mut items: Vec<_> = KINDS.iter().map(|&k| (k, tree.type_size(k))).filter(|t| t.1 > 0).collect();
    items.sort_by_key(|t| std::cmp::Reverse(t.1));
    items
}

/// `full = false` : colonne droite (8 types max) ; `full = true` : vue centrale complète.
pub fn show(ui: &mut Ui, tree: &Tree, full: bool) {
    ui.label(egui::RichText::new("Par type de fichier").strong());
    let items = kind_rows(tree);
    if full && items.is_empty() {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 120.0), egui::Sense::hover());
        ui.painter_at(rect).text(rect.center(), Align2::CENTER_CENTER, "Aucune donnée — lancez un scan", egui::FontId::proportional(13.0), MUTED);
        return;
    }
    let max = items.first().map_or(1, |t| t.1) as f32;
    let total: u64 = items.iter().map(|t| t.1).sum();
    let (row_h, pct_w, limit) = if full { (28.0, 64.0, usize::MAX) } else { (24.0, 0.0, 8) };
    for (k, s) in items.iter().take(limit) {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), row_h), egui::Sense::hover());
        let p = ui.painter_at(rect);
        let cy = rect.center().y;
        let c = kind_color(*k);
        let mut right = rect.right();
        if full {
            p.text(egui::pos2(right, cy), Align2::RIGHT_CENTER, fmt::percent(*s as f64 * 100.0 / total as f64), mono(12.0), MUTED);
            right -= pct_w;
        }
        p.rect_filled(Rect::from_center_size(egui::pos2(rect.left() + 5.0, cy), egui::vec2(10.0, 10.0)), 2.0, c);
        p.text(egui::pos2(rect.left() + 20.0, cy), Align2::LEFT_CENTER, k.label(), egui::FontId::proportional(13.0), TEXT);
        let bar = Rect::from_min_max(egui::pos2(rect.left() + 120.0, cy - 3.0), egui::pos2(right - 82.0, cy + 3.0));
        p.rect_filled(bar, 3.0, TRACK);
        let w = bar.width() * (*s as f32 / max);
        p.rect_filled(Rect::from_min_size(bar.min, egui::vec2(w, 6.0)), 3.0, c);
        p.text(egui::pos2(right, cy), Align2::RIGHT_CENTER, fmt::size(*s), mono(12.0), MUTED);
    }
    if full {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), row_h), egui::Sense::hover());
        let cy = rect.center().y;
        let p = ui.painter_at(rect);
        p.text(egui::pos2(rect.left() + 20.0, cy), Align2::LEFT_CENTER, "Total", egui::FontId::proportional(13.0), TEXT);
        p.text(egui::pos2(rect.right() - pct_w, cy), Align2::RIGHT_CENTER, fmt::size(total), mono(12.0), TEXT);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan::tree::Entry;
    use std::path::PathBuf;

    fn f(name: &str, size: u64, alloc: u64) -> Entry {
        Entry { name: name.into(), is_dir: false, size, alloc, file_id: 0, mtime: 0 }
    }

    #[test]
    fn kind_rows_sorted_desc_without_empty() {
        let mut t = Tree::new(PathBuf::from("R"));
        t.ingest(0, 1, &[f("a.zip", 10, 10), f("b.mp4", 1000, 1000)]);
        assert_eq!(kind_rows(&t), vec![(Kind::Video, 1000), (Kind::Archive, 10)]);
    }

    #[test]
    fn kind_rows_empty_tree() {
        assert!(kind_rows(&Tree::new(PathBuf::from("R"))).is_empty());
    }

    #[test]
    fn kind_rows_follows_on_disk() {
        let mut t = Tree::new(PathBuf::from("R"));
        t.ingest(0, 1, &[f("a.zip", 5000, 8192)]);
        t.set_on_disk(false);
        assert_eq!(kind_rows(&t), vec![(Kind::Archive, 5000)]);
        t.set_on_disk(true);
        assert_eq!(kind_rows(&t), vec![(Kind::Archive, 8192)]);
    }
}
