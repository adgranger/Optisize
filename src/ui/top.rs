use super::theme::*;
use crate::fmt;
use crate::scan::tree::{Tree, NONE};
use eframe::egui::{self, Align2, Color32, Pos2, Rect, Sense, Stroke, Ui};
use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::time::{Duration, Instant};

pub const TOP_N: usize = 100;
const STALE_MS: u64 = 500;
const ROW_H: f32 = 28.0;
const GAP: f32 = 8.0;
const W_RANK: f32 = 40.0;
const W_SIZE: f32 = 110.0;
const W_PCT: f32 = 64.0;
const W_DATE: f32 = 104.0;
const W_KIND: f32 = 96.0;
const MIN_NAME: f32 = 120.0;

/// Les `n` plus gros fichiers de tout l'arbre dans le mode de taille courant,
/// triés par taille décroissante puis id croissant. Exclut dossiers, emplacements non reçus et tailles 0.
pub fn top_files(tree: &Tree, n: usize) -> Vec<u32> {
    if n == 0 {
        return Vec::new();
    }
    let mut heap: BinaryHeap<Reverse<(u64, Reverse<u32>)>> = BinaryHeap::with_capacity(n);
    for id in 1..tree.nodes.len() as u32 {
        let node = &tree.nodes[id as usize];
        if node.is_dir || node.parent == NONE {
            continue;
        }
        let size = tree.size(id);
        if size == 0 {
            continue;
        }
        let key = Reverse((size, Reverse(id)));
        if heap.len() < n {
            heap.push(key);
        } else if heap.peek().is_some_and(|min| key < *min) {
            // Reverse inverse l'ordre : `key < min` ici = clé réelle strictement plus grande.
            heap.pop();
            heap.push(key);
        }
    }
    let mut v: Vec<_> = heap.into_vec().into_iter().map(|r| r.0).collect();
    v.sort_by(|a, b| b.cmp(a));
    v.into_iter().map(|(_, Reverse(id))| id).collect()
}

/// Chemin du dossier parent de `id`, relatif à la racine du scan (composants joints par `\`).
fn rel_dir(tree: &Tree, id: u32) -> String {
    let mut parts = Vec::new();
    let mut cur = tree.nodes[id as usize].parent;
    while cur != 0 && cur != NONE {
        parts.push(tree.name(cur));
        cur = tree.nodes[cur as usize].parent;
    }
    parts.reverse();
    parts.join("\\")
}

/// Vrai si `rows` doit être recalculé. Pendant le scan, au plus toutes les 500 ms.
fn needs_rebuild(built_version: u64, version: u64, scanning: bool, since: Duration) -> bool {
    built_version != version && (!scanning || since >= Duration::from_millis(STALE_MS))
}

struct Row {
    id: u32,
    rank: u32,
    dir: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Col {
    Rank,
    Name,
    Dir,
    Size,
    Pct,
    Date,
    Kind,
}

pub struct TopView {
    rows: Vec<Row>,
    built_version: u64,
    built: Instant,
    sort: Col,
    desc: bool,
    scroll_to: Option<u32>,
    scroll_y: f32,
    view_h: f32,
}

impl TopView {
    pub fn new() -> Self {
        TopView {
            rows: Vec::new(),
            built_version: u64::MAX,
            built: Instant::now(),
            sort: Col::Size,
            desc: true,
            scroll_to: None,
            scroll_y: 0.0,
            view_h: 400.0,
        }
    }

    /// Nouvel arbre : les ids de `rows` ne valent plus rien. Le tri est conservé.
    pub fn reset(&mut self) {
        self.rows.clear();
        self.built_version = u64::MAX;
        self.scroll_to = None;
    }

    fn rebuild(&mut self, tree: &Tree) {
        self.rows = top_files(tree, TOP_N)
            .into_iter()
            .enumerate()
            .map(|(i, id)| Row { id, rank: i as u32 + 1, dir: rel_dir(tree, id) })
            .collect();
        self.apply_sort(tree);
        self.built_version = tree.version;
        self.built = Instant::now();
    }

    /// Tri d'affichage des lignes déjà calculées ; égalités par rang.
    fn apply_sort(&mut self, tree: &Tree) {
        let (col, desc) = (self.sort, self.desc);
        self.rows.sort_by(|a, b| {
            let (na, nb) = (&tree.nodes[a.id as usize], &tree.nodes[b.id as usize]);
            let o = match col {
                Col::Rank => a.rank.cmp(&b.rank),
                Col::Name => tree.name(a.id).to_lowercase().cmp(&tree.name(b.id).to_lowercase()),
                Col::Dir => a.dir.to_lowercase().cmp(&b.dir.to_lowercase()),
                Col::Size | Col::Pct => tree.size(a.id).cmp(&tree.size(b.id)),
                Col::Date => na.mtime.cmp(&nb.mtime),
                Col::Kind => (na.kind as u8).cmp(&(nb.kind as u8)),
            };
            let o = if desc { o.reverse() } else { o };
            o.then(a.rank.cmp(&b.rank))
        });
    }

    fn click_col(&mut self, col: Col, tree: &Tree) {
        if self.sort == col {
            self.desc = !self.desc;
        } else {
            self.sort = col;
            self.desc = matches!(col, Col::Size | Col::Pct | Col::Date);
        }
        self.apply_sort(tree);
    }

    /// Retourne `Some(id)` sur double-clic (révéler dans l'arbre).
    pub fn show(&mut self, ui: &mut Ui, tree: &Tree, selected: &mut Option<u32>, scanning: bool, now: u64) -> Option<u32> {
        let stale = self.rows.iter().any(|r| r.id as usize >= tree.nodes.len());
        if stale || needs_rebuild(self.built_version, tree.version, scanning, self.built.elapsed()) {
            self.rebuild(tree);
        }
        if tree.size(0) == 0 {
            let (rect, _) = ui.allocate_exact_size(ui.available_size(), Sense::hover());
            ui.painter_at(rect).text(rect.center(), Align2::CENTER_CENTER, "Aucune donnée — lancez un scan", egui::FontId::proportional(13.0), MUTED);
            return None;
        }
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(format!("Plus gros fichiers ({})", self.rows.len())).strong());
            let mut s = String::from(if tree.on_disk() { "sur disque" } else { "taille logique" });
            if scanning {
                s.push_str(" — analyse en cours, liste provisoire");
            }
            ui.label(egui::RichText::new(s).size(12.0).color(MUTED));
        });
        ui.add_space(4.0);
        self.header(ui, tree);
        if self.rows.is_empty() {
            let (rect, _) = ui.allocate_exact_size(ui.available_size(), Sense::hover());
            ui.painter_at(rect).text(
                rect.center(),
                Align2::CENTER_CENTER,
                "Aucun fichier de taille non nulle dans ce mode d'affichage",
                egui::FontId::proportional(13.0),
                MUTED,
            );
            return None;
        }

        let mut pick = None;
        let mut dbl = None;
        let rows_n = self.rows.len();
        if ui.memory(|m| m.focused().is_none()) {
            let (up, down) = ui.input(|i| (i.key_pressed(egui::Key::ArrowUp), i.key_pressed(egui::Key::ArrowDown)));
            let idx = selected.and_then(|s| self.rows.iter().position(|r| r.id == s));
            let to = match (idx, up, down) {
                (Some(i), true, _) if i > 0 => Some(self.rows[i - 1].id),
                (Some(i), _, true) if i + 1 < rows_n => Some(self.rows[i + 1].id),
                (None, true, _) | (None, _, true) => Some(self.rows[0].id),
                _ => None,
            };
            if let Some(id) = to {
                pick = Some(id);
                self.scroll_to = Some(id);
            }
        }

        let mut area = egui::ScrollArea::vertical().id_salt("top_rows").auto_shrink([false, false]);
        if let Some(id) = self.scroll_to.take() {
            if let Some(i) = self.rows.iter().position(|r| r.id == id) {
                let (top, bottom) = (i as f32 * ROW_H, (i + 1) as f32 * ROW_H);
                if top < self.scroll_y || bottom > self.scroll_y + self.view_h {
                    area = area.vertical_scroll_offset((top - self.view_h / 2.0).max(0.0));
                }
            }
        }
        let root = tree.size(0).max(1);
        let sel = *selected;
        let rows = &self.rows;
        let foot_h = 22.0;
        let out = ui
            .scope(|ui| {
                ui.set_max_height((ui.available_height() - foot_h).max(60.0));
                area.show_rows(ui, ROW_H, rows_n, |ui, range| {
                    let w = ui.available_width();
                    for r in &rows[range] {
                        let id = r.id;
                        let n = &tree.nodes[id as usize];
                        let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, ROW_H), Sense::click());
                        let p = ui.painter_at(rect);
                        if Some(id) == sel {
                            p.rect_filled(rect, 0.0, SEL);
                        } else if resp.hovered() {
                            p.rect_filled(rect, 0.0, Color32::from_white_alpha(8));
                        }
                        let l = Layout::new(rect);
                        let cy = rect.center().y;
                        let m = mono(12.0);
                        let small = egui::FontId::proportional(12.0);
                        p.text(Pos2::new(l.rank_r, cy), Align2::RIGHT_CENTER, r.rank.to_string(), m.clone(), MUTED);
                        p.rect_filled(Rect::from_min_size(Pos2::new(l.name_x, cy - 6.0), egui::vec2(10.0, 12.0)), 2.0, kind_color(n.kind));
                        let name_rect = Rect::from_min_max(Pos2::new(l.name_x + 16.0, rect.top()), Pos2::new(l.name_x + l.name_w, rect.bottom()));
                        p.with_clip_rect(name_rect).text(Pos2::new(name_rect.left(), cy), Align2::LEFT_CENTER, tree.name(id), egui::FontId::proportional(13.0), TEXT);
                        if l.dir_w > 0.0 {
                            let dir_rect = Rect::from_min_max(Pos2::new(l.dir_x, rect.top()), Pos2::new(l.dir_x + l.dir_w, rect.bottom()));
                            let dir = if r.dir.is_empty() { "(racine du scan)" } else { r.dir.as_str() };
                            p.with_clip_rect(dir_rect).text(Pos2::new(l.dir_x, cy), Align2::LEFT_CENTER, dir, small.clone(), MUTED);
                        }
                        let size = tree.size(id);
                        p.text(Pos2::new(l.size_r, cy), Align2::RIGHT_CENTER, fmt::size(size), m.clone(), TEXT);
                        p.text(Pos2::new(l.pct_r, cy), Align2::RIGHT_CENTER, fmt::percent(size as f64 * 100.0 / root as f64), m, MUTED);
                        p.text(Pos2::new(l.date_r, cy), Align2::RIGHT_CENTER, fmt::date(n.mtime, now), small.clone(), MUTED);
                        p.text(Pos2::new(l.kind_r, cy), Align2::RIGHT_CENTER, n.kind.label(), small, MUTED);
                        if resp.double_clicked() {
                            dbl = Some(id);
                        } else if resp.clicked() {
                            pick = Some(id);
                        }
                        resp.on_hover_ui(|ui| {
                            ui.label(tree.path(id).to_string_lossy().into_owned());
                        });
                    }
                })
            })
            .inner;
        self.scroll_y = out.state.clamped_offset().y;
        self.view_h = out.inner_rect.height();
        let sum: u64 = self.rows.iter().map(|r| tree.size(r.id)).sum();
        ui.label(
            egui::RichText::new(format!(
                "Total de ces {} fichiers : {} ({} de l'analyse)",
                rows_n,
                fmt::size(sum),
                fmt::percent(sum as f64 * 100.0 / root as f64)
            ))
            .size(12.0)
            .color(MUTED),
        );
        if let Some(id) = dbl {
            *selected = Some(id);
            return Some(id);
        }
        if pick.is_some() {
            *selected = pick;
        }
        None
    }

    fn header(&mut self, ui: &mut Ui, tree: &Tree) {
        let w = ui.available_width();
        let (rect, _) = ui.allocate_exact_size(egui::vec2(w, 30.0), Sense::hover());
        let p = ui.painter_at(rect);
        p.rect_filled(rect, 0.0, BAR);
        p.line_segment([rect.left_bottom(), rect.right_bottom()], Stroke::new(1.0, BORDER));
        let l = Layout::new(rect);
        let on_disk = tree.on_disk();
        let cols = [
            (Col::Rank, "#", l.rank_r - W_RANK, Align2::RIGHT_CENTER, l.rank_r, W_RANK),
            (Col::Name, "Nom", l.name_x, Align2::LEFT_CENTER, l.name_x, l.name_w),
            (Col::Dir, "Dossier", l.dir_x, Align2::LEFT_CENTER, l.dir_x, l.dir_w),
            (Col::Size, if on_disk { "Sur disque" } else { "Taille" }, l.size_r - W_SIZE, Align2::RIGHT_CENTER, l.size_r, W_SIZE),
            (Col::Pct, "%", l.pct_r - W_PCT, Align2::RIGHT_CENTER, l.pct_r, W_PCT),
            (Col::Date, "Modifié", l.date_r - W_DATE, Align2::RIGHT_CENTER, l.date_r, W_DATE),
            (Col::Kind, "Type", l.kind_r - W_KIND, Align2::RIGHT_CENTER, l.kind_r, W_KIND),
        ];
        let mut clicked = None;
        for (col, label, left, align, anchor, cw) in cols {
            if cw <= 0.0 {
                continue;
            }
            let cr = Rect::from_min_size(Pos2::new(left, rect.top()), egui::vec2(cw, rect.height()));
            let mut resp = ui.interact(cr, ui.make_persistent_id(("top_col", col as u8)), Sense::click());
            if col == Col::Size {
                resp = resp.on_hover_text(if on_disk {
                    "Espace occupé sur le disque. Un fichier à plusieurs liens physiques n'apparaît qu'une fois. Ctrl+T : taille logique"
                } else {
                    "Taille logique. Un fichier à plusieurs liens physiques apparaît sous chacun de ses noms. Ctrl+T : taille sur disque"
                });
            }
            let mark = if self.sort == col { if self.desc { " (desc.)" } else { " (asc.)" } } else { "" };
            let color = if self.sort == col || resp.hovered() { TEXT } else { MUTED };
            p.with_clip_rect(cr).text(Pos2::new(anchor, rect.center().y), align, format!("{label}{mark}"), egui::FontId::proportional(12.0), color);
            if resp.clicked() {
                clicked = Some(col);
            }
        }
        if let Some(c) = clicked {
            self.click_col(c, tree);
        }
    }
}

/// Positions des colonnes (identiques pour l'en-tête et les lignes).
struct Layout {
    rank_r: f32,
    name_x: f32,
    name_w: f32,
    dir_x: f32,
    dir_w: f32,
    size_r: f32,
    pct_r: f32,
    date_r: f32,
    kind_r: f32,
}

impl Layout {
    fn new(rect: Rect) -> Layout {
        let kind_r = rect.right();
        let date_r = kind_r - W_KIND - GAP;
        let pct_r = date_r - W_DATE - GAP;
        let size_r = pct_r - W_PCT - GAP;
        let rank_r = rect.left() + W_RANK;
        let name_x = rank_r + GAP;
        let flex = (size_r - W_SIZE - GAP - name_x - GAP).max(0.0);
        let name_w = (flex * 0.4).max(MIN_NAME);
        let dir_x = name_x + name_w + GAP;
        let dir_w = (size_r - W_SIZE - GAP - dir_x).max(0.0);
        Layout { rank_r, name_x, name_w, dir_x, dir_w, size_r, pct_r, date_r, kind_r }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan::tree::Entry;
    use std::path::PathBuf;

    fn d(name: &str) -> Entry {
        Entry { name: name.into(), is_dir: true, size: 0, alloc: 0, file_id: 0, mtime: 0 }
    }

    fn f(name: &str, size: u64, alloc: u64, file_id: u64) -> Entry {
        Entry { name: name.into(), is_dir: false, size, alloc, file_id, mtime: 0 }
    }

    fn fs(name: &str, size: u64) -> Entry {
        f(name, size, size, 0)
    }

    fn lcg(s: &mut u64) -> u64 {
        *s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        *s >> 33
    }

    #[test]
    fn top_files_sorted_desc_files_only() {
        let mut t = Tree::new(PathBuf::from("R"));
        t.ingest(0, 1, &[d("A"), fs("f1", 300), fs("f2", 100)]);
        t.ingest(1, 4, &[fs("f3", 200), fs("f4", 0)]);
        assert_eq!(top_files(&t, 100), vec![2, 4, 3]);
    }

    #[test]
    fn top_files_keeps_n_largest() {
        let mut t = Tree::new(PathBuf::from("R"));
        let v: Vec<Entry> = (1..=10).map(|i| fs(&format!("f{i}"), i)).collect();
        t.ingest(0, 1, &v);
        assert_eq!(top_files(&t, 3), vec![10, 9, 8]);
        assert!(top_files(&t, 0).is_empty());
    }

    #[test]
    fn top_files_ties_by_smaller_id() {
        let mut t = Tree::new(PathBuf::from("R"));
        let v: Vec<Entry> = (1..=4).map(|i| fs(&format!("f{i}"), 50)).collect();
        t.ingest(0, 1, &v);
        assert_eq!(top_files(&t, 2), vec![1, 2]);
    }

    #[test]
    fn top_files_matches_full_sort() {
        let mut t = Tree::new(PathBuf::from("R"));
        let dirs: Vec<Entry> = (0..100).map(|i| d(&format!("d{i}"))).collect();
        t.ingest(0, 1, &dirs);
        let mut seed = 7u64;
        for dir in 0..100u32 {
            let v: Vec<Entry> = (0..100).map(|i| fs(&format!("f{i}"), lcg(&mut seed) % 40 * 1000)).collect();
            t.ingest(1 + dir, 101 + dir * 100, &v);
        }
        let mut all: Vec<u32> = (1..t.nodes.len() as u32).filter(|&id| !t.nodes[id as usize].is_dir && t.size(id) > 0).collect();
        all.sort_by_key(|&id| (Reverse(t.size(id)), id));
        all.truncate(100);
        assert_eq!(top_files(&t, 100), all);
    }

    #[test]
    fn top_files_skips_holes() {
        let mut t = Tree::new(PathBuf::from("R"));
        t.ingest(0, 1, &[d("a"), fs("x", 10)]);
        t.ingest(1, 6, &[fs("y", 20)]);
        assert_eq!(t.nodes.len(), 7);
        assert_eq!(top_files(&t, 100), vec![6, 2]);
    }

    #[test]
    fn top_files_follows_on_disk() {
        let mut t = Tree::new(PathBuf::from("R"));
        t.ingest(0, 1, &[f("a", 100, 8192, 0), f("b", 5000, 4096, 0)]);
        assert_eq!(top_files(&t, 100), vec![1, 2]);
        t.set_on_disk(false);
        assert_eq!(top_files(&t, 100), vec![2, 1]);
    }

    #[test]
    fn top_files_hard_links_once_on_disk() {
        let mut t = Tree::new(PathBuf::from("R"));
        t.ingest(0, 1, &[d("a"), d("b")]);
        t.ingest(1, 3, &[f("x", 10_000, 12_288, 42)]);
        t.ingest(2, 4, &[f("y", 10_000, 12_288, 42)]);
        assert_eq!(top_files(&t, 100), vec![3]);
        t.set_on_disk(false);
        assert_eq!(top_files(&t, 100), vec![3, 4]);
    }

    #[test]
    fn rel_dir_relative_to_root() {
        let mut t = Tree::new(PathBuf::from("R"));
        t.ingest(0, 1, &[d("A"), fs("f", 1)]);
        t.ingest(1, 3, &[d("B")]);
        t.ingest(3, 4, &[fs("g", 1)]);
        assert_eq!(rel_dir(&t, 2), "");
        assert_eq!(rel_dir(&t, 4), "A\\B");
    }

    #[test]
    fn needs_rebuild_rules() {
        let ms = Duration::from_millis;
        assert!(!needs_rebuild(3, 3, false, ms(9999)));
        assert!(needs_rebuild(3, 4, false, ms(0)));
        assert!(!needs_rebuild(3, 4, true, ms(100)));
        assert!(needs_rebuild(3, 4, true, ms(600)));
    }

    #[test]
    fn reset_clears_rows_keeps_sort() {
        let mut t = Tree::new(PathBuf::from("R"));
        t.ingest(0, 1, &[fs("b", 20), fs("a", 10)]);
        let mut v = TopView::new();
        v.rebuild(&t);
        v.click_col(Col::Name, &t);
        assert_eq!(v.rows.iter().map(|r| r.id).collect::<Vec<_>>(), vec![2, 1]);
        assert_eq!(v.rows.iter().map(|r| r.rank).collect::<Vec<_>>(), vec![2, 1]);
        v.reset();
        assert!(v.rows.is_empty());
        assert_eq!(v.built_version, u64::MAX);
        assert!(v.sort == Col::Name && !v.desc);
    }

    #[test]
    #[ignore]
    fn top_files_bench_1_3m() {
        let mut t = Tree::new(PathBuf::from("R"));
        let dirs: Vec<Entry> = (0..13_000).map(|i| d(&format!("d{i}"))).collect();
        t.ingest(0, 1, &dirs);
        let mut seed = 1u64;
        let mut v: Vec<Entry> = Vec::new();
        for dir in 0..13_000u32 {
            v.clear();
            v.extend((0..100).map(|i| fs(&format!("f{i}.bin"), 1 + lcg(&mut seed) % 1_000_000_000)));
            t.ingest(1 + dir, 13_001 + dir * 100, &v);
        }
        let start = Instant::now();
        let r = top_files(&t, 100);
        println!("top_files sur {} noeuds : {:?}", t.nodes.len(), start.elapsed());
        assert_eq!(r.len(), 100);
    }
}
