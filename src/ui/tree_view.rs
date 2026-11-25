use super::theme::*;
use crate::fmt;
use crate::scan::tree::{Tree, NONE};
use eframe::egui::{self, Align2, Color32, Pos2, Rect, Sense, Stroke, Ui};
use std::collections::HashSet;
use std::time::{Duration, Instant};

const ROW_H: f32 = 28.0;
const PAD: f32 = 16.0;
const W_DATE: f32 = 104.0;
const W_FILES: f32 = 84.0;
const W_PCT: f32 = 64.0;
const W_SIZE: f32 = 170.0;
const GAP: f32 = 8.0;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Col {
    Name,
    Size,
    Pct,
    Files,
    Date,
}

pub struct TreeView {
    pub expanded: HashSet<u32>,
    sort: Col,
    desc: bool,
    rows: Vec<(u32, u16)>,
    built_version: u64,
    built_filter: String,
    last_build: Instant,
    dirty: bool,
    scroll_to: Option<u32>,
    scroll_y: f32,
    view_h: f32,
}

impl TreeView {
    pub fn new() -> Self {
        TreeView {
            expanded: HashSet::from([0]),
            sort: Col::Size,
            desc: true,
            rows: Vec::new(),
            built_version: u64::MAX,
            built_filter: String::new(),
            last_build: Instant::now(),
            dirty: true,
            scroll_to: None,
            scroll_y: 0.0,
            view_h: 400.0,
        }
    }

    pub fn reset(&mut self) {
        self.expanded.clear();
        self.expanded.insert(0);
        self.rows.clear();
        self.dirty = true;
        self.scroll_to = None;
    }

    pub fn expand_levels(&mut self, tree: &Tree, levels: usize) {
        for id in 0..tree.nodes.len() as u32 {
            let n = &tree.nodes[id as usize];
            if n.is_dir && (id == 0 || n.parent != NONE) && tree.depth(id) < levels {
                self.expanded.insert(id);
            }
        }
        self.dirty = true;
    }

    pub fn collapse_all(&mut self) {
        self.expanded.clear();
        self.expanded.insert(0);
        self.dirty = true;
    }

    /// Déplie les ancêtres de `id` et fait défiler jusqu'à lui.
    pub fn reveal(&mut self, tree: &Tree, id: u32) {
        let mut cur = tree.nodes[id as usize].parent;
        while cur != NONE {
            self.expanded.insert(cur);
            cur = tree.nodes[cur as usize].parent;
        }
        self.scroll_to = Some(id);
        self.dirty = true;
    }

    /// Fait défiler jusqu'à `id` s'il est hors champ, sans rien déplier.
    pub fn scroll_to_row(&mut self, id: u32) {
        self.scroll_to = Some(id);
    }

    fn sort_children(&self, tree: &Tree, ids: &mut [u32]) {
        let key = |id: &u32| -> u64 {
            let n = &tree.nodes[*id as usize];
            match self.sort {
                Col::Size | Col::Pct => tree.size(*id),
                Col::Files => n.files as u64,
                Col::Date => n.mtime as u64,
                Col::Name => 0,
            }
        };
        if self.sort == Col::Name {
            ids.sort_by_cached_key(|id| tree.name(*id).to_lowercase());
        } else {
            ids.sort_by_key(key);
        }
        if self.desc {
            ids.reverse();
        }
    }

    fn rebuild(&mut self, tree: &Tree, filter: &str) {
        self.rows.clear();
        if !filter.is_empty() {
            let needle = filter.as_bytes();
            let mut hits: Vec<u32> = (1..tree.nodes.len() as u32)
                .filter(|&id| {
                    let n = &tree.nodes[id as usize];
                    n.parent != NONE && tree.name(id).as_bytes().windows(needle.len()).any(|w| w.eq_ignore_ascii_case(needle))
                })
                .collect();
            hits.sort_by_key(|&id| std::cmp::Reverse(tree.size(id)));
            hits.truncate(5000);
            self.rows.extend(hits.into_iter().map(|id| (id, 0)));
        } else {
            let mut stack = vec![(0u32, 0u16)];
            let mut kids = Vec::new();
            // Parcours en profondeur itératif ; on empile les enfants en ordre inverse.
            while let Some((id, depth)) = stack.pop() {
                self.rows.push((id, depth));
                if tree.nodes[id as usize].is_dir && self.expanded.contains(&id) {
                    kids.clear();
                    kids.extend(tree.children(id));
                    self.sort_children(tree, &mut kids);
                    stack.extend(kids.iter().rev().map(|&k| (k, depth + 1)));
                }
            }
        }
        self.built_version = tree.version;
        self.built_filter = filter.to_string();
        self.last_build = Instant::now();
        self.dirty = false;
    }

    /// Retourne l'id sélectionné par clic / clavier.
    pub fn show(&mut self, ui: &mut Ui, tree: &Tree, selected: &mut Option<u32>, filter: &str, scanning: bool, now: u64) {
        let stale = tree.version != self.built_version && (!scanning || self.last_build.elapsed() > Duration::from_millis(150));
        if self.dirty || stale || self.built_filter != filter {
            self.rebuild(tree, filter);
        }
        self.header(ui, tree.on_disk());
        let mut toggle = None;
        let mut pick = None;
        let rows_n = self.rows.len();

        // Navigation clavier (si aucun champ texte n'a le focus).
        if ui.memory(|m| m.focused().is_none()) {
            let idx = selected.and_then(|s| self.rows.iter().position(|r| r.0 == s));
            let (up, down, right, left) = ui.input(|i| {
                (
                    i.key_pressed(egui::Key::ArrowUp),
                    i.key_pressed(egui::Key::ArrowDown),
                    i.key_pressed(egui::Key::ArrowRight),
                    i.key_pressed(egui::Key::ArrowLeft),
                )
            });
            let to = match (idx, up, down) {
                (Some(i), true, _) if i > 0 => Some(self.rows[i - 1].0),
                (Some(i), _, true) if i + 1 < rows_n => Some(self.rows[i + 1].0),
                (None, true, _) | (None, _, true) if rows_n > 0 => Some(self.rows[0].0),
                _ => None,
            };
            if let Some(id) = to {
                pick = Some(id);
                self.scroll_to = Some(id);
            }
            if let Some(s) = *selected {
                let n = &tree.nodes[s as usize];
                if right && n.is_dir {
                    toggle = Some((s, true));
                } else if left {
                    if n.is_dir && self.expanded.contains(&s) {
                        toggle = Some((s, false));
                    } else if n.parent != NONE {
                        pick = Some(n.parent);
                        self.scroll_to = Some(n.parent);
                    }
                }
            }
        }

        let mut area = egui::ScrollArea::vertical().id_salt("tree_rows").auto_shrink([false, false]);
        if let Some(id) = self.scroll_to.take() {
            if let Some(i) = self.rows.iter().position(|r| r.0 == id) {
                let (top, bottom) = (i as f32 * ROW_H, (i + 1) as f32 * ROW_H);
                if top < self.scroll_y || bottom > self.scroll_y + self.view_h {
                    area = area.vertical_scroll_offset((top - self.view_h / 2.0).max(0.0));
                }
            }
        }
        let root_size = tree.size(0).max(1);
        let sel = *selected;
        let rows = &self.rows;
        let expanded = &self.expanded;
        let out = area.show_rows(ui, ROW_H, rows_n, |ui, range| {
            let w = ui.available_width();
            for &(id, depth) in &rows[range] {
                let n = &tree.nodes[id as usize];
                let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, ROW_H), Sense::click());
                let p = ui.painter_at(rect);
                if Some(id) == sel {
                    p.rect_filled(rect, 0.0, SEL);
                } else if resp.hovered() {
                    p.rect_filled(rect, 0.0, Color32::from_white_alpha(8));
                }
                let layout = Layout::new(rect);
                let mut x = rect.left() + PAD + depth as f32 * 18.0;
                let cy = rect.center().y;
                let chev = Rect::from_center_size(Pos2::new(x + 6.0, cy), egui::vec2(16.0, ROW_H));
                if n.is_dir && n.child_count > 0 {
                    let open = expanded.contains(&id);
                    let c = Pos2::new(x + 6.0, cy);
                    let pts = if open {
                        [c + egui::vec2(-3.5, -2.0), c + egui::vec2(0.0, 2.5), c + egui::vec2(3.5, -2.0)]
                    } else {
                        [c + egui::vec2(-2.0, -3.5), c + egui::vec2(2.5, 0.0), c + egui::vec2(-2.0, 3.5)]
                    };
                    p.line(pts.to_vec(), Stroke::new(1.6, MUTED));
                }
                x += 18.0;
                let color = if n.is_dir { YELLOW } else { kind_color(n.kind) };
                if n.is_dir {
                    p.rect_filled(Rect::from_min_size(Pos2::new(x, cy - 5.0), egui::vec2(15.0, 11.0)), 2.0, color);
                    p.rect_filled(Rect::from_min_size(Pos2::new(x, cy - 7.0), egui::vec2(6.0, 3.0)), 1.0, color);
                } else {
                    p.rect_filled(Rect::from_min_size(Pos2::new(x + 2.0, cy - 6.0), egui::vec2(10.0, 12.0)), 2.0, color);
                }
                let name_rect = Rect::from_min_max(Pos2::new(x + 22.0, rect.top()), Pos2::new(layout.size_x - GAP, rect.bottom()));
                p.with_clip_rect(name_rect).text(
                    Pos2::new(name_rect.left(), cy),
                    Align2::LEFT_CENTER,
                    tree.name(id),
                    egui::FontId::proportional(13.0),
                    TEXT,
                );
                let parent_size = if n.parent == NONE { root_size } else { tree.size(n.parent).max(1) };
                let frac = (tree.size(id) as f64 / parent_size as f64).clamp(0.0, 1.0);
                let bar = Rect::from_min_size(Pos2::new(layout.size_x, cy - 4.0), egui::vec2(layout.size_w - 64.0 - GAP, 8.0));
                if bar.width() > 12.0 {
                    p.rect_filled(bar, 2.0, TRACK);
                    p.rect_filled(Rect::from_min_size(bar.min, egui::vec2(bar.width() * frac as f32, 8.0)), 2.0, if n.is_dir { ACCENT } else { color });
                }
                let m = mono(12.0);
                p.text(Pos2::new(layout.size_x + layout.size_w, cy), Align2::RIGHT_CENTER, fmt::size(tree.size(id)), m.clone(), TEXT);
                p.text(Pos2::new(layout.pct_r, cy), Align2::RIGHT_CENTER, fmt::percent(frac * 100.0), m.clone(), MUTED);
                let files = if n.is_dir { fmt::thousands(n.files as u64) } else { String::new() };
                p.text(Pos2::new(layout.files_r, cy), Align2::RIGHT_CENTER, files, m, MUTED);
                p.text(
                    Pos2::new(layout.date_r, cy),
                    Align2::RIGHT_CENTER,
                    fmt::date(n.mtime, now),
                    egui::FontId::proportional(12.0),
                    MUTED,
                );
                if resp.clicked() {
                    let on_chev = resp.interact_pointer_pos().is_some_and(|pos| chev.contains(pos));
                    if on_chev && n.is_dir {
                        toggle = Some((id, !expanded.contains(&id)));
                    }
                    pick = Some(id);
                }
                if resp.double_clicked() && n.is_dir {
                    toggle = Some((id, !expanded.contains(&id)));
                }
            }
        });
        self.scroll_y = out.state.clamped_offset().y;
        self.view_h = out.inner_rect.height();
        if let Some((id, open)) = toggle {
            if open {
                self.expanded.insert(id);
            } else {
                self.expanded.remove(&id);
            }
            self.dirty = true;
        }
        if pick.is_some() {
            *selected = pick;
        }
    }

    fn header(&mut self, ui: &mut Ui, on_disk: bool) {
        let w = ui.available_width();
        let (rect, _) = ui.allocate_exact_size(egui::vec2(w, 30.0), Sense::hover());
        let p = ui.painter_at(rect);
        p.rect_filled(rect, 0.0, BAR);
        p.line_segment([rect.left_bottom(), rect.right_bottom()], Stroke::new(1.0, BORDER));
        let l = Layout::new(rect);
        let cols = [
            (Col::Name, "Nom", rect.left() + PAD, Align2::LEFT_CENTER, 120.0),
            (Col::Size, if on_disk { "Sur disque" } else { "Taille" }, l.size_x, Align2::LEFT_CENTER, l.size_w),
            (Col::Pct, "%", l.pct_r, Align2::RIGHT_CENTER, W_PCT),
            (Col::Files, "Fichiers", l.files_r, Align2::RIGHT_CENTER, W_FILES),
            (Col::Date, "Modifié", l.date_r, Align2::RIGHT_CENTER, W_DATE),
        ];
        for (col, label, x, align, cw) in cols {
            let cr = match align {
                Align2::LEFT_CENTER => Rect::from_min_size(Pos2::new(x, rect.top()), egui::vec2(cw, rect.height())),
                _ => Rect::from_min_size(Pos2::new(x - cw, rect.top()), egui::vec2(cw, rect.height())),
            };
            let mut resp = ui.interact(cr, ui.make_persistent_id(col as u8), Sense::click());
            if col == Col::Size {
                resp = resp.on_hover_text(if on_disk {
                    "Espace réellement occupé sur le disque (clusters ; liens physiques comptés une fois). Ctrl+T : taille logique"
                } else {
                    "Taille logique des fichiers. Ctrl+T : taille sur disque"
                });
            }
            let arrow = if self.sort == col { if self.desc { " (desc.)" } else { " (asc.)" } } else { "" };
            let color = if self.sort == col || resp.hovered() { TEXT } else { MUTED };
            p.text(Pos2::new(x, rect.center().y), align, format!("{label}{arrow}"), egui::FontId::proportional(12.0), color);
            if resp.clicked() {
                if self.sort == col {
                    self.desc = !self.desc;
                } else {
                    self.sort = col;
                    self.desc = col != Col::Name;
                }
                self.dirty = true;
            }
        }
    }
}

/// Positions des colonnes de droite (identiques pour l'en-tête et les lignes).
struct Layout {
    date_r: f32,
    files_r: f32,
    pct_r: f32,
    size_x: f32,
    size_w: f32,
}

impl Layout {
    fn new(rect: Rect) -> Layout {
        let date_r = rect.right() - PAD;
        let files_r = date_r - W_DATE - GAP;
        let pct_r = files_r - W_FILES - GAP;
        // Garde une largeur minimale à la colonne Nom : le reste est rogné par le clip.
        // La colonne Taille rétrécit (la barre disparaît) plutôt que de chevaucher les autres.
        let size_w = (pct_r - W_PCT - GAP - (rect.left() + PAD + 120.0)).clamp(70.0, W_SIZE);
        Layout { date_r, files_r, pct_r, size_x: pct_r - W_PCT - GAP - size_w, size_w }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scroll_to_row_keeps_expanded() {
        let mut v = TreeView::new();
        v.scroll_to_row(5);
        assert_eq!(v.scroll_to, Some(5));
        assert_eq!(v.expanded, HashSet::from([0]));
    }
}
