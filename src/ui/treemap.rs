use super::theme::*;
use crate::fmt;
use crate::scan::tree::{Tree, NONE};
use crate::treemap_layout::{self as tl, Cell, Opts, Rect as LRect, Source};
use eframe::egui::{self, Align2, Color32, Rect, Sense, Stroke, StrokeKind, Ui};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Folders,
    Types,
}

struct TreeSrc<'a>(&'a Tree);

impl Source for TreeSrc<'_> {
    fn is_dir(&self, id: u32) -> bool {
        self.0.nodes[id as usize].is_dir
    }
    fn size(&self, id: u32) -> u64 {
        self.0.size(id)
    }
    fn children_sorted(&self, id: u32, out: &mut Vec<u32>) {
        out.extend(self.0.children(id).filter(|&c| self.0.size(c) > 0));
        out.sort_by_key(|&c| std::cmp::Reverse(self.0.size(c)));
    }
}

pub struct TreemapView {
    pub mode: Mode,
    cells: Vec<Cell>,
    key: (i32, i32, u64),
    built: Instant,
}

impl TreemapView {
    pub fn new() -> Self {
        TreemapView { mode: Mode::Folders, cells: Vec::new(), key: (0, 0, u64::MAX), built: Instant::now() }
    }

    /// À appeler quand l'arbre est remplacé : les anciennes `cells` indexeraient le nouvel arbre.
    pub fn reset(&mut self) {
        self.cells.clear();
        self.key = (0, 0, u64::MAX);
    }

    /// Retourne le dossier cliqué (id) le cas échéant.
    pub fn show(&mut self, ui: &mut Ui, tree: &Tree, selected: Option<u32>, scanning: bool, height: f32) -> Option<u32> {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Carte de l'espace").strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.selectable_value(&mut self.mode, Mode::Types, "Types");
                ui.selectable_value(&mut self.mode, Mode::Folders, "Dossiers");
            });
        });
        let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), height), Sense::click());
        let p = ui.painter_at(rect);
        p.rect_filled(rect, 8.0, BG);
        if tree.size(0) == 0 {
            p.text(rect.center(), Align2::CENTER_CENTER, "Aucune donnée — lancez un scan", egui::FontId::proportional(13.0), MUTED);
            return None;
        }
        let key = (rect.width() as i32, rect.height() as i32, tree.version);
        if key != self.key && (!scanning || self.built.elapsed() > Duration::from_millis(250) || key.0 != self.key.0 || key.1 != self.key.1) {
            let area = LRect::new(0.0, 0.0, rect.width(), rect.height());
            self.cells = match self.mode {
                Mode::Folders => tl::layout_nested(&TreeSrc(tree), 0, area, &Opts::default()),
                Mode::Types => Vec::new(),
            };
            self.key = key;
            self.built = Instant::now();
        }
        let origin = rect.min;
        let hover_pos = resp.hover_pos().map(|p| p - origin);

        if self.mode == Mode::Types {
            return self.types(ui, tree, rect);
        }

        let hovered = hover_pos.and_then(|h| self.cells.iter().rposition(|c| c.rect.contains_point(h.x, h.y)));
        let to_rect = |r: LRect| Rect::from_min_size(origin + egui::vec2(r.x, r.y), egui::vec2(r.w, r.h));
        for c in &self.cells {
            let n = &tree.nodes[c.id as usize];
            let r = to_rect(c.rect);
            let label_ok = r.width() > 60.0 && r.height() > 24.0;
            let name = if c.id == 0 { "Racine" } else { tree.name(c.id) };
            if c.frame {
                p.rect_filled(r, 0.0, FRAMES[(c.depth as usize).min(3)]);
                p.rect_stroke(r, 0.0, Stroke::new(1.0, FRAME_BORDER), StrokeKind::Inside);
                if r.width() > 50.0 {
                    label(&p, r, 3.0, &format!("{name} ({})", fmt::size(tree.size(c.id))), TEXT);
                }
            } else {
                let fill = if c.is_dir { FRAMES[3] } else { kind_color(n.kind) };
                p.rect_filled(r, 0.0, fill);
                p.rect_stroke(r, 0.0, Stroke::new(1.0, PANEL2), StrokeKind::Inside);
                if label_ok {
                    let fg = if c.is_dir { TEXT } else { Color32::from_rgb(0x10, 0x14, 0x1a) };
                    label(&p, r, 3.0, &format!("{name} ({})", fmt::size(tree.size(c.id))), fg);
                }
            }
        }
        if let Some(sel) = selected {
            if let Some(c) = self.cells.iter().find(|c| c.id == sel) {
                p.rect_stroke(to_rect(c.rect), 0.0, Stroke::new(2.0, ACCENT), StrokeKind::Inside);
            }
        }
        let mut clicked = None;
        if let Some(i) = hovered {
            let c = self.cells[i];
            p.rect_stroke(to_rect(c.rect), 0.0, Stroke::new(1.5, Color32::WHITE), StrokeKind::Inside);
            let n = &tree.nodes[c.id as usize];
            let tip = format!(
                "{}\n{} sur disque · {} logique · {} fichiers",
                tree.path(c.id).display(),
                fmt::size(n.alloc),
                fmt::size(n.size),
                fmt::thousands(n.files as u64)
            );
            resp.clone().on_hover_text_at_pointer(tip);
            if resp.clicked() {
                // Un clic sur un fichier sélectionne son dossier.
                clicked = Some(if n.is_dir || n.parent == NONE { c.id } else { n.parent });
            }
        }
        clicked
    }

    fn types(&self, ui: &mut Ui, tree: &Tree, rect: Rect) -> Option<u32> {
        let items = super::types::kind_rows(tree);
        let w: Vec<u64> = items.iter().map(|t| t.1).collect();
        let p = ui.painter_at(rect);
        for ((k, s), r) in items.iter().zip(tl::squarify(&w, LRect::new(0.0, 0.0, rect.width(), rect.height()))) {
            let rr = Rect::from_min_size(rect.min + egui::vec2(r.x, r.y), egui::vec2(r.w, r.h));
            p.rect_filled(rr, 0.0, kind_color(*k));
            p.rect_stroke(rr, 0.0, Stroke::new(1.0, PANEL2), StrokeKind::Inside);
            if rr.width() > 70.0 && rr.height() > 24.0 {
                label(&p, rr, 6.0, &format!("{} ({})", k.label(), fmt::size(*s)), Color32::from_rgb(0x10, 0x14, 0x1a));
            }
        }
        None
    }
}

fn label(p: &egui::Painter, r: Rect, pad: f32, text: &str, color: Color32) {
    p.with_clip_rect(r.shrink(1.0)).text(
        r.left_top() + egui::vec2(pad, pad - 1.0),
        Align2::LEFT_TOP,
        text,
        egui::FontId::proportional(12.0),
        color,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reset_clears_cells_and_key() {
        let mut v = TreemapView::new();
        v.cells.push(Cell { id: 1, rect: LRect::new(0.0, 0.0, 1.0, 1.0), depth: 0, is_dir: true, frame: false });
        v.key = (10, 10, 3);
        v.reset();
        assert!(v.cells.is_empty() && v.key == (0, 0, u64::MAX));
    }
}
