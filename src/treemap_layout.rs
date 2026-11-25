//! Disposition de treemap pure (sans egui) : squarified + imbrication façon TreeSize
//! (chaque dossier = cadre avec bandeau d'en-tête de hauteur fixe, enfants dessinés dedans).

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub const ZERO: Rect = Rect { x: 0.0, y: 0.0, w: 0.0, h: 0.0 };

    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect { x, y, w, h }
    }

    pub fn area(&self) -> f32 {
        self.w * self.h
    }

    pub fn contains_point(&self, px: f32, py: f32) -> bool {
        px >= self.x && px < self.x + self.w && py >= self.y && py < self.y + self.h
    }
}

/// Squarified treemap. `weights` doit idéalement être trié par ordre décroissant.
/// Retourne un rectangle par poids (même ordre) ; les poids nuls reçoivent un rectangle vide.
pub fn squarify(weights: &[u64], bounds: Rect) -> Vec<Rect> {
    let n = weights.len();
    let mut out = vec![Rect::ZERO; n];
    let total: u64 = weights.iter().sum();
    if total == 0 || bounds.w <= 0.0 || bounds.h <= 0.0 {
        return out;
    }
    let scale = bounds.area() as f64 / total as f64;
    let areas: Vec<f32> = weights.iter().map(|&w| (w as f64 * scale) as f32).collect();
    let mut rest = bounds;
    let mut i = 0;
    while i < n && weights[i] > 0 && rest.w > 0.0 && rest.h > 0.0 {
        let short = rest.w.min(rest.h);
        // Étend la rangée tant que le pire rapport d'aspect s'améliore.
        let (mut j, mut sum, mut prev_worst) = (i, 0.0f32, f32::INFINITY);
        while j < n && weights[j] > 0 {
            let s = sum + areas[j];
            // areas est décroissant : max = areas[i], min = areas[j]
            let worst = ((short * short * areas[i]) / (s * s)).max((s * s) / (short * short * areas[j]));
            if j > i && worst > prev_worst {
                break;
            }
            sum = s;
            prev_worst = worst;
            j += 1;
        }
        if rest.w >= rest.h {
            let cw = sum / rest.h;
            let mut y = rest.y;
            for k in i..j {
                let ch = areas[k] / cw;
                out[k] = Rect::new(rest.x, y, cw, ch);
                y += ch;
            }
            rest.x += cw;
            rest.w -= cw;
        } else {
            let ch = sum / rest.w;
            let mut x = rest.x;
            for k in i..j {
                let cw = areas[k] / ch;
                out[k] = Rect::new(x, rest.y, cw, ch);
                x += cw;
            }
            rest.y += ch;
            rest.h -= ch;
        }
        i = j;
    }
    out
}

/// Source de données abstraite (implémentée par l'arbre de scan, ou un faux arbre en test).
pub trait Source {
    fn is_dir(&self, id: u32) -> bool;
    fn size(&self, id: u32) -> u64;
    /// Enfants de `id` de taille > 0, triés par taille décroissante.
    fn children_sorted(&self, id: u32, out: &mut Vec<u32>);
}

#[derive(Clone, Copy)]
pub struct Opts {
    /// Hauteur du bandeau d'en-tête de chaque dossier.
    pub header: f32,
    /// Marge intérieure (gauche/droite/bas) d'un cadre de dossier.
    pub pad: f32,
    /// Taille minimale du contenu pour qu'un dossier soit imbriqué (sinon dessiné plein).
    pub min_inner: f32,
    pub max_depth: u8,
    pub max_children: usize,
}

impl Default for Opts {
    fn default() -> Self {
        Opts { header: 18.0, pad: 2.0, min_inner: 14.0, max_depth: 10, max_children: 200 }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Cell {
    pub id: u32,
    pub rect: Rect,
    pub depth: u8,
    pub is_dir: bool,
    /// Vrai si le dossier est dessiné comme un cadre (en-tête + enfants à l'intérieur).
    pub frame: bool,
}

/// Cellules dans l'ordre parent -> enfants (un parent précède toujours ses enfants).
pub fn layout_nested<S: Source>(src: &S, root: u32, area: Rect, o: &Opts) -> Vec<Cell> {
    let mut out = Vec::new();
    place(src, root, area, 0, o, &mut out);
    out
}

fn place<S: Source>(src: &S, id: u32, rect: Rect, depth: u8, o: &Opts, out: &mut Vec<Cell>) {
    let is_dir = src.is_dir(id);
    let inner = Rect::new(rect.x + o.pad, rect.y + o.header, rect.w - 2.0 * o.pad, rect.h - o.header - o.pad);
    let mut kids = Vec::new();
    if is_dir && depth < o.max_depth && inner.w >= o.min_inner && inner.h >= o.min_inner {
        src.children_sorted(id, &mut kids);
        kids.truncate(o.max_children);
    }
    if kids.is_empty() {
        out.push(Cell { id, rect, depth, is_dir, frame: false });
        return;
    }
    out.push(Cell { id, rect, depth, is_dir, frame: true });
    let weights: Vec<u64> = kids.iter().map(|&k| src.size(k)).collect();
    for (k, r) in kids.iter().zip(squarify(&weights, inner)) {
        if r.area() >= 1.0 {
            place(src, *k, r, depth + 1, o, out);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn overlap(a: &Rect, b: &Rect) -> f32 {
        let w = (a.x + a.w).min(b.x + b.w) - a.x.max(b.x);
        let h = (a.y + a.h).min(b.y + b.h) - a.y.max(b.y);
        if w > 0.0 && h > 0.0 {
            w * h
        } else {
            0.0
        }
    }

    fn inside(inner: &Rect, outer: &Rect) -> bool {
        let e = 1e-2;
        inner.x >= outer.x - e
            && inner.y >= outer.y - e
            && inner.x + inner.w <= outer.x + outer.w + e
            && inner.y + inner.h <= outer.y + outer.h + e
    }

    #[test]
    fn squarify_fills_bounds_without_overlap_and_proportional() {
        let w = [600u64, 600, 400, 300, 200, 200, 100, 50];
        let b = Rect::new(10.0, 20.0, 600.0, 400.0);
        let r = squarify(&w, b);
        let total: u64 = w.iter().sum();
        let mut sum_area = 0.0;
        for (i, ri) in r.iter().enumerate() {
            assert!(inside(ri, &b), "rect {i} hors limites: {ri:?}");
            let expect = b.area() * w[i] as f32 / total as f32;
            assert!((ri.area() - expect).abs() / expect < 1e-3, "aire {i}: {} vs {}", ri.area(), expect);
            sum_area += ri.area();
            for rj in &r[i + 1..] {
                assert!(overlap(ri, rj) < 1e-1, "chevauchement {ri:?} {rj:?}");
            }
        }
        assert!((sum_area - b.area()).abs() / b.area() < 1e-3);
    }

    #[test]
    fn squarify_zero_weights() {
        assert!(squarify(&[0, 0], Rect::new(0.0, 0.0, 10.0, 10.0)).iter().all(|r| r.area() == 0.0));
        let r = squarify(&[5, 0], Rect::new(0.0, 0.0, 10.0, 10.0));
        assert!((r[0].area() - 100.0).abs() < 1e-3 && r[1].area() == 0.0);
    }

    struct Fake {
        // (is_dir, size, enfants)
        n: Vec<(bool, u64, Vec<u32>)>,
    }
    impl Source for Fake {
        fn is_dir(&self, id: u32) -> bool {
            self.n[id as usize].0
        }
        fn size(&self, id: u32) -> u64 {
            self.n[id as usize].1
        }
        fn children_sorted(&self, id: u32, out: &mut Vec<u32>) {
            out.extend(&self.n[id as usize].2);
            out.sort_by_key(|&c| std::cmp::Reverse(self.size(c)));
        }
    }

    #[test]
    fn nested_children_inside_parent_no_overlap_proportional() {
        // 0 racine: A(1)=60 [a1=40,a2=20], B(4)=30 [b1=30], C(6)=10 (fichier)
        let f = Fake {
            n: vec![
                (true, 100, vec![1, 4, 6]),
                (true, 60, vec![2, 3]),
                (false, 40, vec![]),
                (false, 20, vec![]),
                (true, 30, vec![5]),
                (false, 30, vec![]),
                (false, 10, vec![]),
            ],
        };
        let o = Opts::default();
        let cells = layout_nested(&f, 0, Rect::new(0.0, 0.0, 800.0, 500.0), &o);
        assert_eq!(cells.len(), 7);
        assert!(cells[0].frame);
        let rect_of = |id: u32| cells.iter().find(|c| c.id == id).unwrap().rect;
        let parent_of = |id: u32| f.n.iter().position(|n| n.2.contains(&id)).unwrap() as u32;
        for c in cells.iter().skip(1) {
            let p = rect_of(parent_of(c.id));
            let inner = Rect::new(p.x + o.pad, p.y + o.header, p.w - 2.0 * o.pad, p.h - o.header - o.pad);
            assert!(inside(&c.rect, &inner), "{} hors du parent", c.id);
        }
        // pas de chevauchement entre frères, aires proportionnelles aux tailles
        for kids in [vec![1u32, 4, 6], vec![2, 3]] {
            let sum: u64 = kids.iter().map(|&k| f.size(k)).sum();
            let parent = rect_of(parent_of(kids[0]));
            let inner_area = (parent.w - 2.0 * o.pad) * (parent.h - o.header - o.pad);
            for (i, &a) in kids.iter().enumerate() {
                let expect = inner_area * f.size(a) as f32 / sum as f32;
                assert!((rect_of(a).area() - expect).abs() / expect < 1e-3);
                for &b in &kids[i + 1..] {
                    assert!(overlap(&rect_of(a), &rect_of(b)) < 1e-1);
                }
            }
        }
    }
}
