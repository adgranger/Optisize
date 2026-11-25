use super::theme::*;
use crate::fmt;
use crate::scan::tree::{Tree, NONE};
use crate::treemap_layout::{self as tl, Cell, Opts, Rect as LRect, Source};
use eframe::egui::{self, Align2, Color32, PointerButton, Rect, Sense, Stroke, StrokeKind, Ui};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
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

type Key = (i32, i32, u64, u32, Mode);
const SENTINEL: Key = (0, 0, u64::MAX, 0, Mode::Folders);
/// Largeur réservée à la bascule Dossiers / Types à droite de l'en-tête.
const TOGGLE_W: f32 = 140.0;

pub struct TreemapView {
    pub mode: Mode,
    cells: Vec<Cell>,
    key: Key,
    built: Instant,
    /// Dossier affiché en racine de la carte (zoom) ; 0 = racine du scan.
    root: u32,
}

/// Vrai si `cells` doit être recalculé. Seul un changement de `version` pendant le scan est limité à 250 ms.
fn needs_rebuild(old: Key, new: Key, scanning: bool, since_build: Duration) -> bool {
    if old == new {
        return false;
    }
    if (old.0, old.1, old.3, old.4) != (new.0, new.1, new.3, new.4) {
        return true;
    }
    !scanning || since_build > Duration::from_millis(250)
}

fn zoomable(tree: &Tree, id: u32) -> bool {
    tree.nodes.get(id as usize).is_some_and(|n| n.is_dir && (id == 0 || n.parent != NONE))
}

/// Dossier visé par un clic sur `id` : lui-même s'il est dossier ou racine, sinon son parent.
fn click_target(tree: &Tree, id: u32) -> u32 {
    let n = &tree.nodes[id as usize];
    if n.is_dir || n.parent == NONE { id } else { n.parent }
}

/// Ids du chemin de la racine du scan jusqu'à `root` : `[0, …, root]`.
fn crumbs(tree: &Tree, root: u32) -> Vec<u32> {
    let mut v = Vec::new();
    let mut cur = root;
    while cur != NONE && v.len() <= tree.nodes.len() {
        v.push(cur);
        cur = tree.nodes[cur as usize].parent;
    }
    v.reverse();
    v
}

fn short(s: &str, max: usize) -> String {
    if s.chars().count() > max {
        s.chars().take(max.saturating_sub(1)).chain(std::iter::once('…')).collect()
    } else {
        s.to_string()
    }
}

fn crumb_label(tree: &Tree, id: u32) -> String {
    let full = if id == 0 {
        tree.root_path.file_name().map_or_else(|| tree.root_path.to_string_lossy().into_owned(), |n| n.to_string_lossy().into_owned())
    } else {
        tree.name(id).to_string()
    };
    short(&full, 24)
}

/// Segments à afficher : `Some(i)` = segment i, `None` = « … ». Toujours le premier et le dernier.
fn fit_crumbs(widths: &[f32], sep: f32, ellipsis: f32, avail: f32) -> Vec<Option<usize>> {
    let n = widths.len();
    if n <= 1 {
        return (0..n).map(Some).collect();
    }
    let total: f32 = widths.iter().sum::<f32>() + sep * (n - 1) as f32;
    if total <= avail {
        return (0..n).map(Some).collect();
    }
    // Garde le premier, un « … », puis les `k` derniers segments (k maximal qui tient).
    for k in (1..n - 1).rev() {
        let tail: f32 = widths[n - k..].iter().sum();
        let w = widths[0] + ellipsis + tail + sep * (k + 1) as f32;
        if w <= avail {
            let mut v = vec![Some(0), None];
            v.extend((n - k..n).map(Some));
            return v;
        }
    }
    if widths[0] + ellipsis + widths[n - 1] + sep * 2.0 <= avail {
        return vec![Some(0), None, Some(n - 1)];
    }
    vec![None, Some(n - 1)]
}

impl TreemapView {
    pub fn new() -> Self {
        TreemapView { mode: Mode::Folders, cells: Vec::new(), key: SENTINEL, built: Instant::now(), root: 0 }
    }

    /// À appeler quand l'arbre est remplacé : les anciennes `cells` et `root` indexeraient le nouvel arbre.
    pub fn reset(&mut self) {
        self.cells.clear();
        self.key = SENTINEL;
        self.root = 0;
    }

    /// Remonte d'un niveau ; `false` si on est déjà à la racine.
    pub fn zoom_out(&mut self, tree: &Tree) -> bool {
        if self.root == 0 {
            return false;
        }
        let p = tree.nodes.get(self.root as usize).map_or(0, |n| n.parent);
        self.root = if p != NONE && zoomable(tree, p) { p } else { 0 };
        true
    }

    fn zoom_to(&mut self, tree: &Tree, id: u32) -> bool {
        if id == self.root || !zoomable(tree, id) {
            return false;
        }
        self.root = id;
        true
    }

    /// En-tête : titre ou (zoomé, mode Dossiers) bouton retour + fil d'Ariane, puis la bascule de mode.
    fn header(&mut self, ui: &mut Ui, tree: &Tree) {
        let mut jump = None;
        let mut up = false;
        ui.horizontal(|ui| {
            if self.root == 0 || self.mode == Mode::Types {
                ui.label(egui::RichText::new("Carte de l'espace").strong());
            } else {
                up = ui.small_button("Remonter").on_hover_text("Remonter d'un niveau (Retour arrière)").clicked();
                let avail = (ui.available_width() - TOGGLE_W).max(0.0);
                let ids = crumbs(tree, self.root);
                let labels: Vec<String> = ids.iter().map(|&i| crumb_label(tree, i)).collect();
                let font = egui::FontId::proportional(13.0);
                let measure = |t: &str| ui.painter().layout_no_wrap(t.to_string(), font.clone(), MUTED).size().x;
                let widths: Vec<f32> = labels.iter().map(|l| measure(l)).collect();
                let sep = measure("›") + 2.0 * ui.spacing().item_spacing.x;
                let ell = measure("…");
                let full = tree.path(self.root).display().to_string();
                let last = ids.len() - 1;
                let mut first = true;
                for seg in fit_crumbs(&widths, sep, ell, avail) {
                    if !first {
                        ui.label(egui::RichText::new("›").color(MUTED));
                    }
                    first = false;
                    match seg {
                        None => {
                            ui.label(egui::RichText::new("…").color(MUTED)).on_hover_text(&full);
                        }
                        Some(i) if i == last => {
                            ui.label(egui::RichText::new(&labels[i]).strong()).on_hover_text(&full);
                        }
                        Some(i) => {
                            if ui.link(&labels[i]).on_hover_text(tree.path(ids[i]).display().to_string()).clicked() {
                                jump = Some(ids[i]);
                            }
                        }
                    }
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.selectable_value(&mut self.mode, Mode::Types, "Types");
                ui.selectable_value(&mut self.mode, Mode::Folders, "Dossiers");
            });
        });
        if up {
            self.zoom_out(tree);
        } else if let Some(id) = jump {
            self.zoom_to(tree, id);
        }
    }

    /// Retourne le dossier cliqué (id) le cas échéant.
    pub fn show(&mut self, ui: &mut Ui, tree: &Tree, selected: Option<u32>, scanning: bool, height: f32) -> Option<u32> {
        if !zoomable(tree, self.root) {
            self.root = 0;
        }
        self.header(ui, tree);
        let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), height), Sense::click());
        let p = ui.painter_at(rect);
        p.rect_filled(rect, 8.0, BG);
        if tree.size(0) == 0 {
            p.text(rect.center(), Align2::CENTER_CENTER, "Aucune donnée — lancez un scan", egui::FontId::proportional(13.0), MUTED);
            return None;
        }
        let key = (rect.width() as i32, rect.height() as i32, tree.version, self.root, self.mode);
        if needs_rebuild(self.key, key, scanning, self.built.elapsed()) {
            let area = LRect::new(0.0, 0.0, rect.width(), rect.height());
            self.cells = match self.mode {
                Mode::Folders => tl::layout_nested(&TreeSrc(tree), self.root, area, &Opts::default()),
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

        if tree.size(self.root) == 0 {
            p.text(rect.center(), Align2::CENTER_CENTER, "Ce dossier est vide dans ce mode d'affichage", egui::FontId::proportional(13.0), MUTED);
            if resp.hovered() && ui.input(|i| i.pointer.button_clicked(PointerButton::Extra1)) {
                self.zoom_out(tree);
            }
            return None;
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
            let target = click_target(tree, c.id);
            if resp.clicked() {
                // Un clic sur un fichier sélectionne son dossier.
                clicked = Some(target);
            }
            if resp.double_clicked() && zoomable(tree, target) && target != self.root {
                self.root = target;
                ui.ctx().request_repaint();
            }
        }
        if resp.hovered() && ui.input(|i| i.pointer.button_clicked(PointerButton::Extra1)) {
            self.zoom_out(tree);
            ui.ctx().request_repaint();
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
    use crate::scan::tree::Entry;
    use std::path::PathBuf;

    fn e(name: &str, is_dir: bool) -> Entry {
        Entry { name: name.into(), is_dir, size: 10, alloc: 10, file_id: 0, mtime: 0 }
    }

    /// 0 → [A=1, f.txt=2] ; A → [B=3, g.mp4=4] ; B → [h.zip=5]
    fn sample(root: &str) -> Tree {
        let mut t = Tree::new(PathBuf::from(root));
        t.ingest(0, 1, &[e("A", true), e("f.txt", false)]);
        t.ingest(1, 3, &[e("B", true), e("g.mp4", false)]);
        t.ingest(3, 5, &[e("h.zip", false)]);
        t
    }

    #[test]
    fn reset_clears_cells_key_and_root() {
        let mut v = TreemapView::new();
        v.cells.push(Cell { id: 1, rect: LRect::new(0.0, 0.0, 1.0, 1.0), depth: 0, is_dir: true, frame: false });
        v.key = (10, 10, 3, 3, Mode::Types);
        v.root = 3;
        v.reset();
        assert!(v.cells.is_empty() && v.key == SENTINEL && v.root == 0);
    }

    #[test]
    fn zoomable_accepts_dirs_only() {
        let mut t = sample("r");
        for id in [0, 1, 3] {
            assert!(zoomable(&t, id), "{id}");
        }
        for id in [2, 4, 5, t.nodes.len() as u32, u32::MAX] {
            assert!(!zoomable(&t, id), "{id}");
        }
        t.ingest(1, 8, &[e("C", true)]); // ids 6 et 7 restent des trous
        assert!(!zoomable(&t, 6) && zoomable(&t, 8));
    }

    #[test]
    fn click_target_file_goes_to_parent() {
        let t = sample("r");
        assert_eq!((click_target(&t, 4), click_target(&t, 3), click_target(&t, 0)), (1, 3, 0));
    }

    #[test]
    fn crumbs_from_root_to_node() {
        let t = sample("r");
        assert_eq!(crumbs(&t, 3), [0, 1, 3]);
        assert_eq!(crumbs(&t, 0), [0]);
    }

    #[test]
    fn crumb_label_root_and_children() {
        #[cfg(windows)]
        assert_eq!(crumb_label(&sample("C:\\"), 0), "C:\\");
        let t = sample("C:\\x\\Users");
        assert_eq!(crumb_label(&t, 0), "Users");
        assert_eq!(crumb_label(&t, 1), "A");
    }

    #[test]
    fn short_cuts_on_chars_not_bytes() {
        assert_eq!(short("éééééé", 4), "ééé…");
        let s24 = "é".repeat(24);
        assert_eq!(short(&s24, 24), s24);
        assert_eq!(short(&"é".repeat(25), 24), format!("{}…", "é".repeat(23)));
    }

    #[test]
    fn fit_crumbs_all_fit() {
        assert_eq!(fit_crumbs(&[50.0; 3], 10.0, 20.0, 500.0), [Some(0), Some(1), Some(2)]);
    }

    #[test]
    fn fit_crumbs_drops_from_the_root_side() {
        let v = fit_crumbs(&[50.0; 6], 10.0, 20.0, 220.0);
        assert_eq!(v[..2], [Some(0), None]);
        assert_eq!(v.last(), Some(&Some(5)));
        let w: f32 = v.iter().map(|s| s.map_or(20.0, |_| 50.0)).sum::<f32>() + 10.0 * (v.len() - 1) as f32;
        assert!(w <= 220.0, "{w}");
    }

    #[test]
    fn fit_crumbs_too_narrow_keeps_last() {
        assert_eq!(fit_crumbs(&[50.0; 4], 10.0, 20.0, 10.0), [None, Some(3)]);
        assert_eq!(fit_crumbs(&[50.0], 10.0, 20.0, 1.0), [Some(0)]);
    }

    #[test]
    fn needs_rebuild_rules() {
        let k: Key = (100, 100, 1, 0, Mode::Folders);
        let (z, ms) = (Duration::ZERO, Duration::from_millis);
        assert!(!needs_rebuild(k, k, true, ms(999)));
        assert!(needs_rebuild(k, (100, 100, 1, 3, Mode::Folders), true, z));
        assert!(needs_rebuild(k, (100, 100, 1, 0, Mode::Types), true, z));
        assert!(needs_rebuild(k, (101, 100, 1, 0, Mode::Folders), true, z));
        let v = (100, 100, 2, 0, Mode::Folders);
        assert!(!needs_rebuild(k, v, true, ms(100)));
        assert!(needs_rebuild(k, v, true, ms(300)));
        assert!(needs_rebuild(k, v, false, z));
    }

    #[test]
    fn zoom_out_climbs_to_root() {
        let t = sample("r");
        let mut v = TreemapView::new();
        v.root = 3;
        assert!(v.zoom_out(&t) && v.root == 1);
        assert!(v.zoom_out(&t) && v.root == 0);
        assert!(!v.zoom_out(&t) && v.root == 0);
    }

    #[test]
    fn zoom_to_ignores_files_and_invalid() {
        let t = sample("r");
        let mut v = TreemapView::new();
        assert!(!v.zoom_to(&t, 4) && !v.zoom_to(&t, 99) && v.root == 0);
        assert!(v.zoom_to(&t, 1) && v.root == 1);
        assert!(!v.zoom_to(&t, 1));
    }

    #[test]
    fn ids_survive_later_batches() {
        let mut t = sample("r");
        let before = crumbs(&t, 3);
        t.ingest(1, 6, &[e("D", true), e("k.txt", false)]);
        assert!(zoomable(&t, 3));
        assert_eq!(crumbs(&t, 3), before);
    }
}
